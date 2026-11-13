use super::*;

fn reader() -> Reader {
    let lib = chilpa_library::Library::in_memory().unwrap();
    lib.connection().execute_batch("INSERT INTO sources(id,provider,instance) VALUES (1,'local','default');
        INSERT INTO albums(id,title,album_artist) VALUES (1,'First','Quartet'),(2,'Second','Orchestra');
        INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,work,duration_ms,added_at) VALUES
        (1,1,'/a','Part I','Quartet',1,'Suite',1000,1),
        (2,1,'/b','Part II','Quartet',1,'Suite',2000,1),
        (3,1,'/c','Part I','Orchestra',2,'Suite',3000,1);").unwrap();
    lib.set_track_composers(1, &["Writer".into()]).unwrap();
    lib.set_track_composers(2, &["Writer".into()]).unwrap();
    lib.set_track_composers(3, &["Writer".into()]).unwrap();
    lib.set_track_genres(1, &["Technical Death Metal".into()]).unwrap();
    lib.set_track_genres(2, &["Technical Death Metal".into()]).unwrap();
    lib.set_track_genres(3, &["Jazz".into()]).unwrap();
    Reader::of(lib)
}

#[test]
fn a_parent_genre_finds_deep_tags_and_unknown_genres_never_mean_everything() {
    let reader = reader();
    let key = json!({"kind":"genre","name":"Metal"}).to_string();
    let snapshot = reader.discovery_snapshot(&key);
    assert_eq!(snapshot.ids, [1,2]);
    assert_eq!(snapshot.album_ids, [1]);
    assert_eq!(reader.discovery_artists(&key)[0].artist.name, "Quartet");
    assert!(reader.discovery_snapshot(&json!({"kind":"genre","name":"Unknown"}).to_string()).ids.is_empty());
    let roots = reader.discovery_page("genres", "", "", 0);
    assert!(roots["items"].as_array().unwrap().iter().any(|item| item["name"] == "Metal"));
    let search = reader.discovery_page("genres", "", "technical", 0);
    assert_eq!(search["items"][0]["name"], "Technical Death Metal");
}

#[test]
fn a_work_keeps_its_recordings_apart_and_a_recording_queues_all_available_parts() {
    let reader = reader();
    let works = reader.discovery_page("compositions", "", "", 0);
    assert_eq!(works["total"], 1);
    let mut selection = json!({"kind":"work","name":"Suite","work":works["items"][0]["work"]});
    let recordings = reader.discovery_page("recordings", &selection.to_string(), "", 0);
    assert_eq!(recordings["total"], 2);
    let quartet = recordings["items"].as_array().unwrap().iter().find(|row| row["artist"] == "Quartet").unwrap();
    assert_eq!((quartet["tracks"].as_u64(), quartet["duration"].as_u64()), (Some(2), Some(3000)));
    selection["recording"] = quartet["recording"].clone();
    let parts = reader.discovery_snapshot(&selection.to_string());
    assert_eq!(parts.ids, [1,2]);
    assert_eq!(reader.paths(&parts.ids), ["/a", "/b"]);
    assert_eq!(parts.totals.duration_ms, 3000);
}

#[test]
fn invalid_routes_do_not_start_a_library_wide_query() {
    let reader = reader();
    for key in ["", "not json", r#"{"kind":"wrong"}"#, r#"{"kind":"genre","name":""}"#] {
        assert!(reader.discovery_snapshot(key).ids.is_empty(), "{key}");
    }
    assert!(Selection::parse(&" ".repeat(8193)).is_none());
}
