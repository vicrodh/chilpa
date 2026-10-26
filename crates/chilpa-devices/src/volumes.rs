//! I enumerate mounted removable storage without opening music or creating sync metadata.
#[cfg(any(target_os = "linux", target_os = "android"))]
#[path = "volumes/linux.rs"]
mod native;
#[cfg(target_os = "macos")]
#[path = "volumes/macos.rs"]
mod native;
#[cfg(target_os = "windows")]
#[path = "volumes/windows.rs"]
mod native;

use crate::{conflict, Result};
use std::path::{Path, PathBuf};

pub const MAX_VOLUMES: usize = 128;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub available: u64,
    pub capacity: u64,
    pub writable: bool,
}
pub fn mounted() -> Result<Vec<Volume>> {
    if std::env::var("CHILPA_DEVICE_TEST_MOUNTED_ONLY").as_deref() == Ok("1") { return Ok(Vec::new()); }
    native::mounted()
}
impl Volume {
    pub fn resolve(id: &str) -> Result<Self> {
        mounted()?.into_iter().find(|volume| volume.id == id)
            .ok_or_else(|| conflict("the memory card disconnected or its mount changed"))
    }
    /// I only create a dedicated relative folder after the user saves this volume's profile.
    pub fn folder(&self, relative: &str, create: bool) -> Result<PathBuf> {
        let relative = crate::RelativePath::new(relative.to_owned())?;
        if Self::resolve(&self.id)?.path != self.path { return Err(conflict("the memory card mount changed")); }
        let anchor = self.pin()?;
        #[cfg(unix)] self.create_relative(&anchor, &relative, create)?;
        #[cfg(windows)] {
            let mut path = self.path.clone();
            for component in relative.as_str().split('/') {
                self.check_pin(&anchor)?;
                path.push(component);
                match std::fs::symlink_metadata(&path) {
                    Ok(info) if info.file_type().is_symlink() || !info.is_dir() => return Err(conflict("the sync folder is not a real directory")),
                    Ok(_) => (),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => { std::fs::create_dir(&path)?; }
                    Err(error) => return Err(error.into()),
                }
            }
        }
        let path = self.path.join(relative.as_str());
        if Self::resolve(&self.id)?.path != self.path { return Err(conflict("the memory card mount changed")); }
        self.check_pin(&anchor)?;
        Ok(path)
    }
    #[cfg(unix)]
    fn create_relative(&self, anchor: &std::fs::File, relative: &crate::RelativePath, create: bool) -> Result<()> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let mut parent = anchor.try_clone()?;
        for component in relative.as_str().split('/') {
            self.check_pin(anchor)?;
            let name = std::ffi::CString::new(component).map_err(|_| crate::invalid("invalid sync folder name"))?;
            if create && unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o755) } != 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() != std::io::ErrorKind::AlreadyExists { return Err(error.into()); }
            }
            let child = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW) };
            if child < 0 { return Err(std::io::Error::last_os_error().into()); }
            parent = unsafe { std::fs::File::from_raw_fd(child) };
        }
        self.check_pin(anchor)
    }
    fn pin(&self) -> Result<std::fs::File> {
        let file = directory_handle(&self.path)?;
        self.check_pin(&file)?;
        Ok(file)
    }
    fn check_pin(&self, file: &std::fs::File) -> Result<()> { unchanged(&self.path, file) }

}
pub(crate) fn directory_handle(path: &Path) -> Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x02000000);
    }
    Ok(options.open(path)?)
}
pub(crate) fn unchanged(path: &Path, file: &std::fs::File) -> Result<()> {
    #[cfg(unix)] {
        use std::os::unix::fs::MetadataExt;
        let (original, current) = (file.metadata()?, std::fs::metadata(path)?);
        if original.dev() != current.dev() || original.ino() != current.ino() { return Err(conflict("the mounted memory changed")); }
    }
    #[cfg(windows)] {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
        let current = directory_handle(path)?;
        let mut before: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        let mut after: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut before) } == 0 || unsafe { GetFileInformationByHandle(current.as_raw_handle(), &mut after) } == 0 { return Err(std::io::Error::last_os_error().into()); }
        if (before.dwVolumeSerialNumber, before.nFileIndexHigh, before.nFileIndexLow) != (after.dwVolumeSerialNumber, after.nFileIndexHigh, after.nFileIndexLow) { return Err(conflict("the mounted memory changed")); }
    }
    Ok(())
}
/// I read only a known identity file, never acquire the writable Mounted lease during discovery.
pub fn identity(root: &Path) -> Result<Option<chilpa_sync::Target>> {
    let mut path = PathBuf::new();
    for component in root.components() {
        path.push(component);
        match std::fs::symlink_metadata(&path) {
            Ok(info) if info.file_type().is_symlink() => return Err(conflict("the saved storage path contains a symbolic link")),
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    }
    for component in [".chilpa-sync", "identity.json"] {
        path.push(component);
        match std::fs::symlink_metadata(&path) {
            Ok(info) if info.file_type().is_symlink() => return Err(conflict("the storage identity contains a symbolic link")),
            Ok(info) if component == "identity.json" && (!info.is_file() || info.len() > 4096) => return Err(conflict("damaged storage identity")),
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)] {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
    options.open(path)?.take(4097).read_to_end(&mut bytes)?;
    chilpa_sync::parse_target_identity(&bytes).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_reads_the_marker_without_creating_a_lock_or_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Chilpa");
        std::fs::create_dir(&root).unwrap();
        assert!(identity(&root).unwrap().is_none());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::create_dir(root.join(".chilpa-sync")).unwrap();
        std::fs::write(root.join(".chilpa-sync/identity.json"), format!(r#"{{"version":1,"id":"{}","name":"Card"}}"#, "a".repeat(32))).unwrap();
        assert_eq!(identity(&root).unwrap().unwrap().id, "a".repeat(32));
        assert!(!root.join(".chilpa-sync/lock").exists());
        std::fs::write(root.join(".chilpa-sync/identity.json"), vec![b'x';4097]).unwrap();
        assert!(identity(&root).is_err());
    }
    #[test]
    fn a_disconnected_volume_never_creates_a_folder_on_the_host() {
        let directory = tempfile::tempdir().unwrap();
        let volume = Volume { id:"volume:disconnected-fixture".into(), name:"Card".into(), path:directory.path().to_path_buf(), available:0, capacity:0, writable:true };
        assert!(volume.folder("Chilpa", true).is_err());
        assert!(!directory.path().join("Chilpa").exists());
    }
    #[cfg(unix)]
    #[test]
    fn discovery_rejects_a_symbolic_marker_and_changed_mount_directory() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("card");
        std::fs::create_dir_all(root.join(".chilpa-sync")).unwrap();
        std::os::unix::fs::symlink(directory.path().join("elsewhere"), root.join(".chilpa-sync/identity.json")).unwrap();
        assert!(identity(&root).is_err());
        let handle = directory_handle(&root).unwrap();
        std::fs::rename(&root, directory.path().join("removed-card")).unwrap();
        std::fs::create_dir(&root).unwrap();
        assert!(unchanged(&root, &handle).is_err());
    }
}
