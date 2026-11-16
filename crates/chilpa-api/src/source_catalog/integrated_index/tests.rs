use super::*;
use super::super::{composers, genres, library_index, store::{Format, Shelf}};
use serde_json::json;

mod measurement;

fn source() -> Source {
    Source { id: "fixture".into(), albums: true, tracks: true, name: "Fixture".into(), icon: "fixture.svg".into() }
}

fn fixture() -> (Arc<Catalog>, i64, i64) {
    let catalog = Catalog::new();
    let (album, track) = {
        let mut inner = catalog.lock();
        let album = inner.store.take_item("fixture", &json!({"kind":"album", "ref":"album:1", "title":"Album",
            "artist":"Artist", "upc":"123456789012", "genres":["Heavy Metal"], "added_at":12, "favorite":true}), 10).unwrap();
        let track = inner.store.take_item_track("fixture", &json!({"kind":"track", "ref":"track:1", "title":"Track",
            "artist":"Guest", "duration_ms":1000, "isrc":"USAAA0100001"}), None, 10).unwrap();
        inner.store.tracks.get_mut(&track).unwrap().album_id = album;
        inner.store.cards.get_mut(&album).unwrap().tracks = Some(vec![track]);
        inner.store.shelves.insert("fixture section:albums".into(), Shelf { cards:vec![album], read_at:10, ..Default::default() });
        inner.store.shelves.insert("fixture section:tracks".into(), Shelf { tracks:vec![track], read_at:10, ..Default::default() });
        (album, track)
    };
    (catalog, album, track)
}

fn legacy(catalog: &Catalog, sources: &[Source], at: u64) -> SourceIndex {
    let inner = catalog.lock();
    let albums = sources.iter().filter(|s| s.albums).map(|s| s.id.clone()).collect::<Vec<_>>();
    let tracks = sources.iter().filter(|s| s.tracks).map(|s| s.id.clone()).collect::<Vec<_>>();
    let chosen = sources.iter().map(|s| (s.id.clone(), s.albums, s.tracks)).collect::<Vec<_>>();
    let items = library_index::items_of(&inner.store, &albums, &tracks, &inner.genres, &inner.composers, at);
    let mut facets: Vec<crate::FacetEntry> = Vec::new();
    for item in &items {
        if let Some(facet) = facets.iter_mut().find(|f| f.key == item.source_key()) { facet.albums += item.tracks; }
        else {
            let source = sources.iter().find(|s| s.id == item.plugin).unwrap();
            facets.push(crate::FacetEntry { key:item.source_key(), label:inner.store.names.get(&source.id).unwrap_or(&source.name).clone(),
                icon:source.icon.clone(), albums:item.tracks, year:None });
        }
    }
    let mut index = SourceIndex::new(items, facets);
    index.versions.extend(library_index::records_of(&inner.store, &chosen).into_iter().map(|r| (r.id, r.version())));
    index
}

fn equal(left: &SourceIndex, right: &SourceIndex) {
    assert_eq!(left.items, right.items);
    assert_eq!(left.keys, right.keys);
    assert_eq!(left.versions, right.versions);
    assert_eq!(left.sources, right.sources);
}

#[test]
fn same_music_reuses_publication_despite_read_timestamps_and_artwork() {
    let (catalog, album, _) = fixture();
    let sources = [source()];
    let (stamp, first) = catalog.integrated_index(&sources, catalog.integration_epoch());
    equal(&first, &legacy(&catalog, &sources, super::super::now()));
    {
        let mut inner = catalog.lock();
        let card = inner.store.cards.get_mut(&album).unwrap();
        card.read_at += 1;
        card.seen_at += 1;
        card.image = "other-cover".into();
        card.art = "cached-cover".into();
        inner.store.shelves.get_mut("fixture section:albums").unwrap().read_at += 1;
    }
    let (again, second) = catalog.integrated_index(&sources, catalog.integration_epoch());
    assert_eq!(stamp, again);
    assert!(Arc::ptr_eq(&first, &second));
    equal(&second, &legacy(&catalog, &sources, super::super::now()));
}

