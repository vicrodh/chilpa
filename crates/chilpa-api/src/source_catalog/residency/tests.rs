use super::*;
use crate::source_catalog::store::{Shelf, Store};
use serde_json::json;
use std::sync::Arc;

fn fixture() -> (Arc<Catalog>, i64, i64, String) {
    let catalog = Catalog::new();
    let (first, second, path) = {
        let mut inner = catalog.lock();
        inner.loaded = true;
        let first = inner.store.take_item("fixture", &json!({"kind":"playlist","ref":"list:one","title":"One","editable":true}), 7).unwrap();
        let second = inner.store.take_item("fixture", &json!({"kind":"playlist","ref":"list:two","title":"Two","folder":"Saved"}), 8).unwrap();
        let mut path = String::new();
        for (list, reference) in [(first, "one"), (second, "two")] {
            let track = inner.store.take_item_track("fixture", &json!({"kind":"track","ref":format!("track:{reference}"),"title":reference,
                "album_ref":format!("album:{reference}"),"album":reference,"duration_ms":123000,"quality":{"codec":"flac","bits":24,"rate":96000}}), None, 9).unwrap();
            inner.store.cards.get_mut(&list).unwrap().tracks = Some(vec![track]);
            inner.store.cards.get_mut(&list).unwrap().read_at = 9;
            if list == first { path = inner.store.tracks[&track].path.clone(); }
        }
        inner.store.shelves.insert("fixture section:playlists".into(), Shelf {
            cards:vec![second, first], membership:[(first, "favorites,purchases".into()), (second, "favorites".into())].into(),
            document:json!({"folders":[{"id":"folder:1","name":"Saved"}]}), read_at:10, ..Default::default()
        });
        inner.store.names.insert("fixture".into(), "Fixture".into());
        inner.store.drawn.insert("fixture".into(), true);
        inner.store.take_item("unknown", &json!({"kind":"album","ref":"album:legacy","title":"Legacy"}), 11);
        (first, second, path)
    };
    catalog.publish(false);
    (catalog, first, second, path)
}

fn maintain(catalog: &Catalog, root: &Path) {
    catalog.maintain_residency_with(root, write);
}

fn restart(catalog: &Catalog) -> Arc<Catalog> {
    let json = serde_json::to_vec(&catalog.lock().store.snapshot()).unwrap();
    let mut store: Store = serde_json::from_slice(&json).unwrap();
    store.index();
    let next = Catalog::new();
    let mut inner = next.lock();
    inner.store = store;
    inner.loaded = true;
    inner.residency.changed();
    drop(inner);
    next
}

#[test]
fn disable_save_restart_enable_preserves_membership_ids_and_metadata() {
    let root = tempfile::tempdir().unwrap();
    let (catalog, first, second, path) = fixture();
    let before = catalog.lock().store.source_snapshot("fixture");
    catalog.set_source_states([("fixture".into(), false)]);
    maintain(&catalog, root.path());
    assert!(catalog.lock().store.cards.values().all(|card| card.plugin != "fixture"));
    assert!(catalog.lock().store.shelves.get("fixture section:playlists").is_none());
    assert!(archive_path(root.path(), "fixture").is_file());
    let resumed = restart(&catalog);
    resumed.set_source_states([("fixture".into(), true)]);
    maintain(&resumed, root.path());
    let mut inner = resumed.lock();
    assert_eq!(inner.store.card_id("fixture", "list:one"), first);
    assert_eq!(inner.store.card_id("fixture", "list:two"), second);
    assert_eq!(inner.store.track_by_path(&path), before.track_by_path(&path));
    assert_eq!(inner.store.cards[&first], before.cards[&first]);
    assert_eq!(inner.store.shelves["fixture section:playlists"], before.shelves["fixture section:playlists"]);
    assert!(!inner.store.cold_sources.contains_key("fixture"));
    assert!(archive_path(root.path(), "fixture").is_file(), "I retain the archive until the main snapshot is durable");
    let new_card = inner.store.card_id("fresh", "album:new");
    let new_track = inner.store.track_id("fresh-track");
    assert!(new_card < *before.cards.keys().min().unwrap());
    assert!(new_track < *before.tracks.keys().min().unwrap());
}

