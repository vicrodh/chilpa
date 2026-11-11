use super::*;

const GIBIBYTE: u64 = 1024 * 1024 * 1024;

fn track(id: &str, title: &str, artist: &str, album: &str, number: u16) -> chilpa_library::ServerTrack {
    chilpa_library::ServerTrack {
        external_id: format!("remote:subsonic/0123456789abcdef/{id}"),
        title: title.into(),
        artist: artist.into(),
        album: album.into(),
        album_artist: artist.into(),
        album_key: album.into(),
        track_number: Some(number),
        duration_ms: 279_000,
        ..Default::default()
    }
}

fn library() -> (tempfile::TempDir, Library, i64) {
    let root = tempfile::tempdir().unwrap();
    let lib = Library::open(&root.path().join("library.sqlite3")).unwrap();
    let source = lib
        .add_server(&chilpa_library::NewServer {
            provider: "subsonic".into(),
            instance: "0123456789abcdef".into(),
            label: "Living room".into(),
            account: "fixture".into(),
            libraries: "[]".into(),
            details: "{}".into(),
            ..Default::default()
        })
        .unwrap();
    let generation = lib.begin_server_sync(source).unwrap();
    lib.save_server_tracks(source, generation, &[
        track("two", "Orangenlied", "Annenmaykantereit", "Es ist Abend", 2),
        track("one", "Tommi", "Annenmaykantereit", "Es ist Abend", 1),
        track("long", "The Scarecrow", "Avantasia", "The Scarecrow", 4),
    ]).unwrap();
    lib.finish_server_sync(source, generation).unwrap();
    (root, lib, source)
}

fn keep(lib: &Library, source: i64, id: &str, bytes: u64, state: &str) {
    lib.connection()
        .execute(
            "INSERT INTO offline_audio (key,source_id,account,identity,entry,revision,path,bytes,state,error,changed_at)
            VALUES (?1,?2,'fixture','',?3,'r','/copy',?4,?5,CASE ?5 WHEN 'failed' THEN 'The server answered 500' ELSE '' END,?6)",
            rusqlite::params![format!("key-{id}"), source, format!("remote:subsonic/0123456789abcdef/{id}"), bytes as i64, state, bytes as i64],
        )
        .unwrap();
}

fn context(quota: Option<u64>) -> Context {
    Context { root: Ok(PathBuf::from("/music/offline")), chosen: String::new(), limit: Limit { quota } }
}

fn titles(doc: &Value) -> Vec<String> {
    doc["rows"].as_array().unwrap().iter().map(|row| format!("{}:{}", row["kind"].as_str().unwrap(), row["title"].as_str().unwrap())).collect()
}

#[test]
fn copies_are_filed_by_record_under_their_artist_with_totals_and_folders() {
    let (_root, lib, source) = library();
    keep(&lib, source, "two", 300, "complete");
    keep(&lib, source, "one", 200, "complete");
    keep(&lib, source, "long", 900, "complete");
    let doc = compute(&lib, &context(Some(2 * GIBIBYTE)), &Filters::parse(""));
    assert_eq!(titles(&doc), ["album:Es ist Abend", "track:Tommi", "track:Orangenlied", "album:The Scarecrow", "track:The Scarecrow"]);
    assert_eq!(doc["files"], 3);
    assert_eq!(doc["size"], "2 KiB");
    assert_eq!(doc["rows"][0]["tracks"], 2);
    assert_eq!(doc["rows"][0]["artist"], "Annenmaykantereit");
    assert_eq!(doc["rows"][0]["ids"].as_array().unwrap().len(), 2);
    assert_eq!(doc["rows"][1]["duration"], "4:39");
    assert_eq!(doc["artists"], json!([{"name": "Annenmaykantereit", "albums": 1, "tracks": 2}, {"name": "Avantasia", "albums": 1, "tracks": 1}]));
    let server = &doc["servers"][0];
    assert_eq!((server["id"].as_i64(), server["name"].as_str(), server["files"].as_u64()), (Some(source), Some("Living room"), Some(3)));
    assert!(server["inherited"].as_str().unwrap().starts_with("file:///music/offline/"), "{server}");
    assert_eq!(server["folder"], "");
    assert_eq!(doc["limitSize"], "2.0 GiB");
    assert!(doc["usage"].as_f64().unwrap() > 0.0 && doc["full"] == false);
}

