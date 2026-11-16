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
//!
//! An export folder chosen in the file chooser is, without a filesystem
//! permission, a folder of the Documents portal. The portal cannot mount
//! one in another sandbox: it skips it and starts the process all the same,
//! and that sandbox has no Documents portal of its own. Such a folder goes
//! over as a descriptor instead, which reaches the Documents portal from
//! any sandbox, and the plugin is told `/proc/self/fd/3`. Every export
//! folder is held at descriptor 3, and the runner checks that the path the
//! plugin is told is that folder and can be written before the plugin
//! starts. A folder the portal mounts never reaches the plugin as that
//! descriptor: the runner closes it, since a path resolved from a descriptor
//! opened here walks this sandbox's tree, out of the folder with `..`. A
//! plugin that must keep it is confined by the runner to its own root and
//! that folder, or not started. The backend waits for the runner's ready
//! line, which comes on descriptor 4 once the plugin's program runs, so a
//! start the runner refuses, or a program that cannot be executed, fails
//! here, at launch, and not later on the plugin's first write.

use crate::portal::{Bus, Portal, Spawn};
use crate::process::{log_errors, LaunchedNative, NativeLauncher, NativeProcess, Notice, CACHE_DIR_VARIABLE, DATA_DIR_VARIABLE};
use crate::{Incompatible, NativePaths, Unavailable};
use chilpa_plugin::{bridge::BridgeError, native::NativeClient};
use std::ffi::{CString, OsStr};
use std::io::Read;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Where the Flatpak installs the runner, visible from the plugin's sandbox.
const RUNNER: &str = "/app/libexec/chilpa-plugin-runner";
/// Where the runner finds the export folder: the runner's own
/// `EXPORT_DESCRIPTOR`.
const EXPORT_DESCRIPTOR: u32 = 3;
/// Where the runner writes its ready line, a pipe of its own.
const READY_DESCRIPTOR: u32 = 4;
/// How long the runner has to say the plugin started, as long as the portal
/// has to answer the spawn.
const READY_WAIT: Duration = Duration::from_secs(10);

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

/// Whether this kernel lets the runner keep a plugin inside a folder it
/// holds as a descriptor: Landlock 3 or later, the runner's own minimum.
pub fn confines_descriptor_exports() -> bool {
    static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| {
        let version = unsafe { libc::syscall(libc::SYS_landlock_create_ruleset, std::ptr::null::<u8>(), 0usize, 1u32) };
        version >= 3
    })
}

/// Whether `folder`, as an export folder, would reach a plugin only as a
/// descriptor, a folder of the Documents portal inside a Flatpak, that this
/// kernel cannot keep the plugin inside. Such a folder is never handed over.
pub fn unconfinable_export(folder: &Path) -> bool {
    chilpa_dirs::sandbox() == chilpa_dirs::Sandbox::Flatpak
        && chilpa_dirs::document_portal_mounts().iter().any(|mount| folder.starts_with(mount))
        && !confines_descriptor_exports()
}

fn text(path: &Path) -> Result<String, BridgeError> {
    path.to_str().map(str::to_owned).ok_or(BridgeError::Denied)
}

/// Whether this process can write in the folder `fd` holds, which the
/// portal requires of a folder it mounts writable.
fn can_write(fd: &OwnedFd) -> bool {
    let path = chilpa_dirs::own_descriptors().join(fd.as_raw_fd().to_string());
    CString::new(path.as_os_str().as_bytes()).is_ok_and(|name| unsafe { libc::access(name.as_ptr(), libc::W_OK | libc::X_OK) } == 0)
}

/// Reads the runner's ready line, waiting up to `bound`. False when its pipe
/// closed first, when anything else came, or when nothing came in time.
fn ready(output: &mut std::io::PipeReader, bound: Duration) -> bool {
    let deadline = Instant::now() + bound;
    let mut waited = libc::pollfd { fd: output.as_raw_fd(), events: libc::POLLIN, revents: 0 };
    loop {
        let left = deadline.saturating_duration_since(Instant::now()).as_millis().min(i32::MAX as u128) as i32;
        let polled = unsafe { libc::poll(&mut waited, 1, left) };
        if polled < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) { continue; }
        if polled <= 0 { return false; }
        let mut byte = [0u8; 1];
        return matches!(output.read(&mut byte), Ok(1)) && byte[0] == b'\n';
    }
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
    /// Where the Documents portal's folders appear in this sandbox.
    documents: Vec<PathBuf>,
    /// Whether a folder of theirs can be handed over: the runner can keep
    /// the plugin inside it ([`confines_descriptor_exports`]).
    confines: bool,
}

/// The one Flatpak backend of this process: process plugins and the
/// compatibility worker share its portal connection.
pub(crate) fn shared() -> Arc<Flatpak> {
    static SHARED: std::sync::OnceLock<Arc<Flatpak>> = std::sync::OnceLock::new();
    SHARED.get_or_init(|| Arc::new(Flatpak::default())).clone()
}

impl Default for Flatpak {
    fn default() -> Self {
        Self { runner: RUNNER.into(), bus: Bus::Session, portal: Mutex::new(None), documents: chilpa_dirs::document_portal_mounts(),
            confines: confines_descriptor_exports() }
    }
}

