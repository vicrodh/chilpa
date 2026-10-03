//! Installing a native plugin from a repository the listener added.
//!
//! A native catalog entry names a package and its SHA-256. The package was
//! downloaded and checked against it before it reaches here (`stage`). Here
//! its manifest is read, its id must be the entry's and what its runtime
//! runs must be in it; then it takes its place in the plugins folder,
//! replacing an earlier version whole. Installing never approves: the
//! plugin waits switched off until the listener turns it on, which is
//! where its permissions are granted.

use crate::StagedPackage;
use chilpa_plugin::{catalog::CatalogEntry, Manifest, Runtime};
use serde_json::json;
use std::path::{Path, PathBuf};

/// Where a package keeps its manifest: at its root, or inside the one
/// folder an archive often wraps everything in.
fn package_root(package: &StagedPackage) -> Result<PathBuf, String> {
    let manifests: Vec<&PathBuf> = package.files().iter().filter(|path| path.file_name().is_some_and(|name| name == "manifest.json")).collect();
    match manifests.as_slice() {
        [one] if one.components().count() <= 2 => Ok(package.root().join(one.parent().unwrap_or(Path::new("")))),
        [] => Err("The package has no plugin manifest.".into()),
        _ => Err("The package has more than one plugin manifest.".into()),
    }
}

/// The manifest a package brings, checked against the entry that named it.
pub(crate) fn check(package: &StagedPackage, entry: &CatalogEntry) -> Result<(PathBuf, Manifest), String> {
    let (root, manifest) = inspect(package)?;
    if manifest.id != entry.id {
        return Err("The package is a different plugin than the catalog says.".into());
    }
    Ok((root, manifest))
}

pub(crate) fn inspect(package: &StagedPackage) -> Result<(PathBuf, Manifest), String> {
    let root = package_root(package)?;
    let text = std::fs::read_to_string(root.join("manifest.json")).map_err(|_| "The plugin manifest could not be read.")?;
    let runtime = serde_json::from_str::<serde_json::Value>(&text).ok()
        .map(|value| match value["runtime"].as_str() { Some("process") => Runtime::Process, _ => Runtime::Wasm })
        .ok_or("The plugin manifest is not JSON.")?;
    let manifest = Manifest::parse_for_runtime(&text, runtime).map_err(|error| error.to_string())?;
    let program = match runtime { Runtime::Process => "plugin", _ => "plugin.wasm" };
    if !root.join(program).is_file() {
        return Err("The package does not contain the plugin's program.".into());
    }
    Ok((root, manifest))
}

/// Puts a checked package in the plugins folder, in place of any earlier
/// version, and notes where it came from. Returns the plugin's id.
pub(crate) fn install(plugins_dir: &Path, package: &StagedPackage, entry: &CatalogEntry, repository: &str) -> Result<String, String> {
    let (root, manifest) = check(package, entry)?;
    let id = manifest.id.clone();
    let target = plugins_dir.join(&id);
    let incoming = plugins_dir.join(format!(".incoming-{id}"));
    let outgoing = plugins_dir.join(format!(".outgoing-{id}"));
    let _ = std::fs::remove_dir_all(&incoming);
    let _ = std::fs::remove_dir_all(&outgoing);
    copy_tree(&root, &incoming).map_err(|_| "The plugin could not be copied into place.")?;
    if let Some(icon) = crate::plugin_icons::included(&incoming, &manifest.icon) {
        std::fs::write(incoming.join(crate::plugin_icons::INCLUDED_ICON).with_extension(icon.extension), icon.bytes)
            .map_err(|_| "The plugin icon could not be installed.")?;
    }
    #[cfg(unix)]
    if manifest.runtime == Runtime::Process {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(incoming.join("plugin"), std::fs::Permissions::from_mode(0o755))
            .map_err(|_| "The plugin could not be made runnable.")?;
    }
    let source = json!({"repository": repository, "package": entry.package, "version": manifest.version, "sha256": package.sha256});
    std::fs::write(incoming.join("source.json"), serde_json::to_vec_pretty(&source).unwrap_or_default())
        .map_err(|_| "The plugin could not be copied into place.")?;
    // The earlier version leaves whole, and only then the new one enters:
    // the loader never sees a folder half of one and half of the other.
    if target.exists() {
        std::fs::rename(&target, &outgoing).map_err(|_| "The earlier version could not be replaced.")?;
    }
    if std::fs::rename(&incoming, &target).is_err() {
        let _ = std::fs::rename(&outgoing, &target);
        return Err("The plugin could not be put in place.".into());
    }
    let _ = std::fs::remove_dir_all(&outgoing);
    Ok(id)
}