#[test]
fn an_unopened_cold_source_is_not_decoded_and_unknown_live_data_is_kept() {
    let root = tempfile::tempdir().unwrap();
    let (catalog, _, _, _) = fixture();
    catalog.set_source_states([("fixture".into(), false)]);
    maintain(&catalog, root.path());
    let resumed = restart(&catalog);
    std::fs::write(archive_path(root.path(), "fixture"), b"not decoded").unwrap();
    resumed.set_source_states([("fixture".into(), false)]);
    maintain(&resumed, root.path());
    let inner = resumed.lock();
    assert!(inner.residency.retry_after.is_none(), "an unopened source does not even decode its cold cache");
    assert!(inner.store.cards.values().all(|card| card.plugin != "fixture"));
    assert!(inner.store.cards.values().any(|card| card.plugin == "unknown"), "missing registry entries are not disabled");
}

#[test]
fn disabling_waits_for_pending_rpc_and_archives_its_final_answer() {
    let root = tempfile::tempdir().unwrap();
    let (catalog, _, _, _) = fixture();
    let job = Job::Shelf("fixture section:playlists".into());
    catalog.lock().running.insert(job.clone());
    catalog.set_source_states([("fixture".into(), false)]);
    maintain(&catalog, root.path());
    assert!(!archive_path(root.path(), "fixture").exists());
    {
        let mut inner = catalog.lock();
        inner.store.take_item("fixture", &json!({"kind":"album","ref":"album:late","title":"Last answer"}), 20);
        inner.running.remove(&job);
    }
    maintain(&catalog, root.path());
    let archived = read(root.path(), "fixture").unwrap();
    assert!(archived.cards.values().any(|card| card.identity == "album:late" && card.title == "Last answer"));
    assert!(catalog.lock().store.cards.values().all(|card| card.plugin != "fixture"));
}

#[test]
fn enable_during_archive_write_cancels_detach_and_write_errors_preserve_live_data() {
    let root = tempfile::tempdir().unwrap();
    let (catalog, first, _, _) = fixture();
    catalog.set_source_states([("fixture".into(), false)]);
    catalog.maintain_residency_with(root.path(), |root, plugin, source| {
        write(root, plugin, source)?;
        catalog.set_source_states([("fixture".into(), true)]);
        Ok(())
    });
    assert!(catalog.lock().store.cards.contains_key(&first));
    assert!(!catalog.lock().store.cold_sources.contains_key("fixture"));
    catalog.set_source_states([("fixture".into(), false)]);
    catalog.maintain_residency_with(root.path(), |_, _, _| Err(std::io::Error::other("disk full")));
    let inner = catalog.lock();
    assert!(inner.store.cards.contains_key(&first));
    assert!(!inner.store.cold_sources.contains_key("fixture"));
    assert!(inner.residency.pending);
}

#[test]
fn a_changed_live_snapshot_is_never_detached_after_a_stale_write() {
    let root = tempfile::tempdir().unwrap();
    let (catalog, first, _, _) = fixture();
    catalog.set_source_states([("fixture".into(), false)]);
    catalog.maintain_residency_with(root.path(), |root, plugin, source| {
        write(root, plugin, source)?;
        catalog.lock().store.cards.get_mut(&first).unwrap().title = "Newer metadata".into();
        Ok(())
    });
    assert_eq!(catalog.lock().store.cards[&first].title, "Newer metadata");
    assert!(!catalog.lock().store.cold_sources.contains_key("fixture"));
    maintain(&catalog, root.path());
    assert_eq!(read(root.path(), "fixture").unwrap().cards[&first].title, "Newer metadata");
}

#[test]
fn queue_open_pages_and_selection_roots_remain_readable_after_disable_and_restart() {
    let root = tempfile::tempdir().unwrap();
    let (catalog, first, second, path) = fixture();
    catalog.protect_tracks([path.clone()].into());
    let selection = catalog.hold([("fixture".into(), "list:two".into())], []);
    catalog.protect_navigation(&[crate::navigation::Route { screen:crate::navigation::Screen::Source, id:0, key:"fixture section:playlists".into() }]);
    catalog.set_source_states([("fixture".into(), false)]);
    maintain(&catalog, root.path());
    let resumed = restart(&catalog);
    let inner = resumed.lock();
    for id in [first, second] {
        let card = &inner.store.cards[&id];
        for track in card.tracks.as_ref().unwrap() {
            assert!(inner.store.tracks.contains_key(track));
            assert!(inner.store.cards.contains_key(&inner.store.tracks[track].album_id));
        }
    }
    assert!(inner.store.track_by_path(&path).is_some());
    assert_eq!(inner.store.shelves["fixture section:playlists"].cards, vec![second, first]);
    drop(inner);
    drop(selection);
}

