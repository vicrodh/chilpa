use super::*;
use crate::library_sources::{SourceIndex, Stars};
use std::sync::{atomic::{AtomicUsize, Ordering}, Mutex};

struct Fixture {
    calls: AtomicUsize,
    replies: Mutex<std::collections::VecDeque<Value>>,
}

impl Fixture {
    fn new(replies: Vec<Value>) -> Self { Self { calls: AtomicUsize::new(0), replies: Mutex::new(replies.into()) } }
}

impl Http for Fixture {
    fn get(&self, _: &str, _: &str, _: Duration, _: usize) -> Result<Reply, String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(Reply { status: 200, headers: Default::default(),
            body: serde_json::to_vec(&self.replies.lock().unwrap().pop_front().unwrap_or(json!({"releases":[]}))).unwrap() })
    }
}

fn album(catalog: &Catalog, reference: &str, upc: &str, genre: &str) -> i64 {
    catalog.lock().store.take_item("fixture", &json!({"kind":"album","ref":reference,
        "title":"Record","artist":"Band","upc":upc,"genre":genre,"tracks":1}), super::super::now()).unwrap()
}

#[test]
fn opt_out_hidden_windows_and_empty_queues_start_no_worker_and_ask_nothing() {
    let catalog = Catalog::new();
    album(&catalog, "album:1", "123456789012", "");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("genres.json");
    let http = Fixture::new(vec![]);
    catalog.advance_genres();
    assert!(!catalog.lock().genres.running);
    catalog.genre_pass(&[("fixture".into(), "123456789012".into())], &http, &path, false);
    assert_eq!(http.calls.load(Ordering::Relaxed), 0);
    assert!(!path.exists());
    catalog.genre_source("fixture", true);
    catalog.genre_window(false);
    catalog.advance_genres();
    assert!(!catalog.lock().genres.running);
    catalog.genre_pass(&[("fixture".into(), "123456789012".into())], &http, &path, false);
    assert_eq!(http.calls.load(Ordering::Relaxed), 0);
    catalog.genre_window(true);
    catalog.lock().store.cards.retain(|_, _| false);
    catalog.advance_genres();
    assert!(!catalog.lock().genres.running);
}

#[test]
fn a_pass_stops_at_sixty_and_cached_barcodes_need_no_requests() {
    let catalog = Catalog::new();
    catalog.genre_source("fixture", true);
    for n in 0..65 { album(&catalog, &format!("album:{n}"), &format!("{n:012}"), ""); }
    album(&catalog, "album:duplicate", "0000000000000", "");
    album(&catalog, "album:own", "999999999999", "Soul");
    let pending = { let inner = catalog.lock(); inner.genres.pending(&inner.store, super::super::now()) };
    assert_eq!(pending.len(), PASS);
    let http = Fixture::new(vec![]);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("genres.json");
    catalog.genre_pass(&pending, &http, &path, false);
    assert_eq!(http.calls.load(Ordering::Relaxed), PASS);
    assert!(!catalog.lock().genres.running);
    catalog.genre_pass(&pending, &http, &path, false);
    assert_eq!(http.calls.load(Ordering::Relaxed), PASS);
    let remaining = { let inner = catalog.lock(); inner.genres.pending(&inner.store, super::super::now()) };
    assert_eq!(remaining.len(), 5);
    catalog.genre_pass(&remaining, &http, &path, false);
    let inner = catalog.lock();
    assert!(inner.genres.pending(&inner.store, super::super::now()).is_empty());
    drop(inner);
    catalog.advance_genres();
    assert!(!catalog.lock().genres.running);
    assert_eq!(http.calls.load(Ordering::Relaxed), 65);
}

#[test]
fn hiding_or_opting_out_between_search_and_release_prevents_the_second_request() {
    struct Hide<'a> { catalog: &'a Catalog, calls: AtomicUsize, disable: bool }
    impl Http for Hide<'_> {
        fn get(&self, _: &str, _: &str, _: Duration, _: usize) -> Result<Reply, String> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if self.disable { self.catalog.genre_source("fixture", false); } else { self.catalog.genre_window(false); }
            Ok(Reply { status: 200, headers: Default::default(), body: serde_json::to_vec(&json!({"releases":[
                {"id":"00000000-0000-4000-8000-000000000001","barcode":"123456789012"}
            ]})).unwrap() })
        }
    }
    for disable in [false, true] {
        let catalog = Catalog::new();
        catalog.genre_source("fixture", true);
        let http = Hide { catalog: &catalog, calls: AtomicUsize::new(0), disable };
        let directory = tempfile::tempdir().unwrap();
        catalog.genre_pass(&[("fixture".into(), "123456789012".into())], &http, &directory.path().join("genres.json"), false);
        assert_eq!(http.calls.load(Ordering::Relaxed), 1);
        assert!(!catalog.lock().genres.running);
        assert!(catalog.lock().genres.cache.as_ref().is_none_or(|cache| cache.get("123456789012", super::super::now()).is_none()));
    }
}