#[test]
fn musical_changes_in_the_same_second_invalidate_all_consumed_values() {
    let (catalog, album, track) = fixture();
    let sources = [source()];
    let (_, mut previous) = catalog.integrated_index(&sources, catalog.integration_epoch());
    for field in ["title", "duration", "quality", "genres", "composer", "work", "playable", "added_at", "favorite", "advisory", "membership"] {
        {
            let mut inner = catalog.lock();
            match field {
                "title" => inner.store.cards.get_mut(&album).unwrap().title = "Changed album".into(),
                "duration" => inner.store.tracks.get_mut(&track).unwrap().duration_ms = 8888,
                "quality" => inner.store.tracks.get_mut(&track).unwrap().format = Some(Format { codec:"flac".into(), bits:Some(24), rate:Some(96000), ..Default::default() }),
                "genres" => inner.store.tracks.get_mut(&track).unwrap().metadata["genres"] = json!(["Jazz"]),
                "composer" => inner.store.tracks.get_mut(&track).unwrap().metadata["composers"] = json!(["Writer"]),
                "work" => inner.store.tracks.get_mut(&track).unwrap().metadata["work"] = json!("Suite"),
                "playable" => { let song = inner.store.tracks.get_mut(&track).unwrap(); song.playable = !song.playable; },
                "added_at" => inner.store.cards.get_mut(&album).unwrap().metadata["added_at"] = json!(42),
                "favorite" => inner.store.cards.get_mut(&album).unwrap().metadata["favorite"] = json!(false),
                "advisory" => inner.store.cards.get_mut(&album).unwrap().metadata["explicit"] = json!(true),
                "membership" => inner.store.shelves.get_mut("fixture section:tracks").unwrap().tracks.clear(),
                _ => unreachable!(),
            }
            assert_eq!(inner.store.shelves.get("fixture section:albums").unwrap().read_at, 10);
        }
        let (_, next) = catalog.integrated_index(&sources, catalog.integration_epoch());
        assert!(!Arc::ptr_eq(&previous, &next), "{field}");
        equal(&next, &legacy(&catalog, &sources, super::super::now()));
        previous = next;
    }
}

#[test]
fn context_changes_and_opt_out_release_the_previous_publication() {
    let (catalog, _, _) = fixture();
    let mut sources = [source()];
    let (_, first) = catalog.integrated_index(&sources, catalog.integration_epoch());
    let weak = Arc::downgrade(&first);
    drop(first);
    catalog.lock().store.names.insert("fixture".into(), "Renamed source".into());
    let (_, renamed) = catalog.integrated_index(&sources, catalog.integration_epoch());
    assert!(weak.upgrade().is_none());
    assert_eq!(renamed.sources[0].label, "Renamed source");
    sources[0].icon = "new.svg".into();
    let (_, icon) = catalog.integrated_index(&sources, catalog.integration_epoch());
    assert_eq!(icon.sources[0].icon, "new.svg");
    sources[0].albums = false;
    let (_, tracks) = catalog.integrated_index(&sources, catalog.integration_epoch());
    assert!(!tracks.items[0].saved_album);
    let weak = Arc::downgrade(&tracks);
    drop(tracks);
    assert!(catalog.integrated_index(&[], catalog.integration_epoch()).1.items.is_empty());
    assert!(weak.upgrade().is_none());
}

#[test]
fn enrichment_expires_without_a_timer_or_timestamp_mutation() {
    let (catalog, album, _) = fixture();
    {
        let mut inner = catalog.lock();
        inner.store.cards.get_mut(&album).unwrap().metadata["genres"] = json!([]);
        inner.genres = genres::State::fixture("fixture", "123456789012", vec!["Enriched genre".into()], 100);
        inner.composers = composers::State::fixture("fixture", "USAAA0100001", vec!["Enriched composer".into()], 100);
    }
    let sources = [source()];
    let (_, first) = catalog.integrated_index_with_clock(&sources, catalog.integration_epoch(), || 101);
    assert_eq!(first.items[0].genres, vec!["Enriched genre"]);
    assert_eq!(first.items[0].composers, vec!["Enriched composer"]);
    let (before, same) = catalog.integrated_index_with_clock(&sources, catalog.integration_epoch(), || 102);
    assert!(Arc::ptr_eq(&first, &same));
    let (after, expired) = catalog.integrated_index_with_clock(&sources, catalog.integration_epoch(), || 100 + 91 * 86400);
    assert_ne!(before, after);
    assert!(expired.items[0].genres.is_empty());
    assert!(expired.items[0].composers.is_empty());
    equal(&expired, &legacy(&catalog, &sources, 100 + 91 * 86400));
}

