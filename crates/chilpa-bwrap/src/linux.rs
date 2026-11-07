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

/// Where the application installed the compatibility runtime: beside its
/// executable, or in the plugins folder's `.runtime`.
pub(crate) fn compat_runtime(root: &Path) -> PathBuf {
    std::env::current_exe().ok().and_then(|exe| exe.parent().map(|folder| folder.join("compat-runtime")))
        .filter(|adjacent| adjacent.join("adapter/worker.pl").is_file()).unwrap_or_else(|| root.to_path_buf())
}

/// A hash of the adapter and its modules: an inspection made with others is
/// not reused.
pub(crate) fn runtime_revision(adapter: &Path, libraries: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    fn hash_tree(path: &Path, hash: &mut Sha256) -> std::io::Result<()> {
        let mut entries = std::fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            hash.update(entry.file_name().as_encoded_bytes());
            if entry.file_type()?.is_dir() { hash_tree(&entry.path(), hash)?; }
            else { hash.update(std::fs::read(entry.path())?); }
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    hash_tree(adapter, &mut hash).ok()?;
    hash_tree(libraries, &mut hash).ok()?;
    Some(format!("{:x}", hash.finalize()))
}

/// The compatibility worker this system starts, or why there is none:
/// inside Flatpak through the portal, elsewhere with the host's bubblewrap.
/// It never starts one outside a sandbox.
pub fn compatibility(root: &Path) -> Result<std::sync::Arc<dyn chilpa_plugin::bridge::WorkerLauncher>, crate::Unavailable> {
    use crate::Unavailable;
    if in_flatpak() {
        return crate::FlatpakWorker::installed(root).map(|worker| std::sync::Arc::new(worker) as std::sync::Arc<dyn chilpa_plugin::bridge::WorkerLauncher>);
    }
    if unsafe { libc::geteuid() } == 0 { return Err(Unavailable::Privileged); }
    crate::RuntimePaths::installed(root).map(|paths| std::sync::Arc::new(paths) as std::sync::Arc<dyn chilpa_plugin::bridge::WorkerLauncher>)
        .ok_or_else(|| {
            let runtime = compat_runtime(root);
            let missing = [PathBuf::from("/usr/bin/bwrap"), runtime.join("adapter/worker.pl"), runtime.join("libraries/lib/perl5")]
                .into_iter().find(|path| !path.exists()).unwrap_or_else(|| runtime.join("adapter/worker.pl"));
            Unavailable::MissingProgram(missing)
        })
}

pub(crate) fn canonical_directory(path: &Path) -> Result<PathBuf, BridgeError> {
    let path = path.canonicalize().map_err(|_| BridgeError::Disconnected)?;
    if !path.is_dir() { return Err(BridgeError::Disconnected); }
    Ok(path)
}
