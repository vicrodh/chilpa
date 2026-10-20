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
fn artist_cards(catalog: &Catalog, key: &str) -> Vec<i64> {
    let inner = catalog.lock();
    inner.store.shelves.get(key).into_iter().flat_map(|shelf| &shelf.cards)
        .filter(|id| inner.store.cards.get(id).is_some_and(|card| card.kind == super::store::CardKind::Artist)).copied().collect()
}
fn context_card(catalog: &Catalog, plugin: &str, key: &str, shelf: &mut Value) {
    let header_key = format!("{key}-header");
    if shelf["header"].is_object() && (shelf["header_key"] != header_key || catalog.stored_shelf(&header_key).is_none()) {
        ingest(catalog, plugin, &header_key, &json!({"items":[shelf["header"]]}));
        shelf["header_key"] = json!(header_key);
    }
}
pub(super) fn read(catalog: &Catalog, natives: &Arc<Natives>, plugin: &str, reference: &str) -> Result<(), String> {
    let parent = reference.split('|').next().unwrap_or(reference);
    let reading = {
        let mut reading = catalog.page_reads.lock().unwrap_or_else(|e| e.into_inner());
        if reading.len() >= 64 { reading.retain(|_, lock| Arc::strong_count(lock) > 1); }
        reading.entry(format!("{plugin} native:{parent}")).or_default().clone()
    };
    let _one = reading.lock().unwrap_or_else(|e| e.into_inner());
    let current = catalog.stored_shelf(&format!("{plugin} native:{reference}"));
    if current.as_ref().is_some_and(|s| s.epoch > 0 && s.read_at > 0 && now().saturating_sub(s.read_at) < 30) { return Ok(()); }
    let refresh = current.is_some_and(|s| s.read_at == 0);
    let answer = natives.call(plugin, "page.get", json!({"ref":parent,"refresh":refresh}), Duration::from_secs(90))?;
    if !natives.is_native(plugin) { return Err("The plugin was disabled.".into()); }
    ingest_document(catalog, plugin, parent, &answer);
    if catalog.stored_shelf(&format!("{plugin} native:{reference}")).is_none_or(|s| s.epoch == 0 || now().saturating_sub(s.read_at) >= 30) {
        return Err(chilpa_i18n::t("this part of the source is not there now"));
    }
    Ok(())
}
pub(super) fn ingest_document(catalog: &Catalog, plugin: &str, reference: &str, answer: &Value) {
    let key = format!("{plugin} native:{reference}");
    let mut document = answer.clone();
    let personal = document["personal"] == true;
    if let Some(shelves) = document.get_mut("shelves").and_then(Value::as_array_mut) {
        for shelf in shelves {
            shelf["personal"] = json!(personal);
            let id = shelf["id"].as_str().unwrap_or_default();
            let child = format!("{key}|{id}");
            context_card(catalog, plugin, &child, shelf);
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
pub(super) fn append(catalog: &Catalog, natives: &Arc<Natives>, key: &str) -> Result<(), String> {
    let (plugin, reference) = super::split(key);
    let Some((parent, section)) = reference.strip_prefix("native:").and_then(|reference| reference.split_once('|')) else { return Ok(()); };
    let Some(current) = catalog.stored_shelf(key) else { return Ok(()); };
    let Some(more) = current.document["more"].as_str() else { return Ok(()); };
    let answer = natives.call(plugin, "page.get", json!({"ref":more}), Duration::from_secs(60))?;
    if !natives.is_native(plugin) { return Err("The plugin was disabled.".into()); }
    let child = appended(&current.document, &answer);
    ingest(catalog, plugin, key, &child);
    let mut inner = catalog.lock();
    if let Some(page) = inner.store.shelves.get_mut(&format!("{plugin} native:{parent}")) {
        if let Some(row) = page.document["shelves"].as_array_mut().into_iter().flatten().find(|row| row["id"] == section) {
            row["items"] = child["items"].clone(); row["more"] = child["more"].clone(); row["error"] = child["error"].clone();
        }
    }
    inner.dirty = true;
    Ok(())
}

fn appended(current: &Value, answer: &Value) -> Value {
    let mut child = current.clone();
    let mut items = child["items"].as_array().cloned().unwrap_or_default();
    let mut seen: std::collections::HashSet<_> = items.iter().filter_map(|value| value["ref"].as_str().map(str::to_string)).collect();
    items.extend(answer["items"].as_array().into_iter().flatten().filter(|value| value["ref"].as_str().is_some_and(|reference| seen.insert(reference.into()))).cloned());
    child["items"] = json!(items); child["more"] = answer["next"].clone(); child["error"] = json!("");
    child
}

fn decorate_header(catalog: &Catalog, plugin: &str, value: &mut Value) {
    if !value.is_object() { return; }
    value["plugin"] = json!(plugin);
    if let Some(date) = value["release_date"].as_str() { value["released"] = json!(crate::album_versions::release_label(date)); }
    let kind = value["kind"].as_str().unwrap_or_default();
    if !value["favorite"].is_boolean() && matches!(kind,"album" | "artist" | "playlist" | "label") {
        if let Some(shelf) = catalog.shelf(&format!("{plugin} section:{kind}s")) {
            value["favorite"] = json!(catalog.cards(&shelf.cards).iter().any(|c| c.identity == value["ref"].as_str().unwrap_or_default()));
        }
    }
    if let Some(image) = value["image"].as_str() {
        catalog.want_image(plugin, image);
        value["art"] = json!(crate::pictures::kept(image).and_then(|path| url::Url::from_file_path(path).ok()).map(|url| url.to_string()).unwrap_or_default());
    }
    // A record whose cover the listener chose shows that one in its header.
    let chosen = value["ref"].as_str().and_then(|reference| catalog.lock().store.chosen.get(&(plugin.to_string(), reference.to_string())).cloned());
    if let Some(url) = chosen.and_then(|path| url::Url::from_file_path(path).ok()) {
        value["chosen_art"] = json!(url.to_string());
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
        context_card(catalog, plugin, &child, &mut shelf);
        shelf["artist_cards"] = json!(artist_cards(catalog, &child));
        shelf["loading"] = json!(catalog.loading(&child));
        if let Some(error) = catalog.shelf_error(&child) { shelf["error"] = json!(error); }
        let items = shelf["items"].as_array();
        if items.is_some_and(|items| items.len() == 1 && items[0]["ref"] == radio) {
            if let Some(card) = catalog.lock().store.cards.values().find(|card| card.plugin == plugin && card.identity == radio) {
                header["radio_id"] = json!(card.id);
                continue;
            }
        }
        // An artist's mix, when the source makes one, sits with the radio.
        if shelf["id"] == "mix" && items.is_some_and(|items| items.len() == 1) {
            let identity = items.and_then(|i| i[0]["ref"].as_str()).unwrap_or_default().to_string();
            if let Some(card) = catalog.lock().store.cards.values().find(|card| card.plugin == plugin && card.identity == identity) {
                header["mix_id"] = json!(card.id);
                continue;
            }
        }
        if mixed_artist && !artist.is_empty() && shelf["kind"] == "album" && shelf["more"] == artist { continue; }
        rows.push(shelf);
    }
    json!(rows)
}
/// How many of the tracks the listener keeps of an artist show before
/// "Load more" (QBZ's library tab).
const LIBRARY_PREVIEW: usize = 10;

impl crate::Chilpa {
    pub(crate) fn append_native_section(&self, key: &str) {
        let (plugin, reference) = super::split(key);
        if !self.natives.has_pages(plugin) || !reference.starts_with("native:") || !reference.contains('|') { return; }
        let mut inner = self.catalog.lock();
        if inner.store.shelves.get(key).is_none_or(|shelf| !shelf.document["more"].is_string()) { return; }
        self.catalog.queue(&mut inner, super::Job::Append(key.into()), true);
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
        if info["genre"].as_str().is_none_or(str::is_empty) && !card.genre.is_empty() { info["genre"] = json!(card.genre); }
        if card.contain_art() { info["art_fit"] = json!("contain"); }
        let quality = self.album_card_quality(id);
        info["quality_label"] = json!(quality.short()); info["quality_tier"] = json!(quality.tier.rank());
        decorate_header(&self.catalog, &card.plugin, &mut info);
        info["art"] = json!(url::Url::from_file_path(self.catalog.art_of_card(id)).ok().map(|url| url.to_string()).unwrap_or_default());
        if let Some(url) = self.catalog.chosen_art(id).and_then(|path| url::Url::from_file_path(path).ok()) {
            info["chosen_art"] = json!(url.to_string());
        }
        if card.kind == super::store::CardKind::Playlist && info["editable"] == true {
            info["entries"] = json!(self.catalog.tracks(id).unwrap_or_default().iter().map(|t| json!({"ref":t.metadata["ref"],"member":t.metadata["member"],"title":t.title,"artist":t.artist,"available":t.playable})).collect::<Vec<_>>());
        }
        if card.kind == super::store::CardKind::Album { if let Some(document) = self.catalog.shelf(&format!("{} native:{}", card.plugin, card.identity)) { info["shelves"] = page_shelves(&self.catalog, &card.plugin, &card.identity, &mut info, &document.document["shelves"], true); } }
        info.to_string()
    }
    pub fn native_card_art_colors(&self, id: i64) -> String {
        self.catalog.card(id).and_then(|card| card.metadata["art_colors"].as_array().map(|colors| json!(colors).to_string())).unwrap_or_else(|| "[]".into())
    }
    pub fn source_artist_key(&self, album: i64, path: &str) -> Option<String> {
        let (plugin, metadata) = if album < 0 {
            let card = self.catalog.card(album)?; (card.plugin, card.metadata)
        } else { let (track, _) = self.catalog.track_by_path(path)?; (track.plugin, track.metadata) };
        let reference = metadata["artist_ref"].as_str()?;
        self.natives.has_pages(&plugin).then(|| format!("{plugin} native:{reference}"))
    }

    /// The artist page a source's track leads to, the `place`-th name of
    /// its credit: the one the source names for its first artist, or that
    /// artist found by name in the source. None for a track of no source.
    pub fn source_track_artist(&mut self, path: &str, credit: &str, place: i32) -> Option<String> {
        if place < 0 { return None; }
        let (plugin, _) = crate::source_opener::of_plugin(path)?;
        if place == 0 {
            if let Some(key) = self.source_artist_key(0, path) { return Some(key); }
        }
        let names = self.credit_names(credit);
        let name = names.get(place as usize)?.clone();
        Some(name.trim().to_string()).filter(|n| !n.is_empty()).map(|name| format!("{plugin} matching-artist-v3:{name}"))
    }

    /// An artist's page "In library": what the listener keeps of this
    /// artist in the source -- the favorite tracks and records whose artist
    /// it is -- in the page's shape, under the catalog page's header. The
    /// source's lists are asked for when they are missing or old, and the
    /// page says it waits while they come.
    pub(crate) fn artist_library_view(&mut self, plugin: &str, reference: &str) -> String {
        let mut document: Value = serde_json::from_str(&self.native_page_view(plugin, reference)).unwrap_or_else(|_| json!({}));
        let artist = json!({"ref": reference, "title": document["header"]["title"]});
        let (tracks_key, albums_key) = (format!("{plugin} section:tracks"), format!("{plugin} section:albums"));
        let albums_shelf = self.catalog.shelf(&albums_key);
        let read = self.catalog.shelf(&tracks_key).is_some() && albums_shelf.is_some();
        let ours = |mut item: Value, kind: &str| {
            item["kind"] = json!(kind);
            super::native::same_primary_artist(&item, &artist).then_some(item)
        };
        let tracks: Vec<Value> = self.catalog.shelf_tracks(&tracks_key).into_iter().filter_map(|(track, _)| ours(track.metadata, "track")).collect();
        let albums: Vec<Value> = albums_shelf.map(|shelf| self.catalog.cards(&shelf.cards)).unwrap_or_default()
            .into_iter().filter_map(|card| ours(card.metadata, "album")).collect();
        let mut shelves = Vec::new();
        for (id, kind, items) in [("tracks", "track", tracks), ("albums", "album", albums)] {
            if items.is_empty() { continue; }
            let key = format!("{plugin} artist-library:{reference}|{id}");
            let shelf = json!({"id": id, "kind": kind, "items": items});
            ingest(&self.catalog, plugin, &key, &shelf);
            shelves.push(json!({"id": id, "kind": kind, "key": key, "items": shelf["items"], "tracks": kind == "track", "round": false}));
        }
        document["shelves"] = json!(shelves);
        document["library"] = json!(true);
        document["loading"] = json!(!read || self.catalog.loading(&tracks_key) || self.catalog.loading(&albums_key));
        crate::artist_page::shape(&mut document);
        // The page's own tracks, not the source's most heard: ten first, as QBZ.
        let mine = chilpa_i18n::t("Tracks");
        for shelf in document["shelves"].as_array_mut().into_iter().flatten().filter(|s| s["role"] == "popular") {
            shelf["title"] = json!(mine);
            shelf["preview"] = json!(LIBRARY_PREVIEW);
        }
        for entry in document["jump"].as_array_mut().into_iter().flatten().filter(|e| e["id"].as_str().is_some_and(|id| id.ends_with("|tracks"))) { entry["title"] = json!(mine); }
        document.to_string()
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
        if document["shelves"].is_array() {
            let shelves = document["shelves"].clone();
            document["shelves"] = page_shelves(&self.catalog, plugin, reference, &mut document["header"], &shelves, false);
        }
        if !document["shelves"].is_array() { document.as_object_mut().unwrap().remove("shelves"); }
        decorate_header(&self.catalog, plugin, &mut document["header"]);
        if let Some(rows) = document.get_mut("shelves").and_then(Value::as_array_mut) { for row in rows { decorate_header(&self.catalog, plugin, &mut row["header"]); } }
        let kind = shelf.document["items"].as_array().and_then(|items| items.first()).and_then(|item| item["kind"].as_str()).unwrap_or("album");
        document["entity"] = json!(kind);
        let artists = artist_cards(&self.catalog, &key);
        document["round"] = json!(!shelf.cards.is_empty() && artists.len() == shelf.cards.len());
        document["artist_cards"] = json!(artists);
        document["albumsOnly"] = json!(shelf.tracks.is_empty());
        if document.get("shelves").is_none() && document["more"].is_string() && document["next"].is_null() {
            document["next"] = document["more"].clone();
        }
        if document["header"]["kind"] == "artist" {
            // How much "In library" holds, for its badge.
            let artist = json!({"ref": document["header"]["ref"].as_str().unwrap_or(reference), "title": document["header"]["title"]});
            let lists = [format!("{plugin} section:tracks"), format!("{plugin} section:albums")];
            document["header"]["kept"] = json!(self.catalog.count_kept(&lists, |item| super::native::same_primary_artist(item, &artist)));
            crate::artist_page::shape(&mut document);
        }
        document.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paging_reuses_occurrences_and_decorates_failures_without_losing_rows() {
        let original = json!({"id":"albums","items":[{"ref":"one"},{"ref":"two"}],"more":"next"});
        let next = appended(&original, &json!({"items":[{"ref":"two"},{"ref":"three"}],"next":"later"}));
        assert_eq!(next["items"].as_array().unwrap().len(), 3);
        assert_eq!(next["more"], "later");
        let catalog = Catalog::new();
        catalog.lock().loaded = true;
        catalog.lock().shelf_errors.insert("fixture native:home|albums".into(), "Source failed".into());
        let displayed = page_shelves(&catalog, "fixture", "home", &mut json!({}), &json!([original.clone()]), false);
        assert_eq!(displayed[0]["error"], "Source failed");
        assert_eq!(displayed[0]["items"], original["items"]);
    }
    #[test]
    fn card_artist_links_keep_source_identity_without_opening_the_record() {
        let mut api = crate::Chilpa::new();
        let id = api.catalog.lock().store.take_item("fixture", &json!({"kind":"album","ref":"album:1","title":"Record",
            "artist":"Band","artist_ref":"artist:us:7"}), 1).unwrap();
        let link: Value = serde_json::from_str(&api.album_artist_link(id, "Band")).unwrap();
        assert_eq!(link["target"], "source");
        assert_eq!(link["key"], "fixture native:artist:us:7");
        assert!(api.catalog.card(id).unwrap().tracks.is_none(), "following a credit must not fetch an album's tracks");
        assert_eq!(serde_json::from_str::<Value>(&api.album_artist_link(1, "Local Band")).unwrap()["key"], "Local Band");
        let playlist = api.catalog.lock().store.take_item("fixture", &json!({"kind":"playlist","ref":"playlist:1","title":"List","artist":"Owner"}), 1).unwrap();
        assert_eq!(api.album_artist_link(playlist, "Owner"), "{}");
    }
    #[test]
    fn a_sources_track_finds_its_artist_by_name_when_its_source_names_none() {
        let mut app = crate::Chilpa::new();
        let path = crate::source_opener::entry_of("fixture", "track:1");
        assert_eq!(app.source_track_artist(&path, "Band", 0).as_deref(), Some("fixture matching-artist-v3:Band"));
        assert_eq!(app.source_track_artist("/music/song.flac", "Band", 0), None, "a file of the library has its own page");
        assert_eq!(app.source_track_artist(&path, "  ", 0), None);
        assert_eq!(app.source_track_artist(&path, "Band", -1), None, "a separator in a credit opens nothing");
        assert_eq!(app.source_track_artist(&path, "Band", 4), None, "a missing contributor opens nothing");
    }
    #[test]
    fn an_artists_library_is_what_the_listener_keeps_of_that_artist() {
        let mut app = crate::Chilpa::new();
        // As after the start: the catalog kept on disk has been read.
        app.catalog.lock().loaded = true;
        ingest(&app.catalog, "fixture", "fixture section:tracks", &json!({"items":[
            {"kind":"track","ref":"track:1","title":"Song","artist":"Band","artist_ref":"artist:9"},
            {"kind":"track","ref":"track:2","title":"Other","artist":"Someone","artist_ref":"artist:5"}]}));
        ingest(&app.catalog, "fixture", "fixture section:albums", &json!({"items":[
            {"kind":"album","ref":"album:1","title":"Record","artist":"Band","artist_ref":"artist:9"}]}));
        let page: Value = serde_json::from_str(&app.artist_library_view("fixture", "artist:9")).unwrap();
        let shelves = page["shelves"].as_array().unwrap();
        assert_eq!(shelves.iter().map(|s| s["role"].as_str().unwrap()).collect::<Vec<_>>(), ["popular", "release"]);
        assert_eq!((shelves[0]["items"].as_array().unwrap().len(), shelves[0]["title"].as_str(), shelves[0]["preview"].as_u64()), (1, Some("Tracks"), Some(10)));
        assert_eq!((page["library"].as_bool(), page["loading"].as_bool()), (Some(true), Some(false)));
        ingest_document(&app.catalog, "fixture", "artist:9", &json!({"header":{"kind":"artist","ref":"artist:9","title":"Band"},"shelves":[]}));
        let catalog: Value = serde_json::from_str(&app.native_page_view("fixture", "artist:9")).unwrap();
        assert_eq!(catalog["header"]["kept"].as_u64(), Some(2), "the catalog page says how much its library holds");
    }

    #[test]
    fn see_all_keeps_a_shelf_as_a_listing_instead_of_an_empty_home() {
        let mut app = crate::Chilpa::new();
        for kind in ["album", "playlist", "track"] {
            let page = json!({"title":"Home","shelves":[{"id":"recent","title":"Recent","kind":kind,"more":"browse:next",
                "items":[{"kind":kind,"ref":format!("{kind}:1"),"title":"One"}]}]});
            ingest_document(&app.catalog, "fixture", "discover:home", &page);
            let home: Value = serde_json::from_str(&app.native_page_view("fixture", "discover:home")).unwrap();
            assert!(home["shelves"].is_array());
            let listing: Value = serde_json::from_str(&app.native_page_view("fixture", "discover:home|recent")).unwrap();
            assert!(listing.get("shelves").is_none(), "a shelf must open its records or tracks view");
            assert_eq!(listing["title"], "Recent");
            assert_eq!(listing["next"], "browse:next");
            assert_eq!(listing["albumsOnly"], kind != "track");
            assert_eq!(listing[if kind == "track" { "tracks" } else { "cards" }], 1);
        }
    }

    #[test]
    fn direct_listing_responses_and_cached_null_shelves_keep_their_items() {
        let mut app = crate::Chilpa::new();
        for cached_null in [false, true] {
            let mut page = json!({"title":"More records","items":[{"kind":"album","ref":"album:1","title":"One"}]});
            if cached_null { page["shelves"] = Value::Null; }
            ingest_document(&app.catalog, "fixture", "browse:records", &page);
            let listing: Value = serde_json::from_str(&app.native_page_view("fixture", "browse:records")).unwrap();
            assert!(listing.get("shelves").is_none());
            assert_eq!(listing["cards"], 1);
            assert_eq!(listing["albumsOnly"], true);
        }
    }

    #[test]
    fn mixed_modules_keep_artist_portraits_and_an_actionable_context_record() {
        let mut app = crate::Chilpa::new();
        ingest_document(&app.catalog, "fixture", "discover:home", &json!({"shelves":[{
            "id":"interest:mixed","kind":"mixed","title":"Recommendations",
            "header":{"kind":"album","ref":"album:seed","title":"Seed","artist":"Band","image":"https://example.org/seed.jpg"},
            "items":[{"kind":"artist","ref":"artist:1","title":"Band"},{"kind":"playlist","ref":"playlist:2","title":"List"},
                {"kind":"album","ref":"album:3","title":"Record"}]}]}));
        let page: Value = serde_json::from_str(&app.native_page_view("fixture", "discover:home")).unwrap();
        let shelf = &page["shelves"][0];
        let seed = app.catalog.stored_shelf(shelf["header_key"].as_str().unwrap()).unwrap();
        assert_eq!(seed.cards.len(), 1);
        let card = app.catalog.card(seed.cards[0]).unwrap();
        assert_eq!((card.identity.as_str(), card.title.as_str(), card.artist.as_str()), ("album:seed", "Seed", "Band"));
        let listing: Value = serde_json::from_str(&app.native_page_view("fixture", "discover:home|interest:mixed")).unwrap();
        assert_eq!(listing["cards"], 3);
        assert_eq!(listing["round"], false, "an artist first must not round every record in the mixed grid");
        assert_eq!(shelf["artist_cards"], listing["artist_cards"]);
        let artists = listing["artist_cards"].as_array().unwrap();
        assert_eq!(artists.len(), 1);
        assert_eq!(app.catalog.card(artists[0].as_i64().unwrap()).unwrap().identity, "artist:1");
        let mut legacy = json!({"header":{"kind":"album","ref":"album:old","title":"Cached seed"}});
        context_card(&app.catalog, "fixture", "fixture native:old|module", &mut legacy);
        assert_eq!(app.catalog.stored_shelf(legacy["header_key"].as_str().unwrap()).unwrap().cards.len(), 1);
        legacy["header_key"] = json!("another native:home|context");
        context_card(&app.catalog, "fixture", "fixture native:old|module", &mut legacy);
        assert_eq!(legacy["header_key"], "fixture native:old|module-header");
    }

    #[test]
    fn a_heart_marks_its_record_where_it_shows_and_reads_no_page_again() {
        let catalog = Catalog::new();
        ingest_document(&catalog, "fixture", "album:1", &json!({"header":{"kind":"album","ref":"album:1","title":"Record","favorite":false},"shelves":[]}));
        ingest_document(&catalog, "fixture", "artist:9", &json!({"header":{"kind":"artist","ref":"artist:9","title":"Band"},
            "shelves":[{"id":"albums","kind":"album","items":[{"kind":"album","ref":"album:1","title":"Record","artist":"Band","favorite":false}]}]}));
        let card = catalog.cards(&catalog.shelf("fixture native:artist:9|albums").unwrap().cards)[0].id;
        catalog.mark_native_favorite("fixture", "album:1", true);
        assert_eq!(catalog.card(card).unwrap().metadata["favorite"], true, "the card the artist's page shows wears the heart");
        assert_eq!(catalog.shelf("fixture native:album:1").unwrap().document["header"]["favorite"], true, "and so does the record's own page");
        for key in ["fixture native:album:1", "fixture native:artist:9", "fixture native:artist:9|albums"] {
            assert!(catalog.shelf(key).unwrap().read_at > 0, "{key} is not read again for a heart");
        }
        catalog.mark_native_favorite("fixture", "album:1", false);
        assert_eq!(catalog.card(card).unwrap().metadata["favorite"], false);
        assert_eq!(catalog.shelf("fixture native:artist:9").unwrap().document["header"]["favorite"], Value::Null, "another page's header is not that record");
    }

    #[test]
    fn overlapping_home_reads_reuse_the_document_without_a_live_plugin() {
        let catalog = Catalog::new();
        let natives = Arc::new(Natives::default());
        ingest_document(&catalog, "fixture", "discover:home", &json!({"title":"Home","shelves":[{"id":"recent","items":[]}]}));
        assert!(read(&catalog, &natives, "fixture", "discover:home").is_ok());
        assert!(read(&catalog, &natives, "fixture", "discover:home|recent").is_ok());
        catalog.lock().store.shelves.get_mut("fixture native:discover:home").unwrap().read_at = 0;
        assert!(read(&catalog, &natives, "fixture", "discover:home").is_err(), "explicit refresh must reach the plugin");
    }

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
        catalog.lock().loaded = true;
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
        let job = super::super::Job::Append(child.into());
        catalog.queue(&mut catalog.lock(), job.clone(), true);
        let rows = page_shelves(&catalog,"fixture","artist:7",&mut header,&changed["shelves"],false);
        assert_eq!(rows[0]["loading"], true, "queued pagination waits for worker capacity");
        {
            let mut inner = catalog.lock();
            inner.queue.urgent.clear(); inner.queue.queued.remove(&job);
            inner.running.insert(job.clone());
        }
        let rows = page_shelves(&catalog,"fixture","artist:7",&mut header,&changed["shelves"],false);
        assert_eq!(rows[0]["loading"], true, "running pagination stays loading");
        catalog.lock().running.remove(&job);
        let rows = page_shelves(&catalog,"fixture","artist:7",&mut header,&changed["shelves"],false);
        assert_eq!(rows[0]["loading"], false, "completed pagination releases its loading state");
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
        assert!(catalog.card(ids[0]).unwrap().tracks.is_some(), "I retain visible rows during refresh");
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
