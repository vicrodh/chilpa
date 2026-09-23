//! Escanear sin congelar la interfaz.
//!
//! El escaneo abre su propia conexión en su propio hilo: una conexión de
//! SQLite no se comparte entre hilos, y la interfaz tiene que poder seguir
//! leyendo mientras se escribe. Para eso la base va en WAL (ver
//! [`crate::schema::apply_pragmas`]).
//!
//! Lo que se reporta es lo que de verdad pasó: archivos vistos, leídos y
//! saltados. Una barra que avanza sola sin saber cuántos archivos hay miente,
//! así que aquí se cuenta primero y se avanza después.

use crate::scan::ScanReport;
use crate::store::Library;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::thread::JoinHandle;

/// Cómo va el escaneo.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ScanProgress {
    /// Ya se sabe cuántos archivos hay que mirar.
    Counted { total: usize },
    /// Van `done` de `total`.
    Advanced { done: usize, total: usize },
    /// Terminó, con su reporte.
    Done(ScanReport),
    /// No se pudo.
    Failed(String),
}

/// Un escaneo corriendo en segundo plano.
pub struct ScanJob {
    events: Receiver<ScanProgress>,
    thread: Option<JoinHandle<()>>,
}

impl ScanJob {
    /// Arranca el escaneo de una carpeta sobre la base que está en `db_path`.
    pub fn start(db_path: PathBuf, root: PathBuf, artwork_cache: Option<PathBuf>) -> Self {
        Self::start_all(db_path, vec![root], artwork_cache)
    }

    /// Lo mismo con varias carpetas, una tras otra.
    ///
    /// La cuenta se hace primero sobre todas, para que la barra avance de
    /// principio a fin y no se reinicie en cada carpeta.
    pub fn start_all(
        db_path: PathBuf,
        roots: Vec<PathBuf>,
        artwork_cache: Option<PathBuf>,
    ) -> Self {
        let (tx, rx) = channel();
        let thread = std::thread::Builder::new()
            .name("chilpa-scan".into())
            .spawn(move || {
                let lib = match Library::open(&db_path) {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = tx.send(ScanProgress::Failed(e.to_string()));
                        return;
                    }
                };
                // Contar primero: sin total, una barra de progreso inventa.
                let total: usize = roots.iter().map(|r| crate::scan::count_audio_files(r)).sum();
                let _ = tx.send(ScanProgress::Counted { total });

                let mut acumulado = ScanReport {
                    root: roots.first().cloned().unwrap_or_default(),
                    unreachable: true,
                    ..ScanReport::default()
                };
                // Lo que ya llevan las carpetas anteriores, para que el
                // avance no vuelva a empezar en cada una.
                let mut previos = 0usize;
                for root in &roots {
                    let avisar = |done: usize| {
                        let _ = tx.send(ScanProgress::Advanced { done: previos + done, total });
                    };
                    match crate::scan::scan_root_reporting(
                        &lib,
                        root,
                        artwork_cache.as_deref(),
                        &avisar,
                    ) {
                        Ok(r) => {
                            previos += r.seen;
                            acumulado.seen += r.seen;
                            acumulado.read += r.read;
                            acumulado.unchanged += r.unchanged;
                            acumulado.missing += r.missing;
                            // Con que una carpeta se haya podido leer, el
                            // escaneo sirvió de algo.
                            acumulado.unreachable &= r.unreachable;
                        }
                        Err(e) => {
                            let _ = tx.send(ScanProgress::Failed(e.to_string()));
                            return;
                        }
                    }
                }
                if roots.is_empty() {
                    acumulado.unreachable = false;
                }
                let _ = tx.send(ScanProgress::Done(acumulado));
            })
            .expect("el hilo de escaneo arranca");
        Self { events: rx, thread: Some(thread) }
    }

    /// Lo que haya pasado, sin bloquear.
    pub fn poll(&self) -> Vec<ScanProgress> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(e) => out.push(e),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        out
    }

    /// Cierto si el hilo ya terminó.
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| t.is_finished())
    }
}

