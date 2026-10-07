//! Native page documents keep provider navigation opaque to the host.
use super::{Catalog, now, store::Shelf};
use crate::natives::Natives;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

fn ingest(catalog: &Catalog, plugin: &str, key: &str, value: &Value) {
    let stamp = now();
    let mut inner = catalog.lock();
    let mut shelf = Shelf { title:value["title"].as_str().unwrap_or_default().into(), document:value.clone(), read_at:stamp, epoch:1, ..Default::default() };
    for item in value["items"].as_array().into_iter().flatten() {
        if item["kind"] == "track" { shelf.tracks.extend(inner.store.take_item_track(plugin, item, None, stamp)); }
        else { shelf.cards.extend(inner.store.take_item(plugin, item, stamp)); }
    }
    inner.shelf_errors.remove(key);
    inner.store.shelves.insert(key.into(), shelf);
    inner.dirty = true;
}
pub(super) fn read(catalog: &Catalog, natives: &Arc<Natives>, plugin: &str, reference: &str) -> Result<(), String> {
    let parent = reference.split('|').next().unwrap_or(reference);
    let current = catalog.stored_shelf(&format!("{plugin} native:{reference}"));
    if reference != parent && current.as_ref().is_some_and(|s| s.epoch > 0 && s.read_at > 0 && now().saturating_sub(s.read_at) < 30) { return Ok(()); }
    let refresh = current.is_some_and(|s| s.read_at == 0);
    let answer = natives.call(plugin, "page.get", json!({"ref":parent,"refresh":refresh}), Duration::from_secs(90))?;
    if !natives.is_native(plugin) { return Err("The plugin was disabled.".into()); }
    ingest_document(catalog, plugin, parent, &answer);
    Ok(())
}
pub(super) fn ingest_document(catalog: &Catalog, plugin: &str, reference: &str, answer: &Value) {
    let key = format!("{plugin} native:{reference}");
    let mut document = answer.clone();
    let personal = document["personal"] == true;
    if let Some(shelves) = document["shelves"].as_array_mut() {
        for shelf in shelves {
            shelf["personal"] = json!(personal);
            let id = shelf["id"].as_str().unwrap_or_default();
            let child = format!("{key}|{id}");
            if let Some(tags) = shelf["tags"].as_array().cloned() {
                let mut choices = vec![];
                for mut tag in tags {
                    let slug = tag["slug"].as_str().unwrap_or_default();
                    let tagged_key = format!("{child}#{slug}");
                    let rows: Vec<_> = shelf["items"].as_array().into_iter().flatten().filter(|i| i["tags"].as_array().into_iter().flatten().any(|t| t["slug"] == slug)).cloned().collect();
                    ingest(catalog, plugin, &tagged_key, &json!({"title":tag["name"],"items":rows}));
                    tag["key"] = json!(tagged_key); choices.push(tag);
                }
                shelf["tags"] = json!(choices);
            }
            if shelf["extra_tracks"].is_array() {
                let tracks_key = format!("{child}-tracks");
                ingest(catalog, plugin, &tracks_key, &json!({"title":shelf["title"],"items":shelf["extra_tracks"]}));
                shelf["tracks_key"] = json!(tracks_key);
            }
            // A shelf that loaded more keeps it: a page read again brings
            // its first page, and if that is still the start of what was
            // loaded, the loaded rest stays, with where it goes on from.
            if let Some(kept) = loaded_beyond(catalog, &child, shelf) {
                shelf["items"] = kept.0;
                shelf["more"] = kept.1;
            }
            ingest(catalog, plugin, &child, shelf);
            shelf["key"] = json!(child);
            shelf["tracks"] = json!(shelf["kind"] == "track");
            shelf["round"] = json!(shelf["kind"] == "artist");
            if let Some(more) = shelf["more"].as_str() { shelf["more_key"] = json!(format!("{plugin} native:{more}")); }
        }
    }
    if let Some(tabs) = document["tabs"].as_array_mut() {
        for tab in tabs { if let Some(reference) = tab["ref"].as_str() { tab["key"] = json!(format!("{plugin} native:{reference}")); } }
    }
    if document["header"].is_object() {
        catalog.lock().store.take_item(plugin, &document["header"], now());
    }
    ingest(catalog, plugin, &key, &document);
 }
