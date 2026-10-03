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
        value["art"] = json!(crate::pictures::kept(image).unwrap_or_default());
    }
}
impl crate::Chilpa {
    pub fn native_image(&self, plugin: &str, image: &str) -> String {
        if !self.natives.is_native(plugin) { return String::new(); }
        self.catalog.want_image(plugin, image);
        crate::pictures::kept(image).unwrap_or_default()
    }

    pub fn native_card_info(&mut self, id: i64) -> String {
        let Some(card) = self.catalog.card(id) else { return "{}".into(); };
        if !self.natives.has_pages(&card.plugin) { return "{}".into(); }
        let mut info = card.metadata.clone();
        decorate_header(&self.catalog, &card.plugin, &mut info);
        info["art"] = json!(self.catalog.art_of_card(id));
        if card.kind == super::store::CardKind::Album { if let Some(document) = self.catalog.shelf(&format!("{} native:{}", card.plugin, card.identity)) { info["shelves"] = document.document["shelves"].clone(); } }
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
