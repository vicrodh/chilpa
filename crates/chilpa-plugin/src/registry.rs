//! The registry: which plugins exist, which work and which are in the corner.
//!
//! It is the piece that joins the manifest, the host and the watchdog, and
//! the one that keeps ADR-003's promise: *before breaking Chilpa, it does
//! not load.* A plugin that does not pass is noted with its reason and the
//! rest carry on; startup is never aborted because of one.

use crate::host::{Host, Plugin, PluginError, Services};
use crate::limits::Limits;
use crate::manifest::{Asked, Kind, Manifest};
use crate::watchdog::{State, Watchdog};
use crate::Reply;
use std::collections::BTreeMap;

/// A plugin found on disk.
pub struct Installed {
    pub id: String,
    pub name: String,
    /// What it says about itself, if its manifest could be read. It is
    /// needed even when it does not load: it is what the user is shown so
    /// they can approve or refuse.
    pub manifest: Option<Manifest>,
    /// The plugin, ready, or `None` if it did not load or nobody approved it.
    pub plugin: Option<Plugin>,
    /// Why it did not load, if it did not.
    pub problem: Option<String>,
    /// True if it is waiting for the user to approve what it asks for.
    pub pending: bool,
}

impl Installed {
    pub fn loaded(&self) -> bool {
        self.plugin.is_some()
    }

    /// What it asks for, in words, for asking.
    ///
    /// It comes from comparing against "nothing approved", which is the
    /// same reckoning approval does: that way there are not two lists of
    /// what a plugin asks for that can drift apart.
    pub fn asks(&self) -> Vec<String> {
        let Some(m) = &self.manifest else { return Vec::new() };
        let nothing = Asked { kind: m.kind, capabilities: Vec::new(), auth: None };
        m.asks_for().beyond(&nothing)
    }
}

/// Every plugin in this session.
#[derive(Default)]
pub struct Registry {
    plugins: BTreeMap<String, Installed>,
    watchdog: Watchdog,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tries to load a plugin. It never fails outward: what could not be done
    /// is noted and things carry on.
    ///
    /// `approved` receives the already read manifest and answers whether the
    /// user approved that very thing. If it says no, the wasm **is not
    /// compiled**: the plugin is noted as pending and its code is never
    /// touched (ADR-008). The registry does not know where approvals live;
    /// the caller sorts that out.
    ///
    /// Returns true if it ended up usable.
    pub fn install(
        &mut self,
        host: &Host,
        manifest_json: &str,
        wasm: &[u8],
        approved: impl FnOnce(&Manifest) -> bool,
    ) -> bool {
        let manifest = match Manifest::parse(manifest_json) {
            Ok(m) => m,
            Err(e) => {
                // The manifest did not pass, but its JSON may still parse: the
                // id and the name are salvaged so it shows up in the list as
                // what it is and not as a string of dots.
                let (id, name) = salvage(manifest_json);
                self.watchdog.quarantine(&id, &e.to_string());
                self.plugins.insert(
                    id.clone(),
                    Installed {
                        id,
                        name,
                        manifest: None,
                        plugin: None,
                        problem: Some(e.to_string()),
                        pending: false,
                    },
                );
                return false;
            }
        };
        let (id, name) = (manifest.id.clone(), manifest.name.clone());

        if !approved(&manifest) {
            self.plugins.insert(
                id.clone(),
                Installed {
                    id,
                    name,
                    manifest: Some(manifest),
                    plugin: None,
                    problem: None,
                    pending: true,
                },
            );
            return false;
        }

        let shown = manifest.clone();
        match host.load(manifest, wasm) {
            Ok(p) => {
                self.plugins.insert(
                    id.clone(),
                    Installed {
                        id,
                        name,
                        manifest: Some(shown),
                        plugin: Some(p),
                        problem: None,
                        pending: false,
                    },
                );
                true
            }
            Err(e) => {
                let reason = e.to_string();
                self.watchdog.quarantine(&id, &reason);
                self.plugins.insert(
                    id.clone(),
                    Installed {
                        id,
                        name,
                        manifest: Some(shown),
                        plugin: None,
                        problem: Some(reason),
                        pending: false,
                    },
                );
                false
            }
        }
    }

    /// What a plugin asks for, for asking the user. Empty if it is not there
    /// or its manifest could not be read.
    pub fn asks(&self, id: &str) -> Vec<String> {
        self.plugins.get(id).map(Installed::asks).unwrap_or_default()
    }

    /// What would have to be approved of that plugin, exactly as it is kept.
    pub fn asked(&self, id: &str) -> Option<Asked> {
        self.plugins.get(id)?.manifest.as_ref().map(Manifest::asks_for)
    }

    /// The installed plugins, loaded or not.
    pub fn installed(&self) -> impl Iterator<Item = &Installed> {
        self.plugins.values()
    }

    /// The ones usable right now.
    pub fn usable(&self) -> Vec<&str> {
        self.plugins
            .values()
            .filter(|i| i.loaded() && self.watchdog.usable(&i.id))
            .map(|i| i.id.as_str())
            .collect()
    }

    /// The ones that work now and are of that kind, with their name, in the
    /// order they were installed.
    ///
    /// The order matters: one plugin cannot cut in front of another, and
    /// the map is sorted by id, so the list is always the same.
    pub fn usable_of_kind(&self, kind: Kind) -> Vec<(String, String)> {
        self.plugins
            .values()
            .filter(|i| self.watchdog.usable(&i.id))
            .filter_map(|i| {
                let p = i.plugin.as_ref()?;
                (p.manifest().kind == kind).then(|| {
                    let name = if i.name.is_empty() { i.id.clone() } else { i.name.clone() };
                    (i.id.clone(), name)
                })
            })
            .collect()
    }