fn decorate_header(catalog: &Catalog, plugin: &str, value: &mut Value) {
    if !value.is_object() { return; }
    value["plugin"] = json!(plugin);
    let kind = value["kind"].as_str().unwrap_or_default();
    if matches!(kind,"album" | "artist" | "playlist" | "label") {
        if let Some(shelf) = catalog.shelf(&format!("{plugin} section:{kind}s")) {
            value["favorite"] = json!(catalog.cards(&shelf.cards).iter().any(|c| c.identity == value["ref"].as_str().unwrap_or_default()));
        }
    }
    if let Some(image) = value["image"].as_str() {
        catalog.want_image(plugin, image);
        value["art"] = json!(crate::pictures::kept(image).and_then(|path| url::Url::from_file_path(path).ok()).map(|url| url.to_string()).unwrap_or_default());
    }
}
// I keep navigation cards in the catalog while presenting radio as a header action.
/// What a shelf had loaded beyond the first page of a fresh read of its
/// document, when that first page is still the start of it: the whole
/// list and the reference it goes on from. `None` when nothing was loaded
/// beyond it, or the page changed.
fn loaded_beyond(catalog: &Catalog, child: &str, fresh: &Value) -> Option<(Value, Value)> {
    let inner = catalog.lock();
    let kept = &inner.store.shelves.get(child)?.document;
    let (kept_items, fresh_items) = (kept["items"].as_array()?, fresh["items"].as_array()?);
    if kept_items.len() <= fresh_items.len() || fresh_items.is_empty() {
        return None;
    }
    let same_start = fresh_items.iter().zip(kept_items).all(|(a, b)| a["ref"] == b["ref"]);
    same_start.then(|| (kept["items"].clone(), kept["more"].clone()))
}

fn page_shelves(catalog: &Catalog, plugin: &str, reference: &str, header: &mut Value, shelves: &Value, mixed_artist: bool) -> Value {
    let radio = format!("radio:{reference}");
    let artist = header["artist_ref"].as_str().unwrap_or_default().to_string();
    let mut rows = Vec::new();
    let page_key = format!("{plugin} native:{reference}");
    for shelf in shelves.as_array().into_iter().flatten() {
        let mut shelf = shelf.clone();
        // Its "Load more" is on its way: the button says so and waits.
        let child = format!("{page_key}|{}", shelf["id"].as_str().unwrap_or_default());
        shelf["loading"] = json!(catalog.lock().running.contains(&super::Job::Shelf(child)));
        let items = shelf["items"].as_array();
        if items.is_some_and(|items| items.len() == 1 && items[0]["ref"] == radio) {
            if let Some(card) = catalog.lock().store.cards.values().find(|card| card.plugin == plugin && card.identity == radio) {
                header["radio_id"] = json!(card.id);
                continue;
            }
        }
        if mixed_artist && !artist.is_empty() && shelf["kind"] == "album" && shelf["more"] == artist { continue; }
        rows.push(shelf);
    }
    json!(rows)
}
impl crate::Chilpa {
    pub(crate) fn append_native_section(&self, key: &str) {
        let (plugin, reference) = super::split(key);
        let Some(reference) = reference.strip_prefix("native:") else { return };
        let Some((parent, section)) = reference.split_once('|') else { return };
        if !self.natives.has_pages(plugin) { return; }
        let job = super::Job::Shelf(key.into());
        let current = {
            let mut inner = self.catalog.lock();
            if inner.running.contains(&job) { return; }
            let Some(current) = inner.store.shelves.get(key).cloned() else { return };
            if current.document["more"].as_str().is_none() { return; }
            inner.running.insert(job.clone());
            current
        };
        let (catalog, natives, plugin, key, parent, section) = (self.catalog.clone(), self.natives.clone(), plugin.to_string(), key.to_string(), parent.to_string(), section.to_string());
        std::thread::spawn(move || {
            let result = natives.call(&plugin, "page.get", json!({"ref":current.document["more"]}), Duration::from_secs(60));
            let mut child = current.document;
            let outcome = match result {
                Ok(answer) if natives.is_native(&plugin) => {
                    let mut items = child["items"].as_array().cloned().unwrap_or_default();
                    let mut seen: std::collections::HashSet<_> = items.iter().filter_map(|v| v["ref"].as_str().map(str::to_string)).collect();
                    items.extend(answer["items"].as_array().into_iter().flatten().filter(|v| v["ref"].as_str().is_some_and(|r| seen.insert(r.into()))).cloned());
                    child["items"] = json!(items); child["more"] = answer["next"].clone(); child["error"] = json!("");
                    ingest(&catalog, &plugin, &key, &child);
                    Ok(())
                },
                Ok(_) => Err("The plugin was disabled.".to_string()),
                Err(error) => Err(error.to_string()),
            };
            let mut inner = catalog.lock();
            inner.running.remove(&job);
            if let Err(error) = outcome { child["error"] = json!(error); }
            if let Some(page) = inner.store.shelves.get_mut(&format!("{plugin} native:{parent}")) {
                if let Some(rows) = page.document["shelves"].as_array_mut() {
                    if let Some(row) = rows.iter_mut().find(|row| row["id"] == section) {
                        row["items"] = child["items"].clone(); row["more"] = child["more"].clone(); row["error"] = child["error"].clone();
                    }
                }
            }
            inner.dirty = true;
            drop(inner); catalog.changed();
        });
    }

