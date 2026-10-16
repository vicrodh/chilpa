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
//!
//! Lyrics that carry no translation in the reader's language are sent to
//! the service for one, when the reader asked for that in Settings. The
//! translation comes after the lyrics, in the same place: the lines show
//! while it is made.

use crate::natives::Natives;
use chilpa_lyrics::pipeline::{Answer, Limits, Pipeline, Remote, Request, Resolved, Source, Translator};
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

/// The preference that says which language a translation is wanted in:
/// empty or "auto" for the window's own, else a language code.
pub(crate) const TRANSLATION_LANGUAGE: &str = "ui.lyrics.translation";
/// Whether lyrics without a translation in that language are sent to the
/// service for one. Enabled unless the reader turned it off.
pub(crate) const TRANSLATE: &str = "integrations.lyrics.translate";
pub(crate) const LYRICS_API: &str = "integrations.lyrics.service";
pub(crate) const ENABLED: &str = "integrations.lyrics.enabled";
pub(crate) const REMOTE_PROVIDERS: &[&str] = &["lrclib", "netease", "qq", "kugou"];

/// Where the lyrics of what plays stand, for the window to say so.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum State {
    /// Nothing is playing that could have lyrics.
    #[default]
    Idle,
    Searching,
    None,
    Found,
}

/// The lyrics of what is playing, as the facade keeps them.
#[derive(Default)]
pub(crate) struct Lyrics {
    /// The document, so it is not reread on every tick.
    pub(crate) document: Option<chilpa_lyrics::Document>,
    /// The thread that finds lyrics, and the track it is finding them for.
    service: Option<LyricsService>,
    generation: u64,
    state: State,
    /// What the last lookup was for, to look again when the language of
    /// the translation changes.
    asked: Option<(std::path::PathBuf, Option<u32>)>,
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
            // source has, and for the translation of what is shown.
            let service = Arc::new(crate::lyrics_client::LyricsServiceClient::new());
            let pipeline = Pipeline::new(cache, Some(service.clone() as Arc<dyn Remote>), Limits::default()).with_translator(service as Arc<dyn Translator>);
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
                // A translation the lyrics lack comes after them, in the
                // same place. A track that stopped playing meanwhile is not
                // translated.
                let translated = |resolved: &Option<Resolved>| -> Option<Resolved> {
                    if job.generation != thread_wanted.load(Ordering::SeqCst) {
                        return None;
                    }
                    let found = resolved.as_ref()?;
                    let layer = pipeline.translate(&job.request, &found.document)?;
                    let mut document = found.document.clone();
                    document.layers.push(layer);
                    Some(Resolved { document, origin: found.origin.clone() })
                };
                if let Some(with) = translated(&outcome.resolved) {
                    deliver(Some(with));
                }
                if outcome.refresh && job.generation == thread_wanted.load(Ordering::SeqCst) {
                    // What the cache had is old: it was shown, and now the
                    // source and the service are asked again. What they
                    // say now replaces it.
                    let again = pipeline.resolve(&job.request, true);
                    if again.resolved.is_some() && again.resolved != outcome.resolved {
                        deliver(again.resolved.clone());
                        if let Some(with) = translated(&again.resolved) {
                            deliver(Some(with));
                        }
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
    pub(crate) fn request(&self, natives: &Arc<Natives>, generation: u64, request: (Track, Option<std::path::PathBuf>, Option<(String, String)>, Option<String>, bool, bool, Option<Vec<String>>)) {
        let (track, path, source, language, translate, allow_remote, remote_providers) = request;
        self.wanted.store(generation, Ordering::SeqCst);
        let source = source
            .filter(|(id, _)| natives.has_track_lyrics(id))
            .map(|(id, reference)| {
                let name = natives.display_name(&id);
                (Arc::new(NativeSource { natives: natives.clone(), id, name }) as Arc<dyn Source>, reference)
            });
        let _ = self.jobs.send(Job { generation, request: Request { track, path, source, language, translate, allow_remote, offline: chilpa_core::network::current().offline, remote_providers } });
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
        self.lyrics.document = None;
        self.lyrics.generation += 1;
        self.lyrics.asked = Some((path.to_path_buf(), length_ms));
        if let Some(service) = &self.lyrics.service { service.wanted.store(self.lyrics.generation, Ordering::SeqCst); }
        if !chilpa_core::network::current().lyrics {
            self.lyrics.state = State::Idle;
            return;
        }
        self.lyrics.state = if self.now.live { State::Idle } else { State::Searching };
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
        let language = self.translation_language();
        let translate = self.translating();
        let policy = chilpa_core::network::current();
        let allow_remote = policy.lyrics && policy.lyrics_service;
        let selected: Vec<String> = REMOTE_PROVIDERS.iter().filter(|id| {
            self.preference(&format!("integrations.lyrics.provider.{id}")).as_deref() != Some("false")
        }).map(|id| (*id).to_string()).collect();
        let remote_providers = (selected.len() != REMOTE_PROVIDERS.len()).then_some(selected);
        let generation = self.lyrics.generation;
        let service = self.lyrics.service.get_or_insert_with(LyricsService::start);
        service.request(&self.natives, generation, (track, file, source, Some(language), translate, allow_remote, remote_providers));
    }

    /// Whether the reader asked for lyrics without a translation in their
    /// language to be translated.
    fn translating(&self) -> bool {
        chilpa_core::network::current().lyrics && self.preference(TRANSLATE).as_deref() != Some("false")
    }

    /// The language a translation is wanted in: the one chosen for lyrics,
    /// or the window's own. Its primary subtag only: "es", not "es-MX".
    fn translation_language(&self) -> String {
        let chosen = self.preference(TRANSLATION_LANGUAGE).filter(|c| !c.is_empty() && c != "auto");
        let code = chosen.unwrap_or_else(|| chilpa_i18n::current_language().to_string());
        code.split(['-', '_']).next().unwrap_or_default().to_ascii_lowercase()
    }

    /// A preference of the lyrics changed. Another language for the
    /// translation asks the track's source again, which is who translates,
    /// and the service when translating is on; for any other track it only
    /// changes which translation is offered.
    pub(crate) fn lyrics_preference_changed(&mut self, key: &str) {
        if !matches!(key, TRANSLATION_LANGUAGE | TRANSLATE | LYRICS_API | ENABLED | "integrations.offline")
            && !key.starts_with("integrations.lyrics.provider.") {
            return;
        }
        let again = key != TRANSLATION_LANGUAGE || self.translating() || crate::source_opener::of_plugin(&self.now.path).is_some();
        match self.lyrics.asked.clone() {
            Some((path, length_ms)) if again => self.load_lyrics(&path, length_ms),
            _ => self.now.lyrics_revision += 1,
        }
    }

    /// Shows the lyrics the lyrics thread found for what is playing.
    pub(crate) fn advance_lyrics(&mut self) {
        let Some(service) = &self.lyrics.service else { return };
        let Some(answer) = service.take(self.lyrics.generation) else { return };
        self.now.lyrics_revision += 1;
        let Some(found) = answer else {
            // It arrived and there are none: the window stops saying it is
            // looking.
            self.lyrics.state = State::None;
            return;
        };
        self.lyrics.state = State::Found;
        self.now.lyrics = found.document.lines.iter().map(|l| l.text.clone()).collect();
        self.now.lyrics_synced = found.document.timing > chilpa_lyrics::Timing::Unsynced;
        self.now.lyrics_source = match found.origin.provider.as_str() {
            "file" => chilpa_i18n::t("file"),
            "tags" => chilpa_i18n::t("tags"),
            _ => found.origin.name.clone(),
        };
        self.lyrics.document = Some(found.document);
        self.now.lyrics_line = self.line_at(self.now.seconds);
    }

    /// The lyrics' own buttons in Settings: what each does, and what its
    /// row says afterwards.
    pub(crate) fn lyrics_maintenance(&mut self, key: &str) -> Option<String> {
        (key == "library.lyricsCache").then(|| self.clear_lyrics_cache())
    }

    /// Forgets every lyric kept on this machine, found or not, and looks
    /// again for the lyrics of what plays. Answers what to say about it.
    fn clear_lyrics_cache(&mut self) -> String {
        // A connection of its own: the lyrics thread keeps the other, and
        // SQLite has them take turns.
        let Ok(cache) = Cache::open(&cache_path()) else {
            return chilpa_i18n::t("The lyrics cache could not be opened.");
        };
        let forgotten = cache.clear();
        drop(cache);
        if let Some((path, length_ms)) = self.lyrics.asked.clone() {
            self.load_lyrics(&path, length_ms);
        }
        chilpa_i18n::t_args("Cleared: {} lyrics forgotten. They are looked up again as tracks play.", &[&forgotten.to_string()])
    }

    /// The lyrics of what is playing, whole, as JSON. Empty when nothing
    /// that could have lyrics plays; `{"status":"searching"}` while they
    /// are looked for; `{"status":"none"}` when there are none; and found,
    /// `{"status":"found","source","document","translation"}`: who gave
    /// them, the document with its lines, words and times, and the position
    /// in its layers of the translation in the reader's language, or null
    /// when it carries none. Read it again
    /// when `lyrics_revision` changes.
    pub fn lyrics_json(&self) -> String {
        match (self.lyrics.state, &self.lyrics.document) {
            (State::Idle, _) => String::new(),
            (State::Searching, _) => serde_json::json!({ "status": "searching" }).to_string(),
            (State::Found, Some(document)) => serde_json::json!({
                "status": "found",
                "source": self.now.lyrics_source,
                "document": document,
                "translation": document.translation_for(&self.translation_language()),
            })
            .to_string(),
            (State::None | State::Found, _) => serde_json::json!({ "status": "none" }).to_string(),
        }
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
    fn lyrics(&self, reference: &str, track: &Track, language: Option<&str>, timeout: Duration) -> Answer {
        let policy = chilpa_core::network::current();
        if policy.offline || !policy.lyrics { return Answer::None; }
        match self.natives.call_raw(&self.id, "track.lyrics", wire::source_params(reference, language), timeout) {
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
