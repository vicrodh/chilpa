use super::*;
use crate::library_sources::{SourceIndex, SourceItem};
use std::sync::Arc;

#[test]
fn megabyte_genre_labels_remain_searchable_and_routes_preserve_collections() {
    let (reader, _) = super::source_tests::fixture();
    let suffix = " searchable ending 742";
    let original = format!("{}{suffix}", "x".repeat(chilpa_library::genre_index::MAX_TAG_BYTES - suffix.len()));
    for id in [1, 2, 4] { reader.lib.set_track_genres(id, &[original.clone()]).unwrap(); }
    reader.lib.set_track_genres(3, &[]).unwrap();
    reader.lib.connection().execute("UPDATE tracks SET present=0 WHERE id=3", []).unwrap();
    let before = reader.lib.track_genres(1).unwrap();
    let revision = reader.lib.read_revision().unwrap();
    reader.lib.connection().execute_batch("PRAGMA query_only=ON").unwrap();
    let label = presentation::Label::new(&original);
    assert_eq!(label.text.chars().count(), 128);
    assert!(label.text.ends_with(suffix));

    let header = reader.album_genre_links(1);
    assert_eq!(header["items"][0]["name"], label.text);
    assert_eq!(header["items"][0]["nameChars"], original.len());
    assert_eq!(header["items"][0]["originalsCount"], 1);
    assert!(header.to_string().len() < 32 * 1024);
    let album_key = header["items"][0]["key"].as_str().unwrap();
    let from_album = reader.discovery_page("genres", album_key, "", 0);
    assert!(from_album.to_string().len() < 32 * 1024);
    assert_eq!(from_album["breadcrumb"], json!([label.text]));
    assert_eq!(from_album["breadcrumbTruncated"], json!([true]));
    assert_eq!(from_album["breadcrumbChars"], json!([original.len()]));
    assert_eq!(from_album["resolutions"][0]["original"], label.text);
    assert_eq!(from_album["resolutions"][0]["originalChars"], original.len());
    assert_eq!(from_album["resolutions"][0]["originalTruncated"], true);
    assert_eq!(from_album["resolutionTotal"], 1);
    assert_eq!(reader.discovery_snapshot(album_key).ids.into_iter().collect::<HashSet<_>>(), HashSet::from([1, 4]));

    let options = r#"{"scope":"tags","excluded_sources":["server:home"]}"#;
    let page = reader.discovery_page_options("genres", "", "ending 742", options, 0);
    assert_eq!(page["total"], 1);
    assert!(page.to_string().len() < 32 * 1024);
    let row = &page["items"][0];
    assert_eq!(row["name"], label.text);
    assert_eq!(row["nameTruncated"], true);
    assert_eq!(row["nameChars"], original.len());
    let raw_key = row["key"].as_str().unwrap();
    assert!(raw_key.len() < 1024);
    let mut route: Value = serde_json::from_str(raw_key).unwrap();
    assert_eq!(route["genreId"], reader.lib.genre_id_for_name(&original).unwrap().unwrap());
    assert!(route.get("album").is_none());
    assert!(route.get("excluded_sources").is_none());
    // The screen preserves the effective source choice in the opaque route.
    route["excluded_sources"] = json!(["server:home"]);
    let key = route.to_string();
    let selected = Selection::parse(&key).unwrap();
    assert_eq!(reader.resolve_album_genre_using(selected, &SourceIndex::default()).unwrap().name, original);
    let mut ids = Vec::new();
    reader.collection_tracks("discovery", &key, &Filter::default(), |found| ids = found.snapshot.ids);
    assert_eq!(ids.iter().copied().collect::<HashSet<_>>(), HashSet::from([1, 4]));
    assert_eq!(reader.paths(&ids).into_iter().collect::<HashSet<_>>(), HashSet::from(["/one".to_string(), "/loose".to_string()]));
    assert_eq!(reader.collection_albums("discovery", "title", "", "", &Filter::default(), &key).ids, [1]);
    assert_eq!(reader.discovery_artists(&key).len(), 1);
    let current = reader.discovery_page_options("genres", &key, "ending 742", options, 0);
    assert_eq!(current["total"], 1);
    assert_eq!(current["summary"]["albums"], 1);
    assert_eq!(current["summary"]["looseTracks"], 1);
    assert!(current.to_string().len() < 32 * 1024);
    route["excluded_sources"] = json!([]);
    assert_eq!(reader.discovery_snapshot(&route.to_string()).ids.into_iter().collect::<HashSet<_>>(), HashSet::from([1, 2, 4]));
    assert_eq!(reader.lib.track_genres(1).unwrap(), before);
    assert_eq!(reader.lib.read_revision().unwrap(), revision);
}

