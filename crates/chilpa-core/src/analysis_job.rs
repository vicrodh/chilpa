//! Analizar la biblioteca sin estorbarle a nadie.
//!
//! El análisis lee archivos y escribe números. No toca la ruta de salida, no
//! comparte la conexión de la interfaz y corre en su propio hilo, como el
//! escaneo.
//!
//! Dos cuidados que lo hacen barato:
//!
//! 1. **No se lee la pista entera.** Se salta al cuarto de la pista y se
//!    toma un minuto. Una intro larga o un final que se apaga no dicen nada
//!    del pulso, y leer sesenta minutos de disco por cada disco convertiría
//!    esto en algo que nadie deja terminar.
//! 2. **Se pide por tandas.** La lista de pendientes sale de la base de a
//!    poco; una biblioteca de cien mil pistas no entra en memoria y no tiene
//!    por qué hacerlo.

use chilpa_library::Library;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Cómo se llama este analizador en la base. Es el de `chilpa-analysis`,
/// repetido aquí para que la fachada no tenga que conocer ese crate.
pub const ANALYZER: &str = chilpa_analysis::NAME;

/// Cuánto audio se mira de cada pista.
const SECONDS_EXAMINED: u64 = 60;

/// Cuántas pistas se piden por tanda.
const BATCH: usize = 200;

/// Cómo va el análisis.
#[derive(Clone, PartialEq, Debug)]
pub enum AnalysisProgress {
    /// Ya se sabe cuántas faltan.
    Counted { total: usize },
    /// Van `done` de `total`, y esta fue la última que se miró.
    Advanced {
        done: usize,
        total: usize,
        title: String,
        bpm: Option<f32>,
        key: Option<String>,
    },
    Done { analyzed: usize, without_pulse: usize, failed: usize },
    Failed(String),
}

/// Un análisis corriendo en segundo plano.
pub struct AnalysisJob {
    events: Receiver<AnalysisProgress>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AnalysisJob {
    /// Arranca el análisis de lo que falte en la base que está en `db_path`.
    pub fn start(db_path: PathBuf) -> Self {
        let (tx, rx) = channel();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let thread = std::thread::Builder::new()
            .name("chilpa-analisis".into())
            .spawn(move || {
                let lib = match Library::open(&db_path) {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = tx.send(AnalysisProgress::Failed(e.to_string()));
                        return;
                    }
                };
                let analyzer = chilpa_analysis::NAME;
                let version = chilpa_analysis::VERSION;

                let total = match lib.tracks_pending_analysis(analyzer, version, usize::MAX) {
                    Ok(v) => v.len(),
                    Err(e) => {
                        let _ = tx.send(AnalysisProgress::Failed(e.to_string()));
                        return;
                    }
                };
                let _ = tx.send(AnalysisProgress::Counted { total });

                let (mut made, mut no_pulse, mut failed) = (0usize, 0usize, 0usize);
                loop {
                    if flag.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    let batch = match lib.tracks_pending_analysis(analyzer, version, BATCH) {
                        Ok(v) if v.is_empty() => break,
                        Ok(v) => v,
                        Err(e) => {
                            let _ = tx.send(AnalysisProgress::Failed(e.to_string()));
                            return;
                        }
                    };
                    for track in batch {
                        if flag.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                        let features = analyze_file(Path::new(&track.external_id));
                        // Aunque no se le haya encontrado pulso se guarda la
                        // fila: si no, la pista saldría pendiente para
                        // siempre y el análisis nunca terminaría.
                        let saved = lib.save_features(
                            track.id,
                            analyzer,
                            version,
                            features.as_ref().and_then(|f| f.bpm),
                            features.as_ref().and_then(|f| f.tempo_confidence),
                            features
                                .as_ref()
                                .and_then(|f| f.key.as_ref())
                                .map(|k| format!("{} · {}", k.name, k.camelot)),
                            features.as_ref().and_then(|f| f.loudness_lufs),
                        );
                        match (&features, &saved) {
                            (_, Err(e)) => {
                                let _ = tx.send(AnalysisProgress::Failed(e.to_string()));
                                return;
                            }
                            (None, _) => failed += 1,
                            (Some(f), _) if f.bpm.is_none() => no_pulse += 1,
                            _ => {}
                        }
                        made += 1;
                        let _ = tx.send(AnalysisProgress::Advanced {
                            done: made,
                            total,
                            title: track.title.clone(),
                            bpm: features.as_ref().and_then(|f| f.bpm),
                            key: features
                                .as_ref()
                                .and_then(|f| f.key.as_ref())
                                .map(|k| k.camelot.clone()),
                        });
                    }
                }
                let _ = tx.send(AnalysisProgress::Done {
                    analyzed: made,
                    without_pulse: no_pulse,
                    failed: failed,
                });
            })
            .expect("el hilo de análisis arranca");
        Self { events: rx, cancel, thread: Some(thread) }
    }

