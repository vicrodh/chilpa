use super::*;
use crate::library_sources::{SourceIndex,SourceItem};

#[test]
fn album_header_and_pages_bound_payload_without_losing_searchable_genres() {
    let (reader,_) = super::source_tests::fixture();
    let tags: Vec<String> = (0..137).map(|at|format!("Collection label {at:03}")).collect();
    reader.lib.set_track_genres(1,&tags).unwrap();
    reader.lib.set_track_genres(3,&[]).unwrap();
    reader.lib.connection().execute_batch("PRAGMA query_only=ON").unwrap();
    let revision = reader.lib.read_revision().unwrap();
    let header = reader.album_genre_links(1);
    assert_eq!(header["items"].as_array().unwrap().len(),8);
    assert_eq!(header["total"],137);
    assert_eq!(header["hasMore"],true);
    assert!(header.to_string().len() <= 32 * 1024);
    let mut seen = HashSet::new();
    for page in 0..3 {
        let result = reader.discovery_page_options("album_genres","1","","{}",page);
        assert_eq!(result["total"],137);
        assert_eq!(result["items"].as_array().unwrap().len(),if page < 2 { 60 } else { 17 });
        assert!(result.to_string().len() <= 128 * 1024);
        for item in result["items"].as_array().unwrap() {
            assert!(seen.insert(item["name"].as_str().unwrap().to_string()));
            assert!(item["key"].as_str().unwrap().len() < 2048);
            assert!(item.get("excluded_sources").is_none());
            assert!(item.get("originals").is_none());
        }
    }
    assert_eq!(seen,tags.into_iter().collect());
    let found = reader.discovery_page("album_genres","1","label 136",0);
    assert_eq!(found["total"],1);
    assert_eq!(found["items"][0]["name"],"Collection label 136");
    assert_eq!(reader.discovery_page("album_genres","1","missing",0)["total"],0);
    assert_eq!(reader.discovery_page("album_genres","1","",usize::MAX)["items"],json!([]));
    let descending = reader.discovery_page_options("album_genres","1","",r#"{"order":"name-desc"}"#,0);
    assert_eq!(descending["items"][0]["name"],"Collection label 136");
    assert_eq!(reader.lib.read_revision().unwrap(),revision);
}

#[test]
fn compact_album_routes_resolve_sources_for_pages_tracks_albums_and_artists() {
    let (reader,index) = super::source_tests::fixture();
    reader.lib.set_track_genres(1,&["heavy-metal".into()]).unwrap();
    reader.lib.set_track_genres(2,&["Heavy Metal".into()]).unwrap();
    reader.lib.set_track_genres(3,&["Heavy Metal".into()]).unwrap();
    // Only the local source contributes to this edition.
    reader.lib.connection().execute("UPDATE tracks SET present=0 WHERE id=3",[]).unwrap();
    let header = reader.album_genre_links_using(1,&index);
    let key = header["items"][0]["key"].as_str().unwrap();
    assert!(key.len() < 512);
    assert_eq!(reader.discovery_snapshot(key).ids,[1]);
    assert_eq!(reader.collection_albums("discovery","title","","",&Filter::default(),key).ids,[1]);
    assert_eq!(reader.discovery_artists(key).len(),1);
    let scoped = reader.discovery_page_options("genres",key,"",r#"{"excluded_sources":[]}"#,0);
    assert_eq!(scoped["summary"]["albums"],1);
    assert_eq!(scoped["excluded_sources"],json!(["server:home"]));
    let mut explicit: Value = serde_json::from_str(key).unwrap();
    explicit["albumScope"] = json!(false);
    explicit["excluded_sources"] = json!([]);
    let all = reader.discovery_page_options("genres",&explicit.to_string(),"",r#"{"excluded_sources":[]}"#,0);
    assert_eq!(all["summary"]["albums"],2);
    assert_eq!(reader.discovery_snapshot(&explicit.to_string()).ids.into_iter().collect::<HashSet<_>>(),HashSet::from([1,2]));
    let remote = reader.album_genres_page_using(-10,"",0,60,false,&index);
    let selected = Selection::parse(remote["items"][0]["key"].as_str().unwrap()).unwrap();
    let resolved = reader.resolve_album_genre_using(selected,&index).unwrap();
    assert_eq!(resolved.excluded_sources,["local:default","plugin:beta","server:home"]);
    reader.lib.set_track_genres(1,&["Jazz".into()]).unwrap();
    assert!(reader.discovery_snapshot(key).ids.is_empty());
    assert!(reader.discovery_page("genres",key,"",0)["error"].is_string());
}

#[test]
fn long_labels_and_many_originals_keep_compact_stable_routes() {
    let (reader,_) = super::source_tests::fixture();
    let long = format!("{} distinctive ending", "\"\\\n".repeat(300));
    let mut tags = vec![long.clone(),"Heavy Metal".into(),"heavy-metal".into(),"dnb".into(),"Drum and Bass".into(),
        "Progresive Metal".into(),"Progressive Metal".into()];
    tags.extend((1..128).map(|spaces|format!("Personal{}Shelf"," ".repeat(spaces))));
    let item = |genres| SourceItem { album:-10,track:-11,plugin:"alpha".into(),genres,..Default::default() };
    let index = SourceIndex::new(vec![item(tags.clone())],Vec::new());
    tags.reverse();
    let reverse = SourceIndex::new(vec![item(tags)],Vec::new());
    let header = reader.album_genre_links_using(-10,&index);
    assert_eq!(header,reader.album_genre_links_using(-10,&reverse));
    assert!(header.to_string().len() <= 32 * 1024);
    assert_eq!(header["total"],6);
    let custom = header["items"].as_array().unwrap().iter().find(|row|row["originalsCount"]==127).unwrap();
    assert!(!custom["key"].as_str().unwrap().contains("excluded_sources"));
    let found = reader.album_genres_page_using(-10,"distinctive ending",0,60,false,&index);
    assert_eq!(found["total"],1);
    let row = &found["items"][0];
    assert_eq!(row["nameTruncated"],true);
    let selected = Selection::parse(row["key"].as_str().unwrap()).unwrap();
    assert_eq!(reader.resolve_album_genre_using(selected,&index).unwrap().name,long);
    assert_eq!(reader.album_genres_page_using(-10,"drum and bass",0,60,false,&index)["total"],1);
    let too_long = SourceIndex::new(vec![item(vec!["x".repeat(chilpa_library::genre_index::MAX_TAG_BYTES+1)])],Vec::new());
    let rejected = reader.album_genre_links_using(-10,&too_long);
    assert!(rejected["error"].is_string());
    assert_eq!(rejected["items"],json!([]),"resource failures never publish a partial inventory");
}

#[test]
fn compact_genre_routes_validate_their_album_and_token() {
    assert!(Selection::parse(r#"{"kind":"genre","album":1,"albumScope":true}"#).is_none());
    assert!(Selection::parse(&json!({"kind":"genre","album":0,"genreToken":"a".repeat(64)}).to_string()).is_none());
    assert!(Selection::parse(&json!({"kind":"genre","album":1,"genreToken":"z".repeat(64)}).to_string()).is_none());
    let (reader,_) = super::source_tests::fixture();
    assert!(reader.discovery_page("album_genres","wrong","",0)["error"].is_string());
}

#[test]
fn long_normalized_tags_reach_the_same_tracks_as_the_header_identity() {
    let (reader,_) = super::source_tests::fixture();
    let original = format!("Heavy{}Metal", " _-".repeat(100));
    reader.lib.set_track_genres(1,&[original.clone()]).unwrap();
    reader.lib.set_track_genres(3,&[]).unwrap();
    let revision = reader.lib.read_revision().unwrap();
    let header = reader.album_genre_links(1);
    assert_eq!(header["total"],1);
    assert_eq!(header["items"][0]["nameTruncated"],true);
    let key = header["items"][0]["key"].as_str().unwrap();
    assert_eq!(reader.discovery_snapshot(key).ids,[1]);
    let page = reader.discovery_page("genres",key,"",0);
    assert_eq!(page["summary"]["albums"],1);
    assert_eq!(page["resolutions"][0]["recognized"],"Heavy Metal");
    assert_eq!(page["resolutions"][0]["method"],"normalized");
    assert_eq!(reader.lib.track_genres(1).unwrap(),[original]);
    assert_eq!(reader.lib.read_revision().unwrap(),revision);
}