/// The version installed of a plugin, when it is installed.
pub(crate) fn installed_version(plugins_dir: &Path, id: &str) -> Option<String> {
    let text = std::fs::read_to_string(plugins_dir.join(id).join("manifest.json")).ok()?;
    serde_json::from_str::<serde_json::Value>(&text).ok()?["version"].as_str().map(str::to_string)
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_plugin::catalog::Digest;
    use sha2::{Digest as _, Sha256};
    use std::io::Write;

    fn package(files: &[(&str, &[u8])], staging: &Path) -> StagedPackage {
        let mut bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            for (name, contents) in files {
                zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
                zip.write_all(contents).unwrap();
            }
            zip.finish().unwrap();
        }
        let digest = Digest { algorithm: "sha256".into(), value: format!("{:x}", Sha256::digest(&bytes)) };
        StagedPackage::unpack(staging, &bytes, &digest).unwrap()
    }

    fn entry(id: &str) -> CatalogEntry {
        CatalogEntry { id: id.into(), name: "One".into(), version: "0.2.0".into(), description: String::new(), category: String::new(),
            author: String::new(), repository: String::new(), package: "https://example.org/one.zip".into(),
            digest: Digest { algorithm: "sha256".into(), value: String::new() }, adapter: "native".into(), problem: String::new() }
    }

    fn manifest(id: &str, version: &str) -> String {
        format!(r#"{{"id": "{id}", "name": "One", "version": "{version}", "api_version": 1, "kind": "source", "runtime": "process",
            "capabilities": [{{"type": "register_source"}}]}}"#)
    }

    #[test]
    fn a_package_takes_its_place_and_replaces_the_one_before() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        std::fs::create_dir_all(&plugins).unwrap();
        let first = package(&[("one/manifest.json", manifest("app.test.one", "0.1.0").as_bytes()), ("one/plugin", b"v1")], &dir.path().join("staging"));
        assert_eq!(install(&plugins, &first, &entry("app.test.one"), "repo").unwrap(), "app.test.one");
        assert_eq!(installed_version(&plugins, "app.test.one").as_deref(), Some("0.1.0"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(plugins.join("app.test.one/plugin")).unwrap().permissions().mode() & 0o111, 0o111, "it can run");
        }
        let second = package(&[("manifest.json", manifest("app.test.one", "0.2.0").as_bytes()), ("plugin", b"v2")], &dir.path().join("staging"));
        install(&plugins, &second, &entry("app.test.one"), "repo").unwrap();
        assert_eq!(std::fs::read(plugins.join("app.test.one/plugin")).unwrap(), b"v2");
        assert_eq!(installed_version(&plugins, "app.test.one").as_deref(), Some("0.2.0"));
        assert!(plugins.join("app.test.one/source.json").is_file(), "where it came from is noted");
        assert_eq!(std::fs::read_dir(&plugins).unwrap().count(), 1, "nothing is left behind");
    }

    #[test]
    fn a_package_that_is_not_what_its_entry_says_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("staging");
        let other = package(&[("manifest.json", manifest("app.test.other", "0.1.0").as_bytes()), ("plugin", b"x")], &staging);
        assert!(check(&other, &entry("app.test.one")).is_err(), "another plugin's id");
        let empty = package(&[("manifest.json", manifest("app.test.one", "0.1.0").as_bytes())], &staging);
        assert!(check(&empty, &entry("app.test.one")).is_err(), "no program in it");
        let none = package(&[("plugin", b"x")], &staging);
        assert!(check(&none, &entry("app.test.one")).is_err(), "no manifest");
    }
}