#[test]
fn cached_genres_reach_filters_groups_lists_and_the_record_page_without_replacing_source_genres() {
    let mut app = crate::Chilpa::new();
    app.library = Some(chilpa_library::Library::in_memory().unwrap());
    let catalog = app.catalog.clone();
    let missing = album(&catalog, "album:1", "123456789012", "");
    let own = album(&catalog, "album:2", "223456789012", "Soul");
    let track_album = album(&catalog, "album:3", "123456789012", "");
    let track = {
        let mut inner = catalog.lock();
        let track = inner.store.track_id("plugin:fixture:track:1");
        let row = inner.store.tracks.get_mut(&track).unwrap();
        row.plugin = "fixture".into(); row.album_id = track_album; row.title = "Song".into();
        inner.store.cards.get_mut(&track_album).unwrap().tracks = Some(vec![track]);
        inner.store.shelves.insert("fixture section:albums".into(), super::super::store::Shelf {
            cards: vec![missing, own, track_album], ..Default::default()
        });
        track
    };
    catalog.genre_source("fixture", true);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("genres.json");
    let mut cache = Cache::default();
    for code in ["123456789012", "223456789012"] { cache.insert(code, vec!["Jazz".into()], super::super::now()); }
    cache.write(&path).unwrap();
    let http = Fixture::new(vec![]);
    let signature = catalog.integrated_signature(&[("fixture".into(), true, false)]);
    let entity = catalog.entity_revision(missing);
    catalog.genre_pass(&[("fixture".into(), "123456789012".into())], &http, &path, false);
    assert_eq!(http.calls.load(Ordering::Relaxed), 0);
    assert_ne!(signature, catalog.integrated_signature(&[("fixture".into(), true, false)]));
    assert_ne!(entity, catalog.entity_revision(missing));
    let index = SourceIndex::new(catalog.integrated_items(&["fixture".into()], &[]), vec![]);
    let filter = crate::Filter { genre: Some("Jazz".into()), ..Default::default() };
    assert_eq!(index.items.iter().filter(|item| crate::library_sources::passes(item, &filter, &Stars::default())).count(), 2);
    let mut facets = crate::Facets::default();
    crate::library_sources::merge_facets(&mut facets, &crate::Filter::default(), &index, &Stars::default(), None);
    assert!(facets.genres.iter().any(|g| g.label == "Jazz" && g.albums == 2));
    let reader = crate::Reader::of(chilpa_library::Library::in_memory().unwrap());
    let grouped = reader.album_groups(vec![missing, own], crate::library_sources::AlbumGroup::Genre, false, false, &index);
    assert_eq!(grouped.groups.iter().map(|(_, name)| name.as_str()).collect::<Vec<_>>(), ["Jazz", "Soul"]);
    use chilpa_library::store::query::{Field, Node, Values};
    let query = crate::Filter { query: Some(Node::Is(Field::Genre, Values::Any(vec!["Jazz".into()]))), ..Default::default() };
    assert_eq!(index.items.iter().filter(|item| crate::library_sources::passes(item, &query, &Stars::default())).count(), 2);
    for (id, genre) in [(missing, "Jazz"), (own, "Soul")] {
        assert_eq!(serde_json::from_str::<Value>(&app.album_list_facts(id)).unwrap()["genre"], genre);
        assert_eq!(serde_json::from_str::<Value>(&app.album_information(id)).unwrap()["genre"], genre);
    }
    assert_eq!(catalog.tracks_by_ids(&[track])[0].0.metadata["genres"], json!(["Jazz"]));
    catalog.lock().store.tracks.get_mut(&track).unwrap().metadata = json!({"genre":"Rock"});
    assert_eq!(catalog.integrated_items(&["fixture".into()], &[]).iter().find(|i| i.track == track).unwrap().genres, ["Rock"]);
    assert_eq!(catalog.tracks_by_ids(&[track])[0].0.metadata["genre"], "Rock");
    {
        let mut inner = catalog.lock();
        let card = inner.store.cards.get_mut(&own).unwrap();
        card.genre.clear();
        card.metadata["genre"] = json!("");
        card.metadata["genres"] = json!(["Folk"]);
    }
    assert_eq!(serde_json::from_str::<Value>(&app.album_list_facts(own)).unwrap()["genre"], "Folk");
    assert_eq!(serde_json::from_str::<Value>(&app.album_information(own)).unwrap()["genre"], "Folk");
    assert!(catalog.lock().store.cards[&missing].genre.is_empty());
    catalog.genre_source("fixture", false);
    assert_eq!(serde_json::from_str::<Value>(&app.album_list_facts(missing)).unwrap()["genre"], "");
    assert_eq!(serde_json::from_str::<Value>(&app.album_list_facts(own)).unwrap()["genre"], "Folk");
    assert!(catalog.lock().genres.cache.is_none());
}

