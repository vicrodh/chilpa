use super::{store::{Shelf, Store}, Catalog, Job};
use serde_json::json;

#[test]
fn historical_pages_do_not_pin_the_entire_catalog_and_live_collections_remain_complete() {
    let mut store = Store::default();
    let mut collection = Vec::new();
    for number in 0..6001 { collection.push(store.card_id("fixture", &format!("kept:{number}"))); }
    store.shelves.insert("fixture section:albums".into(), Shelf { cards: collection.clone(), ..Default::default() });
    for number in 0..1000 {
        let id = store.card_id("fixture", &format!("historical:{number}"));
        store.shelves.insert(format!("fixture native:page:{number}"), Shelf { cards:vec![id], read_at:number, ..Default::default() });
    }
    store.prune(&Default::default(), &Default::default());
    assert!(store.shelves.len() <= 65);
    assert_eq!(store.cards.len(), collection.len(), "collection membership is a live root, historical shelves are not");
    assert_eq!(store.shelves["fixture section:albums"].cards, collection);
}

#[test]
fn rooted_pages_queue_rows_and_id_counters_survive_pruning_and_restart() {
    let catalog = Catalog::new();
    let (card, track, album, path) = {
        let mut inner = catalog.lock();
        let card = inner.store.take_item("fixture", &json!({"kind":"playlist","ref":"list:open","title":"Open"}), 1).unwrap();
        let track = inner.store.take_item_track("fixture", &json!({"kind":"track","ref":"track:1","title":"Song","album_ref":"album:1"}), None, 1).unwrap();
        inner.store.cards.get_mut(&card).unwrap().tracks = Some(vec![track]);
        let row = &inner.store.tracks[&track];
        (card, track, row.album_id, row.path.clone())
    };
    catalog.protect_tracks([path.clone()].into());
    catalog.protect_navigation(&[crate::navigation::Route { screen:crate::navigation::Screen::Source, id:0, key:"fixture native:open".into() }]);
    let mut inner = catalog.lock();
    inner.store.shelves.insert("fixture native:open".into(), Shelf { cards:vec![card], ..Default::default() });
    for number in 0..6100 { inner.store.card_id("history", &format!("album:{number}")); }
    let (tracks, cards) = inner.protection();
    let shelves = inner.protected_shelves();
    inner.store.prune_with_shelves(&tracks, &cards, &shelves);
    let json = serde_json::to_string(&inner.store).unwrap();
    let mut restored: Store = serde_json::from_str(&json).unwrap();
    restored.index();
    assert_eq!(restored.card_id("fixture", "list:open"), card);
    assert_eq!(restored.track_by_path(&path).unwrap().id, track);
    assert!(restored.cards.contains_key(&album));
    assert!(restored.shelves.contains_key("fixture native:open"));
    let next = restored.card_id("fixture", "album:new");
    assert!(next < -6100, "I never reuse ids from evicted entries");
}

#[test]
fn closing_a_native_route_cancels_queued_work_and_duplicates_share_one_job() {
    let catalog = Catalog::new();
    let route = |key: &str| crate::navigation::Route { screen:crate::navigation::Screen::Source, id:0, key:key.into() };
    catalog.protect_navigation(&[route("fixture native:old")]);
    {
        let mut inner = catalog.lock();
        catalog.queue(&mut inner, Job::Shelf("fixture native:old|albums".into()), false);
        catalog.queue(&mut inner, Job::Shelf("fixture native:old|albums".into()), true);
        assert_eq!(inner.queue.queued.len(), 1);
        assert!(inner.queue.later.is_empty());
    }
    catalog.protect_navigation(&[route("fixture native:new")]);
    let inner = catalog.lock();
    assert!(inner.queue.queued.is_empty());
    assert!(inner.queue.urgent.is_empty());
}

#[test]
fn overlapping_cached_lists_release_only_their_last_track_reference() {
    let mut store = Store::default();
    let first = store.card_id("fixture", "list:oldest");
    let duplicate = store.card_id("fixture", "list:duplicate");
    let recent = store.card_id("fixture", "list:recent");
    for id in 1..=40_100 {
        store.tracks.insert(-id, super::store::Track { id:-id, path:format!("track:{id}"), ..Default::default() });
    }
    for (id, stamp) in [(first, 1), (duplicate, 2)] {
        let card = store.cards.get_mut(&id).unwrap();
        card.tracks = Some((1..=20_000).map(|id| -id).collect());
        card.read_at = stamp;
    }
    let card = store.cards.get_mut(&recent).unwrap();
    card.tracks = Some((20_001..=40_000).map(|id| -id).collect());
    card.read_at = 3;
    store.prune(&Default::default(), &Default::default());
    assert!(store.cards[&first].tracks.is_none());
    assert!(store.cards[&duplicate].tracks.is_none(), "removing the first shared list frees no track payloads");
    assert_eq!(store.cards[&recent].tracks.as_ref().unwrap().len(), 20_000);
    assert_eq!(store.tracks.len(), 20_000, "orphan rows do not consume the held-track budget");
}

#[test]
fn saved_collection_cards_do_not_pin_inactive_row_caches() {
    let mut store = Store::default();
    let old = store.card_id("fixture", "list:old");
    let recent = store.card_id("fixture", "list:recent");
    let open = store.card_id("fixture", "list:open");
    let membership = vec![old, recent, open];
    store.shelves.insert("fixture section:playlists".into(), Shelf { cards:membership.clone(), ..Default::default() });
    for number in 1..=36_000 {
        store.track_id(&format!("track:{number}"));
    }
    for (id, first, last, stamp) in [(old, 1, 12_000, 1), (recent, 12_001, 24_000, 2), (open, 24_001, 36_000, 3)] {
        let card = store.cards.get_mut(&id).unwrap();
        card.tracks = Some((first..=last).map(|number| -number).collect()); card.read_at = stamp;
    }
    let loose = store.take_item_track("fixture", &json!({"kind":"track","ref":"track:favorite","title":"Favorite"}), None, 1).unwrap();
    store.shelves.insert("fixture section:tracks".into(), Shelf { tracks:vec![loose], ..Default::default() });
    store.index();
    store.prune(&["track:1".into()].into(), &[open].into());
    assert_eq!(store.shelves["fixture section:playlists"].cards, membership, "I retain every saved card and its order");
    assert!(store.cards[&old].tracks.is_none(), "saved membership does not retain an inactive full row cache");
    assert_eq!(store.cards[&recent].tracks.as_ref().unwrap().len(), 12_000);
    assert_eq!(store.cards[&open].tracks.as_ref().unwrap().len(), 12_000, "the open card stays complete");
    assert!(store.track_by_path("track:1").is_some(), "an individual queue root survives its old playlist cache");
    assert_eq!(store.shelves["fixture section:tracks"].tracks, vec![loose]);
    assert!(store.tracks.contains_key(&loose), "loose collection membership remains complete");
    assert_eq!(store.tracks.len(), 24_002);
}
