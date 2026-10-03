//! Verified downloads stay outside the installed plugin tree until reviewed.

use chilpa_plugin::catalog::Digest;
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeSet, io::{Cursor, Read, Write}, path::{Path, PathBuf}};

pub const MAX_PACKAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FILES: usize = 4096;

/// Dropping a staged package removes all extracted files. No package file
/// is executable through this interface and no approval is granted here.
pub struct StagedPackage {
    directory: tempfile::TempDir,
    pub sha256: String,
    files: Vec<PathBuf>,
}

impl StagedPackage {
    /// I stage a selected local archive with the same extraction limits as a download.
    pub fn local(parent: &Path, path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|_| chilpa_i18n::t("The plugin ZIP could not be opened."))?;
        if !file.metadata().map_err(|_| chilpa_i18n::t("The plugin ZIP could not be read."))?.is_file() {
            return Err(chilpa_i18n::t("Choose a local ZIP file."));
        }
        let mut bytes = Vec::new();
        file.take(MAX_PACKAGE_BYTES as u64 + 1).read_to_end(&mut bytes)
            .map_err(|_| chilpa_i18n::t("The plugin ZIP could not be read."))?;
        let digest = Digest { algorithm: "sha256".into(), value: format!("{:x}", Sha256::digest(&bytes)) };
        Self::unpack(parent, &bytes, &digest)
    }

    pub fn root(&self) -> &Path { self.directory.path() }
    pub fn files(&self) -> &[PathBuf] { &self.files }

    /// Release archives may have a wrapper folder. Exactly one complete
    /// extension is accepted; unrelated metadata cannot select its code.
    pub fn extension(&self) -> Result<(PathBuf, chilpa_plugin::extension::ExtensionMetadata), String> {
        let mut roots = self.files.iter().filter(|path| path.file_name().is_some_and(|name| name == "install.xml"))
            .filter_map(|path| path.parent())
            .filter(|parent| self.files.contains(&parent.join("Plugin.pm")));
        let relative = roots.next().ok_or("The package has no supported extension entry point.")?;
        if roots.next().is_some() { return Err("The package contains multiple extension entry points.".into()); }
        let root = self.root().join(relative);
        let file = std::fs::File::open(root.join("install.xml")).map_err(|_| "The extension metadata could not be read.")?;
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes).map_err(|_| "The extension metadata could not be read.")?;
        let metadata = chilpa_plugin::extension::ExtensionMetadata::read(&bytes)?;
        Ok((root, metadata))
    }

    /// The staging parent belongs to the host, outside the plugin loader's
    /// directory. I never extract into an existing installation.
    pub fn unpack(parent: &Path, bytes: &[u8], expected: &Digest) -> Result<Self, String> {
        if bytes.len() > MAX_PACKAGE_BYTES { return Err("The plugin package is too large.".into()); }
        let sha256 = format!("{:x}", Sha256::digest(bytes));
        let actual = match expected.algorithm.as_str() {
            "sha256" => sha256.clone(),
            // Legacy catalogs publish this checksum. It checks the catalog's
            // bytes, not publisher identity; evidence always uses SHA-256.
            "sha1" => format!("{:x}", sha1::Sha1::digest(bytes)),
            _ => return Err("The package checksum algorithm is unsupported.".into()),
        };
        if !actual.eq_ignore_ascii_case(&expected.value) {
            return Err("The plugin package does not match the catalog checksum.".into());
        }
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|_| "The plugin package is not a supported ZIP archive.")?;
        if archive.len() > MAX_FILES { return Err("The plugin package contains too many files.".into()); }
        std::fs::create_dir_all(parent).map_err(|_| "The staging directory could not be created.")?;
        let directory = tempfile::Builder::new().prefix("package-").tempdir_in(parent)
            .map_err(|_| "The package could not be staged.")?;
        let mut paths = BTreeSet::new();
        let mut files = Vec::new();
        let mut total = 0u64;
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).map_err(|_| "The plugin archive is invalid or encrypted.")?;
            let relative = portable_path(file.name())?;
            if !paths.insert(relative.to_string_lossy().to_ascii_lowercase()) {
                return Err("The plugin archive contains conflicting paths.".into());
            }
            let kind = file.unix_mode().unwrap_or(0) & 0o170000;
            if (file.is_dir() && !matches!(kind, 0 | 0o040000))
                || (!file.is_dir() && !matches!(kind, 0 | 0o100000)) {
                return Err("Links and special files are not supported in plugin packages.".into());
            }
            let target = directory.path().join(&relative);
            if file.is_dir() {
                std::fs::create_dir_all(&target).map_err(|_| "The package directory could not be extracted.")?;
                continue;
            }
            if file.size() > MAX_FILE_BYTES || total.saturating_add(file.size()) > MAX_EXPANDED_BYTES {
                return Err("The expanded plugin package is too large.".into());
            }
            let mut contents = Vec::new();
            (&mut file).take(MAX_FILE_BYTES + 1).read_to_end(&mut contents)
                .map_err(|_| "A package file is corrupt or could not be read.")?;
            total = total.saturating_add(contents.len() as u64);
            if contents.len() as u64 > MAX_FILE_BYTES || total > MAX_EXPANDED_BYTES {
                return Err("The expanded plugin package is too large.".into());
            }
            std::fs::create_dir_all(target.parent().ok_or("The package path is invalid.")?)
                .map_err(|_| "The package directory could not be extracted.")?;
            let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(&target)
                .map_err(|_| "The package contains conflicting files.")?;
            output.write_all(&contents).map_err(|_| "The package file could not be extracted.")?;
            files.push(relative);
        }
        Ok(Self { directory, sha256, files })
    }
}