    pub fn native_reference_card(&self, plugin: &str, reference: &str) -> i64 {
        if !self.natives.has_pages(plugin) { return 0; }
        let Some((kind, id)) = reference.split_once(':') else { return 0 };
        if !matches!(kind, "album" | "playlist") || id.is_empty() { return 0; }
        let mut inner = self.catalog.lock();
        if let Some(card) = inner.store.cards.values().find(|card| card.plugin == plugin && card.identity == reference) { return card.id; }
        let card = inner.store.take_item(plugin, &json!({"kind":kind,"ref":reference,"title":""}), now()).unwrap_or_default();
        inner.dirty = true;
        card
    }

    pub fn native_image(&self, plugin: &str, image: &str) -> String {
        if !self.natives.is_native(plugin) { return String::new(); }
        self.catalog.want_image(plugin, image);
        crate::pictures::kept(image).unwrap_or_default()
    }

    pub fn native_card_info(&mut self, id: i64) -> String {
        let Some(card) = self.catalog.card(id) else { return "{}".into(); };
        if !self.natives.has_pages(&card.plugin) { return "{}".into(); }
        let mut info = card.metadata.clone();
        if card.contain_art() { info["art_fit"] = json!("contain"); }
        let quality = self.album_card_quality(id);
        info["quality_label"] = json!(quality.short()); info["quality_tier"] = json!(quality.tier.rank());
        decorate_header(&self.catalog, &card.plugin, &mut info);
        info["art"] = json!(url::Url::from_file_path(self.catalog.art_of_card(id)).ok().map(|url| url.to_string()).unwrap_or_default());
        if card.kind == super::store::CardKind::Playlist && info["editable"] == true {
            info["entries"] = json!(self.catalog.tracks(id).unwrap_or_default().iter().map(|t| json!({"ref":t.metadata["ref"],"member":t.metadata["member"],"title":t.title,"artist":t.artist,"available":t.playable})).collect::<Vec<_>>());
        }
        if card.kind == super::store::CardKind::Album { if let Some(document) = self.catalog.shelf(&format!("{} native:{}", card.plugin, card.identity)) { info["shelves"] = page_shelves(&self.catalog, &card.plugin, &card.identity, &mut info, &document.document["shelves"], true); } }
        info.to_string()
    }
    pub fn source_artist_key(&self, album: i64, path: &str) -> Option<String> {
        let (plugin, metadata) = if album < 0 {
            let card = self.catalog.card(album)?; (card.plugin, card.metadata)
        } else { let (track, _) = self.catalog.track_by_path(path)?; (track.plugin, track.metadata) };
        let reference = metadata["artist_ref"].as_str()?;
        self.natives.has_pages(&plugin).then(|| format!("{plugin} native:{reference}"))
    }

