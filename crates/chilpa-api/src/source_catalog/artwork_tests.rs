//! Visible artwork reads keep working after a snapshot or its cached image ages.
use super::{store::{Card, Track}, *};

fn fixture(image: &str) -> crate::Chilpa {
    let app = crate::Chilpa::new();
    app.catalog.lock().store.cards.insert(-1, Card {
        id: -1, plugin: "fixture".into(), identity: "album:visible".into(),
        image: image.into(), ..Default::default()
    });
    app
}

#[test]
fn visible_artwork_requests_missing_images_without_using_quality_as_a_trigger() {
    let image = "https://example.invalid/visible-missing-cover.jpg";
    let mut app = fixture(image);
    assert_eq!(app.album_card_quality(-1), crate::Quality::default());
    assert!(app.catalog.lock().queue.covers.is_empty(), "quality must remain a metadata-only read");
    for _ in 0..3 { assert!(app.source_album_artwork(-1, true).is_empty()); }
    let inner = app.catalog.lock();
    assert_eq!(inner.queue.covers.iter().map(String::as_str).collect::<Vec<_>>(), [image]);
    assert_eq!(inner.queue.covers_queued.len(), 1, "cached snapshots must not duplicate requests");
    assert!(inner.queue.queued.is_empty(), "an artwork lookup does not request metadata");
}

#[test]
fn visible_artwork_retries_failed_images_only_after_the_existing_backoff() {
    let image = "https://example.invalid/visible-retry-cover.jpg";
    let app = fixture(image);
    app.catalog.lock().queue.covers_failed.insert(image.into(), Instant::now());
    assert!(app.source_album_artwork(-1, true).is_empty());
    assert!(app.catalog.lock().queue.covers.is_empty());
    app.catalog.lock().queue.covers_failed.insert(image.into(), Instant::now() - Duration::from_secs(31));
    assert!(app.source_album_artwork(-1, true).is_empty());
    let inner = app.catalog.lock();
    assert_eq!(inner.queue.covers.front().map(String::as_str), Some(image));
    assert!(!inner.queue.covers_failed.contains_key(image));
}

#[test]
fn visible_artwork_returns_a_stored_file_then_requests_it_again_after_eviction() {
    let image = "https://example.invalid/visible-evicted-cover.jpg";
    let app = fixture(image);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cover.jpg");
    std::fs::write(&path, b"fixture").unwrap();
    let path_text = path.to_string_lossy().into_owned();
    {
        let mut inner = app.catalog.lock();
        inner.store.cards.get_mut(&-1).unwrap().art = path_text.clone();
        inner.art_received.insert(image.into());
    }
    assert_eq!(app.source_album_artwork(-1, true), path_text);
    assert!(app.catalog.lock().queue.covers.is_empty());
    std::fs::remove_file(path).unwrap();
    assert!(app.source_album_artwork(-1, true).is_empty());
    let inner = app.catalog.lock();
    assert_eq!(inner.queue.covers.front().map(String::as_str), Some(image));
    assert!(!inner.art_received.contains(image), "a replacement must notify the visible card again");
}

#[test]
fn visible_artwork_uses_the_same_first_nonempty_track_image_as_card_reads() {
    let image = "https://example.invalid/visible-track-cover.jpg";
    let app = fixture("");
    {
        let mut inner = app.catalog.lock();
        inner.store.cards.get_mut(&-1).unwrap().tracks = Some(vec![-99, -2, -3, -4]);
        inner.store.tracks.insert(-2, Track { id: -2, ..Default::default() });
        inner.store.tracks.insert(-3, Track { id: -3, image: image.into(), ..Default::default() });
        inner.store.tracks.insert(-4, Track { id: -4, image: "https://example.invalid/other-track-cover.jpg".into(), ..Default::default() });
    }
    assert!(app.source_album_artwork(-1, true).is_empty());
    assert_eq!(app.catalog.lock().queue.covers.front().map(String::as_str), Some(image));
    app.catalog.card(-1).unwrap();
    assert_eq!(app.catalog.lock().queue.covers.len(), 1, "both accessors choose and deduplicate the same image");
    assert!(app.source_album_artwork(-99, true).is_empty());
    assert_eq!(app.catalog.lock().queue.covers.len(), 1);
}

#[test]
fn visible_artwork_overtakes_shelf_read_ahead_without_duplicating_running_work() {
    let image = "https://example.invalid/visible-priority-cover.jpg";
    let app = fixture(image);
    {
        let mut inner = app.catalog.lock();
        app.catalog.want_cover(&mut inner, "https://example.invalid/background-cover.jpg");
    }
    // Snapshot refreshes keep the background order; only the visible getter promotes.
    assert!(app.source_album_artwork(-1, false).is_empty());
    assert_eq!(app.catalog.lock().queue.covers.back().map(String::as_str), Some(image));
    assert!(app.source_album_artwork(-1, true).is_empty());
    assert_eq!(app.catalog.lock().queue.covers.front().map(String::as_str), Some(image));
    assert!(app.source_album_artwork(-1, true).is_empty());
    {
        let mut inner = app.catalog.lock();
        assert_eq!(inner.queue.covers.len(), 2);
        assert_eq!(inner.queue.covers.pop_front().as_deref(), Some(image));
    }
    // The worker retains covers_queued while downloading, preventing another request.
    assert!(app.source_album_artwork(-1, true).is_empty());
    let inner = app.catalog.lock();
    assert_eq!(inner.queue.covers.len(), 1);
    assert!(inner.queue.covers_queued.contains(image));
}