fn portable_path(name: &str) -> Result<PathBuf, String> {
    let invalid = || "The package contains an unsafe or nonportable path.".to_string();
    if name.len() > 1024 || name.contains(['\\', ':', '\0']) { return Err(invalid()); }
    let name = name.strip_suffix('/').unwrap_or(name);
    let parts: Vec<_> = name.split('/').collect();
    if parts.len() > 32 { return Err(invalid()); }
    for part in &parts {
        if part.is_empty() || *part == "." || *part == ".." || part.len() > 255
            || part.ends_with(['.', ' ']) || part.chars().any(|ch| ch.is_control() || "<>\"|?*".contains(ch)) {
            return Err(invalid());
        }
        let stem = part.split('.').next().unwrap_or_default().to_ascii_uppercase();
        if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9')) {
            return Err(invalid());
        }
    }
    Ok(parts.iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (path, body) in entries {
            writer.start_file(*path, SimpleFileOptions::default()).unwrap();
            writer.write_all(body).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn digest(bytes: &[u8]) -> Digest {
        Digest { algorithm: "sha256".into(), value: format!("{:x}", Sha256::digest(bytes)) }
    }

    #[test]
    fn verification_precedes_extraction_and_staging_is_removed_on_drop() {
        let parent = tempfile::tempdir().unwrap();
        let bytes = archive(&[("Guest/Plugin.pm", b"unexecuted source")]);
        let mut wrong = digest(&bytes);
        wrong.value = "0".repeat(64);
        assert!(StagedPackage::unpack(parent.path(), &bytes, &wrong).is_err());
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
        let staged = StagedPackage::unpack(parent.path(), &bytes, &digest(&bytes)).unwrap();
        let root = staged.root().to_owned();
        assert_eq!(std::fs::read(root.join("Guest/Plugin.pm")).unwrap(), b"unexecuted source");
        drop(staged);
        assert!(!root.exists());
    }

    #[test]
    fn traversal_links_case_collisions_and_device_names_are_rejected() {
        let parent = tempfile::tempdir().unwrap();
        for bad in ["../escape", "/absolute", "a/../../escape", "a\\b", "C:drive", "NUL.txt", "a/COM1", "a/..", "a/ ", "a//b"] {
            assert!(portable_path(bad).is_err(), "{bad}");
        }
        let collision = archive(&[("Guest/a", b"first"), ("Guest/A", b"second")]);
        assert!(StagedPackage::unpack(parent.path(), &collision, &digest(&collision)).is_err());
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer.add_symlink("link", "../outside", SimpleFileOptions::default()).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert!(StagedPackage::unpack(parent.path(), &bytes, &digest(&bytes)).is_err());
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
    }

    #[test]
    fn compressed_bombs_are_refused_before_writing_the_large_file() {
        let parent = tempfile::tempdir().unwrap();
        let bytes = archive(&[("large", &vec![0; MAX_FILE_BYTES as usize + 1])]);
        assert!(bytes.len() < MAX_PACKAGE_BYTES);
        assert!(StagedPackage::unpack(parent.path(), &bytes, &digest(&bytes)).is_err());
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
    }
}