#[test]
fn concurrent_readers_share_one_builder_and_instances_remain_isolated() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<SourceIndex>();
    let (catalog, _, _) = fixture();
    let barrier = Arc::new(std::sync::Barrier::new(4));
    let threads = (0..4).map(|_| {
        let catalog = catalog.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || { barrier.wait(); catalog.integrated_index(&[source()], catalog.integration_epoch()) })
    }).collect::<Vec<_>>();
    let results = threads.into_iter().map(|t| t.join().unwrap()).collect::<Vec<_>>();
    assert!(results.iter().all(|(stamp, index)| stamp == &results[0].0 && Arc::ptr_eq(index, &results[0].1)));
    assert_eq!(catalog.lock().index_serial, 1);
    assert_eq!(catalog.index_captures.load(std::sync::atomic::Ordering::Relaxed), 1);
    let (other, _, _) = fixture();
    let (_, isolated) = other.integrated_index(&[source()], other.integration_epoch());
    assert!(!Arc::ptr_eq(&isolated, &results[0].1));
    equal(&isolated, &results[0].1);
}

#[test]
fn stale_captures_are_detected_and_invalidation_never_waits_for_builder() {
    let (catalog, album, _) = fixture();
    let captured = inputs::Inputs::capture(&catalog, &[source()], super::super::now());
    catalog.lock().store.cards.get_mut(&album).unwrap().title = "New".into();
    assert!(!captured.is_current(&catalog.lock(), super::super::now()));
    assert_eq!(captured.build().items[0].album_title, "Album");
    let captured = inputs::Inputs::capture(&catalog, &[source()], super::super::now());
    let builder = catalog.index_builder.lock().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let other = catalog.clone();
    let thread = std::thread::spawn(move || { other.clear_integrated_index(); send.send(()).unwrap(); });
    let completed = receive.recv_timeout(std::time::Duration::from_secs(5));
    drop(builder);
    thread.join().unwrap();
    completed.unwrap();
    assert!(!captured.is_current(&catalog.lock(), super::super::now()));
}

#[test]
fn readers_invalidate_refreshes_and_never_retain_an_old_index() {
    let (catalog, _, track) = fixture();
    let readers = (0..2).map(|_| crate::Reader::of(chilpa_library::Library::in_memory().unwrap())).collect::<Vec<_>>();
    let sources = [source()];
    let (stamp, first) = catalog.integrated_index(&sources, catalog.integration_epoch());
    let filter = crate::Filter { genre:Some("Heavy Metal".into()), ..Default::default() };
    for reader in &readers {
        let (_, snapshot) = reader.refresh_using(&filter, crate::TrackOrder::Artist, || true, (stamp.clone(), first.clone())).unwrap();
        assert_eq!(snapshot.ids, vec![track]);
    }
    let old = Arc::downgrade(&first);
    drop(first);
    catalog.lock().store.tracks.get_mut(&track).unwrap().metadata["genres"] = json!(["Jazz"]);
    let (stamp, next) = catalog.integrated_index(&sources, catalog.integration_epoch());
    assert!(old.upgrade().is_none());
    for reader in &readers {
        let (_, snapshot) = reader.refresh_using(&filter, crate::TrackOrder::Artist, || true, (stamp.clone(), next.clone())).unwrap();
        assert!(snapshot.ids.is_empty());
    }
    let weak = Arc::downgrade(&next);
    drop(next);
    drop(catalog);
    assert!(weak.upgrade().is_none());
    assert_eq!(readers.len(), 2);
}