#[test]
fn filters_narrow_the_records_and_the_rail_keeps_every_artist_in_scope() {
    let (_root, lib, source) = library();
    keep(&lib, source, "two", 300, "failed");
    keep(&lib, source, "one", 200, "complete");
    keep(&lib, source, "long", 900, "complete");
    let failed = compute(&lib, &context(None), &Filters::parse(r#"{"failed":true}"#));
    assert_eq!(titles(&failed), ["album:Es ist Abend", "track:Orangenlied"]);
    assert_eq!(failed["rows"][0]["state"], "failed");
    assert_eq!(failed["rows"][1]["error"], "The server answered 500");
    let artist = compute(&lib, &context(None), &Filters::parse(r#"{"artist":"Avantasia"}"#));
    assert_eq!(titles(&artist), ["album:The Scarecrow", "track:The Scarecrow"]);
    assert_eq!(artist["artists"].as_array().unwrap().len(), 2, "the rail still names both");
    let elsewhere = compute(&lib, &context(None), &Filters::parse(&format!(r#"{{"server":{}}}"#, source + 1)));
    assert!(elsewhere["rows"].as_array().unwrap().is_empty());
    assert_eq!(elsewhere["files"], 3, "the totals are the whole of it");
    let largest = compute(&lib, &context(None), &Filters::parse(r#"{"sort":"largest"}"#));
    assert_eq!(largest["rows"][0]["title"], "The Scarecrow");
    let smallest = compute(&lib, &context(None), &Filters::parse(r#"{"sort":"smallest"}"#));
    assert_eq!(smallest["rows"][0]["title"], "Es ist Abend");
    let recent = compute(&lib, &context(None), &Filters::parse(r#"{"sort":"recent"}"#));
    assert_eq!(recent["rows"][0]["title"], "The Scarecrow", "changed last");
}

#[test]
fn unlimited_has_no_usage_bar_and_a_full_limit_says_so() {
    let (_root, lib, source) = library();
    keep(&lib, source, "one", 2 * GIBIBYTE, "complete");
    let unlimited = compute(&lib, &context(None), &Filters::default());
    assert_eq!((unlimited["usage"].as_f64(), unlimited["limitSize"].as_str(), unlimited["limit"].as_str()), (Some(0.0), Some(""), Some("unlimited")));
    let full = compute(&lib, &context(Some(GIBIBYTE)), &Filters::default());
    assert_eq!((full["usage"].as_f64(), full["full"].as_bool()), (Some(1.0), Some(true)));
    assert_eq!(full["limits"]["values"].as_array().unwrap().last().unwrap(), "unlimited");
}

#[test]
fn a_copy_whose_track_left_the_library_is_listed_last_and_offers_no_track() {
    let (_root, lib, source) = library();
    keep(&lib, source, "one", 200, "complete");
    keep(&lib, source, "removed", 100, "complete");
    let doc = compute(&lib, &context(None), &Filters::default());
    let rows = doc["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!((rows[2]["kind"].as_str(), rows[2]["gone"].as_bool()), (Some("album"), Some(true)));
    assert_eq!(rows[2]["ids"], json!([]));
    assert_eq!(rows[3]["id"], 0);
}

#[test]
fn a_listing_carries_at_most_its_share_of_tracks() {
    let (_root, lib, source) = library();
    for n in 0..(MAX_SHOWN + 5) {
        lib.connection()
            .execute(
                "INSERT INTO offline_audio (key,source_id,account,identity,entry,revision,path,bytes,state) VALUES (?1,?2,'a','','gone/'||?1,'r','/c',1,'complete')",
                rusqlite::params![format!("{n:05}"), source],
            )
            .unwrap();
    }
    let doc = compute(&lib, &context(None), &Filters::default());
    assert_eq!((doc["shown"].as_u64(), doc["matching"].as_u64()), (Some(MAX_SHOWN as u64), Some(MAX_SHOWN as u64 + 5)));
    assert_eq!(doc["rows"].as_array().unwrap().len(), MAX_SHOWN + 1);
}

#[test]
fn filters_from_the_page_fall_back_to_the_defaults() {
    assert_eq!(Filters::parse("not json"), Filters { sort: "az".into(), ..Default::default() });
    assert_eq!(Filters::parse(r#"{"sort":"sideways","server":7}"#), Filters { sort: "az".into(), server: 7, ..Default::default() });
    assert_eq!(Filters::parse(&"x".repeat(MAX_FILTERS_BYTES + 1)).sort, "az");
}

#[test]
fn a_listing_is_read_once_per_change_and_sent_whole_only_when_new() {
    let mut manager = Manager::default();
    let ask = |manager: &mut Manager, seen: &str| manager.request(seen, Filters::default(), "", false, || context(None));
    let mut answer: Value = serde_json::from_str(&ask(&mut manager, "")).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while answer["loading"] == true && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
        answer = serde_json::from_str(&ask(&mut manager, "")).unwrap();
    }
    assert_eq!(answer["loading"], false);
    assert!(answer["doc"].is_object(), "{answer}");
    let revision = answer["revision"].to_string();
    let again: Value = serde_json::from_str(&ask(&mut manager, &revision)).unwrap();
    assert!(again.get("doc").is_none() && again["loading"] == false, "nothing changed, nothing sent");
    manager.stale = true;
    let fresh: Value = serde_json::from_str(&ask(&mut manager, &revision)).unwrap();
    assert_eq!(fresh["loading"], true, "a change reads it again");
}
