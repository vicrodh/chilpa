//! I keep device sessions and platform-specific file operations outside the planner.
mod adb;
mod mounted;
mod native_fs;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub mod mtp;
pub use adb::{Adb, AdbSelector};
pub use mounted::Mounted;
/// The bytes a folder's filesystem can still take.
pub use native_fs::available;
pub use chilpa_sync::transport::{Entry, Transport};
