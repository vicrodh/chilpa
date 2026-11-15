//! Successful worker receipts must preserve the musical generation.
use super::*;
use crate::source_catalog::integrated_index::Source;
use serde_json::json;
use std::sync::atomic::Ordering;

fn fixture() -> (Arc<Catalog>, Vec<i64>) {
    let catalog = Catalog::new();
    let mut inner = catalog.lock();
    let albums = (0..2).map(|number| {
        inner.store.take_item("fixture", &json!({
            "kind":"album", "ref":format!("album:{number}"), "title":format!("Album {number}"),
            "artist":"Artist", "tracks":2, "genres":["Metal"],
            "image":"https://example.invalid/album.jpg", "added_at":42,
        }), 100).unwrap()
    }).collect::<Vec<_>>();
    let track = inner.store.take_item_track("fixture", &json!({
        "kind":"track", "ref":"track:only", "title":"Track", "artist":"Artist",
        "duration_ms":1000, "image":"https://example.invalid/track-only.jpg",
    }), None, 100).unwrap();
    assert!(inner.store.cards.values().all(|card| card.image != "https://example.invalid/track-only.jpg"));
    inner.store.shelves.insert("fixture section:albums".into(), Shelf { cards:albums.clone(), ..Default::default() });
    inner.store.shelves.insert("fixture section:tracks".into(), Shelf { tracks:vec![track], ..Default::default() });
    drop(inner);
    (catalog, albums)
}

fn source() -> Source {
    Source { id:"fixture".into(), name:"Fixture".into(), icon:String::new(), albums:true, tracks:true }
}

#[test]
fn cached_cover_receipt_preserves_index_generation_and_snapshot() {
    let (catalog, albums) = fixture();
    let sources = [source()];
    let (stamp, first) = catalog.integrated_index(&sources, catalog.integration_epoch());
    let captures = catalog.index_captures.load(Ordering::Relaxed);
    let before = catalog.lock().store.clone();
    let generation = before.cards.generation();
    let path = "synthetic-cached-cover.jpg";
    {
        let mut inner = catalog.lock();
        assert!(receive_cover(&mut inner, "https://example.invalid/album.jpg".into(), path));
        assert!(inner.dirty);
        assert!(inner.store.cards.matches_generation(&generation));
        for id in &albums {
            let old = before.cards.shared(id).unwrap();
            let current = inner.store.cards.shared(id).unwrap();
            assert_eq!(old.art, "");
            assert_eq!(current.art, path);
            assert!(!Arc::ptr_eq(&old, &current), "the earlier Store snapshot keeps its own Card");
            let mut expected = (*old).clone();
            expected.art = path.into();
            assert_eq!(*current, expected, "a receipt may change only the cached art path");
        }
    }
    for _ in 0..3 {
        let (next_stamp, next) = catalog.integrated_index(&sources, catalog.integration_epoch());
        assert_eq!(next_stamp, stamp);
        assert!(Arc::ptr_eq(&first, &next));
    }
    assert_eq!(catalog.index_captures.load(Ordering::Relaxed), captures);
    // A duplicate receipt changes neither persistence state nor the Card Arc.
    let cached = catalog.lock().store.cards.shared(&albums[0]).unwrap();
    {
        let mut inner = catalog.lock();
        inner.dirty = false;
        assert!(!receive_cover(&mut inner, "https://example.invalid/album.jpg".into(), path));
        assert!(!inner.dirty);
        assert!(Arc::ptr_eq(&cached, &inner.store.cards.shared(&albums[0]).unwrap()));
        assert!(inner.store.cards.matches_generation(&generation));
    }
    let (next_stamp, next) = catalog.integrated_index(&sources, catalog.integration_epoch());
    assert_eq!(next_stamp, stamp);
    assert!(Arc::ptr_eq(&first, &next));
    assert_eq!(catalog.index_captures.load(Ordering::Relaxed), captures);
    let rebuilt_catalog = Catalog::new();
    let current_store = catalog.lock().store.clone();
    rebuilt_catalog.lock().store = current_store;
    let (_, rebuilt) = rebuilt_catalog.integrated_index(&sources, rebuilt_catalog.integration_epoch());
    assert_eq!(first.items, rebuilt.items);
    assert_eq!(first.keys, rebuilt.keys);
    assert_eq!(first.versions, rebuilt.versions);
    assert_eq!(first.sources, rebuilt.sources);
}

#[test]
fn track_only_and_unmatched_receipts_preserve_index_without_card_mutation() {
    let (catalog, _) = fixture();
    let sources = [source()];
    let (stamp, first) = catalog.integrated_index(&sources, catalog.integration_epoch());
    let captures = catalog.index_captures.load(Ordering::Relaxed);
    let before = catalog.lock().store.clone();
    let generation = before.cards.generation();
    for image in ["https://example.invalid/track-only.jpg", "https://example.invalid/unknown.jpg"] {
        {
            let mut inner = catalog.lock();
            inner.dirty = false;
            assert!(receive_cover(&mut inner, image.into(), "synthetic-track-cover.jpg"));
            assert!(inner.art_received.contains(image));
            assert!(!inner.dirty, "a track-only receipt has no persistent Card change");
            assert!(inner.store.cards.matches_generation(&generation));
            for id in before.cards.keys() {
                assert!(Arc::ptr_eq(&before.cards.shared(id).unwrap(), &inner.store.cards.shared(id).unwrap()));
            }
        }
        let (next_stamp, next) = catalog.integrated_index(&sources, catalog.integration_epoch());
        assert_eq!(next_stamp, stamp);
        assert!(Arc::ptr_eq(&first, &next));
        assert_eq!(catalog.index_captures.load(Ordering::Relaxed), captures);
    }
}