    /// The ones that work now and asked to put an entry of their own in the
    /// window: id, name and icon hint, in the same steady order.
    pub fn panels(&self) -> Vec<(String, String, String)> {
        self.plugins
            .values()
            .filter(|i| self.watchdog.usable(&i.id))
            .filter_map(|i| {
                let manifest = i.plugin.as_ref()?.manifest();
                manifest.has(&crate::manifest::Capability::RegisterPanel).then(|| {
                    let name = if i.name.is_empty() { i.id.clone() } else { i.name.clone() };
                    (i.id.clone(), name, manifest.icon.clone())
                })
            })
            .collect()
    }

    pub fn state(&self, id: &str) -> State {
        let Some(inst) = self.plugins.get(id) else {
            return State::Quarantined { reason: chilpa_i18n::t("it is not installed") };
        };
        if inst.pending {
            return State::PendingApproval;
        }
        self.watchdog.state(id)
    }

    /// Takes it out of quarantine. Only the user calls it.
    pub fn reactivate(&mut self, id: &str) {
        self.watchdog.reactivate(id);
    }

    pub fn quarantined(&self) -> Vec<(String, String)> {
        self.watchdog.quarantined()
    }

    /// Asks a plugin for something and keeps score of how it went.
    ///
    /// An error from the plugin (JSON with `error`) does **not** count as a
    /// failure: "I did not find that song" is an answer, not a breakage.
    /// Neither does being asked for an operation it does not have. What
    /// counts is blowing up, going over the limits or not answering.
    pub fn call(
        &mut self,
        host: &Host,
        services: &Services,
        id: &str,
        operation: &str,
        input: &[u8],
        limits: Limits,
    ) -> Result<Reply, PluginError> {
        if !self.watchdog.usable(id) {
            return Err(PluginError::NotAllowed(chilpa_i18n::t_args(
                "calling \"{}\", which is in quarantine",
                &[id],
            )));
        }
        let Some(inst) = self.plugins.get(id) else {
            return Err(PluginError::NotAllowed(chilpa_i18n::t_args(
                "calling \"{}\", which is not installed",
                &[id],
            )));
        };
        if inst.pending {
            return Err(PluginError::NotAllowed(chilpa_i18n::t_args(
                "calling \"{}\", which nobody has approved",
                &[id],
            )));
        }
        let Some(plugin) = &inst.plugin else {
            return Err(PluginError::NotAllowed(chilpa_i18n::t_args(
                "calling \"{}\", which did not load",
                &[id],
            )));
        };

        match plugin.call(host, services, operation, input, limits) {
            Ok(r) => {
                self.watchdog.success(id);
                Ok(r)
            }
            // An operation the plugin does not export is something it does
            // not do, not something it did wrong. Chilpa asks every source
            // for its playlists, and one that has none to give was being
            // counted towards quarantine for it.
            Err(PluginError::MissingExport(name)) if name == operation => {
                Err(PluginError::MissingExport(name))
            }
            Err(e) => {
                self.watchdog.failure(id, &e.to_string());
                Err(e)
            }
        }
    }
}

/// Whatever can be salvaged from a manifest that did not pass.
fn salvage(json: &str) -> (String, String) {
    let v = serde_json::from_str::<serde_json::Value>(json).ok();
    let take = |field: &str| {
        v.as_ref().and_then(|v| v.get(field)?.as_str().map(str::to_string)).unwrap_or_default()
    };
    let id = take("id");
    let id = if id.is_empty() { chilpa_i18n::mark("(unreadable manifest)").to_string() } else { id };
    (id, take("name"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_manifest_is_recorded_and_does_not_bring_the_registry_down() {
        let host = Host::new().unwrap();
        let mut r = Registry::new();
        assert!(!r.install(&host, "this is not json", b"", |_| true));
        assert_eq!(r.installed().count(), 1);
        assert!(!r.usable().is_empty() == false);
        let inst = r.installed().next().unwrap();
        assert!(!inst.loaded());
        assert!(inst.problem.is_some());
    }

    #[test]
    fn a_broken_manifest_that_has_an_id_is_recorded_under_it() {
        let host = Host::new().unwrap();
        let mut r = Registry::new();
        // It has an id but is missing everything else.
        assert!(!r.install(&host, r#"{"id":"com.a.b"}"#, b"", |_| true));
        assert_eq!(r.installed().next().unwrap().id, "com.a.b");
        assert!(matches!(r.state("com.a.b"), State::Quarantined { .. }));
    }

    #[test]
    fn one_that_failed_validation_keeps_its_name() {
        let host = Host::new().unwrap();
        let mut r = Registry::new();
        // The JSON parses; what does not pass is the API version.
        let json = r#"{"id":"com.a.b","name":"It is called this","version":"1",
                       "api_version":42,"kind":"source"}"#;
        assert!(!r.install(&host, json, b"", |_| true));
        let inst = r.installed().next().unwrap();
        assert_eq!(inst.name, "It is called this", "it shows up in the list by its name");
        assert!(inst.problem.as_ref().unwrap().contains("version"));
    }

    #[test]
    fn nothing_can_be_asked_of_one_that_is_not_there() {
        let r = Registry::new();
        assert!(matches!(r.state("com.does.not.exist"), State::Quarantined { .. }));
    }
}
