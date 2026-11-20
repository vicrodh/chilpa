//! Discovery rails use the native catalog's existing page jobs and stable ids.
use super::{Catalog, store::{Card, CardKind}};
use serde_json::{Value, json};
use std::collections::HashSet;

const MIN_RAIL_ITEMS: usize = 3;
const MAX_RAIL_ITEMS: usize = 20;

pub(crate) struct GenrePlugin {
    id: String,
    name: String,
    albums: Vec<Card>,
    pub artists: Vec<Card>,
}

impl Catalog {
    pub(crate) fn genre_plugin_cards(&self, genre: &str) -> Vec<GenrePlugin> {
        let Some(natives) = self.natives() else { return Vec::new() };
        let mut plugins = Vec::new();
        for id in natives.with_capability("genre_discovery") {
            if !natives.has_pages(&id) || !natives.is_native(&id) || natives.capability(&id, "genre_discovery") != 1 { continue; }
            let key = crate::native_page_request::route(&id, "genre-discovery", json!({"filters":{"genre":genre}, "limit":60}), "");
            let Some(page) = self.shelf(&key) else { continue; };
            let cards = |section, kind| {
                if !page.document["shelves"].as_array().into_iter().flatten().any(|row| row["id"] == section) { return Vec::new(); }
                let (rows, _, _, _) = self.shelf_snapshot(&format!("{key}|{section}"), "");
                rows.into_iter().filter(|row| row.kind == kind).take(60).collect()
            };
            let albums = cards("new_releases", CardKind::Album);
            let artists = cards("artists", CardKind::Artist);
            plugins.push(GenrePlugin { name: natives.display_name(&id), id, albums, artists });
        }
        plugins.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
        plugins
    }
}

impl GenrePlugin {
    pub(crate) fn rails(self, in_library: &HashSet<String>) -> Option<Value> {
        let art = |card: &Card| url::Url::from_file_path(&card.art).map(|url| url.to_string()).unwrap_or_default();
        let mut seen = HashSet::new();
        let albums: Vec<_> = self.albums.into_iter().filter(|card| !card.title.is_empty() && seen.insert(card.id))
            .take(MAX_RAIL_ITEMS).map(|card| json!({"id":card.id, "name":card.title, "credit":card.artist, "image":art(&card)})).collect();
        let mut seen = HashSet::new();
        let artists: Vec<_> = self.artists.into_iter().filter(|card| {
            let key = chilpa_library::store::query::fold(&card.title);
            !key.is_empty() && !in_library.contains(&key) && seen.insert(key) && crate::native_page_request::valid_reference(&card.identity)
        }).take(MAX_RAIL_ITEMS).map(|card| json!({"id":card.id, "key":format!("{} native:{}", self.id, card.identity), "name":card.title, "image":art(&card)})).collect();
        let albums = if albums.len() >= MIN_RAIL_ITEMS { albums } else { Vec::new() };
        let artists = if artists.len() >= MIN_RAIL_ITEMS { artists } else { Vec::new() };
        (!albums.is_empty() || !artists.is_empty()).then(|| json!({"id":self.id, "name":self.name, "newReleases":albums, "artists":artists}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genre_plugin_rails_exclude_known_artists_and_require_three_distinct_items() {
        let card = |id, name: &str| Card { id, title: name.into(), identity: format!("artist:{id}"), ..Default::default() };
        let plugin = GenrePlugin { id:"source".into(), name:"Fixture".into(), albums:vec![card(-1, "Only one")],
            artists:vec![card(-2, "Known"), card(-3, "First"), card(-4, "first"), card(-5, "Second"), card(-6, "Third")] };
        let found = plugin.rails(&HashSet::from(["known".into()])).unwrap();
        assert_eq!(found["newReleases"], json!([]));
        assert_eq!(found["artists"].as_array().unwrap().iter().map(|row| row["name"].as_str().unwrap()).collect::<Vec<_>>(), ["First", "Second", "Third"]);
        assert_eq!(found["artists"][0]["key"], "source native:artist:-3");
    }
}
