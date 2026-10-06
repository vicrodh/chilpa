//! Finding the lyrics of what plays, off the window's thread.
//!
//! The window never waits for lyrics. A track that starts sends a request
//! to one thread that owns the pipeline; the window picks the answer up on a
//! later tick. When tracks are skipped faster than lyrics are found, only
//! the last one is looked for: the ones in between are no longer playing.
//!
//! Two things reach the pipeline from here: the lyrics service, asked over
//! HTTP for any track ([`crate::lyrics_client`]), and a source that declares
//! `track_lyrics`, which answers `track.lyrics` for its own. The source goes
//! through [`Natives::call_raw`], so the pause it asks for in its error is
//! kept. No plugin is asked to find lyrics by name.

use crate::natives::Natives;
use chilpa_lyrics::pipeline::{Answer, Limits, Pipeline, Remote, Request, Resolved, Source};
use chilpa_lyrics::{wire, Cache, Track};
use chilpa_plugin::native::NativeError;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Where found lyrics are kept between runs. The version is in the name:
/// a new shape of row starts a new file instead of misreading the old one.
fn cache_path() -> std::path::PathBuf {
    chilpa_dirs::cache().join("lyrics").join("v1.db")
}

struct Job {
    generation: u64,
    request: Request,
}

/// The lyrics thread, seen from the facade.
pub(crate) struct LyricsService {
    jobs: Sender<Job>,
    /// The generation of the last track asked about. Answers for older ones
    /// are dropped.
    wanted: Arc<AtomicU64>,
    delivered: Arc<Mutex<Option<(u64, Option<Resolved>)>>>,
}

impl LyricsService {
    pub(crate) fn start() -> LyricsService {
        let (jobs, rx) = mpsc::channel::<Job>();
        let wanted = Arc::new(AtomicU64::new(0));
        let delivered: Arc<Mutex<Option<(u64, Option<Resolved>)>>> = Arc::new(Mutex::new(None));
        let (thread_wanted, thread_delivered) = (wanted.clone(), delivered.clone());
        let spawned = std::thread::Builder::new().name("lyrics".into()).spawn(move || {
            // A cache that cannot open on disk still works in memory: the
            // lyrics are found again next run, nothing worse.
            let Some(cache) = Cache::open(&cache_path()).or_else(|_| Cache::in_memory()).ok() else { return };
            // The lyrics service, asked for what neither the file nor its
            // source has.
            let remote: Arc<dyn Remote> = Arc::new(crate::lyrics_client::LyricsServiceClient::new());
            let pipeline = Pipeline::new(cache, Some(remote), Limits::default());
            while let Ok(mut job) = rx.recv() {
                while let Ok(newer) = rx.try_recv() {
                    job = newer;
                }
                if job.generation != thread_wanted.load(Ordering::SeqCst) {
                    continue;
                }
                let outcome = pipeline.resolve(&job.request, false);
                let deliver = |resolved: Option<Resolved>| {
                    if job.generation == thread_wanted.load(Ordering::SeqCst) {
                        *thread_delivered.lock().unwrap_or_else(|e| e.into_inner()) = Some((job.generation, resolved));
                    }
                };
                deliver(outcome.resolved.clone());
                if outcome.refresh {
                    // What the cache had is old: it was shown, and now the
                    // source and the service are asked again. What they
                    // say now replaces it.
                    let again = pipeline.resolve(&job.request, true);
                    if again.resolved.is_some() && again.resolved != outcome.resolved {
                        deliver(again.resolved);
                    }
                }
            }
        });
        if let Err(e) = spawned {
            eprintln!("lyrics: the thread did not start: {e}");
        }
        LyricsService { jobs, wanted, delivered }
    }

    /// Asks for the lyrics of a track. `generation` grows with every track
    /// that starts; [`LyricsService::take`] gives back the answer for it.
    ///
    /// The plugins are the ones running now, handed in with every request:
    /// the first track can start before they do, and whoever kept the
    /// plugins of that moment would never ask a source for its lyrics.
    pub(crate) fn request(&self, natives: &Arc<Natives>, generation: u64, track: Track, path: Option<std::path::PathBuf>, source: Option<(String, String)>) {
        self.wanted.store(generation, Ordering::SeqCst);
        let source = source
            .filter(|(id, _)| natives.has_track_lyrics(id))
            .map(|(id, reference)| {
                let name = natives.display_name(&id);
                (Arc::new(NativeSource { natives: natives.clone(), id, name }) as Arc<dyn Source>, reference)
            });
        let _ = self.jobs.send(Job { generation, request: Request { track, path, source } });
    }

    /// The answer for that generation, once, when it has arrived. `Some(None)`
    /// means it arrived and there are no lyrics.
    pub(crate) fn take(&self, generation: u64) -> Option<Option<Resolved>> {
        let mut delivered = self.delivered.lock().unwrap_or_else(|e| e.into_inner());
        match delivered.as_ref() {
            Some((g, _)) if *g == generation => delivered.take().map(|(_, r)| r),
            _ => None,
        }
    }
}

