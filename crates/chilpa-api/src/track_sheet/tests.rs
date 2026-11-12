use super::*;
use crate::library_sources::SourceIndex;

fn key(selection: Value) -> String { format!("title\u{1f}off\u{1f}\u{1f}\u{1f}{selection}") }

fn fixture() -> (Reader, SourceIndex) {
    let lib = chilpa_library::Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    lib.connection().execute("INSERT INTO tracks(id,source_id,external_id,title,artist,added_at,bit_depth,sample_rate_hz)
        VALUES(1,?,'/one.flac','Common','One',1,16,44100)", [source]).unwrap();
    lib.connection().execute("INSERT INTO track_favorites(track_id,added_at) VALUES(1,1)", []).unwrap();
    let items = (1..=205).map(|n| SourceItem {
        track: -n, title: if n == 1 { "Common".into() } else { format!("Title {n:03}") },
        plugin: "fixture".into(), artists: vec!["One".into(), "Album credit".into()],
        album_title: "Record".into(), saved_track: true, tier: "lossless", format: "flac", tracks: 1,
        ..Default::default()
    }).collect();
    (Reader::of(lib), SourceIndex::new(items, Vec::new()))
}

#[test]
fn independent_sheet_does_not_inherit_library_extras() {
    let (reader, index) = fixture();
    let extras = Filter { excluded_sources: vec!["local:default".into(), "plugin:fixture".into()], formats: vec!["mp3".into()], ..Default::default() };
    let found = reader.tracks_using(&key(json!({"title":["Common"]})), &extras, &index);
    assert_eq!(found.snapshot.ids.len(), 2);
    let found = reader.tracks_using(&key(json!({"artist":["Album credit"]})), &extras, &index);
    assert!(found.snapshot.ids.is_empty(), "the displayed track artist is the exact selectable value");
}

#[test]
fn source_hearts_and_local_hearts_filter_together() {
    let (reader, index) = fixture();
    let mut extras = Filter::default();
    extras.sheet.source_favorites = Arc::new(HashSet::from([-1]));
    let favorite = reader.tracks_using(&key(json!({"favorite":true})), &extras, &index);
    assert_eq!(favorite.snapshot.ids.len(), 2);
    assert!(favorite.snapshot.ids.contains(&1) && favorite.snapshot.ids.contains(&-1));
    let rest = reader.tracks_using(&key(json!({"favorite":false})), &extras, &index);
    assert_eq!(rest.snapshot.ids.len(), 204);
    let page = reader.track_column_using(&key(json!({"favorite":true})), &extras, "favorite", "", "", &index);
    assert_eq!(page["items"], json!([{"key":"false","label":"false","count":204},{"key":"true","label":"true","count":2}]));
}

#[test]
fn merged_pages_are_bounded_complete_and_count_duplicate_names_once() {
    let (reader, index) = fixture();
    let query = key(json!({"title":["not present"]}));
    let mut cursor = String::new();
    let mut keys = Vec::new();
    loop {
        let page = reader.track_column_using(&query, &Filter::default(), "title", "", &cursor, &index);
        assert_eq!(page["error"], "");
        let items = page["items"].as_array().unwrap();
        assert!(items.len() <= 100);
        if cursor.is_empty() { assert_eq!(items[0]["count"], 2); }
        keys.extend(items.iter().map(|item| item["key"].as_str().unwrap().to_string()));
        if page["more"] == false { break; }
        cursor = page["next"].as_str().unwrap().to_string();
    }
    assert_eq!(keys.len(), 205);
    assert_eq!(keys.iter().collect::<HashSet<_>>().len(), 205);
    assert_eq!(keys.last().unwrap(), "Title 205");
}

#[test]
fn exact_values_do_not_turn_into_patterns_or_query_syntax() {
    let (reader, index) = fixture();
    for text in ["Comm", "COMMON", "Common%", "title:Common", "' OR 1=1 --"] {
        assert!(reader.tracks_using(&key(json!({"title":[text]})), &Filter::default(), &index).snapshot.ids.is_empty());
    }
    assert!(Selection::parse(r#"{"rating":[9]}"#).invalid);
    assert!(Selection::parse(r#"{"favorite":"yes"}"#).invalid);
}

#[test]
fn missing_artist_and_different_album_names_match_the_literal_track_cells() {
    let mut item = SourceItem { artists: vec!["Album artist".into()], album_title: "Canonical album".into(),
        track_names: Some(Box::new((String::new(), "Track album".into()))), ..Default::default() };
    let selection = Selection::parse(r#"{"artist":[""],"album":["Track album"]}"#);
    assert!(passes(&selection, &item, &Stars::default()));
    item.track_names = None;
    assert!(!passes(&selection, &item, &Stars::default()));
}

#[test]
fn choices_include_all_rating_states_and_search_source_display_names() {
    let (reader, mut index) = fixture();
    index.sources.push(crate::FacetEntry { key:"plugin:fixture".into(),label:"Source Display".into(),albums:205,year:None,icon:String::new() });
    let query = key(json!({}));
    let rating = reader.track_column_using(&query,&Filter::default(),"rating","","",&index);
    assert_eq!(rating["items"].as_array().unwrap().len(),6);
    assert_eq!(rating["items"][0]["count"],206);
    assert_eq!(rating["items"][5]["count"],0);
    let source = reader.track_column_using(&query,&Filter::default(),"source","display","",&index);
    assert_eq!(source["items"],json!([{"key":"plugin:fixture","label":"Source Display","count":205}]));
}