    /// Lo que haya pasado, sin bloquear.
    pub fn poll(&self) -> Vec<AnalysisProgress> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(e) => out.push(e),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        out
    }

    /// Pide que se detenga. La pista en curso se termina.
    pub fn cancel(&self) {
        self.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| t.is_finished())
    }
}

impl Drop for AnalysisJob {
    fn drop(&mut self) {
        self.cancel();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Decodifica un pedazo de la pista y lo analiza.
///
/// `None` cuando el archivo no se pudo abrir o leer. Que no se le encuentre
/// pulso no es una falla: es una respuesta.
pub fn analyze_file(path: &Path) -> Option<chilpa_analysis::Analysis> {
    let mut decoded = chilpa_decode::Decoded::open(path).ok()?;
    let format = decoded.stream_format();
    let channels = format.channels.max(1) as usize;
    let rate = format.sample_rate_hz;
    if rate == 0 {
        return None;
    }

    // Se empieza en el cuarto de la pista: las intros no dicen nada del
    // pulso, y el principio de un disco suele ser lo menos representativo.
    if let Some(total) = decoded.frame_count() {
        let startup = total / 4;
        if startup > u64::from(rate) {
            let _ = decoded.seek_to_frame(startup);
        }
    }

    let want = SECONDS_EXAMINED * u64::from(rate);
    let mut mono: Vec<f32> = Vec::with_capacity(want as usize / 4);
    // La sonoridad se mide al vuelo, con la señal como viene: mezclarla a
    // mono o bajarle la frecuencia cambiaría el número.
    let mut meter = chilpa_analysis::LoudnessMeter::new(rate, format.channels.max(1));
    let mut leidos = 0u64;
    let mut interleaved: Vec<f32> = Vec::new();
    while leidos < want {
        let block = match decoded.next_block() {
            Ok(Some(b)) => b,
            Ok(None) | Err(_) => break,
        };
        interleaved.clear();
        match block {
            chilpa_decode::Block::Int { data, bits } => {
                let scale = 1.0f32 / (1i64 << (bits.max(1) - 1)) as f32;
                interleaved.extend(data.iter().map(|m| *m as f32 * scale));
            }
            chilpa_decode::Block::Float(data) => interleaved.extend(data),
        }
        if interleaved.is_empty() {
            continue;
        }
        if let Some(m) = meter.as_mut() {
            m.push(&interleaved);
        }
        for frame in interleaved.chunks_exact(channels) {
            mono.push(frame.iter().sum::<f32>() / channels as f32);
        }
        leidos += (interleaved.len() / channels) as u64;
    }
    if mono.is_empty() {
        return None;
    }
    let mut features = chilpa_analysis::analyze(&mono, rate);
    features.loudness_lufs = meter.and_then(|m| m.integrated()).map(|l| l as f32);
    Some(features)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(job: &AnalysisJob) -> Vec<AnalysisProgress> {
        let limit = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut events = Vec::new();
        while std::time::Instant::now() < limit {
            events.extend(job.poll());
            if events
                .iter()
                .any(|e| matches!(e, AnalysisProgress::Done { .. } | AnalysisProgress::Failed(_)))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        events
    }

    #[test]
    fn with_nothing_to_analyze_it_finishes_at_once() {
        let dir = std::env::temp_dir().join("chilpa-analisis-vacio");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let job = AnalysisJob::start(dir.join("lib.db"));
        let events = wait(&job);
        match events.first() {
            Some(AnalysisProgress::Counted { total }) => assert_eq!(*total, 0),
            other => panic!("lo primero es la cuenta: {other:?}"),
        }
        assert!(matches!(
            events.last(),
            Some(AnalysisProgress::Done { analyzed: 0, .. })
        ));
        drop(job);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_that_cannot_be_opened_does_not_bring_the_analysis_down() {
        assert!(analyze_file(Path::new("/no/existe.flac")).is_none());
    }
}
