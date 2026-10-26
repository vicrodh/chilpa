use crate::Result;
use std::path::Path;

#[cfg(unix)]
fn c_string_path(path: &Path) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| crate::invalid("path contains NUL"))
}

#[cfg(unix)]
pub fn available(path: &Path) -> Result<u64> {
    let path = c_string_path(path)?;
    let mut info = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // I give statvfs an initialized C string and enough writable space for its output.
    if unsafe { libc::statvfs(path.as_ptr(), info.as_mut_ptr()) } != 0 { return Err(std::io::Error::last_os_error().into()); }
    let info = unsafe { info.assume_init() };
    Ok((info.f_bavail as u64).saturating_mul(info.f_frsize as u64))
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn rename_new(from: &Path, to: &Path) -> Result<()> {
    let (from, to) = (c_string_path(from)?, c_string_path(to)?);
    // RENAME_NOREPLACE protects a file created by another application after my check.
    let code = unsafe { libc::renameat2(libc::AT_FDCWD, from.as_ptr(), libc::AT_FDCWD, to.as_ptr(), libc::RENAME_NOREPLACE) };
    if code != 0 { return Err(std::io::Error::last_os_error().into()); }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn rename_new(from: &Path, to: &Path) -> Result<()> {
    let (from, to) = (c_string_path(from)?, c_string_path(to)?);
    let code = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
    if code != 0 { return Err(std::io::Error::last_os_error().into()); }
    Ok(())
}

#[cfg(windows)]
fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
#[cfg(windows)]
pub fn available(path: &Path) -> Result<u64> {
    let path = wide(path);
    let mut free = 0;
    let code = unsafe { windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(path.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    if code == 0 { return Err(std::io::Error::last_os_error().into()); }
    Ok(free)
}
#[cfg(windows)]
pub fn rename_new(from: &Path, to: &Path) -> Result<()> {
    let (from, to) = (wide(from), wide(to));
    // Zero flags preserve the destination if it already exists.
    let code = unsafe { windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 0) };
    if code == 0 { return Err(std::io::Error::last_os_error().into()); }
    Ok(())
}

#[cfg(unix)]
pub fn sync_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() { std::fs::File::open(parent)?.sync_all()?; }
    Ok(())
}
#[cfg(windows)]
pub fn sync_parent(_path: &Path) -> Result<()> { Ok(()) }
