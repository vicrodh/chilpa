//! One speculative remote decoder per queue, without opening an audio output.
use crate::remote;
use chilpa_audio::sink::StopSignal;
use std::{path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc}, time::Duration};

#[derive(Default)]
pub(crate) struct Preload {
    pending: Option<(PathBuf, mpsc::Receiver<Result<remote::Opened, String>>)>,
    busy: Arc<AtomicBool>,
}

impl Preload {
    pub fn request(&mut self, path: PathBuf) {
        if self.pending.as_ref().is_some_and(|(current, _)| *current == path) { return; }
        let Some(entry) = remote::entry(&path) else { return; };
        if self.busy.swap(true, Ordering::AcqRel) { return; }
        let (tx, rx) = mpsc::channel();
        let busy = self.busy.clone();
        let next = path.clone();
        if std::thread::Builder::new().name("chilpa-remote-preload".into()).spawn(move || {
            let opened = remote::open(&next, &entry);
            busy.store(false, Ordering::Release);
            let _ = tx.send(opened);
        }).is_ok() {
            self.pending = Some((path, rx));
        } else { self.busy.store(false, Ordering::Release); }
    }

    pub fn take(&mut self, path: &Path, stop: &StopSignal) -> Option<Result<remote::Opened, String>> {
        let (expected, rx) = self.pending.take()?;
        if expected != path { return None; }
        while !stop.is_stopped() {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(opened) => return Some(opened),
                Err(mpsc::RecvTimeoutError::Timeout) => {},
                Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            }
        }
        None
    }
}