    pub(crate) fn native_page_view(&mut self, plugin: &str, reference: &str) -> String {
        let key = format!("{plugin} native:{reference}");
        let shelf = self.catalog.shelf(&key).unwrap_or_default();
        let mut document = if shelf.document.is_object() { shelf.document.clone() } else { json!({"shelves":[]}) };
        document["music"] = json!(true);
        document["native_page"] = json!(true);
        document["plugin"] = json!(plugin);
        document["reference"] = json!(reference);
        document["title"] = json!(shelf.title);
        document["loading"] = json!(self.catalog.loading(&key));
        document["error"] = json!(self.catalog.shelf_error(&key).unwrap_or_default());
        document["cards"] = json!(shelf.cards.len());
        document["tracks"] = json!(shelf.tracks.len());
        let shelves = document["shelves"].clone();
        document["shelves"] = page_shelves(&self.catalog, plugin, reference, &mut document["header"], &shelves, false);
        decorate_header(&self.catalog, plugin, &mut document["header"]);
        if let Some(rows) = document["shelves"].as_array_mut() { for row in rows { decorate_header(&self.catalog, plugin, &mut row["header"]); } }
        let kind = shelf.document["items"].as_array().and_then(|items| items.first()).and_then(|item| item["kind"].as_str()).unwrap_or("album");
        document["entity"] = json!(kind);
        document["round"] = json!(kind == "artist");
        document["albumsOnly"] = json!(shelf.tracks.is_empty());
        document.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn radio_moves_to_album_and_artist_headers_without_removing_other_shelves() {
        for reference in ["album:3", "artist:7"] {
            let catalog = Catalog::new();
            let mut header = json!({"ref":reference,"artist_ref":"artist:7"});
            let shelves = json!([
                {"id":"artist","kind":"album","more":"artist:7","items":[]},
                {"id":"related","kind":"album","items":[]},
                {"id":"radio","kind":"playlist","items":[{"kind":"playlist","ref":format!("radio:{reference}"),"title":"Radio"}]}
            ]);
            ingest_document(&catalog,"fixture",reference,&json!({"shelves":shelves}));
            let rows = page_shelves(&catalog,"fixture",reference,&mut header,&shelves,reference.starts_with("album:"));
            assert_eq!(rows.as_array().unwrap().len(), if reference.starts_with("album:") { 1 } else { 2 });
            assert_eq!(rows.as_array().unwrap().last().unwrap()["id"], "related");
            let radio = catalog.card(header["radio_id"].as_i64().unwrap()).unwrap();
            assert_eq!(radio.identity, format!("radio:{reference}"));
            assert_eq!(radio.kind, super::super::store::CardKind::Playlist);
        }
    }
    /// What a shelf loaded beyond its first page stays when the page is read
    /// again and still starts the same way; a page that changed its start
    /// is taken as it comes. And a shelf whose "Load more" is on its way
    /// says so to the window.
    #[test]
    fn a_shelf_keeps_what_it_loaded_across_a_rereading() {
        let catalog = Catalog::new();
        let record = |n: u32| json!({"kind":"album","ref":format!("album:{n}"),"title":format!("Record {n}")});
        let first = json!({"shelves":[{"id":"album","kind":"album","items":[record(1),record(2)],"more":"artist:7:album:2"}]});
        ingest_document(&catalog,"fixture","artist:7",&first);
        // The "Load more" answer, as append_native_section leaves the shelf.
        let child = "fixture native:artist:7|album";
        let loaded = json!({"id":"album","kind":"album","items":[record(1),record(2),record(3),record(4)],"more":"artist:7:album:4"});
        ingest(&catalog,"fixture",child,&loaded);

        ingest_document(&catalog,"fixture","artist:7",&first);
        let kept = catalog.lock().store.shelves[child].document.clone();
        assert_eq!(kept["items"].as_array().unwrap().len(), 4, "the loaded records stay");
        assert_eq!(kept["more"], "artist:7:album:4", "and so does where it goes on from");
        let page = catalog.lock().store.shelves["fixture native:artist:7"].document.clone();
        assert_eq!(page["shelves"][0]["items"].as_array().unwrap().len(), 4, "the page's own row says the same");

        let changed = json!({"shelves":[{"id":"album","kind":"album","items":[record(9),record(2)],"more":"artist:7:album:2"}]});
        ingest_document(&catalog,"fixture","artist:7",&changed);
        let kept = catalog.lock().store.shelves[child].document.clone();
        assert_eq!(kept["items"].as_array().unwrap().len(), 2, "a page that starts differently is taken whole");

        let mut header = json!({"ref":"artist:7"});
        let rows = page_shelves(&catalog,"fixture","artist:7",&mut header,&changed["shelves"],false);
        assert_eq!(rows[0]["loading"], false);
        catalog.lock().running.insert(super::super::Job::Shelf(child.into()));
        let rows = page_shelves(&catalog,"fixture","artist:7",&mut header,&changed["shelves"],false);
        assert_eq!(rows[0]["loading"], true, "its load more is on its way");
    }

    #[test]
    fn a_label_page_enriches_the_existing_image_less_card() {
        let catalog = Catalog::new();
        let id = catalog.lock().store.take_item("fixture",&json!({"kind":"label","ref":"label:1","title":"Label"}),1).unwrap();
        ingest_document(&catalog,"fixture","label:1",&json!({"header":{"kind":"label","ref":"label:1","title":"Label","image":"https://example.org/logo.png"},"shelves":[]}));
        let card = catalog.card(id).unwrap();
        assert_eq!(card.image, "https://example.org/logo.png");
        assert_eq!(catalog.lock().store.cards.len(), 1);
    }
    #[test]
    fn single_playlist_images_fit_even_when_older_plugins_omit_the_hint() {
        let mut store = super::super::store::Store::default();
        let mut item = json!({"kind":"playlist","ref":"playlist:1","title":"List","image":"https://example.org/wide.jpg"});
        let id = store.take_item("fixture",&item,1).unwrap();
        assert!(store.cards[&id].contain_art());
        item["art_fit"] = json!("cover");
        store.take_item("fixture",&item,2);
        assert!(!store.cards[&id].contain_art());
        item["kind"] = json!("album"); item["ref"] = json!("album:1"); item.as_object_mut().unwrap().remove("art_fit");
        let album = store.take_item("fixture",&item,3).unwrap();
        assert!(!store.cards[&album].contain_art());
    }
    #[test]
    fn playlist_edits_invalidate_only_the_affected_sources_cards() {
        let catalog = Catalog::new();
        let ids: Vec<_> = ["first", "second"].iter().map(|plugin| {
            let mut inner = catalog.lock();
            let id = inner.store.take_item(plugin, &json!({"kind":"playlist","ref":"list:1","title":"List"}), 1).unwrap();
            let card = inner.store.cards.get_mut(&id).unwrap();
            card.tracks = Some(vec![]);
            card.read_at = 42;
            id
        }).collect();
        catalog.invalidate_native_cards("first", &[json!("list:1")]);
        assert!(catalog.card(ids[0]).unwrap().tracks.is_none());
        assert_eq!(catalog.card(ids[0]).unwrap().read_at, 0);
        assert!(catalog.card(ids[1]).unwrap().tracks.is_some());
        assert_eq!(catalog.card(ids[1]).unwrap().read_at, 42);
    }
    #[test]
    fn listening_invalidates_only_the_accounts_personal_pages() {
        let catalog = Catalog::new();
        for (plugin, reference, personal) in [("fixture","home",true),("fixture","artist:7",false),("another","home",true)] {
            ingest_document(&catalog,plugin,reference,&json!({"title":"Page","personal":personal,"shelves":[{"id":"row","items":[]}]}));
        }
        catalog.expire_personal_pages("fixture");
        assert_eq!(catalog.stored_shelf("fixture native:home").unwrap().read_at,1);
        assert_eq!(catalog.stored_shelf("fixture native:home|row").unwrap().read_at,1);
        assert!(catalog.stored_shelf("fixture native:artist:7").unwrap().read_at > 0);
        assert!(catalog.stored_shelf("another native:home").unwrap().read_at > 0);
        assert!(catalog.revisions().0 > 0);
    }
    #[test]
    fn sections_tags_and_spotlight_keep_independent_navigation_and_tracks() {
        let catalog = Catalog::new();
        ingest_document(&catalog, "fixture", "discover:home", &json!({"title":"Discover", "tabs":[{"title":"Home","ref":"discover:home"}], "shelves":[
            {"id":"lists","title":"Lists","kind":"playlist","tags":[{"slug":"quiet","name":"Quiet"}],"items":[
                {"kind":"playlist","ref":"playlist:1","title":"One","tags":[{"slug":"quiet"}]},
                {"kind":"playlist","ref":"playlist:2","title":"Two"}]},
            {"id":"spotlight","title":"Spotlight","kind":"album","more":"artist:7","items":[{"kind":"album","ref":"album:3","title":"Record","artist_ref":"artist:7"}],
             "extra_tracks":[{"kind":"track","ref":"track:4@playlist:1","title":"Song","artist_ref":"artist:7","album_ref":"album:3"}]}
        ]}));
        let home = catalog.stored_shelf("fixture native:discover:home").unwrap();
        assert_eq!(home.document["tabs"][0]["key"], "fixture native:discover:home");
        assert_eq!(home.document["shelves"][1]["more_key"], "fixture native:artist:7");
        let tagged = catalog.stored_shelf("fixture native:discover:home|lists#quiet").unwrap();
        assert_eq!(tagged.cards.len(),1);
        let list = catalog.stored_shelf("fixture native:discover:home|lists").unwrap();
        assert_eq!(list.cards.len(),2);
        let tracks = catalog.stored_shelf("fixture native:discover:home|spotlight-tracks").unwrap();
        assert_eq!(tracks.tracks.len(),1);
        let inner = catalog.lock();
        assert_eq!(inner.store.tracks[&tracks.tracks[0]].metadata["ref"],"track:4@playlist:1");
        assert!(inner.store.tracks[&tracks.tracks[0]].album_id < 0);
    }
}
