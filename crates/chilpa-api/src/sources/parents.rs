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
        self.open_media_parent(source.clone(), parent);
    }

    pub(crate) fn source_card_parent(&self, id: i64) -> Option<(String, Entry)> {
        let card = self.catalog.card(id)?;
        self.planned_media_parent(card)
    }

    pub(crate) fn planned_media_parent(&self, card: crate::source_catalog::store::Card) -> Option<(String, Entry)> {
        let id = card.handle.as_ref().map(|h| h.id.clone())
            .or_else(|| card.identity.strip_prefix("handle:").map(str::to_owned))?;
        if !self.is_media_panel(&card.plugin) { return None; }
        let known = self.sources.roots.get(&card.plugin).into_iter().flatten().find(|e| e.id == id);
        let found = self.search.replies.iter().filter(|r| r.source == card.plugin)
            .flat_map(|r| &r.results.albums).find(|a| a.external_id == id);
        let entry = known.cloned().unwrap_or_else(|| Entry {
            id, title: card.title, subtitle: found.map(|a| a.artist.clone()).unwrap_or(card.artist),
            image_url: found.and_then(|a| a.artwork_url.clone()).or_else(|| Some(if card.art.is_empty() { card.image } else { card.art })),
            kind: EntryKind::Folder, ..Default::default()
        });
        Some((card.plugin, entry))
    }

    fn is_media_panel(&self, id: &str) -> bool {
        if let Some(plugins) = self.plugins.as_ref().and_then(|plugins| plugins.try_lock().ok()) {
            if !plugins.panels().iter().any(|(source, _, _)| source == id) { return false; }
            let layout = self.natives.layout(id).or_else(|| plugins.layouts.get(id).cloned());
            return plugins.worker_domains(id).is_none() && layout.is_none_or(|layout| layout.sections.is_empty());
        }
        self.sources.panels.iter().any(|panel| panel.id == id && !panel.music)
    }

    pub(crate) fn playing_media_parent(&self) -> Option<(String, Entry)> {
        if let Some((source, id)) = of_plugin(&self.now.path) {
            if let Some(parent) = self.episode_parent(&source, &id) { return Some((source, parent)); }
        }
        let (track, _) = self.catalog.track_by_path(&self.now.path)?;
        self.source_card_parent(track.album_id)
    }

    pub(crate) fn open_media_parent(&mut self, source: String, parent: Entry) {
        self.sources.medium = self.sources.media_kinds.get(&source).cloned().unwrap_or_default();
        self.sources.current = source;
        self.sources.music = false;
        self.sources.media_tab = "home".into();
        self.sources.play_after_load = false;
        self.sources.text.clear();
        self.sources.filter.clear();
        self.sources.columns.clear();
        self.sources.picks.clear();
        self.sources.trail = vec![Crumb { id: parent.id.clone(), title: parent.title.clone(), entry: Some(parent), presentation: None }];
        self.read_saved();
        self.load_listing();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_show_routes_to_its_media_header_and_music_stays_an_album() {
        let mut app = Chilpa::new();
        app.sources.panels.push(Panel { id: "fixture-media".into(), name: "Media".into(), icon: String::new(), icon_image: String::new(), icon_tint: false, music: false, artists: false, children: vec![] });
        app.sources.media_kinds.insert("fixture-media".into(), "podcasts".into());
        let row = crate::source_search::RemoteFound { source: "fixture-media".into(), name: "Media".into(), id: "opaque show /?#".into(), title: "A show".into(), detail: "Publisher".into(), art: "https://example.invalid/show.jpg".into(), duration_ms: 0, collection: true, entity: "album".into(), item: None, no_audio: false };
        let crate::Leads::Screen("album", id) = app.act_on_remote_found(&row, "open") else { panic!("missing search destination") };
        let planned = app.plan_route("album", id).unwrap();
        assert_eq!(planned.route.screen, crate::navigation::Screen::Source);
        assert_eq!(planned.route.key, row.source);
        assert!(planned.needs_preparation());
        assert!(app.sources.current.is_empty(), "I leave the current source untouched before acknowledgement");
        assert!(app.sources.trail.is_empty(), "I defer the destination's listing until preparation");
        assert!(app.prepare_planned_route(planned));
        let route = app.route_to("album", id).unwrap();
        assert_eq!(route.screen, crate::navigation::Screen::Source);
        assert_eq!(route.key, row.source);
        let about = app.sources.trail.last().unwrap().entry.as_ref().unwrap();
        assert_eq!(about.id, row.id);
        assert_eq!(about.title, row.title);
        assert_eq!(about.image_url.as_deref(), Some(row.art.as_str()));
        assert_eq!(app.sources.medium, "podcasts");
        let episode = crate::source_search::RemoteFound {
            collection: false, item: Some(serde_json::json!({"kind":"track", "ref":"episode", "title":"Episode", "album_ref":format!("handle:{}", row.id)})),
            ..row.clone()
        };
        let track_id = app.catalog.adopt(&row.source, None, &episode);
        app.now.path = app.catalog.tracks_by_ids(&[track_id])[0].0.path.clone();
        let (source, parent) = app.playing_media_parent().expect("catalog playback keeps its parent without visiting episode details");
        assert_eq!(source, row.source);
        assert_eq!(parent.id, row.id);
        app.sources.detail = None;
        app.sources_command("playing_parent", "", "");
        assert_eq!(app.sources.trail.last().unwrap().id, row.id);
        app.sources.panels[0].music = true;
        assert!(app.source_card_parent(id).is_none());
        assert_eq!(app.route_to("album", id).unwrap().screen, crate::navigation::Screen::Album);
    }

    #[test]
    fn planned_music_routes_keep_their_types_without_preparing_a_media_parent() {
        use crate::navigation::Screen;
        let app = Chilpa::new();
        let local = app.plan_route("album", 7).unwrap();
        assert_eq!(local.route.screen, Screen::Album);
        assert_eq!(local.route.id, 7);
        assert!(!local.needs_preparation());
        for kind in ["album", "playlist", "artist", "label"] {
            let id = app.catalog.adopt("fixture-music", None, &crate::source_search::RemoteFound {
                collection: true, item: Some(serde_json::json!({
                    "kind": kind, "ref": format!("{kind}:opaque /?#"), "title": "Record",
                    "image": "https://example.invalid/cover.jpg"
                })), ..Default::default()
            });
            let plan = app.plan_route("album", id).unwrap();
            assert!(!plan.needs_preparation());
            if matches!(kind, "artist" | "label") {
                assert_eq!(plan.route.screen, Screen::Source);
                assert_eq!(plan.route.key, format!("fixture-music card:{id}"));
            } else {
                assert_eq!(plan.route.screen, Screen::Album);
                assert_eq!(plan.route.id, id);
            }
        }
        assert!(app.sources.current.is_empty());
        assert!(app.sources.trail.is_empty());
    }

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
