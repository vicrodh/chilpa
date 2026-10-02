//! Remote results use the same flyout as the library and keep opaque identities.
use crate::{Chilpa, FoundRow, Leads};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteFound {
    pub source: String, pub name: String, pub id: String, pub title: String,
    pub detail: String, pub art: String, pub duration_ms: u64, pub collection: bool,
}
impl Chilpa {
    pub fn search_source_catalogs(&mut self, text: &str, source: &str) {
        self.search = crate::search::SearchState {
            text: text.trim().into(), source: source.into(), keystroke: Some(std::time::Instant::now()),
            last_query: self.search.last_query, playing: self.search.playing.take(), ..Default::default()
        };
    }
    pub fn source_found_rows(&self, scope: &str) -> Vec<FoundRow> {
        let mut rows = Vec::new();
        for reply in &self.search.replies {
            let mut group = Vec::new();
            let entry = |id: String, title: String, detail: String, art: String, duration_ms: u64, collection: bool| FoundRow::Remote(RemoteFound {
                source: reply.source.clone(), name: reply.source_name.clone(), id, title, detail, art, duration_ms, collection,
            });
            if matches!(scope, "all" | "albums") {
                for album in &reply.results.albums {
                    group.push(entry(album.external_id.clone(), album.title.clone(), album.artist.clone(), album.artwork_url.clone().unwrap_or_default(), 0, true));
                }
            }
            if matches!(scope, "all" | "songs") {
                for track in &reply.results.tracks {
                    group.push(entry(track.external_id.clone(), track.title.clone(), [track.artist.clone(), track.album.clone(), track.quality.describe()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "), track.artwork_url.clone().unwrap_or_default(), track.duration_ms, false));
                }
            }
            if !reply.problem.is_empty() { rows.push(FoundRow::Header(format!("{}: {}", reply.source_name, reply.problem))); }
            if !group.is_empty() { rows.push(FoundRow::Header(reply.source_name.clone())); rows.extend(group); }
        }
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