#[test]
fn opening_a_cold_record_restores_only_its_closure_and_preserves_fresh_live_metadata() {
    let root = tempfile::tempdir().unwrap();
    let (catalog, first, second, path) = fixture();
    catalog.set_source_states([("fixture".into(), false)]);
    maintain(&catalog, root.path());
    let resumed = restart(&catalog);
    resumed.set_source_states([("fixture".into(), false)]);
    resumed.protect_navigation(&[crate::navigation::Route { screen:crate::navigation::Screen::Playlist, id:first, key:String::new() }]);
    let before = resumed.revisions();
    maintain(&resumed, root.path());
    assert!(resumed.revisions().0 > before.0, "a previously missing page gets a content notification");
    {
        let mut inner = resumed.lock();
        assert!(inner.store.cards.contains_key(&first));
        assert!(!inner.store.cards.contains_key(&second));
        assert!(inner.store.track_by_path(&path).is_some());
        assert!(inner.store.shelves.get("fixture section:playlists").is_none());
        inner.store.cards.get_mut(&first).unwrap().title = "Fresh resident metadata".into();
    }
    resumed.set_source_states([("fixture".into(), true)]);
    maintain(&resumed, root.path());
    assert_eq!(resumed.lock().store.cards[&first].title, "Fresh resident metadata");
    assert!(resumed.lock().store.cards.contains_key(&second));
}

#[test]
fn cold_identity_reservations_survive_pruning_and_selection_uses_the_same_id() {
    let root = tempfile::tempdir().unwrap();
    let (catalog, first, _, _) = fixture();
    catalog.set_source_states([("fixture".into(), false)]);
    maintain(&catalog, root.path());
    let resumed = restart(&catalog);
    let selection = resumed.hold([("fixture".into(), "list:one".into())], []);
    {
        let mut inner = resumed.lock();
        inner.store.prune(&Default::default(), &Default::default());
        assert_eq!(inner.store.card_id("fixture", "list:one"), first);
    }
    maintain(&resumed, root.path());
    assert_eq!(resumed.lock().store.cards[&first].title, "One");
    assert!(resumed.lock().store.cards[&first].tracks.is_some());
    drop(selection);
}

#[test]
fn a_stale_track_selection_reuses_the_archived_id_without_decoding_on_the_ui_thread() {
    let root = tempfile::tempdir().unwrap();
    let (catalog, _, _, path) = fixture();
    let track = catalog.lock().store.track_by_path(&path).unwrap().id;
    catalog.set_source_states([("fixture".into(), false)]);
    maintain(&catalog, root.path());
    let resumed = restart(&catalog);
    let _hold = resumed.hold([], [path.clone()]);
    let mut inner = resumed.lock();
    let selected = inner.store.take_item_track("fixture", &json!({"kind":"track","ref":"track:one","title":"Fresh selection"}), None, 30).unwrap();
    assert_eq!(selected, track);
    assert_eq!(inner.store.track_by_path(&path).unwrap().title, "Fresh selection");
    drop(inner);
    resumed.set_source_states([("fixture".into(), true)]);
    maintain(&resumed, root.path());
    let inner = resumed.lock();
    assert_eq!(inner.store.track_by_path(&path).unwrap().id, track);
    assert_eq!(inner.store.track_by_path(&path).unwrap().title, "Fresh selection");
    assert_eq!(inner.store.tracks.values().filter(|row| row.path == path).count(), 1);
}

#[test]
fn disabling_cancels_queued_work_and_rejects_stale_queue_requests() {
    let (catalog, first, _, path) = fixture();
    {
        let mut inner = catalog.lock();
        for job in [Job::Shelf("fixture section:playlists".into()), Job::Tracks(first), Job::Track(path.clone())] {
            catalog.queue(&mut inner, job, true);
        }
        catalog.queue(&mut inner, Job::Shelf("unknown root".into()), false);
    }
    catalog.set_source_states([("fixture".into(), false)]);
    let mut inner = catalog.lock();
    assert_eq!(inner.queue.queued.len(), 1);
    assert!(inner.queue.queued.contains(&Job::Shelf("unknown root".into())));
    catalog.queue(&mut inner, Job::Tracks(first), true);
    catalog.queue(&mut inner, Job::Track(path), false);
    assert_eq!(inner.queue.queued.len(), 1);
}

