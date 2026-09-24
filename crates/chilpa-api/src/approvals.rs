//! What the user approved of each plugin (ADR-008).
//!
//! A plugin does not run until someone said yes. What is kept is not a bare
//! "yes" but **what it asked for when it was told yes**: the plugin's kind
//! and its capabilities. If a new version asks for anything more, the
//! approval stops holding and the question is put again, and what is new can
//! be named instead of a bare "it changed, approve it again".
//!
//! It lives in a text file next to the plugins, readable on purpose: anyone
//! who wants to know what they authorized can read it without the app.

use chilpa_plugin::manifest::{Asked, Manifest};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The file, next to the plugins' folders.
const FILE: &str = "approvals.json";

/// What was approved, by plugin id.
#[derive(Default)]
pub struct Approvals {
    path: PathBuf,
    approved: BTreeMap<String, Asked>,
}

impl Approvals {
    /// Reads whatever is there. A file that does not exist means nothing has
    /// been approved, which is right for a fresh install; an unreadable one
    /// means the same, because the alternative would be assuming permissions
    /// nobody gave.
    pub fn load(root: &Path) -> Self {
        let path = root.join(FILE);
        let approved = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self { path, approved }
    }

    /// True if what the manifest asks for today fits in what was approved.
    pub fn covers(&self, manifest: &Manifest) -> bool {
        match self.approved.get(&manifest.id) {
            Some(before) => manifest.asks_for().beyond(before).is_empty(),
            None => false,
        }
    }

    /// What it asks for beyond what was approved. Empty if there is nothing
    /// new; everything it asks for if it was never approved.
    pub fn beyond(&self, manifest: &Manifest) -> Vec<String> {
        // One never approved is compared against "nothing approved", and not
        // down a separate path: keeping them apart meant a brand new plugin
        // never said where it sends you to grant permission, because only
        // the other path added that line.
        let nothing = Asked { kind: manifest.kind, capabilities: Vec::new(), auth: None };
        let before = self.approved.get(&manifest.id).unwrap_or(&nothing);
        manifest.asks_for().beyond(before)
    }

    /// True if anything was ever approved for that id, whether or not it is
    /// what it asks for today. It serves to say "it asks for something new"
    /// instead of "it is new".
    pub fn knows(&self, id: &str) -> bool {
        self.approved.contains_key(id)
    }

    /// Records that the user approved what that plugin asks for now.
    pub fn approve(&mut self, manifest: &Manifest) -> std::io::Result<()> {
        self.approved.insert(manifest.id.clone(), manifest.asks_for());
        self.write()
    }

    /// Withdraws the approval. The plugin stops running at the next startup
    /// and what it asks for is put to the user again.
    pub fn revoke(&mut self, id: &str) -> std::io::Result<()> {
        self.approved.remove(id);
        self.write()
    }

    fn write(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&self.approved)
            .unwrap_or_else(|_| "{}".to_string());
        std::fs::write(&self.path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str, capabilities: &str) -> Manifest {
        Manifest::parse(&format!(
            r#"{{"id":"{id}","name":"One","version":"1","api_version":1,
                 "kind":"source","capabilities":[{capabilities}]}}"#
        ))
        .unwrap()
    }

    fn temporal(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("chilpa-approvals-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn nothing_is_approved_until_someone_says_so() {
        let dir = temporal("new");
        let a = Approvals::load(&dir);
        let m = manifest("com.a.b", r#"{"type":"storage"}"#);
        assert!(!a.covers(&m), "a fresh install approves nothing by itself");
        assert!(!a.knows("com.a.b"));
        assert_eq!(a.beyond(&m).len(), 1, "and what it asks for can be named");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn what_was_approved_survives_a_restart() {
        let dir = temporal("again");
        let m = manifest("com.a.b", r#"{"type":"storage"}"#);
        Approvals::load(&dir).approve(&m).unwrap();
        assert!(Approvals::load(&dir).covers(&m), "it was read back");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn asking_for_more_than_was_approved_asks_again() {
        let dir = temporal("mas");
        let before = manifest("com.a.b", r#"{"type":"storage"}"#);
        let mut a = Approvals::load(&dir);
        a.approve(&before).unwrap();

        // The new version wants to reach the internet, and nobody approved that.
        let now = manifest(
            "com.a.b",
            r#"{"type":"storage"},{"type":"network","domains":["api.example.com"]}"#,
        );
        assert!(!a.covers(&now));
        let added = a.beyond(&now);
        assert_eq!(added.len(), 1, "only what is new, not everything over: {added:?}");
        assert!(added[0].contains("api.example.com"), "{added:?}");
        assert!(a.knows("com.a.b"), "it is not a new plugin: it is one that asks for more");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_version_that_asks_for_the_same_keeps_its_approval() {
        let dir = temporal("same");
        let mut a = Approvals::load(&dir);
        a.approve(&manifest("com.a.b", r#"{"type":"storage"}"#)).unwrap();
        let newer = Manifest::parse(
            r#"{"id":"com.a.b","name":"One","version":"2","api_version":1,
                "kind":"source","capabilities":[{"type":"storage"}]}"#,
        )
        .unwrap();
        assert!(a.covers(&newer), "bumping a version without asking for more does not ask again");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn revoking_asks_again_from_scratch() {
        let dir = temporal("revoke");
        let m = manifest("com.a.b", r#"{"type":"storage"}"#);
        let mut a = Approvals::load(&dir);
        a.approve(&m).unwrap();
        a.revoke("com.a.b").unwrap();
        assert!(!a.covers(&m));
        assert!(!Approvals::load(&dir).covers(&m), "and it was written down");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_unreadable_file_approves_nothing() {
        // The opposite would be assuming permissions nobody gave.
        let dir = temporal("broken");
        std::fs::write(dir.join(FILE), "this is not json").unwrap();
        let a = Approvals::load(&dir);
        assert!(!a.covers(&manifest("com.a.b", "")));
        let _ = std::fs::remove_dir_all(dir);
    }
}
