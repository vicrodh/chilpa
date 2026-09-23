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
pub const ANALIZADOR: &str = chilpa_analysis::NOMBRE;

/// Cuánto audio se mira de cada pista.
const SEGUNDOS_QUE_SE_MIRAN: u64 = 60;

/// Cuántas pistas se piden por tanda.
const TANDA: usize = 200;

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
    cancelar: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AnalysisJob {
    /// Arranca el análisis de lo que falte en la base que está en `db_path`.
    pub fn start(db_path: PathBuf) -> Self {
        let (tx, rx) = channel();
        let cancelar = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let bandera = Arc::clone(&cancelar);
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
                let analizador = chilpa_analysis::NOMBRE;
                let version = chilpa_analysis::VERSION;

                let total = match lib.tracks_pending_analysis(analizador, version, usize::MAX) {
                    Ok(v) => v.len(),
                    Err(e) => {
                        let _ = tx.send(AnalysisProgress::Failed(e.to_string()));
                        return;
                    }
                };
                let _ = tx.send(AnalysisProgress::Counted { total });

                let (mut hechas, mut sin_pulso, mut fallidas) = (0usize, 0usize, 0usize);
                loop {
                    if bandera.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    let tanda = match lib.tracks_pending_analysis(analizador, version, TANDA) {
                        Ok(v) if v.is_empty() => break,
                        Ok(v) => v,
                        Err(e) => {
                            let _ = tx.send(AnalysisProgress::Failed(e.to_string()));
                            return;
                        }
                    };
                    for pista in tanda {
                        if bandera.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                        let features = analizar_archivo(Path::new(&pista.external_id));
                        // Aunque no se le haya encontrado pulso se guarda la
                        // fila: si no, la pista saldría pendiente para
                        // siempre y el análisis nunca terminaría.
                        let guardado = lib.save_features(
                            pista.id,
                            analizador,
                            version,
                            features.as_ref().and_then(|f| f.bpm),
                            features.as_ref().and_then(|f| f.tempo_confidence),
                            features
                                .as_ref()
                                .and_then(|f| f.key.as_ref())
                                .map(|k| format!("{} · {}", k.name, k.camelot)),
                        );
                        match (&features, &guardado) {
                            (_, Err(e)) => {
                                let _ = tx.send(AnalysisProgress::Failed(e.to_string()));
                                return;
                            }
                            (None, _) => fallidas += 1,
                            (Some(f), _) if f.bpm.is_none() => sin_pulso += 1,
                            _ => {}
                        }
                        hechas += 1;
                        let _ = tx.send(AnalysisProgress::Advanced {
                            done: hechas,
                            total,
                            title: pista.title.clone(),
                            bpm: features.as_ref().and_then(|f| f.bpm),
                            key: features
                                .as_ref()
                                .and_then(|f| f.key.as_ref())
                                .map(|k| k.camelot.clone()),
                        });
                    }
                }
                let _ = tx.send(AnalysisProgress::Done {
                    analyzed: hechas,
                    without_pulse: sin_pulso,
                    failed: fallidas,
                });
            })
            .expect("el hilo de análisis arranca");
        Self { events: rx, cancelar, thread: Some(thread) }
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
        self.cancelar.store(true, std::sync::atomic::Ordering::Relaxed);
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
pub fn analizar_archivo(path: &Path) -> Option<chilpa_analysis::Analysis> {
    let mut decoded = chilpa_decode::Decoded::open(path).ok()?;
    let formato = decoded.stream_format();
    let canales = formato.channels.max(1) as usize;
    let rate = formato.sample_rate_hz;
    if rate == 0 {
        return None;
    }

    // Se empieza en el cuarto de la pista: las intros no dicen nada del
    // pulso, y el principio de un disco suele ser lo menos representativo.
    if let Some(total) = decoded.frame_count() {
        let arranque = total / 4;
        if arranque > u64::from(rate) {
            let _ = decoded.seek_to_frame(arranque);
        }
    }

    let quiero = SEGUNDOS_QUE_SE_MIRAN * u64::from(rate);
    let mut mono: Vec<f32> = Vec::with_capacity(quiero as usize / 4);
    let mut leidos = 0u64;
    while leidos < quiero {
        match decoded.next_block() {
            Ok(Some(chilpa_decode::Block::Int { data, bits })) => {
                let escala = 1.0f32 / (1i64 << (bits.max(1) - 1)) as f32;
                for frame in data.chunks_exact(canales) {
                    let suma: f32 = frame.iter().map(|m| *m as f32 * escala).sum();
                    mono.push(suma / canales as f32);
                }
                leidos += (data.len() / canales) as u64;
            }
            Ok(Some(chilpa_decode::Block::Float(data))) => {
                for frame in data.chunks_exact(canales) {
                    mono.push(frame.iter().sum::<f32>() / canales as f32);
                }
                leidos += (data.len() / canales) as u64;
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
    if mono.is_empty() {
        return None;
    }
    Some(chilpa_analysis::analyze(&mono, rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn espera(job: &AnalysisJob) -> Vec<AnalysisProgress> {
        let limite = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut eventos = Vec::new();
        while std::time::Instant::now() < limite {
            eventos.extend(job.poll());
            if eventos
                .iter()
                .any(|e| matches!(e, AnalysisProgress::Done { .. } | AnalysisProgress::Failed(_)))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        eventos
    }

    #[test]
    fn sin_nada_que_analizar_termina_de_inmediato() {
        let dir = std::env::temp_dir().join("chilpa-analisis-vacio");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let job = AnalysisJob::start(dir.join("lib.db"));
        let eventos = espera(&job);
        match eventos.first() {
            Some(AnalysisProgress::Counted { total }) => assert_eq!(*total, 0),
            otro => panic!("lo primero es la cuenta: {otro:?}"),
        }
        assert!(matches!(
            eventos.last(),
            Some(AnalysisProgress::Done { analyzed: 0, .. })
        ));
        drop(job);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn un_archivo_que_no_se_puede_abrir_no_tumba_el_analisis() {
        assert!(analizar_archivo(Path::new("/no/existe.flac")).is_none());
    }
}
