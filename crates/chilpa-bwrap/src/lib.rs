//! Process isolation for desktop plugins and the Linux compatibility worker.
//!
//! Every backend hands back the same three pieces: the plugin's channel, the
//! folders it is told about in `initialize`, and a handle on its process. No
//! backend falls back to starting a plugin outside its sandbox.

use std::path::PathBuf;

/// Where a native plugin lives and keeps its things. Chilpa chooses these;
/// the plugin's manifest never does.
pub struct NativePaths {
    /// The plugin's folder, with its `plugin` executable. Read-only inside.
    pub folder: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
    /// A host-selected export folder, after permission approval.
    pub export: Option<PathBuf>,
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
mod portal;
#[cfg(target_os = "linux")]
mod flatpak;
#[cfg(target_os = "linux")]
pub use flatpak::Flatpak;
#[cfg(all(test, target_os = "linux"))]
mod fake_portal;
#[cfg(all(test, target_os = "linux"))]
mod conformance;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::Seatbelt;