#[test]
fn an_old_integration_selection_cannot_republish_after_opt_out_or_forget() {
    let (catalog, _, _) = fixture();
    let sources = [source()];
    let selected_at = catalog.integration_epoch();
    catalog.clear_integrated_index();
    let (_, in_flight) = catalog.integrated_index(&sources, selected_at);
    assert_eq!(in_flight.items.len(), 1);
    assert!(catalog.lock().integrated_index.is_none());
    drop(in_flight);
    let (_, current) = catalog.integrated_index(&sources, catalog.integration_epoch());
    let old = Arc::downgrade(&current);
    drop(current);
    catalog.forget_plugin("fixture");
    assert!(old.upgrade().is_none());
    assert!(catalog.lock().integrated_index.is_none());
    assert!(catalog.integrated_index(&sources, catalog.integration_epoch()).1.items.is_empty());
}

#[test]
fn warm_hits_skip_capture_and_builder_and_read_timestamp_refreshes_the_hint_once() {
    use std::sync::atomic::Ordering;
    let (catalog, album, _) = fixture();
    let sources = [source()];
    let (stamp, first) = catalog.integrated_index(&sources, catalog.integration_epoch());
    assert_eq!(catalog.index_captures.load(Ordering::Relaxed), 1);
    let builder = catalog.index_builder.lock().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let other = catalog.clone();
    let thread = std::thread::spawn(move || {
        send.send(other.integrated_index(&[source()], other.integration_epoch())).unwrap();
    });
    let completed = receive.recv_timeout(std::time::Duration::from_secs(5));
    drop(builder);
    thread.join().unwrap();
    let (warm_stamp, warm) = completed.unwrap();
    assert_eq!(warm_stamp, stamp);
    assert!(Arc::ptr_eq(&first, &warm));
    assert_eq!(catalog.index_captures.load(Ordering::Relaxed), 1);
    catalog.lock().store.cards.get_mut(&album).unwrap().read_at += 1;
    let (same_stamp, same) = catalog.integrated_index(&sources, catalog.integration_epoch());
    assert_eq!(same_stamp, stamp);
    assert!(Arc::ptr_eq(&first, &same));
    assert_eq!(catalog.index_captures.load(Ordering::Relaxed), 2);
    for _ in 0..3 { assert!(Arc::ptr_eq(&first, &catalog.integrated_index(&sources, catalog.integration_epoch()).1)); }
    assert_eq!(catalog.index_captures.load(Ordering::Relaxed), 2);
}

#[test]
fn hints_cover_whole_store_replacement_missing_rows_and_clock_rollback() {
    let (catalog, album, _) = fixture();
    let sources = [source()];
    {
        let mut inner = catalog.lock();
        inner.store.cards.get_mut(&album).unwrap().metadata["genres"] = json!([]);
        inner.genres = genres::State::fixture("fixture", "123456789012", vec!["Enriched".into()], 100);
    }
    let (_, first) = catalog.integrated_index_with_clock(&sources, catalog.integration_epoch(), || 101);
    assert_eq!(first.items[0].genres, vec!["Enriched"]);
    let (_, before_creation) = catalog.integrated_index_with_clock(&sources, catalog.integration_epoch(), || 99);
    assert!(before_creation.items[0].genres.is_empty());
    let (_, restored_time) = catalog.integrated_index_with_clock(&sources, catalog.integration_epoch(), || 101);
    assert_eq!(restored_time.items[0].genres, first.items[0].genres);
    let captured = inputs::Inputs::capture(&catalog, &sources, 101);
    catalog.lock().store = Default::default();
    assert!(!captured.is_current(&catalog.lock(), 101));
    let (_, empty) = catalog.integrated_index_with_clock(&sources, catalog.integration_epoch(), || 101);
    assert!(empty.items.is_empty());
    catalog.lock().store.shelves.insert("fixture section:tracks".into(), Shelf { tracks:vec![-991], ..Default::default() });
    let missing = inputs::Inputs::capture(&catalog, &sources, 101);
    catalog.lock().store.tracks.insert(-991, super::super::store::Track { id:-991, plugin:"fixture".into(), title:"Arrived".into(), ..Default::default() });
    assert!(!missing.is_current(&catalog.lock(), 101));
    assert_eq!(catalog.integrated_index_with_clock(&sources, catalog.integration_epoch(), || 101).1.items[0].title, "Arrived");
}
