//! Startup never waits for guest code or credential storage on the UI thread.
use crate::{Chilpa, plugins::Plugins};
use std::sync::{Arc, Mutex, mpsc};

#[derive(Default)]
pub(crate) struct Startup {
    attempted: bool,
    pending: Option<mpsc::Receiver<Option<Plugins>>>,
    /// How many plugins there are, how many are loaded, and which is loading.
    pub(crate) progress: Arc<Mutex<(usize, usize, String)>>,
}
impl Startup {
    fn start(&mut self, initialize: impl FnOnce() -> Option<Plugins> + Send + 'static) {
        if self.attempted { return; }
        self.attempted = true;
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let result = initialize();
            eprintln!("plugin-startup: success={} elapsed_ms={}", result.is_some(), started.elapsed().as_millis());
            let _ = tx.send(result);
        });
    }
    fn take(&mut self) -> Option<Option<Plugins>> {
        match self.pending.as_ref()?.try_recv() {
            Ok(result) => { self.pending = None; Some(result) }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => { self.pending = None; Some(None) }
        }
    }
}
impl Chilpa {
    pub fn start_plugins(&mut self) {
        if self.plugins.is_some() { return; }
        let root = Self::plugins_dir();
        let progress = self.plugin_startup.progress.clone();
        self.plugin_startup.start(move || {
            let mut plugins = Plugins::new(root)?;
            plugins.load_all_with(|done, total, name| {
                if let Ok(mut at) = progress.lock() { *at = (done, total, name.to_string()); }
            });
            Some(plugins)
        });
    }
    pub(crate) fn plugins_starting(&self) -> bool { self.plugin_startup.pending.is_some() }

    /// While plugins start: how many there are, how many are loaded and the
    /// name of the one loading, for the window that waits for them.
    pub(crate) fn startup_progress(&self) -> Option<(usize, usize, String)> {
        if !self.plugins_starting() { return None; }
        self.plugin_startup.progress.lock().ok().map(|at| at.clone())
    }
    pub(crate) fn advance_plugins(&mut self) {
        let Some(result) = self.plugin_startup.take() else { return };
        if let Some(plugins) = result {
            self.plugins = Some(Arc::new(Mutex::new(plugins)));
            self.install_openers();
        } else { self.now.error = "The plugin host could not start.".into(); }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_waiting_initializer_never_blocks_polling_or_starts_twice() {
        let (entered, enter) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let mut startup = Startup::default();
        startup.start(move || { entered.send(()).unwrap(); released.recv().unwrap(); None });
        enter.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
        assert!(startup.take().is_none());
        startup.start(|| panic!("initialization must not run twice"));
        assert!(startup.pending.is_some());
        release.send(()).unwrap();
        assert!(startup.pending.as_ref().unwrap().recv_timeout(std::time::Duration::from_secs(3)).unwrap().is_none());
        // Receiving the value above does not wait for the sender to be dropped.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if let Some(result) = startup.take() { assert!(result.is_none()); break; }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(startup.pending.is_none());
    }
}
