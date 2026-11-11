//! Offline destinations retain the instance and account that own the files.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub(super) mod leases;
mod limit;
mod manager;
mod page;
mod worker;
pub(crate) use limit::LIMIT;
pub(super) use page::Page;
pub(super) use worker::State;
#[derive(Default)]
pub(super) struct Invalidations {
    pub(super) pending: std::sync::Mutex<Vec<chilpa_library::store::offline::Copy>>,
    pub(super) scan: std::sync::atomic::AtomicBool,
}
pub(super) type InvalidCopies = std::sync::Arc<Invalidations>;

pub(super) fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CheckError {
    Busy,
    Invalid,
    Cancelled,
}

/// Verification uses one 64 KiB buffer and returns the same open handle
/// that playback will read, keeping the remote identity out of file tags.
pub(super) fn checked(
    item: &chilpa_library::store::offline::Copy,
    cancelled: impl Fn() -> bool,
) -> Result<(std::fs::File, std::sync::Arc<dyn Send + Sync>), CheckError> {
    use std::io::{Read, Seek};
    if cancelled() {
        return Err(CheckError::Cancelled);
    }
    let lease = leases::acquire(&item.key).ok_or(CheckError::Busy)?;
    let mut file = std::fs::File::open(&item.path).map_err(|_| CheckError::Invalid)?;
    let metadata = file.metadata().map_err(|_| CheckError::Invalid)?;
    if !metadata.is_file()
        || metadata.len() != item.bytes
        || item.bytes == 0
        || item.bytes > limit::CEILING_FILE_BYTES
    {
        return Err(CheckError::Invalid);
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        if cancelled() {
            return Err(CheckError::Cancelled);
        }
        let count = file.read(&mut buffer).map_err(|_| CheckError::Invalid)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if format!("{:x}", hash.finalize()) != item.checksum {
        return Err(CheckError::Invalid);
    }
    file.rewind().map_err(|_| CheckError::Invalid)?;
    Ok((file, lease))
}

/// Unreadable files keep their prior reservation until their size is known.
pub(super) fn disk_bytes(item: &chilpa_library::store::offline::Copy) -> u64 {
    let path = Path::new(&item.path);
    let mut bytes = 0u64;
    for file in [path.to_path_buf(), path.with_extension("part")] {
        match std::fs::metadata(file) {
            Ok(metadata) if metadata.is_file() => bytes = bytes.saturating_add(metadata.len()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return item.bytes,
        }
    }
    bytes
}

pub(super) const ROOT_PREFERENCE: &str = "offline.folder";
const MAX_FOLDER_BYTES: usize = 8192;

fn directory(value: &str) -> Result<PathBuf, String> {
    let invalid = || chilpa_i18n::t("The offline folder must be an absolute local folder.");
    if value.len() > MAX_FOLDER_BYTES || value.contains('\0') {
        return Err(invalid());
    }
    let path = if Path::new(value).is_absolute() {
        PathBuf::from(value)
    } else {
        let url = url::Url::parse(value).map_err(|_| invalid())?;
        if !value.starts_with("file://")
            || url.scheme() != "file"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(invalid());
        }
        url.to_file_path().map_err(|_| invalid())?
    };
    path.is_absolute().then_some(path).ok_or_else(invalid)
}

/// Selection is normalized without filesystem access on the window's thread.
pub(super) fn choice(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    url::Url::from_file_path(directory(value)?)
        .map(String::from)
        .map_err(|_| chilpa_i18n::t("The offline folder must be an absolute local folder."))
}

pub(super) fn global_directory(selected: Option<&str>) -> Result<PathBuf, String> {
    match selected.filter(|value| !value.is_empty()) {
        Some(value) => directory(value),
        None => Ok(chilpa_dirs::data().join("OfflineFolder")),
    }
}

/// Remote identifiers and account names never become directory components.
pub(super) fn destination(
    kind: chilpa_servers::Kind,
    product: &str,
    instance: &str,
    account: &str,
    selected: &str,
    global: &Path,
) -> Result<PathBuf, String> {
    if instance.len() != 16 || !instance.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(chilpa_i18n::t("This server is no longer available."));
    }
    let root = if selected.is_empty() {
        global.join(kind.offline_folder_name(product))
    } else {
        directory(selected)?
    };
    Ok(root
        .join(instance)
        .join(format!("{:x}", Sha256::digest(account.as_bytes()))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_servers::Kind;

    #[test]
    fn choices_preserve_encoded_paths_and_refuse_relative_or_remote_addresses() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Music with spaces");
        let url = url::Url::from_file_path(&path).unwrap().to_string();
        assert_eq!(choice(&url).unwrap(), url);
        assert_eq!(directory(&url).unwrap(), path);
        assert_eq!(choice("").unwrap(), "");
        for invalid in [
            "relative/music",
            "https://example.invalid/music",
            "file:relative",
            "file:///music?other=1",
            "file:///music#other",
        ] {
            assert!(choice(invalid).is_err());
        }
        assert!(
            !path.exists(),
            "configuration does not create a download directory"
        );
    }

    #[test]
    fn the_global_and_override_destinations_isolate_accounts_and_instances() {
        let root = tempfile::tempdir().unwrap();
        let global = root.path().join("global");
        let instance = "0123456789abcdef";
        let first = destination(Kind::Subsonic, "Server", instance, "first", "", &global).unwrap();
        let second =
            destination(Kind::Subsonic, "Server", instance, "second", "", &global).unwrap();
        assert_ne!(first, second);
        assert!(first.starts_with(
            global
                .join(Kind::Subsonic.offline_folder_name("Server"))
                .join(instance)
        ));
        let other = destination(
            Kind::Subsonic,
            "Server",
            "fedcba9876543210",
            "first",
            "",
            &global,
        )
        .unwrap();
        assert_ne!(first, other);
        let chosen = root.path().join("override");
        let selected = url::Url::from_file_path(&chosen).unwrap().to_string();
        let overridden = destination(
            Kind::Subsonic,
            "Server",
            instance,
            "first",
            &selected,
            &global,
        )
        .unwrap();
        assert!(overridden.starts_with(chosen.join(instance)));
        assert!(destination(Kind::Subsonic, "Server", "../outside", "first", "", &global).is_err());
        assert!(global_directory(Some("relative")).is_err());
        assert!(!global.exists());
    }
}