#[test]
fn global_genre_routes_reject_missing_changed_or_conflicting_anchors() {
    let (reader, _) = super::source_tests::fixture();
    let original = "A private collection ".repeat(20).trim_end().to_string();
    reader.lib.set_track_genres(1, &[original.clone()]).unwrap();
    let page = reader.discovery_page_options("genres", "", "private", r#"{"scope":"tags"}"#, 0);
    let key = page["items"][0]["key"].as_str().unwrap();
    let mut route: Value = serde_json::from_str(key).unwrap();
    assert_eq!(reader.discovery_snapshot(key).ids, [1]);
    route["albumScope"] = json!(true);
    assert!(Selection::parse(&route.to_string()).is_none());
    route["albumScope"] = json!(false);
    route["album"] = json!(1);
    assert!(Selection::parse(&route.to_string()).is_none());
    route["album"] = json!(0);
    route["genreTrack"] = json!(-1);
    assert!(Selection::parse(&route.to_string()).is_none());
    route["genreTrack"] = json!(0);
    route["genreToken"] = json!("");
    assert!(Selection::parse(&route.to_string()).is_none());
    let id = reader.lib.genre_id_for_name(&original).unwrap().unwrap();
    reader.lib.connection().execute("UPDATE genres SET name='A different private collection' WHERE id=?1", [id]).unwrap();
    assert!(reader.discovery_page("genres", key, "", 0)["error"].is_string());
    assert!(reader.discovery_snapshot(key).ids.is_empty());
    assert!(reader.discovery_albums(key, "").ids.is_empty());
    assert!(reader.discovery_artists(key).is_empty());
    let mut absent: Value = serde_json::from_str(key).unwrap();
    absent["genreId"] = json!(i64::MAX);
    assert!(reader.discovery_page("genres", &absent.to_string(), "", 0)["error"].is_string());
}

#[test]
fn remote_long_genre_routes_preserve_albumless_tracks_and_explicit_source_scope() {
    let (reader, _) = super::source_tests::fixture();
    let name = format!("{} remote tail", "界".repeat(600));
    let item = |track, album, plugin: &str| SourceItem { track, album, plugin:plugin.into(), genres:vec![name.clone()],
        saved_track:true, playable:true, tracks:1, album_identity:format!("album:{album}"), ..Default::default() };
    for album in [0, -10] {
        let index = Arc::new(SourceIndex::new(vec![item(-11, album, "alpha"), item(-21, -20, "beta")], Vec::new()));
        let excluded = vec!["plugin:beta".into()];
        let options = options::Options::parse(r#"{"scope":"tags"}"#).unwrap();
        let page = reader.genre_page_using(&Selection::default(), "remote tail", &options, &excluded, 0, &index);
        assert_eq!(page["total"], usize::from(album != 0));
        let label = presentation::Label::new(&name);
        let key = reader.genre_entry_key(&name, &label, &excluded, &index).unwrap();
        if album != 0 {
            assert_eq!(page["items"][0]["name"].as_str().unwrap().chars().count(), 128);
            assert_eq!(page["items"][0]["key"], key);
        }
        let mut route: Value = serde_json::from_str(&key).unwrap();
        assert_eq!(route[if album == 0 { "genreTrack" } else { "album" }], if album == 0 { -11 } else { -10 });
        route["excluded_sources"] = json!(excluded);
        let selected = reader.resolve_album_genre_using(Selection::parse(&route.to_string()).unwrap(), &index).unwrap();
        assert_eq!(selected.name, name);
        assert!(!selected.album_scope);
        let filter = reader.discovery_filter_using(&selected, &index);
        let (_, snapshot) = reader.refresh_using(&filter, TrackOrder::Album, || true,
            ((1, vec![("alpha".into(), false, true), ("beta".into(), false, true)]), index.clone())).unwrap();
        assert_eq!(snapshot.ids, [-11]);
        let display = reader.genre_page_using(&selected, "", &options, &excluded, 0, &index);
        assert!(display.to_string().len() < 32 * 1024);
        let label = presentation::Label::new(&name);
        assert_eq!(display["resolutions"][0]["original"], label.text);
        assert!(reader.resolve_album_genre_using(Selection::parse(&route.to_string()).unwrap(), &SourceIndex::default()).is_err());
    }
}
