//! Process plugins inside the Flatpak sandbox: the portal starts each in a
//! sandbox of its own, and `chilpa-plugin-runner` sets its resource limits
//! before it becomes the plugin.
//!
//! Its folders are handed over as `O_PATH` descriptors and appear at the
//! paths Chilpa sees them at: the package read-only; its data, cache and an
//! approved export folder writable. Those paths, read from the descriptors
//! themselves, are what `initialize` and the environment name. It also sees
//! the runtime's `/usr` and the application's `/app`, read-only. Given the
//! network, it reaches any address; without it, none.

use crate::portal::{Bus, Portal, Spawn};
use crate::process::{log_errors, LaunchedNative, NativeLauncher, NativeProcess, Notice, CACHE_DIR_VARIABLE, DATA_DIR_VARIABLE};
use crate::{Incompatible, NativePaths, Unavailable};
use chilpa_plugin::{bridge::BridgeError, native::NativeClient};
use std::ffi::{CString, OsStr};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Where the Flatpak installs the runner, visible from the plugin's sandbox.
const RUNNER: &str = "/app/libexec/chilpa-plugin-runner";

/// A folder held open, and the path it has for Chilpa and for the plugin.
pub(crate) struct Exposed {
    pub(crate) fd: OwnedFd,
    pub(crate) path: PathBuf,
}

impl Exposed {
    /// Opens a real directory, never a link to one. Its path comes from the
    /// descriptor, so what the plugin is told is what was opened.
    pub(crate) fn open(path: &Path) -> Result<Self, BridgeError> {
        let name = CString::new(path.as_os_str().as_bytes()).map_err(|_| BridgeError::Denied)?;
        let fd = unsafe { libc::open(name.as_ptr(), libc::O_PATH | libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC) };
        if fd < 0 { return Err(BridgeError::Disconnected); }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let path = std::fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd()))
            .map_err(|_| BridgeError::Disconnected)?;
        if !path.is_absolute() { return Err(BridgeError::Disconnected); }
        Ok(Self { fd, path })
    }
}

fn text(path: &Path) -> Result<String, BridgeError> {
    path.to_str().map(str::to_owned).ok_or(BridgeError::Denied)
}

/// What the plugin starts with, the environment being otherwise empty.
fn environment(data: &Path, cache: &Path) -> Result<Vec<(&'static str, String)>, BridgeError> {
    Ok(vec![
        ("PATH", "/usr/bin".into()),
        ("HOME", text(data)?),
        ("XDG_CACHE_HOME", text(cache)?),
        // Pinned where they fall under this HOME, so none of them can name
        // Chilpa's own folders, whatever the sandbox sets by default.
        ("XDG_CONFIG_HOME", text(&data.join(".config"))?),
        ("XDG_DATA_HOME", text(&data.join(".local/share"))?),
        ("XDG_STATE_HOME", text(&data.join(".local/state"))?),
        // Temporary files go in its own cache, as on macOS.
        ("TMPDIR", text(&cache.join("tmp"))?),
        (DATA_DIR_VARIABLE, text(data)?),
        (CACHE_DIR_VARIABLE, text(cache)?),
        ("SSL_CERT_DIR", "/etc/ssl/certs".into()),
    ])
}

