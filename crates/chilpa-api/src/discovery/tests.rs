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
    assert!(roots["items"].as_array().unwrap().iter().any(|item| item["name"] == "Rock"));
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

#[test]
fn derived_genres_keep_original_assignments_and_count_each_album_once() {
    let reader = reader();
    reader.lib.set_track_genres(1, &["Progresive Metal".into(), "prog metal".into(), "Symphonic Metal".into()]).unwrap();
    reader.lib.set_track_genres(3, &["Jazz Fusion".into(), "My-style".into(), "My style".into()]).unwrap();
    reader.lib.connection().execute_batch("UPDATE albums SET artwork_path='/library/first.jpg' WHERE id=1;
        INSERT INTO tracks(id,source_id,external_id,title,artist,present,added_at) VALUES (4,1,'/loose','Single','Soloist',1,1);").unwrap();
    reader.lib.set_track_genres(4, &["prog metal".into()]).unwrap();
    let original = || reader.lib.connection().query_row(
        "SELECT group_concat(value,'|') FROM (SELECT tg.track_id || ':' || g.name AS value
         FROM track_genres tg JOIN genres g ON g.id=tg.genre_id ORDER BY tg.track_id,g.name)", [], |row| row.get::<_, String>(0)).unwrap();
    // The existing browse optimization writes only its connection-local TEMP
    // eligibility table; read_revision excludes those temporary rows.
    let changes = || reader.lib.read_revision().unwrap();
    let (before, writes) = (original(), changes());
    let root = reader.discovery_page("genres", "", "", 0);
    let rock = root["items"].as_array().unwrap().iter().find(|item| item["name"] == "Rock").unwrap();
    assert_eq!(rock["albums"], 2, "shared tags and several ancestry paths count each album once");
    assert_eq!(rock["looseTracks"], 1);
    assert_eq!(rock["images"], json!(["/library/first.jpg"]));
    let key = json!({"kind":"genre","name":"Metal"}).to_string();
    let page = reader.discovery_page("genres", &key, "", 0);
    assert_eq!(page["summary"]["albums"], 1);
    assert_eq!(page["summary"]["looseTracks"], 1);
    assert!(page["resolutions"].as_array().unwrap().iter().any(|record|
        record["original"] == "Progresive Metal" && record["recognized"] == "Progressive Metal" && record["method"] == "fuzzy"));
    assert_eq!(page["parentRelations"], json!([{"name":"Rock","source":"chilpa-1"}]));
    let snapshot = reader.discovery_snapshot(&key);
    assert_eq!(snapshot.ids.iter().copied().collect::<HashSet<_>>(), HashSet::from([1,2,4]));
    assert_eq!(snapshot.album_ids, [1]);
    assert_eq!(original(), before);
    assert_eq!(changes(), writes, "navigation must not write derived names back to the catalog");
}

#[test]
fn composer_portraits_reuse_the_chosen_person_image_without_fetching_metadata() {
    let reader = reader();
    reader.lib.set_artist_artwork("Writer", Some("/library/writer.jpg")).unwrap();
    let before = reader.lib.read_revision().unwrap();
    let page = reader.discovery_page("composers", "", "Writer", 0);
    assert_eq!(page["items"][0]["portrait"], "/library/writer.jpg");
    assert_eq!(page["items"][0]["albums"], 2);
    assert_eq!(reader.lib.read_revision().unwrap(), before);
}

#[test]
fn genre_counts_follow_the_same_version_preference_as_the_album_grid() {
    let reader = reader();
    reader.lib.connection().execute_batch("INSERT INTO albums(id,title,album_artist) VALUES (3,'First (Remastered)','Quartet');
        INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,added_at) VALUES (4,1,'/edition','Part I','Quartet',3,1);").unwrap();
    reader.lib.set_track_genres(4, &["Progresive Metal".into()]).unwrap();
    let key = json!({"kind":"genre","name":"Metal"}).to_string();
    reader.lib.set_group_album_versions(true).unwrap();
    assert_eq!(reader.discovery_albums(&key, "").ids.len(), 1);
    assert_eq!(reader.discovery_page("genres", &key, "", 0)["summary"]["albums"], 1);
    reader.lib.set_group_album_versions(false).unwrap();
    assert_eq!(reader.discovery_albums(&key, "").ids.len(), 2);
    assert_eq!(reader.discovery_page("genres", &key, "", 0)["summary"]["albums"], 2);
}

#[test]
fn case_variants_keep_both_original_tags_and_find_every_matching_track() {
    let reader = reader();
    reader.lib.set_track_genres(1, &["Électro custom".into()]).unwrap();
    reader.lib.set_track_genres(2, &["électro custom".into()]).unwrap();
    let key = json!({"kind":"genre","name":"Électro custom"}).to_string();
    let page = reader.discovery_page("genres", &key, "", 0);
    let original: HashSet<_> = page["resolutions"].as_array().unwrap().iter().map(|record| record["original"].as_str().unwrap()).collect();
    assert_eq!(original, HashSet::from(["Électro custom", "électro custom"]));
    assert_eq!(reader.discovery_snapshot(&key).ids, [1,2]);
}
