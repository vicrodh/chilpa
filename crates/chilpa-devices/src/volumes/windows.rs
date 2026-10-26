use super::{Volume, MAX_VOLUMES};
use crate::{invalid, Result};
use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};
use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW};

pub(super) fn mounted() -> Result<Vec<Volume>> {
    let mut roots = [0u16; 1024];
    let count = unsafe { GetLogicalDriveStringsW(roots.len() as u32, roots.as_mut_ptr()) };
    if count == 0 || count as usize >= roots.len() { return Err(invalid("the removable drive list is unavailable")); }
    let mut rows = Vec::new();
    for root in roots[..count as usize].split(|value| *value == 0).filter(|part| !part.is_empty()) {
        let mut terminated = root.to_vec(); terminated.push(0);
        if unsafe { GetDriveTypeW(terminated.as_ptr()) } != 2 { continue; }
        let (mut available, mut capacity, mut free) = (0, 0, 0);
        if unsafe { GetDiskFreeSpaceExW(terminated.as_ptr(), &mut available, &mut capacity, &mut free) } == 0 { continue; }
        let (mut serial, mut flags) = (0, 0);
        let mut label = [0u16; 261];
        if unsafe { GetVolumeInformationW(terminated.as_ptr(), label.as_mut_ptr(), label.len() as u32, &mut serial, std::ptr::null_mut(), &mut flags, std::ptr::null_mut(), 0) } == 0 { continue; }
        if rows.len() >= MAX_VOLUMES { return Err(invalid("more than 128 removable volumes are mounted")); }
        let path = PathBuf::from(OsString::from_wide(root));
        let label_end = label.iter().position(|c| *c == 0).unwrap_or(label.len());
        let label = String::from_utf16_lossy(&label[..label_end]);
        rows.push(Volume {
            id: format!("volume:{}", crate::hash_text(&format!("{}:{serial}", path.display()))),
            name: if label.is_empty() { path.display().to_string() } else { label }, path,
            available, capacity, writable: flags & 0x00080000 == 0,
        });
    }
    Ok(rows)
}