/// The runner's option that names the variables the plugin keeps: the ones
/// it is started with, and no other that reaches the runner.
pub(crate) fn keep_env(env: &[(&'static str, String)]) -> String {
    format!("--keep-env={}", env.iter().map(|(name, _)| *name).collect::<Vec<_>>().join(","))
}

/// The Flatpak portal, from inside Chilpa's own sandbox. It never falls back
/// to starting a plugin outside a sandbox.
pub struct Flatpak {
    pub(crate) runner: PathBuf,
    bus: Bus,
    /// The connection every spawn goes through, made once, on first use.
    /// Chilpa asks at startup, before any lock is held; a failed connection
    /// or a lost bus leaves process plugins unavailable for this run, so no
    /// later caller waits on a connection.
    portal: Mutex<Option<Result<Arc<Portal>, ()>>>,
}

/// The one Flatpak backend of this process: process plugins and the
/// compatibility worker share its portal connection.
pub(crate) fn shared() -> Arc<Flatpak> {
    static SHARED: std::sync::OnceLock<Arc<Flatpak>> = std::sync::OnceLock::new();
    SHARED.get_or_init(|| Arc::new(Flatpak::default())).clone()
}

impl Default for Flatpak {
    fn default() -> Self {
        Self { runner: RUNNER.into(), bus: Bus::Session, portal: Mutex::new(None) }
    }
}

impl Flatpak {
    #[cfg(test)]
    pub(crate) fn on(bus: Bus, runner: PathBuf) -> Self {
        Self { runner, bus, portal: Mutex::new(None) }
    }

    pub(crate) fn portal(&self) -> Option<Arc<Portal>> {
        let mut slot = self.portal.lock().unwrap_or_else(|error| error.into_inner());
        let connected = slot.get_or_insert_with(|| Portal::connect(&self.bus)
            .map_err(|problem| eprintln!("flatpak: the portal cannot start process plugins: {problem}")));
        connected.as_ref().ok().filter(|portal| !portal.lost()).cloned()
    }
}

impl NativeLauncher for Flatpak {
    fn availability(&self) -> Result<(), Unavailable> {
        if !self.runner.is_file() { return Err(Unavailable::MissingProgram(self.runner.clone())); }
        self.portal().map(|_| ()).ok_or(Unavailable::Portal)
    }

    /// The plugin's sandbox shares this one's runtime, so what its loader
    /// finds is what is under `/` here.
    fn preflight(&self, folder: &Path) -> Result<(), Incompatible> {
        crate::elf::preflight(&folder.join("plugin"), Path::new("/"))
    }

    fn launch(&self, paths: &NativePaths, name: &str, memory_bytes: usize, network: bool, notice: Notice)
        -> Result<LaunchedNative, BridgeError> {
        if memory_bytes < 64 * 1024 * 1024 { return Err(BridgeError::Denied); }
        let portal = self.portal().ok_or(BridgeError::Disconnected)?;
        for dir in [&paths.data, &paths.cache.join("tmp")] {
            std::fs::create_dir_all(dir).map_err(|_| BridgeError::Disconnected)?;
        }
        let folder = Exposed::open(&paths.folder)?;
        let data = Exposed::open(&paths.data)?;
        let cache = Exposed::open(&paths.cache)?;
        let export = paths.export.as_deref().map(Exposed::open).transpose()?;
        // Checked on what was opened, not on the path it was approved as: a
        // folder above it may have become a link since.
        if export.as_ref().is_some_and(|export| paths.is_private(&export.path)) {
            eprintln!("flatpak: plugin {name}'s export folder now leads into Chilpa's own folders; it is not started");
            return Err(BridgeError::Denied);
        }
        let program = folder.path.join("plugin").canonicalize().map_err(|_| BridgeError::Disconnected)?;
        if !program.is_file() || !program.starts_with(&folder.path) { return Err(BridgeError::Denied); }
        let memory = format!("--as={memory_bytes}");
        let runner = self.runner.as_os_str();
        let env = environment(&data.path, &cache.path)?;
        let keep = keep_env(&env);
        let argv = vec![runner, OsStr::new(&memory), OsStr::new("--nofile=256"), OsStr::new("--core=0"), OsStr::new(&keep),
            OsStr::new("--"), program.as_os_str()];
        let (child_input, input) = std::io::pipe().map_err(|_| BridgeError::Disconnected)?;
        let (output, child_output) = std::io::pipe().map_err(|_| BridgeError::Disconnected)?;
        let (errors, child_errors) = std::io::pipe().map_err(|_| BridgeError::Disconnected)?;
        let mut writable = vec![data.fd.as_fd(), cache.fd.as_fd()];
        writable.extend(export.as_ref().map(|export| export.fd.as_fd()));
        let request = Spawn {
            cwd: &data.path,
            argv,
            env,
            stdio: [child_input.as_fd(), child_output.as_fd(), child_errors.as_fd()],
            network,
            writable,
            readable: vec![folder.fd.as_fd()],
        };
        let process = portal.spawn(&request).map_err(|error| {
            eprintln!("flatpak: the portal did not start plugin {name}: {error}");
            BridgeError::Disconnected
        })?;
        // Only the plugin holds the far ends now, so its exit closes them.
        drop(request);
        drop((child_input, child_output, child_errors));
        log_errors(name, errors);
        let ended = process.clone();
        let client = NativeClient::start(input, output, move || { ended.terminate(); ended.wait(); }, notice);
        Ok(LaunchedNative { client, guest_directories: (data.path, cache.path), process })
    }
}

#[cfg(test)]
#[path = "flatpak_tests.rs"]
mod tests;
