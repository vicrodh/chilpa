//! Moving a reviewed package into the plugin tree does not approve execution.

use crate::{PluginInspection, StagedPackage};
use chilpa_plugin::{catalog::CatalogEntry, Capability, Kind, Manifest, Runtime};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use std::{io::Write, path::{Path, PathBuf}};

pub struct PreparedPlugin {
    package: StagedPackage,
    relative: PathBuf,
    manifest: Manifest,
    namespace: String,
    origin: serde_json::Value,
    icon: Option<crate::plugin_icons::IconImage>,
}

impl StagedPackage {
    /// Domains come from the permission editor, not from the inspection
    /// worker. A role is inferred from registrations observed by the adapter.
    pub fn prepare_extension(self, repository: &str, entry: &CatalogEntry,
        inspection: &PluginInspection, _domains: Vec<String>) -> Result<PreparedPlugin, String> {
        if entry.adapter != "perl" || !entry.problem.is_empty() {
            return Err("This catalog entry cannot use the selected adapter.".into());
        }
        let (root, metadata) = self.extension()?;
        if metadata.version != entry.version {
            return Err("The package version does not match the catalog.".into());
        }
        let relative = root.strip_prefix(self.root()).map_err(|_| "The package entry point is invalid.")?.to_owned();
        for reserved in ["manifest.json", "worker.json", "origin.json", crate::plugin_icons::INCLUDED_ICON, ".host-icon.svg"] {
            if root.join(reserved).exists() { return Err("The package contains a reserved host descriptor.".into()); }
        }
        let identity = serde_json::to_vec(&(repository, &entry.id)).map_err(|_| "The plugin identity is invalid.")?;
        let id = format!("compat.{:x}", Sha256::digest(identity));
        let source = ["browse", "search", "resolve"].iter().any(|role| inspection.features.get(*role) == Some(&true));
        let mut capabilities = vec![Capability::Storage];
        capabilities.push(Capability::Network { domains: vec!["*".into()] });
        if source { capabilities.extend([Capability::RegisterSource, Capability::RegisterPanel]); }
        if !inspection.settings_pages.is_empty() {
            capabilities.push(Capability::GuestCredentials);
        }
        let manifest = Manifest { id, name: entry.name.clone(), version: metadata.version, author: entry.author.clone(), api_version: chilpa_plugin::API_VERSION,
            kind: if source { Kind::Source } else { Kind::Metadata }, runtime: Runtime::Bridge, auth: None, capabilities,
            delivers_audio: inspection.features.get("resolve") == Some(&true), description: entry.description.clone(), icon: String::new(), folder_icon: String::new() };
        let encoded = serde_json::to_string(&manifest).map_err(|_| "The plugin descriptor could not be created.")?;
        let manifest = Manifest::parse_for_runtime(&encoded, Runtime::Bridge).map_err(|error| error.to_string())?;
        let origin = json!({"repository":repository,"catalog_entry":entry,"package_sha256":self.sha256,
            "inspection":inspection});
        let icon = crate::plugin_icons::included(&root, &metadata.icon);
        Ok(PreparedPlugin { package: self, relative, manifest, namespace: metadata.namespace, origin, icon })
    }
}

impl PreparedPlugin {
    pub fn manifest(&self) -> &Manifest { &self.manifest }

    /// Installs a new identity only. Updates need a separate transactional
    /// replacement so a running worker never observes partially changed code.
    /// The registry will still mark this installation as awaiting approval.
    pub fn install_new(self, plugins: &Path) -> Result<String, String> {
        let id = self.manifest.id.clone();
        self.place(plugins, false)?.commit()?;
        Ok(id)
    }

    pub(crate) fn replace(self, plugins: &Path) -> Result<crate::plugin_replacement::Replacement, String> {
        self.place(plugins, true)
    }

