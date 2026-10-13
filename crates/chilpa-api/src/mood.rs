//! Optional plugin analysis, run after Chilpa's built-in analysis.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chilpa_library::{store::mood, Library};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{atomic::{AtomicBool, Ordering}, mpsc::{channel, Receiver, TryRecvError}, Arc};
use std::thread::JoinHandle;
use std::time::Duration;

const CALL: Duration = Duration::from_secs(20);
const INFERENCE: Duration = Duration::from_secs(120);
const DOWNLOAD: Duration = Duration::from_secs(180);
const PCM_SAMPLES_PER_CHUNK: usize = 524_288;

pub(crate) enum Progress {
    Skipped,
    Cancelled,
    Counted { total: usize },
    Advanced { done: usize, total: usize, title: String },
    Done { analyzed: usize, failed: usize, playlists: usize },
    Failed(String),
}

pub(crate) struct Job {
    events: Receiver<Progress>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Job {
    pub(crate) fn start(db_path: PathBuf, folder: Option<String>, natives: Arc<crate::natives::Natives>) -> Self {
        let (sender, events) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let thread = std::thread::Builder::new().name("chilpa-mood-analysis".into()).spawn(move || {
            let run = || -> Result<(), String> {
                let status = natives.call(mood::ANALYZER, "analysis.status", json!({}), CALL)?;
                if status["enabled"] != true {
                    let _ = sender.send(Progress::Skipped);
                    return Ok(());
                }
                if status["ready"] != true {
                    let ready = natives.call(mood::ANALYZER, "analysis.ensure", json!({}), DOWNLOAD)?;
                    if ready["ready"] != true { return Err("The analyzer models are unavailable.".into()); }
                }
                let version = status["version"].as_i64().filter(|value| *value > 0)
                    .ok_or("The analyzer did not report a model version.")?;
                let library = Library::open(&db_path).map_err(|error| error.to_string())?;
                let tracks = library.tracks_pending_mood_under(version, folder.as_deref())
                    .map_err(|error| error.to_string())?;
                let total = tracks.len();
                let _ = sender.send(Progress::Counted { total });
                let (mut analyzed, mut failed) = (0, 0);
                for (index, track) in tracks.into_iter().enumerate() {
                    if flag.load(Ordering::Relaxed) {
                        let _ = sender.send(Progress::Cancelled);
                        return Ok(());
                    }
                    if let Some((pcm, rate)) = chilpa_core::analysis_job::mono_excerpt(Path::new(&track.external_id)) {
                        let features = send_audio(&natives, &pcm, rate)?;
                        let scores = parse_scores(&features)?;
                        let embedding = parse_embedding(&features)?;
                        library.save_mood_features(track.id, version, scores, &embedding)
                            .map_err(|error| error.to_string())?;
                        analyzed += 1;
                    } else {
                        failed += 1;
                    }
                    let _ = sender.send(Progress::Advanced { done: index + 1, total, title: track.title });
                }
                if flag.load(Ordering::Relaxed) {
                    let _ = sender.send(Progress::Cancelled);
                    return Ok(());
                }
                let playlists = library.rebuild_mood_playlists(version).map_err(|error| error.to_string())?;
                let _ = sender.send(Progress::Done { analyzed, failed, playlists });
                Ok(())
            };
            if let Err(error) = run() { let _ = sender.send(Progress::Failed(error)); }
        }).expect("the mood analysis thread starts");
        Self { events, cancel, thread: Some(thread) }
    }

    pub(crate) fn poll(&self) -> Vec<Progress> {
        let mut output = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(event) => output.push(event),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        output
    }

    pub(crate) fn cancel(&self) { self.cancel.store(true, Ordering::Relaxed); }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}

fn send_audio(natives: &Arc<crate::natives::Natives>, pcm: &[f32], rate: u32) -> Result<Value, String> {
    let answer = natives.call(mood::ANALYZER, "analysis.begin", json!({"sample_rate_hz":rate}), CALL)?;
    let stream = answer["stream"].as_str().ok_or("The analyzer did not open the audio stream.")?;
    let result = (|| {
        for samples in pcm.chunks(PCM_SAMPLES_PER_CHUNK) {
            let mut bytes = Vec::with_capacity(samples.len() * 2);
            for sample in samples {
                let integer = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
                bytes.extend_from_slice(&integer.to_le_bytes());
            }
            natives.call(mood::ANALYZER, "analysis.append", json!({
                "stream":stream, "pcm_i16_le":STANDARD.encode(bytes)
            }), CALL)?;
        }
        natives.call(mood::ANALYZER, "analysis.finish", json!({"stream":stream}), INFERENCE)
    })();
    if result.is_err() {
        let _ = natives.call(mood::ANALYZER, "analysis.abort", json!({"stream":stream}), CALL);
    }
    result
}

fn parse_scores(value: &Value) -> Result<[f32; 6], String> {
    let keys = ["mood_happy", "mood_sad", "mood_relaxed", "mood_aggressive", "mood_party", "danceability"];
    let mut scores = [0.0; 6];
    for (index, key) in keys.iter().enumerate() {
        scores[index] = value["scores"][key].as_f64().filter(|number| number.is_finite() && (0.0..=1.0).contains(number))
            .ok_or("The analyzer returned an invalid mood score.")? as f32;
    }
    Ok(scores)
}

fn parse_embedding(value: &Value) -> Result<Vec<f32>, String> {
    let values = value["embedding"].as_array().filter(|values| values.len() == mood::VECTOR_LENGTH)
        .ok_or("The analyzer returned an invalid similarity vector.")?;
    values.iter().map(|value| value.as_f64().filter(|number| number.is_finite())
        .map(|number| number as f32).ok_or_else(|| "The analyzer returned an invalid similarity vector.".into())).collect()
}