#[test]
fn successful_lookups_are_published_and_none_does_not_refresh_the_view() {
    let catalog = Catalog::new();
    let id = album(&catalog, "album:1", "123456789012", "");
    catalog.genre_source("fixture", true);
    let http = Fixture::new(vec![
        json!({"releases":[{"id":"00000000-0000-4000-8000-000000000001","barcode":"123456789012"}]}),
        json!({"id":"00000000-0000-4000-8000-000000000001","genres":[{"name":"Jazz","count":2}]}),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("genres.json");
    catalog.genre_pass(&[("fixture".into(), "123456789012".into())], &http, &path, false);
    assert_eq!(http.calls.load(Ordering::Relaxed), 2);
    assert_eq!(catalog.card(id).unwrap().genre, "Jazz");
    assert_eq!(Cache::read(&path, super::super::now()).get("123456789012", super::super::now()).unwrap(), ["Jazz"]);
    let revision = catalog.revisions();
    catalog.genre_pass(&[("fixture".into(), "223456789012".into())], &http, &path, false);
    assert_eq!(catalog.revisions(), revision);
    let other = catalog.lock().store.take_item("other", &json!({"kind":"album","ref":"album:1",
        "title":"Other","upc":"123456789012"}), 1).unwrap();
    assert!(catalog.card(other).unwrap().genre.is_empty());
    let revision = catalog.revisions();
    catalog.advance_genres();
    let scanned = catalog.lock().genres.scanned;
    for _ in 0..100 { catalog.advance_genres(); }
    assert!(!catalog.lock().genres.running);
    assert_eq!(catalog.lock().genres.scanned, scanned);
    assert_eq!(catalog.revisions(), revision);
}

#[test]
fn a_failed_request_is_not_cached_as_an_absent_genre_and_waits_before_retrying() {
    let catalog = Catalog::new();
    catalog.genre_source("fixture", true);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("genres.json");
    let http = Fixture::new(vec![json!({"error":"bad response"})]);
    catalog.genre_pass(&[("fixture".into(), "123456789012".into())], &http, &path, false);
    let inner = catalog.lock();
    assert!(inner.genres.retry_at >= super::super::now() + RETRY - 1);
    assert!(inner.genres.cache.as_ref().unwrap().get("123456789012", super::super::now()).is_none());
    assert!(!inner.genres.running);
    assert!(!path.exists());
}

#[test]
fn a_genre_result_refreshes_only_pages_whose_displayed_genres_changed() {
    let catalog = Catalog::new();
    let changed = album(&catalog, "album:changed", "123456789012", "");
    let untouched = album(&catalog, "album:untouched", "223456789012", "");
    let own = album(&catalog, "album:own", "123456789012", "Soul");
    let playlist = {
        let mut inner = catalog.lock();
        let playlist = inner.store.take_item("fixture", &json!({"kind":"playlist","ref":"playlist:1","title":"List"}), 1).unwrap();
        let track = inner.store.track_id("plugin:fixture:track:1");
        inner.store.tracks.get_mut(&track).unwrap().plugin = "fixture".into();
        inner.store.tracks.get_mut(&track).unwrap().album_id = changed;
        inner.store.cards.get_mut(&playlist).unwrap().tracks = Some(vec![track]);
        playlist
    };
    catalog.genre_source("fixture", true);
    let before = [changed, untouched, own, playlist].map(|id| catalog.entity_revision(id));
    let http = Fixture::new(vec![
        json!({"releases":[{"id":"00000000-0000-4000-8000-000000000001","barcode":"123456789012"}]}),
        json!({"id":"00000000-0000-4000-8000-000000000001","genres":[{"name":"Jazz","count":2}]}),
    ]);
    let directory = tempfile::tempdir().unwrap();
    catalog.genre_pass(&[("fixture".into(), "123456789012".into())], &http, &directory.path().join("genres.json"), false);
    let after = [changed, untouched, own, playlist].map(|id| catalog.entity_revision(id));
    assert_ne!(before[0], after[0]);
    assert_eq!(before[1], after[1]);
    assert_eq!(before[2], after[2]);
    assert_ne!(before[3], after[3]);
    catalog.genre_source("fixture", false);
    assert_ne!(catalog.entity_revision(changed), after[0]);
    assert_eq!(catalog.entity_revision(untouched), after[1]);
    assert_eq!(catalog.entity_revision(own), after[2]);
    assert_ne!(catalog.entity_revision(playlist), after[3]);
}