    fn place(self, plugins: &Path, replacing: bool) -> Result<crate::plugin_replacement::Replacement, String> {
        // Revalidate because callers may edit the permission proposal.
        let encoded = serde_json::to_vec_pretty(&self.manifest).map_err(|_| "The plugin descriptor is invalid.")?;
        let manifest = Manifest::parse_for_runtime(std::str::from_utf8(&encoded).map_err(|_| "The plugin descriptor is invalid.")?, Runtime::Bridge)
            .map_err(|error| error.to_string())?;
        std::fs::create_dir_all(plugins).map_err(|_| "The plugin directory could not be created.")?;
        let target = plugins.join(&manifest.id);
        if !replacing && target.symlink_metadata().is_ok() { return Err("This plugin is already installed.".into()); }
        let temporary = tempfile::Builder::new().prefix(".install-").tempdir_in(plugins)
            .map_err(|_| "The installation could not be prepared.")?;
        for path in self.package.files() {
            let Ok(relative) = path.strip_prefix(&self.relative) else { continue };
            let to = temporary.path().join(relative);
            std::fs::create_dir_all(to.parent().ok_or("The package path is invalid.")?)
                .map_err(|_| "The package directory could not be installed.")?;
            std::fs::copy(self.package.root().join(path), &to).map_err(|_| "The package file could not be installed.")?;
        }
        write_new(&temporary.path().join("worker.json"), &serde_json::to_vec(&json!({"namespace":self.namespace})).unwrap())?;
        write_new(&temporary.path().join("origin.json"), &serde_json::to_vec_pretty(&self.origin).map_err(|_| "The package origin could not be saved.")?)?;
        write_new(&temporary.path().join("manifest.json"), &encoded)?;
        if let Some(icon) = &self.icon { write_new(&temporary.path().join(format!(".host-icon.{}", icon.extension)), &icon.bytes)?; }
        if !replacing && target.symlink_metadata().is_ok() { return Err("This plugin is already installed.".into()); }
        crate::plugin_replacement::Replacement::begin(plugins, &manifest.id, temporary.path())
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)
        .map_err(|_| "The host descriptor could not be created.")?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|_| "The host descriptor could not be saved.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, io::Cursor};
    use zip::{write::SimpleFileOptions, ZipWriter};

    #[test]
    fn original_code_is_installed_without_granting_execution_or_overwriting_an_installation() {
        let temporary = tempfile::tempdir().unwrap();
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in [("Release/install.xml", "<extension><module>Plugins::Guest::Plugin</module><version>1</version></extension>"),
            ("Release/Plugin.pm", "original guest source")] {
            archive.start_file(name, SimpleFileOptions::default()).unwrap();
            archive.write_all(body.as_bytes()).unwrap();
        }
        let bytes = archive.finish().unwrap().into_inner();
        let digest = chilpa_plugin::catalog::Digest { algorithm: "sha256".into(), value: format!("{:x}", Sha256::digest(&bytes)) };
        let entry = CatalogEntry { id: "Guest".into(), name: "Guest".into(), version: "1".into(), description: String::new(),
            category: String::new(), author: String::new(), repository: String::new(), package: "https://example.org/package.zip".into(),
            digest: digest.clone(), adapter: "perl".into(), problem: String::new() };
        let inspection = PluginInspection { features: BTreeMap::from([("browse".into(), true), ("resolve".into(), true)]),
            settings_pages: vec!["settings.html".into()], limitations: vec![], diagnostics: BTreeMap::new() };
        let prepare = || StagedPackage::unpack(&temporary.path().join("staging"), &bytes, &digest).unwrap()
            .prepare_extension("repository-id", &entry, &inspection, vec!["example.org".into()]).unwrap();
        let prepared = prepare();
        assert_eq!(prepared.manifest().kind, Kind::Source);
        assert!(prepared.manifest().has(&Capability::RegisterPanel));
        assert!(prepared.manifest().has(&Capability::GuestCredentials));
        let installed = temporary.path().join("plugins");
        let id = prepared.install_new(&installed).unwrap();
        assert_eq!(std::fs::read(installed.join(&id).join("Plugin.pm")).unwrap(), b"original guest source");
        let manifest = std::fs::read_to_string(installed.join(&id).join("manifest.json")).unwrap();
        let mut registry = chilpa_plugin::Registry::new();
        assert!(!registry.install_adapter(Runtime::Bridge, &manifest, |_| false, |_| panic!("unapproved code must not run")));
        assert!(matches!(registry.state(&id), chilpa_plugin::State::PendingApproval));
        assert!(prepare().install_new(&installed).is_err());
        assert_eq!(std::fs::read(installed.join(&id).join("Plugin.pm")).unwrap(), b"original guest source");
        assert!(std::fs::read_dir(&installed).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().starts_with('.')));
    }
}