#[test]
fn revoked_domains_do_not_fetch_disabled_artwork_and_shared_images_keep_an_active_owner() {
    let (catalog, first, _, _) = fixture();
    {
        let mut inner = catalog.lock();
        inner.store.cards.get_mut(&first).unwrap().image = "shared-image".into();
        inner.domains.insert("fixture".into(), vec!["old-approval".into()]);
    }
    catalog.set_source_states([("fixture".into(), false)]);
    let mut inner = catalog.lock();
    assert!(!inner.domains.contains_key("fixture"));
    assert_eq!(cover_source(&inner, "shared-image"), None);
    let other = inner.store.card_id("active", "album:shared");
    inner.store.cards.get_mut(&other).unwrap().image = "shared-image".into();
    assert_eq!(cover_source(&inner, "shared-image"), Some("active".into()));
}

#[test]
fn detaching_releases_published_and_entity_observer_payloads() {
    let root = tempfile::tempdir().unwrap();
    let (catalog, first, _, _) = fixture();
    let weak = Arc::downgrade(&catalog.lock().store.cards.shared(&first).unwrap());
    catalog.entity_revision(first);
    catalog.set_source_states([("fixture".into(), false)]);
    maintain(&catalog, root.path());
    assert!(weak.upgrade().is_none(), "published and entity observations cannot retain a detached source's payload");
    assert!(!catalog.lock().entity_revisions.contains_key(&first));
}

#[test]
fn compact_cached_playlists_preserve_public_rows_and_local_organization_across_residency() {
    let root = tempfile::tempdir().unwrap();
    let (catalog, first, second, _) = fixture();
    {
        let mut inner = catalog.lock();
        let card = inner.store.cards.get_mut(&first).unwrap();
        card.metadata["description"] = json!("Cached description");
        card.metadata["tracks"] = json!(1);
        card.metadata["updated_at"] = json!(77);
        card.metadata["unused_payload"] = json!("x".repeat(100_000));
        card.art = "cached-cover.png".into();
    }
    let mut api = crate::Chilpa::new();
    api.library = Some(chilpa_library::Library::in_memory().unwrap());
    api.catalog = catalog.clone();
    api.cached_playlists();
    let stored = api.library.as_ref().unwrap().playlists().unwrap();
    let storage = stored.iter().find(|row| row.external_id == "list:one").unwrap().id;
    let folder = {
        let lib = api.library.as_ref().unwrap();
        let folder = lib.create_playlist_folder("Grouped", "blue").unwrap();
        lib.update_playlist(storage, &chilpa_library::PlaylistEdit { name:"Local organization".into(), tags:vec!["tag".into()], folder_id:Some(folder), ..Default::default() }).unwrap();
        lib.set_playlist_custom_order(&[storage]).unwrap();
        lib.set_playlist_visible(storage, false).unwrap();
        folder
    };
    let before = api.cached_playlists();
    let expected = before.iter().find(|row| row.id == first).unwrap().clone();
    assert_eq!(expected.folder_id, Some(folder));
    assert_eq!(expected.tags, vec!["tag"]);
    assert_eq!(expected.provider, "Fixture");
    assert_eq!(expected.artwork_path, "cached-cover.png");
    catalog.set_source_states([("fixture".into(), false)]);
    maintain(&catalog, root.path());
    {
        let inner = catalog.lock();
        assert!(inner.store.cards.values().all(|card| card.plugin != "fixture"));
        assert!(inner.store.tracks.values().all(|track| track.plugin != "fixture"));
        let summaries = &inner.store.cold_sources["fixture"].playlists;
        assert_eq!(summaries.iter().map(|row| row.id).collect::<Vec<_>>(), vec![second, first]);
        assert_eq!(summaries[1].membership, "favorites,purchases");
        assert!(summaries[1].editable);
        assert!(!serde_json::to_string(summaries).unwrap().contains("unused_payload"));
    }
    let archive = archive_path(root.path(), "fixture");
    let bytes = std::fs::read(&archive).unwrap();
    std::fs::write(&archive, b"not decoded by cached playlist APIs").unwrap();
    assert_eq!(api.cached_playlists(), before);
    assert_eq!(api.playlist(first), Some(expected.clone()));
    assert_eq!(api.playlist_storage_id(first), Some(storage));
    assert!(api.playlist_targets(&[], "").iter().any(|target| target.playlist == expected && !target.accepts));
    assert!(api.managed_playlists("One", "playlists", "custom", true, Some(folder)).iter().any(|row| row.id == first));
    api.catalog = restart(&catalog);
    assert_eq!(api.cached_playlists(), before);
    assert_eq!(api.playlist(first), Some(expected));
    std::fs::write(&archive, bytes).unwrap();
    api.catalog.set_source_states([("fixture".into(), true)]);
    maintain(&api.catalog, root.path());
    assert_eq!(api.cached_playlists(), before);
}

