use super::store::{Card, Store, Track};
use super::view::{album_of, row_of};
use serde_json::{json, Value};

#[test]
fn ai_labels_keep_known_false_unknown_and_malformed_values_distinct() {
    for value in [json!(true), json!(false), Value::Null, json!("true"), json!(1)] {
        let metadata = json!({"ai_generated": value});
        let album = album_of(&Card { metadata: metadata.clone(), ..Default::default() });
        let track = row_of(&Track { metadata, ..Default::default() }, String::new());
        assert_eq!(album.ai_generated, value.as_bool());
        assert_eq!(track.ai_generated, value.as_bool());
    }
    assert_eq!(album_of(&Card::default()).ai_generated, None);
    assert_eq!(row_of(&Track::default(), String::new()).ai_generated, None);
}

#[test]
fn album_classification_does_not_classify_its_tracks() {
    let mut store = Store::default();
    let album = store.take_item("fixture", &json!({"kind":"album","ref":"album:1","title":"Record","ai_generated":true}), 1).unwrap();
    let card = store.cards[&album].clone();
    let track = store.take_item_track("fixture", &json!({"kind":"track","ref":"track:7","title":"Song"}), Some(&card), 1).unwrap();
    assert_eq!(album_of(&card).ai_generated, Some(true));
    assert_eq!(row_of(&store.tracks[&track], String::new()).ai_generated, None);
}

#[test]
fn revalidation_and_restore_keep_identity_and_refresh_only_the_label() {
    let mut store = Store::default();
    let mut item = json!({"kind":"album","ref":"album:1","title":"Record","ai_generated":true});
    let id = store.take_item("fixture", &item, 1).unwrap();
    let original = album_of(&store.cards[&id]);
    assert_eq!(store.take_item("fixture", &item, 2), Some(id));
    assert_eq!(album_of(&store.cards[&id]), original);
    item["ai_generated"] = json!(false);
    assert_eq!(store.take_item("fixture", &item, 3), Some(id));
    let changed = album_of(&store.cards[&id]);
    assert_eq!((&changed.title, changed.id), (&original.title, original.id));
    assert_eq!(changed.ai_generated, Some(false));
    let encoded = serde_json::to_vec(&store).unwrap();
    let restored: Store = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(album_of(&restored.cards[&id]), changed);
}

#[test]
fn playback_label_is_read_at_track_change_and_cleared_for_an_unclassified_track() {
    let mut app = crate::Chilpa::new();
    let path = {
        let mut inner = app.catalog.lock();
        let id = inner.store.take_item_track("fixture", &json!({"kind":"track","ref":"track:7","title":"Song","ai_generated":true}), None, 1).unwrap();
        inner.store.tracks[&id].path.clone()
    };
    app.name_from_source(std::path::Path::new(&path));
    assert_eq!(app.now.ai_generated, Some(true));
    let view = crate::playback_view::PlaybackView::read(&app.now, app.now.lyrics_revision);
    assert_eq!(view.ai_generated, Some(true));
    app.name_from_source(std::path::Path::new("unclassified.flac"));
    assert_eq!(app.now.ai_generated, None);
}

#[test]
fn classification_updates_the_row_without_replacing_its_occurrence() {
    let old = row_of(&Track { id: -7, path: "fixture/track:7".into(), metadata: json!({"ai_generated":true}), ..Default::default() }, String::new());
    let mut new = old.clone();
    new.ai_generated = Some(false);
    let changes = crate::list_changes::tracks(&[crate::TrackEntry::Track(old)], &[crate::TrackEntry::Track(new)], &[], &[]).unwrap();
    assert!(changes.steps.is_empty());
    assert_eq!(changes.changed, [0]);
}