impl crate::Chilpa {
    /// Asks for the lyrics of what is starting to play.
    ///
    /// The lyrics thread looks for them, in the order `chilpa_lyrics::pipeline`
    /// describes: what came with the file, the cache, the source and the
    /// lyrics service. Playback does not wait; they show when they arrive.
    /// A station has no lyrics to look for.
    pub(crate) fn load_lyrics(&mut self, path: &std::path::Path, length_ms: Option<u32>) {
        self.now.lyrics.clear();
        self.now.lyrics_synced = false;
        self.now.lyrics_source.clear();
        self.now.lyrics_line = -1;
        self.now.lyrics_revision += 1;
        self.lyrics = None;
        self.lyrics_generation += 1;
        if self.now.live {
            return;
        }
        let source = crate::source_opener::of_plugin(&self.now.path);
        // The ISRC the scan read from the tags, when the file is in the
        // library: with it, lyrics for another recording cannot get through.
        let (isrc, mbid) = match source {
            None => self
                .library()
                .and_then(|library| {
                    let row = library.tracks_by_paths(&[path.display().to_string()]).ok()?.pop().flatten()?;
                    library.fingerprints_for(&[row.id]).ok()?.remove(&row.id)
                })
                .unwrap_or_default(),
            Some(_) => (None, None),
        };
        let track = chilpa_lyrics::Track {
            title: self.now.title.clone(),
            artist: self.now.artist.clone(),
            album: Some(self.now.album.clone()).filter(|a| !a.trim().is_empty()),
            duration_ms: length_ms.or_else(|| (self.now.total_seconds > 0.0).then(|| (self.now.total_seconds * 1000.0).min(f64::from(u32::MAX)) as u32)),
            isrc,
            mbid,
        };
        let file = (source.is_none() && path.is_file()).then(|| path.to_path_buf());
        let service = self.lyrics_service.get_or_insert_with(LyricsService::start);
        service.request(&self.natives, self.lyrics_generation, track, file, source);
    }

    /// Shows the lyrics the lyrics thread found for what is playing.
    pub(crate) fn advance_lyrics(&mut self) {
        let Some(service) = &self.lyrics_service else { return };
        let Some(Some(found)) = service.take(self.lyrics_generation) else { return };
        self.now.lyrics = found.document.lines.iter().map(|l| l.text.clone()).collect();
        self.now.lyrics_synced = found.document.timing > chilpa_lyrics::Timing::Unsynced;
        self.now.lyrics_source = match found.origin.provider.as_str() {
            "file" => chilpa_i18n::t("file"),
            "tags" => chilpa_i18n::t("tags"),
            _ => found.origin.name.clone(),
        };
        self.lyrics = Some(found.document);
        self.now.lyrics_revision += 1;
        self.now.lyrics_line = self.line_at(self.now.seconds);
    }

    /// The lyrics of what is playing, whole, as JSON: `{"source", "document"}`,
    /// the document with its lines, words, times and how finely it is timed,
    /// for a view that follows them word by word. Empty when there are none.
    /// Read it again when `lyrics_revision` changes.
    pub fn lyrics_json(&self) -> String {
        self.lyrics
            .as_ref()
            .map(|document| serde_json::json!({ "source": self.now.lyrics_source, "document": document }).to_string())
            .unwrap_or_default()
    }
}

/// A track's length in milliseconds from its frames and rate, when known.
pub(crate) fn length_ms(frames: Option<u64>, rate: u32) -> Option<u32> {
    match (frames, rate) {
        (Some(f), rate) if rate > 0 => Some((f * 1000 / u64::from(rate)).min(u64::from(u32::MAX)) as u32),
        _ => None,
    }
}

/// What a plugin's failure means to the pipeline: a failure, with the pause
/// it asked for when it asked for one.
fn failed(error: NativeError) -> Answer {
    let retry_after = match &error {
        NativeError::Rpc { data: Some(data), .. } => wire::retry_after_ms(&serde_json::json!({ "data": data })).map(Duration::from_millis),
        _ => None,
    };
    Answer::Failed { retry_after, message: error.to_string() }
}

/// A source that has lyrics for its own tracks.
struct NativeSource {
    natives: Arc<Natives>,
    id: String,
    name: String,
}

impl Source for NativeSource {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn lyrics(&self, reference: &str, track: &Track, timeout: Duration) -> Answer {
        match self.natives.call_raw(&self.id, "track.lyrics", wire::source_params(reference), timeout) {
            Ok(answer) => match wire::read_source(&answer, track) {
                Ok(Some(candidate)) => Answer::Found(vec![candidate]),
                Ok(None) => Answer::None,
                Err(message) => Answer::Failed { retry_after: None, message },
            },
            Err(error) => failed(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pause_asked_for_in_an_error_is_kept() {
        let error = NativeError::Rpc { code: -32000, message: "busy".into(), data: Some(serde_json::json!({"retry_after_ms": 2500})) };
        assert_eq!(failed(error), Answer::Failed { retry_after: Some(Duration::from_millis(2500)), message: "busy".into() });
        assert_eq!(failed(NativeError::Timeout), Answer::Failed { retry_after: None, message: NativeError::Timeout.to_string() });
    }

    #[test]
    fn the_length_comes_from_frames_and_rate() {
        assert_eq!(length_ms(Some(441_000), 44_100), Some(10_000));
        assert_eq!(length_ms(Some(441_000), 0), None);
        assert_eq!(length_ms(None, 44_100), None);
    }
}
