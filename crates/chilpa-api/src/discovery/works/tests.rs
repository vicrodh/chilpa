use super::*;

fn reader() -> Reader {
    let lib = chilpa_library::Library::in_memory().unwrap();
    lib.connection().execute_batch("INSERT INTO sources(id,provider,instance) VALUES (1,'local','default');
        INSERT INTO albums(id,title,album_artist,year,artwork_path) VALUES (1,'Local interpretation','Player',1987,'/covers/local.jpg');
        INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,work,added_at,duration_ms) VALUES
            (1,1,'/part','Part I','Player',1,'Suite',1,500);").unwrap();
    lib.set_track_composers(1, &["Writer".into()]).unwrap();
    Reader::of(lib)
}

fn source(id: i64, number: i64) -> SourceItem {
    SourceItem { track:id, album:-10, plugin:"guest".into(), work:"Suite".into(), composers:vec!["Writer".into()],
        album_title:"Source interpretation".into(), title:format!("Part {number}"), artists:vec!["Quartet".into()],
        number:Some(number), disc:Some(1), duration_ms:1000, year:Some(2007), saved_album:true, playable:true, ..Default::default() }
}

#[test]
fn source_works_merge_with_sql_tags_without_discarding_their_interpretations() {
    let reader = reader();
    let index = SourceIndex::new(vec![source(-2,2),source(-1,1)], vec![]);
    let (works, total) = reader.discovery_compositions_using("Writer", "", 0, &index).unwrap();
    assert_eq!((total, works[0].tracks, works[0].albums), (1,3,2));
    assert_eq!((works[0].recordings, works[0].duration_ms), (2,2500));
    let page = reader.compositions_page("Writer", "", 0, false, &index);
    assert_eq!(page["items"][0]["artwork"], "/covers/local.jpg");
    let mut selection = Selection { kind:"work".into(), work:works[0].key.clone(), ..Default::default() };
    let local = reader.recordings_page_ordered(&selection, "1987", 0, false, &index);
    assert_eq!(local["total"], 1);
    assert_eq!(local["items"][0]["artwork"], "/covers/local.jpg");
    assert_eq!(local["items"][0]["year"], 1987);
    let recordings = reader.recordings_page_ordered(&selection, "", 0, false, &index);
    assert_eq!(recordings["total"], 2);
    let remote = recordings["items"].as_array().unwrap().iter().find(|row| row["album"] == -10).unwrap();
    assert_eq!(remote["source"], "guest");
    assert_eq!(remote["year"], 2007);
    selection.recording = remote["recording"].as_str().unwrap().into();
    let parts = snapshot(reader.work_parts_using(&selection, &index).unwrap());
    assert_eq!(parts.ids, [-1,-2]);
    assert_eq!(parts.album_ids, [-10]);
    assert_eq!(parts.totals.duration_ms, 2000);
}

#[test]
fn absent_composers_never_merge_across_source_and_library_identity_spaces() {
    let reader = reader();
    reader.lib.connection().execute("DELETE FROM track_composers", []).unwrap();
    let mut a = source(-1,1); a.composers.clear(); a.album = -1;
    let mut b = a.clone(); b.track = -2; b.plugin = "other".into();
    let index = SourceIndex::new(vec![a,b], vec![]);
    let (works, total) = reader.discovery_compositions_using("", "", 0, &index).unwrap();
    assert_eq!((works.len(), total), (3,3));
    assert!(works.iter().all(|work| work.tracks == 1));
}

#[test]
fn merged_pages_are_ordered_searchable_and_do_not_drop_a_source_only_tail() {
    let reader = reader();
    for n in 0..125 {
        reader.lib.connection().execute("INSERT INTO tracks(source_id,external_id,title,work,added_at) VALUES (1,?1,'Part',?2,1)",
            [format!("/extra/{n}"),format!("Work {n:03}")]).unwrap();
    }
    let mut extra = source(-1,1); extra.work = "Work 200".into();
    let index = SourceIndex::new(vec![extra], vec![]);
    let (page, total) = reader.discovery_compositions_using("", "Work", 2, &index).unwrap();
    assert_eq!((page.len(), total), (6,126));
    assert_eq!(page.last().unwrap().title, "Work 200");
    assert_eq!(reader.discovery_compositions_using("Writer", "200", 0, &index).unwrap().1, 1);
    assert_eq!(reader.discovery_compositions_using("Different", "200", 0, &index).unwrap().1, 0);
}

#[test]
fn work_keys_match_sql_unicode_casing_and_ignore_composer_insertion_order() {
    let reader = reader();
    reader.lib.set_track_composers(1, &["Ångström".into(),"Second".into()]).unwrap();
    let mut item = source(-1,1); item.composers = vec!["Second".into(),"Ångström".into()];
    assert_eq!(key(&item), reader.lib.compositions("", "", 0).unwrap().0[0].key);
    let changes = reader.lib.connection().total_changes();
    let index = SourceIndex::new(vec![item], vec![]);
    assert_eq!(reader.discovery_compositions_using("", "", 0, &index).unwrap().1, 1);
    assert_eq!(reader.lib.connection().total_changes(), changes);
}

#[test]
fn unicode_search_finds_both_library_and_source_work_tags_without_rewriting_identity() {
    let reader = reader();
    reader.lib.connection().execute("UPDATE tracks SET work='Préludes'", []).unwrap();
    let mut item = source(-1,1); item.work = "Évocations".into();
    let index = SourceIndex::new(vec![item], vec![]);
    let (local, _) = reader.discovery_compositions_using("", "PRELUDES", 0, &index).unwrap();
    assert_eq!(local[0].title, "Préludes");
    let (remote, _) = reader.discovery_compositions_using("", "evocations", 0, &index).unwrap();
    assert_eq!(remote[0].title, "Évocations");
}
