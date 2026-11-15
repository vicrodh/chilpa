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
    assert!(search["items"].as_array().unwrap().iter().any(|row| row["name"] == "Technical Death Metal" && row["albums"] == 1));
    assert!(!search["items"].as_array().unwrap().iter().any(|row| row["name"] == "Rock"));
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
    assert_eq!(rock["images"], json!([]), "album covers are not artist portraits");
    assert_eq!(changes(), writes, "navigation must not write derived names back to the catalog");
    reader.lib.set_artist_artwork("Quartet", Some("/library/quartet.jpg")).unwrap();
    let portraits = reader.discovery_page("genres", "", "", 0);
    let rock = portraits["items"].as_array().unwrap().iter().find(|item| item["name"] == "Rock").unwrap();
    assert_eq!(rock["images"], json!(["/library/quartet.jpg"]));
    reader.lib.set_artist_artwork("Quartet", None).unwrap();
    let writes = changes();
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
fn canonical_families_require_populated_albums_and_alias_only_names_stay_in_tags() {
    let reader = reader();
    reader.lib.set_track_genres(1, &["Personal shelf".into(), "Aakhar".into()]).unwrap();
    let names = |value: Value| value["items"].as_array().unwrap().iter().map(|row| row["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    let all = names(reader.discovery_page("genres", "", "", 0));
    assert!(!all.contains(&"Classical".into()) && all.contains(&"Rock".into()));
    assert!(!all.contains(&"Personal shelf".into()) && !all.contains(&"Aakhar".into()));
    reader.lib.connection().execute_batch("DELETE FROM track_genres").unwrap();
    assert!(names(reader.discovery_page("genres", "", "", 0)).is_empty());
    assert!(names(reader.discovery_page_options("genres", "", "", r#"{"scope":"library"}"#, 0)).is_empty());
}

#[test]
fn browsing_options_apply_before_the_window_and_keep_personal_tags_accessible() {
    let reader = reader();
    reader.lib.set_track_genres(1, &["Personal shelf".into(), "Progresive Metal".into()]).unwrap();
    let tags = reader.discovery_page_options("genres", "", "personal", r#"{"scope":"tags"}"#, 0);
    assert_eq!(tags["items"][0]["name"], "Personal shelf");
    let forward = reader.discovery_page("genres", "", "", 0);
    let backward = reader.discovery_page_options("genres", "", "", r#"{"order":"name-desc"}"#, 0);
    assert_eq!(backward["items"][0], *forward["items"].as_array().unwrap().last().unwrap());
    for at in 0..125 {
        reader.lib.connection().execute("INSERT INTO tracks(source_id,external_id,title,work,added_at) VALUES (1,?1,'Part',?2,1)",
            rusqlite::params![format!("/extra/{at}"),format!("Work {at:03}")]).unwrap();
    }
    let page = reader.discovery_page_options("compositions", "", "Work", r#"{"order":"name-desc"}"#, 1);
    assert_eq!(page["total"], 125);
    assert_eq!(page["items"][0]["name"], "Work 064");
    assert_eq!(page["items"].as_array().unwrap().len(), 60);
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

#[test]
fn genre_album_ordering_cannot_replace_an_eligible_edition_with_a_global_preference() {
    let reader = reader();
    reader.lib.connection().execute_batch("INSERT INTO albums(id,title,album_artist) VALUES (3,'First (Remastered)','Quartet');
        INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,added_at) VALUES (4,1,'/other-edition','Part I','Quartet',3,1);").unwrap();
    reader.lib.set_track_genres(4, &["Jazz".into()]).unwrap();
    assert!(reader.lib.select_album_version(1, 3).unwrap());
    let key = json!({"kind":"genre","name":"Metal"}).to_string();
    let within: HashSet<_> = reader.discovery_snapshot(&key).album_ids.into_iter().collect();
    assert_eq!(within, HashSet::from([1]));
    // This is the previous implementation, including its global regrouping.
    // The preferred edition has no Metal tracks, so that implementation lost the card.
    let legacy = || {
        let mut ids = reader.albums_view("title", "", "", &Filter::default()).ids;
        assert!(ids.contains(&3) && !ids.contains(&1));
        ids.retain(|id| within.contains(id));
        ids
    };
    assert!(legacy().is_empty());
    for order in ["title", "title-desc", "artist", "year", "added"] {
        assert_eq!(reader.collection_albums("discovery", order, "", "", &Filter::default(), &key).ids, [1]);
        assert_eq!(reader.collection_albums("discovery", order, "", "First", &Filter::default(), &key).ids, [1]);
    }
    assert!(reader.discovery_albums(&key, "Remastered").ids.is_empty());
    let unmatched = json!({"kind":"genre","name":"Unknown genre"}).to_string();
    assert!(reader.discovery_albums(&unmatched, "").ids.is_empty());
}

#[test]
fn recognized_genre_spellings_share_queries_and_counts_without_changing_storage() {
    let reader = reader();
    reader.lib.set_track_genres(1, &["heavy metal".into(), "heavy-metal".into()]).unwrap();
    reader.lib.set_track_genres(2, &["Heavy Metal".into()]).unwrap();
    reader.lib.set_track_genres(3, &["heavy-metal".into()]).unwrap();
    let stored = || (1..=3).map(|id| reader.lib.track_genres(id).unwrap()).collect::<Vec<_>>();
    let before = stored();
    assert_eq!(before[0].len(), 2, "separators remain original stored spellings");
    let revision = reader.lib.read_revision().unwrap();
    for spelling in ["heavy metal", "Heavy Metal", "heavy-metal", "HEAVY-METAL"] {
        let selected = Selection { kind:"genre".into(), name:spelling.into(), ..Default::default() };
        let filter = reader.discovery_filter(&selected);
        assert!(filter.genres.contains(&"heavy metal".into()));
        assert!(filter.genres.contains(&"heavy-metal".into()));
        assert_eq!(reader.lib.browse_ids(&filter, TrackOrder::Album).unwrap().ids.len(), 3);
        let key = serde_json::to_string(&selected).unwrap();
        let snapshot = reader.discovery_snapshot(&key);
        assert_eq!(snapshot.ids.iter().copied().collect::<HashSet<_>>(), HashSet::from([1,2,3]));
        assert_eq!(snapshot.album_ids.len(), 2);
        assert_eq!(reader.discovery_page("genres", &key, "", 0)["summary"]["albums"], 2,
            "several spellings on one album must count once");
        assert_eq!(reader.discovery_albums(&key, "").ids.len(), 2);
    }
    let tags = reader.discovery_page_options("genres", "", "", r#"{"scope":"tags"}"#, 0);
    assert_eq!(tags["items"].as_array().unwrap().len(), 1);
    assert_eq!(tags["items"][0]["name"], "Heavy Metal");
    assert_eq!(tags["items"][0]["albums"], 2);
    assert_eq!(stored(), before);
    assert_eq!(reader.lib.read_revision().unwrap(), revision);
}

#[test]
fn genre_case_variants_keep_literal_facets_and_case_insensitive_membership() {
    let reader = reader();
    reader.lib.set_track_genres(1, &["sountrack".into()]).unwrap();
    reader.lib.set_track_genres(2, &["Sountrack".into()]).unwrap();
    reader.lib.set_track_genres(3, &["Soundtrack".into()]).unwrap();
    let before = (1..=3).map(|id| reader.lib.track_genres(id).unwrap()).collect::<Vec<_>>();
    let revision = reader.lib.read_revision().unwrap();
    let facets = reader.lib.facet_genres(&Filter::default()).unwrap();
    assert_eq!(facets.iter().filter(|row| row.label.eq_ignore_ascii_case("sountrack")).count(), 1);
    assert_eq!(facets.iter().find(|row| row.label.eq_ignore_ascii_case("sountrack")).unwrap().albums, 1);
    let typo = reader.discovery_snapshot(&json!({"kind":"genre","name":"sountrack"}).to_string());
    assert!(typo.ids.contains(&1) && typo.ids.contains(&2));
    for spelling in ["sountrack", "Sountrack", "SOUNTRACK"] {
        let literal = Filter { genres:vec![spelling.into()], ..Default::default() };
        assert_eq!(reader.lib.browse_ids(&literal, TrackOrder::Album).unwrap().ids.len(), 2);
        let key = json!({"kind":"genre","name":spelling}).to_string();
        assert_eq!(reader.discovery_snapshot(&key).ids, typo.ids);
    }
    let canonical = reader.discovery_snapshot(&json!({"kind":"genre","name":"soundtrack"}).to_string());
    assert!(canonical.ids.contains(&3));
    for spelling in ["soundtrack", "Soundtrack"] {
        let key = json!({"kind":"genre","name":spelling}).to_string();
        assert_eq!(reader.discovery_snapshot(&key).ids, canonical.ids);
        assert_eq!(reader.discovery_page("genres", &key, "", 0)["summary"]["albums"], canonical.album_ids.len());
    }
    assert_eq!((1..=3).map(|id| reader.lib.track_genres(id).unwrap()).collect::<Vec<_>>(), before);
    assert_eq!(reader.lib.read_revision().unwrap(), revision);
}

#[test]
fn recognized_genre_selection_combines_local_and_source_spellings_once_per_record() {
    use crate::library_sources::{SourceIndex, SourceItem};
    use std::sync::Arc;
    let reader = reader();
    reader.lib.set_track_genres(1, &["heavy metal".into(), "heavy-metal".into()]).unwrap();
    reader.lib.set_track_genres(2, &["Heavy Metal".into()]).unwrap();
    reader.lib.set_track_genres(3, &["heavy-metal".into()]).unwrap();
    let item = |track, album, tags:Vec<String>| SourceItem {
        track,album,plugin:"fixture".into(),album_identity:format!("album:{album}"),
        album_title:format!("Source record {album}"),album_artist:"Player".into(),artists:vec!["Player".into()],
        genres:tags,saved_track:true,saved_album:true,playable:true,tracks:1,..Default::default()
    };
    let index = Arc::new(SourceIndex::new(vec![
        item(-101,-100,vec!["heavy metal".into(),"Heavy Metal".into(),"heavy-metal".into()]),
        item(-102,-100,vec!["HEAVY-METAL".into()]),
        item(-201,-200,vec!["Heavy Metal".into()]),
        item(-301,-300,vec!["Électro custom".into()]),
        item(-302,-300,vec!["électro custom".into()]),
    ],Vec::new()));
    let originals = index.items.iter().map(|item| item.genres.clone()).collect::<Vec<_>>();
    let mut names = reader.lib.genre_names().unwrap();
    names.extend(index.items.iter().flat_map(|item| item.genres.iter().cloned()));
    let inventory = genres::Inventory::new(&names);
    let expanded = inventory.matching("Heavy Metal");
    assert!(expanded.iter().any(|name| name == "heavy metal"));
    assert!(expanded.iter().any(|name| name == "heavy-metal"));
    assert!(expanded.iter().any(|name| name == "Heavy Metal"));
    let filter = Filter { genres:expanded, ..Default::default() };
    let revision = reader.lib.read_revision().unwrap();
    let (_, snapshot) = reader.refresh_using(&filter, TrackOrder::Album, || true,
        ((1,vec![("fixture".into(),true,true)]),index.clone())).unwrap();
    assert_eq!(snapshot.ids.iter().copied().collect::<HashSet<_>>(),HashSet::from([1,2,3,-101,-102,-201]));
    assert_eq!(snapshot.album_ids.iter().copied().collect::<HashSet<_>>(),HashSet::from([1,2,-100,-200]));
    assert_eq!(snapshot.totals.albums,4);
    for name in ["Électro custom", "électro custom"] {
        let filter = Filter { genres:inventory.matching(name), ..Default::default() };
        let (_, snapshot) = reader.refresh_using(&filter, TrackOrder::Album, || true,
            ((1,vec![("fixture".into(),true,true)]),index.clone())).unwrap();
        assert_eq!(snapshot.ids.iter().copied().collect::<HashSet<_>>(),HashSet::from([-301,-302]));
        assert_eq!(snapshot.album_ids,[-300]);
        assert_eq!(snapshot.totals.albums,1);
    }
    assert_eq!(index.items.iter().map(|item| item.genres.clone()).collect::<Vec<_>>(), originals);
    assert_eq!(reader.lib.read_revision().unwrap(),revision);
}
