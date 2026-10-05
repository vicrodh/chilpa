//! Cache the opaque parent supplied by browsing, without decoding provider IDs.
use super::*;
use std::hash::{Hash, Hasher};

fn path(source: &str) -> std::path::PathBuf {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hash);
    chilpa_dirs::cache().join("source-parents").join(format!("{:016x}.json", hash.finish()))
}
fn read(source: &str) -> BTreeMap<String, Entry> {
    std::fs::read(path(source)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn remember(file: &std::path::Path, parent: &Entry, entries: &[Entry]) {
    let mut parents: BTreeMap<String, Entry> = std::fs::read(file).ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    for entry in entries.iter().filter(|e| e.kind == EntryKind::Item && !e.live) {
        parents.insert(entry.id.clone(), parent.clone());
    }
    let Some(directory) = file.parent() else { return };
    if std::fs::create_dir_all(directory).is_err() { return; }
    let Ok(mut temporary) = tempfile::NamedTempFile::new_in(directory) else { return };
    if serde_json::to_writer(temporary.as_file_mut(), &parents).is_ok() { let _ = temporary.persist(file); }
}

impl Chilpa {
    pub(super) fn remember_episode_parents(&self, entries: &[Entry]) {
        if self.sources.medium != "podcasts" { return; }
        let Some(parent) = self.sources.trail.last().and_then(|c| c.entry.as_ref()) else { return };
        remember(&path(&self.sources.current), parent, entries);
    }

    pub(super) fn episode_parent(&self, source: &str, id: &str) -> Option<Entry> {
        read(source).remove(id)
    }

    pub(super) fn open_episode_parent(&mut self) {
        let Some((source, episode)) = &self.sources.detail else { return };
        let Some(parent) = self.episode_parent(source, &episode.id) else { return };
        let source = source.clone();
        self.sources.current = source;
        self.sources.medium = "podcasts".into();
        self.sources.music = false;
        self.sources.media_tab = "home".into();
        self.sources.play_after_load = false;
        self.sources.text.clear();
        self.sources.filter.clear();
        self.sources.trail = vec![Crumb { id: parent.id.clone(), title: parent.title.clone(), entry: Some(parent), presentation: None }];
        self.read_saved();
        self.load_listing();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parents_survive_navigation_and_keep_opaque_episode_ids() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("parents.json");
        let show = Entry { id: "show ?/#".into(), title: "Podcast".into(), kind: EntryKind::Folder, ..Default::default() };
        let episode = Entry { id: "episode:opaque #/?".into(), kind: EntryKind::Item, ..Default::default() };
        remember(&file, &show, &[episode.clone()]);
        let other = Entry { id: "another".into(), title: "Another show".into(), ..show.clone() };
        remember(&file, &other, &[Entry { id: "second".into(), ..episode.clone() }]);
        let saved: BTreeMap<String, Entry> = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        assert_eq!(saved[&episode.id], show);
        assert_eq!(saved["second"], other);
        assert_ne!(path("one"), path("two"), "sources cannot share episode identity");
    }
}
