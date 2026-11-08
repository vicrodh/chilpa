//! The compatibility worker inside the Flatpak sandbox. Perl and its modules
//! come with the application under /app, which the worker's sandbox sees
//! read-only; the package is handed over read-only at its own path, and so
//! are the adapter and its modules when they are not under /app. The worker
//! gets no network and no writable folder: what it reaches outside goes
//! through the host's WorkerServices, as under bubblewrap. Its limits come
//! from chilpa-plugin-runner.
//!
//! Known limit: the 16 MiB bound bubblewrap puts on its /tmp has no portal
//! option and is not there. No forks, 1 MiB per file and 120 s of CPU bound
//! each file and the time it has to write, not the sum: a worker that keeps
//! writing and closing files can fill the sandbox's /tmp until its CPU runs
//! out. Lower limits would not change that order of magnitude and would cut
//! its real work first, so they stay as bubblewrap's (packaging/flatpak/
//! BUILD-NOTES.md, section 10).

use crate::flatpak::{keep_env, Exposed, Flatpak};
use crate::portal::Spawn;
use crate::process::{NativeLauncher, NativeProcess};
use crate::Unavailable;
use chilpa_plugin::bridge::{BridgeError, WorkerLauncher, WorkerTransport};
use chilpa_plugin::pipe_transport::PipeTransport;
use std::ffi::OsStr;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Where the Flatpak installs Perl, built against the runtime's glibc.
pub(crate) const PERL: &str = "/app/bin/perl";
/// Address space the worker may reserve, as under bubblewrap.
const MEMORY: usize = 256 * 1024 * 1024;

pub struct FlatpakWorker {
    flatpak: Arc<Flatpak>,
    perl: PathBuf,
    adapter: PathBuf,
    libraries: PathBuf,
}

impl FlatpakWorker {
    /// The worker this application installed, or why it has none.
    pub(crate) fn installed(root: &Path) -> Result<Self, Unavailable> {
        Self::with(crate::flatpak::shared(), PERL.into(), &crate::linux::compat_runtime(root))
    }

    /// `runtime` holds `adapter/worker.pl` and `libraries/lib/perl5`.
    pub(crate) fn with(flatpak: Arc<Flatpak>, perl: PathBuf, runtime: &Path) -> Result<Self, Unavailable> {
        let (adapter, libraries) = (runtime.join("adapter"), runtime.join("libraries"));
        if !perl.is_file() { return Err(Unavailable::MissingProgram(perl)); }
        if !adapter.join("worker.pl").is_file() { return Err(Unavailable::MissingProgram(adapter.join("worker.pl"))); }
        if !libraries.join("lib/perl5").is_dir() { return Err(Unavailable::MissingProgram(libraries.join("lib/perl5"))); }
        flatpak.availability()?;
        Ok(Self { flatpak, perl, adapter, libraries })
    }
}

/// What lives under /app is already in the worker's sandbox, read-only.
fn visible(path: &Path) -> bool {
    path.starts_with("/app")
}

impl WorkerLauncher for FlatpakWorker {
    /// The package appears at its own path.
    fn guest_root(&self, guest: &Path) -> String {
        guest.canonicalize().map(|path| path.display().to_string()).unwrap_or_default()
    }

    fn inspection_revision(&self) -> Option<String> {
        crate::linux::runtime_revision(&self.adapter, &self.libraries)
    }

    fn launch(&self, guest: &Path) -> Result<Box<dyn WorkerTransport>, BridgeError> {
        // A privileged caller could evade the process-count limit.
        if unsafe { libc::geteuid() } == 0 { return Err(BridgeError::Denied); }
        let portal = self.flatpak.portal().ok_or(BridgeError::Disconnected)?;
        let guest = Exposed::open(guest)?;
        let mut handed = Vec::new();
        let mut seen = |folder: &Path| -> Result<PathBuf, BridgeError> {
            if visible(folder) { return Ok(folder.to_path_buf()); }
            let exposed = Exposed::open(folder)?;
            let path = exposed.path.clone();
            handed.push(exposed);
            Ok(path)
        };
        let worker = seen(&self.adapter)?.join("worker.pl");
        let modules = seen(&self.libraries)?.join("lib/perl5");
        let memory = format!("--as={MEMORY}");
        let modules = modules.to_str().ok_or(BridgeError::Denied)?.to_string();
        let env = vec![("PATH", "/usr/bin".to_string()), ("PERL5LIB", modules)];
        let keep = keep_env(&env);
        let argv = vec![self.flatpak.runner.as_os_str(), OsStr::new(&memory), OsStr::new("--nproc=0"), OsStr::new("--nofile=64"),
            OsStr::new("--fsize=1048576"), OsStr::new("--cpu=120:120"), OsStr::new("--core=0"), OsStr::new(&keep), OsStr::new("--"),
            self.perl.as_os_str(), worker.as_os_str()];
        let (child_input, input) = std::io::pipe().map_err(|_| BridgeError::Disconnected)?;
        let (output, child_output) = std::io::pipe().map_err(|_| BridgeError::Disconnected)?;
        let errors = std::fs::OpenOptions::new().write(true).open("/dev/null").map_err(|_| BridgeError::Disconnected)?;
        let mut readable = vec![guest.fd.as_fd()];
        readable.extend(handed.iter().map(|exposed| exposed.fd.as_fd()));
        let request = Spawn {
            cwd: &guest.path,
            argv,
            env,
            stdio: [child_input.as_fd(), child_output.as_fd(), errors.as_fd()],
            descriptors: Vec::new(),
            network: false,
            writable: Vec::new(),
            readable,
        };
        let process = portal.spawn(&request).map_err(|error| {
            eprintln!("flatpak: the portal did not start the compatibility worker: {error}");
            BridgeError::Disconnected
        })?;
        drop(request);
        drop((child_input, child_output, errors));
        Ok(Box::new(PipeTransport::isolated(input, output, move || { process.terminate(); process.wait(); })))
    }
}

#[cfg(test)]
#[path = "flatpak_worker_tests.rs"]
mod tests;
