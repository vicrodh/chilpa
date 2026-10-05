//! I serialize progress writes away from the playback/UI thread.
use super::Library;
use rusqlite::{Connection, ErrorCode, OpenFlags};
use std::{collections::BTreeMap, sync::mpsc, time::Duration};

type Update = (String, String, u64, bool);

pub struct ProgressWriter(mpsc::Sender<Update>);

impl ProgressWriter {
    pub fn save(&self, source: &str, entry: &str, position: u64, played: bool) -> bool {
        self.0.send((source.into(), entry.into(), position, played)).is_ok()
    }
}

impl Library {
    /// I reuse the current database without migrations or library maintenance.
    /// In-memory libraries have no second connection and return no worker.
    pub fn progress_writer(&self) -> std::io::Result<Option<ProgressWriter>> {
        let Some(path) = self.db.path().filter(|p| !p.is_empty()).map(str::to_owned) else { return Ok(None) };
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new().name("chilpa-progress".into()).spawn(move || {
            let opened = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE).and_then(|db| {
                db.busy_timeout(Duration::from_millis(100))?;
                Ok(Library { db, versions: Default::default(), related: Default::default() })
            });
            let library = match opened {
                Ok(library) => library,
                Err(error) => { eprintln!("source-progress: open failed: {error}"); return; }
            };
            write(&library, rx);
        })?;
        Ok(Some(ProgressWriter(tx)))
    }
}

fn write(library: &Library, rx: mpsc::Receiver<Update>) {
    let mut pending = BTreeMap::new();
    let mut closed = false;
    loop {
        if pending.is_empty() && closed { return; }
        let next = if pending.is_empty() {
            rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        } else {
            rx.recv_timeout(Duration::from_millis(50))
        };
        match next {
            Ok((source, entry, position, played)) => { pending.insert((source, entry), (position, played)); }
            Err(mpsc::RecvTimeoutError::Disconnected) => closed = true,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        for (source, entry, position, played) in rx.try_iter() {
            pending.insert((source, entry), (position, played));
        }
        pending.retain(|(source, entry), (position, played)| {
            // Catalog tracks without an episode card need no resume write.
            let result = library.heard(source, entry).and_then(|card| {
                if card.is_some() { library.set_heard(source, entry, *position, *played) } else { Ok(()) }
            });
            match result {
                Ok(()) => false,
                Err(error) if matches!(error.sqlite_error_code(), Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)) => true,
                Err(error) => { eprintln!("source-progress: save failed: {error}"); false }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::progress::Heard;
    #[test]
    fn queued_final_positions_survive_contention_and_sender_drop() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.db");
        let library = Library::open(&path).unwrap();
        for id in ["one", "two"] {
            library.remember_heard("source", &Heard { external_id: id.into(), ..Default::default() }).unwrap();
        }
        let blocker = Library::open(&path).unwrap();
        blocker.connection().execute_batch("BEGIN IMMEDIATE").unwrap();
        let writer = library.progress_writer().unwrap().unwrap();
        assert!(writer.save("source", "one", 1000, false));
        assert!(writer.save("source", "one", 2000, false));
        assert!(writer.save("source", "two", 0, true));
        drop(writer);
        std::thread::sleep(Duration::from_millis(200));
        blocker.connection().execute_batch("ROLLBACK").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            let one = library.heard("source", "one").unwrap().unwrap();
            let two = library.heard("source", "two").unwrap().unwrap();
            if one.position_ms == 2000 && two.played { break; }
            assert!(std::time::Instant::now() < deadline, "the last positions must be saved after contention");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
