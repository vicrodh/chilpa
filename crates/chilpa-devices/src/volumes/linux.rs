use super::{Volume, MAX_VOLUMES};
use crate::{invalid, Result};
use std::{io::Read, path::PathBuf};

fn unescape(raw: &str) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    let mut bytes = Vec::with_capacity(raw.len());
    let mut input = raw.bytes();
    while let Some(byte) = input.next() {
        if byte == b'\\' {
            let code: Vec<_> = input.by_ref().take(3).collect();
            if code.len() == 3 && code.iter().all(|b| (b'0'..=b'7').contains(b)) {
                bytes.push((code[0] - b'0') * 64 + (code[1] - b'0') * 8 + code[2] - b'0');
            } else { bytes.push(byte); bytes.extend(code); }
        } else { bytes.push(byte); }
    }
    std::ffi::OsString::from_vec(bytes).into()
}
fn removable(node: &std::path::Path) -> bool {
    // I recognize native SD controllers whose block devices deliberately have removable=0.
    node.ancestors().take(16).any(|parent| {
        std::fs::read_to_string(parent.join("removable")).is_ok_and(|value| value.trim() == "1")
            || std::fs::read_to_string(parent.join("type")).is_ok_and(|value| matches!(value.trim(), "SD" | "SDcombo"))
    }) || node.components().any(|part| part.as_os_str().as_encoded_bytes().starts_with(b"usb"))
}
pub(super) fn mounted() -> Result<Vec<Volume>> {
    let mut table = String::new();
    std::fs::File::open(chilpa_dirs::mount_info_file())?.take(4 * 1024 * 1024 + 1).read_to_string(&mut table)?;
    if table.len() > 4 * 1024 * 1024 { return Err(invalid("the mount table exceeds 4 MiB")); }
    let mut rows = Vec::new();
    for (index, line) in table.lines().enumerate() {
        if index >= 4096 { return Err(invalid("the mount table exceeds 4096 entries")); }
        let mut fields = line.split_whitespace();
        let Some(mount_id) = fields.next() else { continue };
        let Some(_parent) = fields.next() else { continue };
        let Some(device) = fields.next() else { continue };
        let Some(root) = fields.next() else { continue };
        let Some(mount) = fields.next() else { continue };
        let Some(flags) = fields.next() else { continue };
        if root != "/" || mount == "/" { continue; }
        let Some(node) = chilpa_dirs::block_device_node(device) else { continue };
        let Ok(node) = std::fs::canonicalize(node) else { continue };
        if !removable(&node) { continue; }
        let path = unescape(mount);
        if !path.is_absolute() || std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) { continue; }
        use std::os::unix::ffi::OsStrExt;
        let Ok(name) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { continue };
        let mut info = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::statvfs(name.as_ptr(), info.as_mut_ptr()) } != 0 { continue; }
        let info = unsafe { info.assume_init() };
        if rows.len() >= MAX_VOLUMES { return Err(invalid("more than 128 removable volumes are mounted")); }
        rows.push(Volume {
            id: format!("volume:{}", crate::hash_text(&format!("{mount_id}:{device}:{mount}"))),
            name: path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into_owned(),
            path, available: u64::from(info.f_bavail).saturating_mul(u64::from(info.f_frsize)), capacity: u64::from(info.f_blocks).saturating_mul(u64::from(info.f_frsize)),
            writable: !flags.split(',').any(|flag| flag == "ro"),
        });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    #[test]
    fn mount_paths_keep_spaces_and_literal_escape_sequences() {
        assert_eq!(super::unescape("/media/a\\040b\\134040"), std::path::Path::new("/media/a b\\040"));
    }
    #[test]
    fn native_sd_cards_are_detected_without_classifying_emmc_as_removable() {
        let directory = tempfile::tempdir().unwrap();
        let card = directory.path().join("mmc_host/mmc0/mmc0:0001");
        let node = card.join("block/mmcblk0/mmcblk0p1");
        std::fs::create_dir_all(&node).unwrap();
        std::fs::write(node.parent().unwrap().join("removable"), "0\n").unwrap();
        for (kind, expected) in [("SD", true), ("SDcombo", true), ("MMC", false), ("SDIO", false)] {
            std::fs::write(card.join("type"), kind).unwrap();
            assert_eq!(super::removable(&node), expected, "card type: {kind}");
        }
        std::fs::write(node.parent().unwrap().join("removable"), "1\n").unwrap();
        assert!(super::removable(&node));
        assert!(super::removable(&directory.path().join("usb1/1-1/block/sda/sda1")));
    }
}
