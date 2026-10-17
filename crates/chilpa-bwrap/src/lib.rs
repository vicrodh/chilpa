//! Process isolation for desktop plugins and the Linux compatibility worker.

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

impl NativePaths {
    /// The paths I send in initialize; Linux mounts private namespace roots.
    pub fn guest_directories(&self) -> std::io::Result<(PathBuf, PathBuf)> {
        #[cfg(target_os = "linux")]
        { Ok(("/data".into(), "/cache".into())) }
        #[cfg(not(target_os = "linux"))]
        { Ok((self.data.canonicalize()?, self.cache.canonicalize()?)) }
    }
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{launch, launch_native, RuntimePaths};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::launch_native;
