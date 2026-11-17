use super::*;
use crate::source_catalog::{collection_window::ready_test_catalog, fetch, Job};

fn record(reference: &str) -> Value {
    json!({"kind":"album","ref":reference,"title":"Record"})
}

fn page() -> Value {
    json!({"layout":"collection","items":[record("album:first"),record("album:second"),
        {"kind":"track","ref":"track:other","title":"Another track"}]})
}

#[test]
fn a_card_action_does_not_adopt_its_unselected_neighbors() {
    let catalog = ready_test_catalog();
    let (lease, action) = catalog.collection_action("fixture", &page(), 1, 1).unwrap();
    let id = action["cardId"].as_i64().unwrap();
    assert_eq!(catalog.lock().store.cards[&id].identity, "album:second");
    assert_eq!(catalog.lock().store.cards.len(), 1);
    assert!(catalog.lock().store.tracks.is_empty());
    drop(lease);
}

#[test]
fn overlapping_preview_and_action_release_their_shared_entity_after_the_last_close() {
    let catalog = ready_test_catalog();
    let wire = json!({"layout":"collection","items":[record("album:first")]});
    let (first, initial) = catalog.collection_action("fixture", &wire, 0, 1).unwrap();
    let (second, same) = catalog.collection_action("fixture", &wire, 0, 2).unwrap();
    assert_eq!(initial["cardId"], same["cardId"]);
    drop(first);
    catalog.reap_collection_actions();
    assert_eq!(catalog.lock().store.cards.len(), 1);
    drop(second);
    catalog.reap_collection_actions();
    assert!(catalog.lock().store.cards.is_empty());
}

#[test]
fn hydrated_preview_rows_are_released_after_close_or_late_job_completion() {
    for close_early in [false, true] {
        let catalog = ready_test_catalog();
        let album = record("album:first");
        let wire = json!({"layout":"collection","items":[album]});
        let (lease, action) = catalog.collection_action("fixture", &wire, 0, 1).unwrap();
        let mut held = Some(lease);
        let id = action["cardId"].as_i64().unwrap();
        let job = Job::Tracks(id);
        catalog.lock().running.insert(job.clone());
        if close_early {
            drop(held.take());
            catalog.reap_collection_actions();
            assert!(catalog.lock().store.cards.contains_key(&id));
        }
        let mut answer = json!({"album":album,"tracks":[{"kind":"track","ref":"track:1","title":"Song"}]});
        take_record_tracks(&mut catalog.lock(), "fixture", id, CardKind::Album, "album", &mut answer, 2);
        assert_eq!(catalog.lock().store.tracks.len(), 1);
        fetch::finish(&catalog, &job, Ok(()));
        if !close_early {
            assert!(catalog.lock().store.cards.contains_key(&id));
            drop(held.take());
            catalog.reap_collection_actions();
        }
        let inner = catalog.lock();
        assert!(inner.store.cards.is_empty() && inner.store.tracks.is_empty(), "unrooted preview payload survived close_early={close_early}");
    }
}


#[test]
fn releasing_a_hydrated_playlist_keeps_queue_parents_and_preexisting_tracks() {
    let catalog = ready_test_catalog();
    let old = json!({"kind":"track","ref":"track:old","album_ref":"album:old","title":"Existing"});
    let old_id = catalog.lock().store.take_item_track("fixture", &old, None, 1).unwrap();
    let playlist = json!({"kind":"playlist","ref":"playlist:1","title":"List"});
    let wire = json!({"layout":"collection","items":[playlist]});
    let (lease, action) = catalog.collection_action("fixture", &wire, 0, 1).unwrap();
    let id = action["cardId"].as_i64().unwrap();
    let mut answer = json!({"playlist":playlist,"tracks":[old,
        {"kind":"track","ref":"track:queued","album_ref":"album:queued","title":"Queued"},
        {"kind":"track","ref":"track:temporary","album_ref":"album:temporary","title":"Temporary"}]});
    take_record_tracks(&mut catalog.lock(), "fixture", id, CardKind::Playlist, "playlist", &mut answer, 2);
    let path = crate::source_opener::entry_of("fixture", "track:queued");
    catalog.protect_tracks([path.clone()].into());
    fetch::persist(&catalog, |snapshot| {
        let mut restored: Store = serde_json::from_slice(&serde_json::to_vec(snapshot).unwrap()).unwrap();
        restored.restore_index();
        assert!(!restored.cards.contains_key(&id));
        assert!(restored.shelves.is_empty());
        assert_eq!(restored.cards.len(), 2);
        assert_eq!(restored.tracks.len(), 2);
        assert!(restored.track_by_path(&path).is_some());
        Ok(())
    });
    assert!(catalog.lock().store.cards.contains_key(&id), "saving must preserve the live preview");
    drop(lease);
    catalog.reap_collection_actions();
    let inner = catalog.lock();
    assert!(inner.store.tracks.contains_key(&old_id));
    let queued = inner.store.track_by_path(&path).unwrap();
    assert!(inner.store.cards.contains_key(&queued.album_id));
    assert_eq!(inner.store.tracks.len(), 2);
    assert_eq!(inner.store.cards.len(), 2);
    assert!(!inner.store.cards.contains_key(&id));
}


#[test]
fn older_catalogs_drop_stale_action_shelves_without_guessing_which_ids_were_temporary() {
    let mut old = Store::default();
    let id = old.take_item("fixture", &record("album:old"), 1).unwrap();
    old.shelves.insert("fixture _collection-action:7".into(), Shelf { cards:vec![id], ..Default::default() });
    old.shelves.insert("fixture section:albums".into(), Shelf { cards:vec![id], ..Default::default() });
    let mut restored: Store = serde_json::from_slice(&serde_json::to_vec(&old).unwrap()).unwrap();
    restored.restore_index();
    assert!(!restored.shelves.contains_key("fixture _collection-action:7"));
    assert_eq!(restored.shelves["fixture section:albums"].cards, [id]);
    assert_eq!(restored.card_id("fixture", "album:old"), id);
}
