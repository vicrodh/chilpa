//! Process isolation for desktop plugins and the Linux compatibility worker.
//!
//! Every backend hands back the same three pieces: the plugin's channel, the
//! folders it is told about in `initialize`, and a handle on its process. No
//! backend falls back to starting a plugin outside its sandbox.

use std::path::{Path, PathBuf};

/// Where a native plugin lives and keeps its things. Chilpa chooses these;
/// the plugin's manifest never does.
pub struct NativePaths {
    /// The plugin's folder, with its `plugin` executable. Read-only inside.
    pub folder: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
    /// A host-selected export folder, after permission approval.
    pub export: Option<PathBuf>,
    /// Chilpa's own folders, canonical. An export folder that is one of
    /// them, lies inside one or holds one is refused at launch, whatever it
    /// was when it was approved.
    pub private: Vec<PathBuf>,
}

impl NativePaths {
    /// Whether `folder` is one of Chilpa's own folders, lies inside one or
    /// holds one. Both sides are compared as given: canonical paths.
    pub fn is_private(&self, folder: &Path) -> bool {
        self.private.iter().any(|own| own.starts_with(folder) || folder.starts_with(own))
    }
}

/// Why process plugins cannot start on this system, found without starting one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unavailable {
    /// Chilpa runs in a Flatpak sandbox, and the portal that starts a
    /// sandbox of its own did not answer or is older than version 3.
    Portal,
    /// A program the sandbox is built from is not installed.
    MissingProgram(PathBuf),
    /// Chilpa runs as root, and the sandbox does not contain a privileged caller.
    Privileged,
}

/// Why one process plugin's program cannot run on this system, read from its
/// file without starting it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Incompatible {
    /// It is no program this system runs: another processor, a 32-bit
    /// build, or neither an ELF program nor a script.
    Format,
    /// The program loader or script interpreter it names is not there.
    Interpreter(PathBuf),
    /// A library it links is not there.
    Library(String),
    /// A library is there without a symbol version it asks for. `newest`
    /// is the newest one of the same family that is there, if any.
    Version { library: String, needed: String, newest: Option<String> },
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod process;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use process::{launcher, LaunchedNative, NativeLauncher, NativeProcess, Notice};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod linux_bwrap;
#[cfg(target_os = "linux")]
pub use linux_bwrap::{launch, Bwrap, RuntimePaths};
#[cfg(target_os = "linux")]
mod elf;
#[cfg(target_os = "linux")]
mod portal;
#[cfg(target_os = "linux")]
mod flatpak;
#[cfg(target_os = "linux")]
pub use flatpak::{confines_descriptor_exports, unconfinable_export, Flatpak};
#[cfg(target_os = "linux")]
mod flatpak_worker;
#[cfg(target_os = "linux")]
pub use flatpak_worker::FlatpakWorker;
#[cfg(target_os = "linux")]
pub use linux::compatibility;
#[cfg(all(test, target_os = "linux"))]
mod worker_conformance;
#[cfg(all(test, target_os = "linux"))]
mod fake_portal;
#[cfg(all(test, target_os = "linux"))]
mod conformance;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::Seatbelt;

/// Explicitly approved desktop helpers; separate from sandboxed plugins.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod desktop;
