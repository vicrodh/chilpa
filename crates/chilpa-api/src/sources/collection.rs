//! Collection actions do not change the browser's location.
use super::*;

#[derive(Default)]
pub(super) struct Collection {
    serial: u64,
    first: String,
    pub(super) source: String,
    id: String,
    action: String,
    pub(super) document: SourceDocument,
    cursors: HashSet<String>,
    loading: bool,
    error: String,
}

impl Chilpa {
    pub(super) fn collection_json(&self) -> serde_json::Value {
        let c = &self.sources.collection;
        let entries: Vec<_> = c.document.listing.entries.iter().map(|entry| {
            let mut row = self.entry_json(&c.source, entry, self.now.path == entry_of(&c.source, &entry.id));
            row["presentation"] = serde_json::json!(c.document.presentation.get(&entry.id));
            row["quality_label"] = c.document.presentation.get(&entry.id)
                .map(|p| p.advertised_quality.label()).unwrap_or_default().into();
            row
        }).collect();
        serde_json::json!({"id": c.id, "loading": c.loading, "error": c.error, "entries": entries})
    }

    pub(super) fn collection_command(&mut self, action: &str, id: &str) {
        if action == "play_preview_track" {
            let c = &mut self.sources.collection;
            if c.loading || !c.document.listing.entries.iter().any(|e| e.id == id) { return; }
            c.first = id.into(); c.action = "play_collection".into();
            self.finish_collection(); return;
        }
        if action == "close_preview" {
            self.sources.collection.serial += 1;
            self.sources.collection.id.clear();
            self.sources.collection.loading = false;
            return;
        }
        let id = if id.is_empty() { self.sources.trail.last().map(|c| c.id.as_str()).unwrap_or_default() } else { id }.to_string();
        let Some((source, entry)) = self.entry_by_id(&id) else { return };
        if entry.kind != EntryKind::Folder { return; }
        let c = &mut self.sources.collection;
        if c.source == source && c.id == id && !c.loading && c.error.is_empty() && !c.document.listing.entries.is_empty() {
            c.first.clear();
            c.action = action.into();
            self.finish_collection();
            return;
        }
        *c = Collection { serial: c.serial + 1, source, id, action: action.into(), loading: true, ..Collection::default() };
        self.ask_collection(None);
    }

    fn ask_collection(&mut self, cursor: Option<String>) {
        let c = &self.sources.collection;
        let (serial, source, tx) = (c.serial, c.source.clone(), self.sources.tx.clone());
        let ask = Browse { at: c.id.clone(), cursor: cursor.clone(), language: self.language().into(), ..Browse::default() };
        self.ask_plugin(source, messages::op::BROWSE, messages::write(&ask), move |answer| {
            let result = answer.and_then(|bytes| messages::read::<SourceDocument>(&bytes))
                .and_then(|doc| doc.clean(cursor.as_deref()).map_err(str::to_string));
            let _ = tx.send(Answer::Collection { serial, result });
        });
    }

    pub(super) fn take_collection(&mut self, serial: u64, result: Result<SourceDocument, String>) {
        let c = &mut self.sources.collection;
        if serial != c.serial { return; }
        let document = match result {
            Ok(document) => document,
            Err(error) => { c.loading = false; c.error = error; return; }
        };
        c.document.presentation.extend(document.presentation);
        c.document.listing.entries.extend(document.listing.entries.into_iter().filter(|e| e.kind == EntryKind::Item && e.playable && !e.live));
        if let Some(cursor) = document.listing.next {
            if c.document.listing.entries.len() > 10000 || c.cursors.len() >= 100 || !c.cursors.insert(cursor.clone()) {
                c.loading = false;
                c.error = chilpa_i18n::t("This collection exceeds the current page limit.");
            } else { self.ask_collection(Some(cursor)); }
        } else {
            c.loading = false;
            self.finish_collection();
        }
    }

    fn finish_collection(&mut self) {
        let c = &self.sources.collection;
        if c.action == "preview" { return; }
        let (source, entries, action) = (c.source.clone(), c.document.listing.entries.clone(), c.action.clone());
        self.sources.collection.action = "preview".into();
        if entries.is_empty() { return; }
        let cards: Vec<_> = entries.iter().map(|e| Heard { external_id: e.id.clone(), title: e.title.clone(), subtitle: e.subtitle.clone(),
            image_url: e.image_url.clone().unwrap_or_default(), duration_ms: e.duration_ms, ..Heard::default() }).collect();
        if !self.library().is_some_and(|lib| lib.remember_heard_many(&source, &cards).is_ok()) {
            self.sources.collection.error = chilpa_i18n::t("it could not be saved"); return;
        }
        let index = entries.iter().position(|e| e.id == self.sources.collection.first).unwrap_or(0);
        let paths: Vec<_> = entries.iter().map(|e| entry_of(&source, &e.id)).collect();
        let (device, exact) = self.chosen_output();
        match action.as_str() {
            "queue_collection" => { self.enqueue(&paths, &device, exact); },
            "next_collection" => self.play_next(&paths, &device, exact),
            _ => { self.replace_queue(paths); self.spawn_from(index, &device, exact, None, crate::Relaunch::Track); },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page() -> SourceDocument {
        serde_json::from_value(serde_json::json!({"entries": [
            {"id":"notice", "title":"Track count", "kind":"item", "playable":false},
            {"id":"track:one", "title":"First", "kind":"item", "playable":true, "duration_ms":180000}
        ], "presentation":{"track:one":{"entity":"track", "track_number":1}}})).unwrap()
    }
    #[test]
    fn preview_keeps_the_browser_and_rejects_late_answers() {
        let mut app = Chilpa::default();
        app.sources.current = "fixture".into();
        app.sources.entries = vec![Entry { id: "album".into(), kind: EntryKind::Folder, ..Entry::default() }];
        app.sources.collection = Collection { serial: 2, source: "fixture".into(), id: "album".into(), action: "preview".into(), loading: true, ..Collection::default() };
        app.take_collection(1, Ok(page()));
        assert!(app.sources.collection.loading);
        app.take_collection(2, Ok(page()));
        assert!(!app.sources.collection.loading);
        assert!(app.sources.trail.is_empty());
        assert_eq!(app.sources.entries[0].id, "album");
        let snapshot = app.collection_json();
        assert_eq!(snapshot["entries"].as_array().unwrap().len(), 1);
        assert_eq!(snapshot["entries"][0]["duration"], "3:00");
        assert_eq!(snapshot["entries"][0]["presentation"]["track_number"], 1);
    }
    #[test]
    fn preview_errors_stop_loading_and_closing_invalidates_pending_work() {
        let mut app = Chilpa::default();
        app.sources.collection = Collection { serial: 2, id: "album".into(), loading: true, ..Collection::default() };
        app.take_collection(2, Err("request failed".into()));
        assert!(!app.sources.collection.loading);
        assert_eq!(app.collection_json()["error"], "request failed");
        app.collection_command("close_preview", "");
        app.take_collection(2, Ok(page()));
        assert!(app.sources.collection.id.is_empty());
        assert!(app.sources.collection.document.listing.entries.is_empty());
    }
}