impl Drop for ScanJob {
    fn drop(&mut self) {
        // El escaneo no se interrumpe a la mitad: dejar la base con archivos
        // a medio registrar es peor que esperar.
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Cuenta los archivos de audio de una carpeta, sin tocar la base.
pub fn count(root: &Path) -> usize {
    crate::scan::count_audio_files(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Espera a que el escaneo termine y regresa todo lo que reportó.
    fn hasta_el_final(job: &ScanJob) -> Vec<ScanProgress> {
        let limite = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut eventos = Vec::new();
        while std::time::Instant::now() < limite {
            eventos.extend(job.poll());
            if eventos.iter().any(|e| matches!(e, ScanProgress::Done(_) | ScanProgress::Failed(_)))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        eventos
    }

    #[test]
    fn se_cuenta_antes_de_avanzar_y_el_total_no_cambia() {
        let dir = std::env::temp_dir().join("chilpa-scanjob-progreso");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("disco")).unwrap();
        for n in 0..4 {
            std::fs::write(dir.join("disco").join(format!("{n:02}.flac")), b"no es audio").unwrap();
        }
        std::fs::write(dir.join("notas.txt"), b"x").unwrap();

        let db = dir.join("lib.db");
        let job = ScanJob::start(db, dir.clone(), None);
        let eventos = hasta_el_final(&job);

        // El total llega primero: una barra sin total inventa su avance.
        match eventos.first() {
            Some(ScanProgress::Counted { total }) => {
                assert_eq!(*total, 4, "se cuentan los de audio, no el .txt")
            }
            otro => panic!("lo primero es la cuenta: {otro:?}"),
        }
        let mut visto = 0;
        for e in &eventos {
            if let ScanProgress::Advanced { done, total } = e {
                assert_eq!(*total, 4, "el total no se mueve a media cuenta");
                assert!(*done >= visto, "el avance no retrocede");
                visto = *done;
            }
        }
        match eventos.last() {
            Some(ScanProgress::Done(r)) => assert_eq!(r.seen, 4),
            otro => panic!("debía terminar: {otro:?}"),
        }
        drop(job);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn varias_carpetas_cuentan_juntas_y_el_avance_no_reinicia() {
        let dir = std::env::temp_dir().join("chilpa-scanjob-varias");
        let _ = std::fs::remove_dir_all(&dir);
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        for n in 0..3 {
            std::fs::write(a.join(format!("{n}.flac")), b"x").unwrap();
        }
        for n in 0..2 {
            std::fs::write(b.join(format!("{n}.flac")), b"x").unwrap();
        }

        let job = ScanJob::start_all(dir.join("lib.db"), vec![a, b], None);
        let eventos = hasta_el_final(&job);

        match eventos.first() {
            Some(ScanProgress::Counted { total }) => assert_eq!(*total, 5, "las dos juntas"),
            otro => panic!("lo primero es la cuenta: {otro:?}"),
        }
        let mut visto = 0;
        for e in &eventos {
            if let ScanProgress::Advanced { done, .. } = e {
                assert!(*done >= visto, "el avance no reinicia entre carpetas");
                visto = *done;
            }
        }
        match eventos.last() {
            Some(ScanProgress::Done(r)) => {
                assert_eq!(r.seen, 5, "se vieron las cinco");
                assert!(!r.unreachable);
            }
            otro => panic!("debía terminar: {otro:?}"),
        }
        drop(job);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn si_una_carpeta_falta_las_demas_igual_se_escanean() {
        let dir = std::env::temp_dir().join("chilpa-scanjob-falta");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("una.flac"), b"x").unwrap();

        let job = ScanJob::start_all(
            dir.join("lib.db"),
            vec![PathBuf::from("/no/existe"), dir.clone()],
            None,
        );
        let eventos = hasta_el_final(&job);
        match eventos.last() {
            Some(ScanProgress::Done(r)) => {
                assert_eq!(r.seen, 1);
                assert!(!r.unreachable, "una sí se pudo leer");
            }
            otro => panic!("debía terminar: {otro:?}"),
        }
        drop(job);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn un_root_que_no_existe_termina_con_su_reporte() {
        let db = std::env::temp_dir().join("chilpa-scanjob-test.db");
        let _ = std::fs::remove_file(&db);
        let job = ScanJob::start(db.clone(), PathBuf::from("/no/existe"), None);
        let limite = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut eventos = Vec::new();
        while std::time::Instant::now() < limite {
            eventos.extend(job.poll());
            if eventos.iter().any(|e| matches!(e, ScanProgress::Done(_) | ScanProgress::Failed(_)))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        match eventos.last() {
            Some(ScanProgress::Done(r)) => assert!(r.unreachable, "se marcó inalcanzable"),
            otro => panic!("debía terminar con un reporte: {otro:?}"),
        }
        drop(job);
        let _ = std::fs::remove_file(db);
    }
}
