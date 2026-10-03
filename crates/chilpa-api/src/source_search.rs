//! Remote results use the same flyout as the library and keep opaque identities.
use crate::{Chilpa, FoundRow, Leads};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteFound {
    pub source: String, pub name: String, pub id: String, pub title: String,
    pub detail: String, pub art: String, pub duration_ms: u64, pub collection: bool,
    pub entity: String, pub item: Option<serde_json::Value>,
}
/// How many of a source's results the flyout shows before "more".


impl Chilpa {
    pub fn search_source_catalogs(&mut self, text: &str, source: &str) {
        self.search = crate::search::SearchState {
            text: text.trim().into(), source: source.into(), keystroke: Some(std::time::Instant::now()),
            last_query: self.search.last_query, playing: self.search.playing.take(), ..Default::default()
        };
    }
    /// Remote entities retain source identity while sharing the local sections.
    pub fn source_found_rows(&self, scope: &str) -> Vec<FoundRow> {
        let mut rows = Vec::new();
        for reply in &self.search.replies {
            let mut group = Vec::new();
            // The cover on disk once it is there; until then it is asked for.
            let entry = |id: String, title: String, detail: String, art: String, duration_ms: u64, collection: bool| {
                let kept = crate::pictures::kept(&art);
                if kept.is_none() { self.catalog.want_image(&reply.source, &art); }
                FoundRow::Remote(RemoteFound {
                    source: reply.source.clone(), name: reply.source_name.clone(), id, title, detail, art: kept.unwrap_or_default(), duration_ms, collection, entity: if collection { "album" } else { "track" }.into(), item: None,
                })
            };
            if reply.entities.is_empty() && matches!(scope, "all" | "albums") {
                for album in &reply.results.albums {
                    // Described as a record of the library is: kind, artist, year.
                    let (artist, said) = crate::source_catalog::store::split_year(&album.artist);
                    let mut detail = vec![chilpa_i18n::t(chilpa_i18n::mark("Album")), artist];
                    detail.extend(album.year.or(said).map(|y| y.to_string()));
                    detail.retain(|part| !part.is_empty());
                    group.push(entry(album.external_id.clone(), album.title.clone(), detail.join(" · "), album.artwork_url.clone().unwrap_or_default(), 0, true));
                }
            }
            if reply.entities.is_empty() && matches!(scope, "all" | "songs") {
                for track in &reply.results.tracks {
                    group.push(entry(track.external_id.clone(), track.title.clone(), [track.artist.clone(), track.album.clone(), track.quality.describe()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "), track.artwork_url.clone().unwrap_or_default(), track.duration_ms, false));
                }
            }
            if reply.entities.is_empty() && matches!(scope, "all" | "playlists") {
                for playlist in &reply.results.playlists {
                    if let FoundRow::Remote(mut row) = entry(playlist.external_id.clone(), playlist.name.clone(), playlist.description.clone(), playlist.artwork_url.clone().unwrap_or_default(), 0, true) {
                        row.entity = "playlist".into(); group.push(FoundRow::Remote(row));
                    }
                }
            }
            for item in &reply.entities {
                let kind = item["kind"].as_str().unwrap_or_default();
                let wanted = match kind { "artist" => "artists", "album" => "albums", "track" => "songs", "playlist" => "playlists", _ => continue };
                if scope != "all" && scope != wanted { continue; }
                let image = item["image"].as_str().unwrap_or_default();
                self.catalog.want_image(&reply.source, image);
                group.push(FoundRow::Remote(RemoteFound {
                    source: reply.source.clone(), name: reply.source_name.clone(),
                    id: item["ref"].as_str().unwrap_or_default().into(), title: item["title"].as_str().unwrap_or_default().into(),
                    detail: [item["artist"].as_str().unwrap_or_default(), item["album"].as_str().unwrap_or_default()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),
                    art: crate::pictures::kept(image).unwrap_or_default(), duration_ms: item["duration_ms"].as_u64().unwrap_or_default(),
                    collection: kind != "track", entity: kind.into(), item: Some(item.clone()),
                }));
            }
            rows.extend(group);
            if !reply.problem.is_empty() { rows.push(FoundRow::Header(format!("{}: {}", reply.source_name, reply.problem))); }
        }
        rows
    }
    pub fn unified_found_rows(&mut self, found: &crate::Found, scope: &str) -> Vec<FoundRow> {
        let remote = self.source_found_rows(scope);
        let mut rows = Vec::new();
        for (kind, name, key) in [(2, "Artists", "artists"), (3, "Albums", "albums"), (4, "Songs", "songs"), (6, "Playlists", "playlists")] {
            if scope != "all" && scope != key { continue; }
            let mut section: Vec<_> = crate::found_rows(found, key).into_iter().filter(|r| r.kind() == kind).collect();
            if kind == 6 && crate::search::searches_source(&self.search.source, "local") && !self.search.text.trim().is_empty() {
                let words: Vec<_> = self.search.text.to_lowercase().split_whitespace().map(str::to_owned).collect();
                section.extend(self.local_playlists().into_iter().filter(|p| words.iter().all(|w| p.name.to_lowercase().contains(w))).take(50).map(FoundRow::Playlist));
            }
            section.extend(remote.iter().filter(|r| r.kind() == kind).cloned());
            if !section.is_empty() { rows.push(FoundRow::Header(chilpa_i18n::t(name))); rows.extend(section); }
        }
        rows.extend(remote.into_iter().filter(|r| r.kind() == 0));
        rows
    }
    pub(crate) fn act_on_remote_found(&mut self, row: &RemoteFound, action: &str) -> Leads {
        // The host is not waited on: while it answers someone else, the load
        // the catalog last saw is the one the search ran in.
        let epoch = self.plugins.as_ref().and_then(|p| p.try_lock().ok().map(|p| p.epoch(&row.source)));
        let adopted = self.catalog.adopt(&row.source, epoch, row);
        if row.collection {
            // A record found opens on the record page, as a local one does.
            if action == "open" { return Leads::Screen("album", adopted); }
        } else if matches!(action, "open" | "play" | "queue") {
            let card = chilpa_library::store::progress::Heard { external_id: row.id.clone(), title: row.title.clone(), subtitle: row.detail.clone(), image_url: row.art.clone(), duration_ms: row.duration_ms, ..Default::default() };
            if let Some(library) = self.library() { let _ = library.remember_heard(&row.source, &card); }
            let path = crate::source_opener::entry_of(&row.source, &row.id);
            if action == "queue" { self.enqueue_here(&[path]); } else { self.play_from(&[path], 0); }
        }
        Leads::Nowhere
    }
}

/// Native search uses the native protocol and releases the compatibility host lock.
pub(crate) fn native_reply(natives: &std::sync::Arc<crate::natives::Natives>, id: &str, name: &str, text: &str, limit: usize) -> crate::search::SourceReply {
    let mut reply = crate::search::SourceReply { source: id.into(), source_name: name.into(), results: Default::default(), entities: Vec::new(), problem: String::new(), delivers_audio: true };
    match natives.call(id, "search", serde_json::json!({"text":text,"limit":limit}), std::time::Duration::from_secs(30)) {
        Ok(value) => {
            for key in ["artists", "albums", "tracks", "playlists"] {
                reply.entities.extend(value[key].as_array().into_iter().flatten().filter(|item| item["ref"].as_str().is_some_and(|s| !s.is_empty())).cloned());
            }
            for item in reply.entities.iter().filter(|item| item["kind"] == "track") {
                if let Ok(track) = serde_json::from_value(serde_json::json!({"external_id":item["ref"],"title":item["title"],
                    "artist":item["artist"].as_str().unwrap_or_default(),"album":item["album"].as_str().unwrap_or_default(),
                    "duration_ms":item["duration_ms"].as_u64().unwrap_or_default(),"isrc":item["isrc"],"artwork_url":item["image"]})) { reply.results.tracks.push(track); }
            }
        }
        Err(error) => reply.problem = error,
    }
    reply
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn local_and_remote_playlists_share_a_section_and_obey_source_selection() {
        let mut api = Chilpa::new();
        api.library = Some(chilpa_library::Library::in_memory().unwrap());
        let id = api.create_playlist("Road trips").unwrap();
        api.search.text = "road".into(); api.search.source = "all".into();
        api.search.replies.push(crate::search::SourceReply { source: "source-a".into(), source_name: "A".into(), results: Default::default(), problem: String::new(), delivers_audio: true,
            entities: vec![json!({"kind":"playlist","ref":"playlist:3","title":"Road streams"})] });
        let rows = api.unified_found_rows(&crate::Found::default(), "playlists");
        assert_eq!(rows.iter().map(FoundRow::kind).collect::<Vec<_>>(), [0,6,6]);
        assert_eq!(api.act_on_found(&rows, 1, "open"), Leads::Screen("playlist", id));
        api.search.source = "source-a".into();
        assert_eq!(api.unified_found_rows(&crate::Found::default(), "playlists").len(), 2);
    }
    #[test]
    fn sources_share_entity_sections_and_artists_keep_their_native_route() {
        let mut api = Chilpa::new();
        api.search.replies.push(crate::search::SourceReply { source: "source-a".into(), source_name: "A".into(), results: Default::default(), problem: String::new(), delivers_audio: true,
            entities: vec![json!({"kind":"artist","ref":"artist:9","title":"Remote artist"}),json!({"kind":"album","ref":"album:2","title":"Release"}),json!({"kind":"playlist","ref":"playlist:3","title":"List"})] });
        let found = crate::Found { artists: vec![crate::Artist { name: "Local artist".into(), album_count:1,track_count:2,artwork_path:String::new() }], ..Default::default() };
        let rows = api.unified_found_rows(&found, "all");
        assert_eq!(rows.iter().map(FoundRow::kind).collect::<Vec<_>>(), [0,2,2,0,3,0,6]);
        let FoundRow::Remote(artist) = &rows[2] else { panic!("native artist missing") };
        let Leads::Screen(_, id) = api.act_on_remote_found(artist, "open") else { panic!("native artist has no route") };
        assert_eq!(api.route_to("album", id).unwrap().key, format!("source-a card:{id}"));
        assert_eq!(api.unified_found_rows(&found, "artists").len(), 3);
    }
}