#[test]
fn a_deferred_cold_playlist_waits_for_archive_hydration_instead_of_disappearing() {
    let root = tempfile::tempdir().unwrap();
    let (catalog, first, _, _) = fixture();
    catalog.set_source_states([("fixture".into(), false)]);
    maintain(&catalog, root.path());
    let resumed = restart(&catalog);
    resumed.defer(vec![first], "later", false, None);
    assert!(resumed.ready().0.is_empty());
    assert!(resumed.actions_pending());
    maintain(&resumed, root.path());
    let (ready, hold) = resumed.ready();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].albums, vec![first]);
    assert!(hold.is_some());
    assert!(resumed.tracks(first).is_some());
}

#[test]
fn missing_or_corrupt_enabled_archives_allow_rebuild_with_reserved_ids() {
    for corrupt in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (catalog, first, _, path) = fixture();
        let track = catalog.lock().store.track_by_path(&path).unwrap().id;
        let album = catalog.lock().store.track_by_path(&path).unwrap().album_id;
        catalog.set_source_states([("fixture".into(), false)]);
        maintain(&catalog, root.path());
        let bytes = std::fs::read(archive_path(root.path(), "fixture")).unwrap();
        let resumed = restart(&catalog);
        if corrupt { std::fs::write(archive_path(root.path(), "fixture"), b"invalid JSON").unwrap(); }
        else { std::fs::remove_file(archive_path(root.path(), "fixture")).unwrap(); }
        resumed.set_source_states([("fixture".into(), true)]);
        let before = resumed.revisions();
        maintain(&resumed, root.path());
        assert!(resumed.revisions().0 > before.0, "rebuilding publishes a fresh opportunity to request work");
        resumed.defer(vec![first, album], "later", false, None);
        assert!(resumed.ready().0.is_empty());
        assert!(resumed.actions_pending());
        std::fs::write(archive_path(root.path(), "fixture"), bytes).unwrap();
        maintain(&resumed, root.path());
        {
            let mut inner = resumed.lock();
            assert!(!source_inactive(&inner, "fixture"));
            assert!(inner.store.cold_sources.contains_key("fixture"));
            assert_eq!(inner.store.cards[&first].kind, super::super::store::CardKind::Playlist);
            assert_eq!(inner.store.cards[&album].kind, super::super::store::CardKind::Album);
            assert_eq!(inner.store.cards.values().filter(|card| card.plugin == "fixture").count(), 2);
            assert!(inner.store.cards[&first].tracks.is_none(), "a missing archive rebuilds only requested identities");
            assert!(inner.queue.queued.contains(&Job::Tracks(first)));
            assert!(inner.queue.queued.contains(&Job::Tracks(album)));
            resumed.queue(&mut inner, Job::Shelf("fixture section:playlists".into()), true);
            assert!(inner.queue.queued.contains(&Job::Shelf("fixture section:playlists".into())));
            assert_eq!(inner.store.card_id("fixture", "list:one"), first);
            assert_eq!(inner.store.track_id(&path), track);
            inner.residency.changed();
        }
        resumed.maintain_residency_with(root.path(), |_, _, _| panic!("ordinary root changes cannot retry a failed enabled archive"));
        assert!(resumed.lock().residency.unavailable_archives.contains("fixture"));
        assert!(resumed.lock().store.cold_sources.contains_key("fixture"));
        let restarted = restart(&resumed);
        assert_eq!(restarted.lock().store.card_id("fixture", "list:one"), first);
        assert_eq!(restarted.lock().store.track_id(&path), track);
        resumed.set_source_states([("fixture".into(), false)]);
        assert!(source_inactive(&resumed.lock(), "fixture"));
    }
}

#[test]
fn chosen_art_survives_detach_and_enable_without_retaining_card_payloads() {
    let root = tempfile::tempdir().unwrap();
    let (catalog, first, _, _) = fixture();
    catalog.lock().store.chosen.insert(("fixture".into(), "list:one".into()), "chosen-cover.png".into());
    catalog.set_source_states([("fixture".into(), false)]);
    maintain(&catalog, root.path());
    assert!(!catalog.lock().store.cards.contains_key(&first));
    assert_eq!(catalog.lock().store.chosen.get(&("fixture".into(), "list:one".into())).map(String::as_str), Some("chosen-cover.png"));
    catalog.set_source_states([("fixture".into(), true)]);
    maintain(&catalog, root.path());
    assert_eq!(catalog.card(first).unwrap().art, "chosen-cover.png");
}
