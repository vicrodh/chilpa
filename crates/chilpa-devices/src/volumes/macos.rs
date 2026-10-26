use super::{Volume, MAX_VOLUMES};
use crate::{invalid, Result};
use std::{ffi::CStr, os::unix::ffi::OsStrExt, path::Path};

pub(super) fn mounted() -> Result<Vec<Volume>> {
    let count = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    if !(0..=4096).contains(&count) { return Err(invalid("the mounted volume count is unavailable or exceeds 4096")); }
    let mut entries: Vec<libc::statfs> = (0..(count + 16).min(4096)).map(|_| unsafe { std::mem::zeroed() }).collect();
    let bytes = (entries.len() * std::mem::size_of::<libc::statfs>()) as libc::c_int;
    let count = unsafe { libc::getfsstat(entries.as_mut_ptr(), bytes, libc::MNT_NOWAIT) };
    if count < 0 || count as usize >= entries.len() { return Err(invalid("the mount table changed during discovery")); }
    let mut rows = Vec::new();
    for info in entries.into_iter().take(count as usize) {
        let name = unsafe { CStr::from_ptr(info.f_mntonname.as_ptr()) };
        let path = Path::new(std::ffi::OsStr::from_bytes(name.to_bytes()));
        if path.parent() != Some(chilpa_dirs::mounted_volume_directory()) || info.f_flags & libc::MNT_LOCAL == 0 { continue; }
        if rows.len() >= MAX_VOLUMES { return Err(invalid("more than 128 removable volumes are mounted")); }
        let source = unsafe { CStr::from_ptr(info.f_mntfromname.as_ptr()) };
        rows.push(Volume {
            id: format!("volume:{}", crate::hash_text(&format!("{}:{}", source.to_string_lossy(), name.to_string_lossy()))),
            name: path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned(), path: path.to_path_buf(),
            available: info.f_bavail.saturating_mul(info.f_bsize as u64), capacity: info.f_blocks.saturating_mul(info.f_bsize as u64),
            writable: info.f_flags & libc::MNT_RDONLY == 0,
        });
    }
    Ok(rows)
}