impl Flatpak {
    /// The fake portal runs no runner, so these tests confine nothing.
    #[cfg(test)]
    pub(crate) fn on(bus: Bus, runner: PathBuf) -> Self {
        Self { runner, bus, portal: Mutex::new(None), documents: Vec::new(), confines: true }
    }

    /// The same backend, with the Documents portal's folders under `mounts`.
    #[cfg(test)]
    pub(crate) fn with_documents(self, mounts: Vec<PathBuf>) -> Self {
        Self { documents: mounts, ..self }
    }

    /// The same backend on a kernel that cannot confine a plugin.
    #[cfg(test)]
    pub(crate) fn without_confinement(self) -> Self {
        Self { confines: false, ..self }
    }

    /// Where the plugin is told to find `export`: at its own path, where the
    /// portal mounts it, or through its descriptor for a folder of the
    /// Documents portal, which the portal does not mount.
    fn guest_export(&self, export: &Exposed) -> PathBuf {
        if self.in_documents(&export.path) {
            chilpa_dirs::own_descriptors().join(EXPORT_DESCRIPTOR.to_string())
        } else {
            export.path.clone()
        }
    }

    fn in_documents(&self, path: &Path) -> bool {
        self.documents.iter().any(|mount| path.starts_with(mount))
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
        // Chilpa drops such a folder before the launch; this is the second
        // guard, and the runner the third.
        if export.as_ref().is_some_and(|export| self.in_documents(&export.path)) && !self.confines {
            eprintln!("flatpak: plugin {name}'s export folder is a folder of the Documents portal, and this kernel has no Landlock 3 to keep the plugin inside it; it is not started");
            return Err(BridgeError::Denied);
        }
        // The portal would skip a folder it cannot mount writable and start
        // the plugin all the same: refused here, before anything starts.
        if let Some(export) = export.as_ref().filter(|export| !can_write(&export.fd)) {
            eprintln!("flatpak: plugin {name}'s export folder {} cannot be written; it is not started", export.path.display());
            return Err(BridgeError::Denied);
        }
        let guest_export = export.as_ref().map(|export| self.guest_export(export));
        let program = folder.path.join("plugin").canonicalize().map_err(|_| BridgeError::Disconnected)?;
        if !program.is_file() || !program.starts_with(&folder.path) { return Err(BridgeError::Denied); }
        let memory = format!("--as={memory_bytes}");
        let runner = self.runner.as_os_str();
        let env = environment(&data.path, &cache.path)?;
        let keep = keep_env(&env);
        let told = guest_export.as_deref().map(text).transpose()?.map(|path| format!("--export={path}"));
        let ready_option = format!("--ready={READY_DESCRIPTOR}");
        let mut argv = vec![runner, OsStr::new(&memory), OsStr::new("--nofile=256"), OsStr::new("--core=0"), OsStr::new(&keep)];
        argv.extend(told.as_deref().map(OsStr::new));
        argv.extend([OsStr::new(&ready_option), OsStr::new("--"), program.as_os_str()]);
        let (child_input, input) = std::io::pipe().map_err(|_| BridgeError::Disconnected)?;
        let (output, child_output) = std::io::pipe().map_err(|_| BridgeError::Disconnected)?;
        let (errors, child_errors) = std::io::pipe().map_err(|_| BridgeError::Disconnected)?;
        let (mut started, child_started) = std::io::pipe().map_err(|_| BridgeError::Disconnected)?;
        let mut writable = vec![data.fd.as_fd(), cache.fd.as_fd()];
        writable.extend(export.as_ref().filter(|export| !self.in_documents(&export.path)).map(|export| export.fd.as_fd()));
        let mut descriptors: Vec<(u32, std::os::fd::BorrowedFd<'_>)> = export.as_ref().map(|export| (EXPORT_DESCRIPTOR, export.fd.as_fd())).into_iter().collect();
        descriptors.push((READY_DESCRIPTOR, child_started.as_fd()));
        let request = Spawn {
            cwd: &data.path,
            argv,
            env,
            stdio: [child_input.as_fd(), child_output.as_fd(), child_errors.as_fd()],
            descriptors,
            network,
            writable,
            readable: vec![folder.fd.as_fd()],
        };
        let process = portal.spawn(&request).map_err(|error| {
            eprintln!("flatpak: the portal did not start plugin {name}: {error}");
            BridgeError::Disconnected
        })?;
        // Only the runner and the plugin hold the far ends now, so their
        // exits close them.
        drop(request);
        drop((child_input, child_output, child_errors, child_started));
        log_errors(name, errors);
        if !ready(&mut started, READY_WAIT) {
            eprintln!("flatpak: plugin {name} did not start: its runner refused it or could not execute it, its reason above, or said nothing within {} s",
                READY_WAIT.as_secs());
            process.terminate();
            process.wait();
            return Err(BridgeError::Disconnected);
        }
        let ended = process.clone();
        let client = NativeClient::start(input, output, move || { ended.terminate(); ended.wait(); }, notice);
        Ok(LaunchedNative { client, guest_directories: (data.path, cache.path), export: guest_export, process })
    }
}

#[cfg(test)]
#[path = "flatpak_tests.rs"]
mod tests;
