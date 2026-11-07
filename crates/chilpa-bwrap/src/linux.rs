//! What every Linux backend shares, whichever sandbox it builds.

use chilpa_plugin::bridge::BridgeError;
use std::{path::{Path, PathBuf}, process::Command};

/// True inside a Flatpak sandbox, which marks itself with this file.
pub(crate) fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// Starts a worker from the one thread that lives as long as Chilpa.
///
/// `--die-with-parent` is a parent-death signal, and Linux sends it when the
/// thread that started the child ends, not the process. Workers started at
/// startup, on a thread that then finished, were killed with it and every
/// first call found them gone.
pub(crate) fn spawn(command: Command) -> std::io::Result<std::process::Child> {
    type Ask = (Command, std::sync::mpsc::Sender<std::io::Result<std::process::Child>>);
    static LAUNCHER: std::sync::OnceLock<Option<std::sync::Mutex<std::sync::mpsc::Sender<Ask>>>> = std::sync::OnceLock::new();
    let launcher = LAUNCHER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Ask>();
        std::thread::Builder::new().name("chilpa-worker-launcher".into())
            .spawn(move || for (mut command, reply) in rx { let _ = reply.send(command.spawn()); })
            .ok().map(|_| std::sync::Mutex::new(tx))
    });
    let gone = || std::io::Error::other("the worker launcher is not running");
    let (reply, answer) = std::sync::mpsc::channel();
    launcher.as_ref().ok_or_else(gone)?.lock().unwrap_or_else(|e| e.into_inner()).send((command, reply)).map_err(|_| gone())?;
    answer.recv().map_err(|_| gone())?
}

pub(crate) fn canonical_directory(path: &Path) -> Result<PathBuf, BridgeError> {
    let path = path.canonicalize().map_err(|_| BridgeError::Disconnected)?;
    if !path.is_dir() { return Err(BridgeError::Disconnected); }
    Ok(path)
}
