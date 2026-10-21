use super::*;

fn edit(text: &str, edit: Value) -> String { refine_edit(text, &edit.to_string()) }

#[test]
fn every_kind_reads_and_edits_without_changing_other_rows() {
    for (text,kind) in [("genre:Jazz,Soul","list"),("-artist:Band","list"),("year:1960..1969","range"),
        ("year:1960,1969","list"),("rating:4..","rating"),("rating:0","rating"),("added:6w","added"),
        ("label:none","none"),("title:blue","words"),("album:blue","words"),("blue","words"),
        ("plays:0,2","list"),("plays:1..","range"),("type:single,ep","list"),("quality:cd,hires","list"),
        ("words:\"A. Smith\"","words"),("genre=:dnb","list")] {
        let rows = refine(text); assert_eq!(rows["complex"], Value::Null, "{text}");
        assert_eq!(rows["rows"][0]["kind"],kind,"{text}");
        let new = edit(text,json!({"op":"added","n":3,"unit":"m"}));
        let parsed = read(&new).unwrap(); assert_eq!(read(&parsed.unwrap().to_string()).unwrap(),read(&new).unwrap());
        assert_eq!(edit(text,json!({"op":"remove","id":"0"})),"");
    }
    assert_eq!(refine("plays:0,2")["rows"][0]["values"],json!([0,2]));
}

#[test]
fn operators_follow_the_controls() {
    for (text,to,expected) in [
        ("genre:Jazz","exclude","-genre:Jazz"),("-genre:Jazz","include","genre:Jazz"),
        ("genre:Jazz","none","genre:none"),("genre:none","include",""),("genre:none","exclude",""),
        ("year:1970..1979","from","year:1970.."),("year:1970..1979","upTo","year:..1979"),
        ("year:1970..","between","year:1970..1979"),("year:1950..","between","year:1950..1969"),("year:..1969","from","year:1990.."),
        ("rating:0","atLeast","rating:4..5"),("rating:0","exactly","rating:5..5"),
        ("rating:3..","exactly","rating:3..3"),("rating:3","ratingBetween","rating:2..4"),
        ("rating:3","unrated","rating:0..0"),("plays:1..9","upTo","plays:..9")
    ] { assert_eq!(edit(text,json!({"op":"operator","id":"0","to":to})),expected); }
}

#[test]
fn values_move_and_empty_rows_disappear_inside_the_target() {
    let text = "genre:Jazz (genre:Jazz,Soul -genre:Pop)";
    let moved = edit(text,json!({"op":"value","field":"genre","value":"Soul","state":"exclude","target":"1"}));
    assert_eq!(moved,"genre:Jazz (genre:Jazz -genre:Pop,Soul)");
    let removed = edit(&moved,json!({"op":"removeValue","id":"1.0","value":"Jazz"}));
    assert_eq!(removed,"genre:Jazz (-genre:Pop,Soul)");
    assert_eq!(edit(&removed,json!({"op":"remove","id":"1.0"})),"genre:Jazz");
}

#[test]
fn groups_survive_single_children_and_same_mode_parents() {
    let text = edit("",json!({"op":"value","field":"genre","value":"Jazz","state":"include","target":"new"}));
    assert_eq!(text,"(genre:Jazz)");
    assert_eq!(refine(&text)["rows"][0]["group"],true);
    let text = edit(&text,json!({"op":"words","field":"title","word":"blue","target":"0"}));
    assert_eq!(text,"(genre:Jazz | title:blue)");
    let text = edit(&text,json!({"op":"mode","mode":"all","target":"0"}));
    assert_eq!(text,"(genre:Jazz title:blue)");
    assert_eq!(refine(&text)["rows"][0]["rows"][1]["id"],"0.1");
    assert_eq!(edit(&text,json!({"op":"ungroup","id":"0"})),"genre:Jazz title:blue");
    assert_eq!(edit("genre:Jazz title:blue year:1960",json!({"op":"group","ids":["0","1"]})),"(genre:Jazz | title:blue) year:1960");
}

#[test]
fn replacements_and_bad_edits_keep_the_text_contract() {
    for (op,expected) in [
        (json!({"op":"range","from":1960,"to":null}),"year:1960.."),
        (json!({"op":"range","field":"plays","from":1,"to":3}),"plays:1..3"),
        (json!({"op":"rating","min":2,"max":4}),"rating:2..4"),
        (json!({"op":"added","n":6,"unit":"w"}),"added:6w"),
        (json!({"op":"none","field":"composer","on":true}),"composer:none"),
        (json!({"op":"words","field":"album","word":"blue"}),"album:blue")
    ] { assert_eq!(edit("",op.clone()),expected); assert_eq!(edit(expected,op),expected); }
    let original = "genre:Jazz";
    for op in [json!({"op":"range","from":2000,"to":1900}), json!({"op":"rating","min":7,"max":7}),
        json!({"op":"added","n":i64::MAX,"unit":"y"}), json!({"op":"remove","id":"0.0"}),
        json!({"op":"words","field":"album","word":"blue","target":"4"})] { assert_eq!(edit(original,op),original); }
    for text in ["genre:(", "(genre:Jazz (artist:Band | label:Label))", "-year:1960..", "rating:1,5", "added:1..2"] {
        assert!(refine(text)["rows"].as_array().unwrap().is_empty());
        assert!(!refine(text)["complex"].is_null());
        assert_eq!(edit(text,json!({"op":"added","n":1,"unit":"d"})),text);
    }
}

#[test]
fn error_positions_count_characters_including_non_ascii_text() {
    for text in ["genre:","été year:","音楽 genre:","(genre:Jazz"] {
        let error = chilpa_library::store::query::parse(text).unwrap_err();
        assert_eq!(check(text)["at"],error.at); assert_eq!(check(text)["ok"],false);
        assert!(!check(text)["message"].as_str().unwrap().is_empty());
    }
    assert_eq!(check("音楽 year:")["at"],3);
    assert_eq!(check(""),json!({"ok":true}));
}

fn library() -> chilpa_library::Library {
    let lib=chilpa_library::Library::in_memory().unwrap();
    let source=lib.source_id("local","default").unwrap();
    let db=lib.connection();
    for (id,title,artist,year) in [(1,"Blue Record","Álvaro",1960),(2,"Gold Record","Other",1980)] {
        db.execute("INSERT INTO albums(id,title,album_artist,year) VALUES (?1,?2,?3,?4)",rusqlite::params![id,title,artist,year]).unwrap();
    }
    for (id,album,title,artist,label) in [(1,1,"Blue One","Álvaro","Label"),(2,1,"Blue Two","Álvaro","Label"),(3,2,"Gold","Other","Other Label")] {
        db.execute("INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,label,added_at) VALUES (?1,?2,?3,?4,?5,?6,?7,1)",rusqlite::params![id,source,format!("/{id}.flac"),title,artist,album,label]).unwrap();
    }
    for (id,name) in [(1,"Jazz"),(2,"Drum and Bass"),(3,"Electronica")] { db.execute("INSERT INTO genres(id,name) VALUES (?1,?2)",rusqlite::params![id,name]).unwrap(); }
    for (track,genre) in [(1,1),(2,1),(3,2),(3,3)] { db.execute("INSERT INTO track_genres(track_id,genre_id) VALUES (?1,?2)",rusqlite::params![track,genre]).unwrap(); }
    lib.set_track_composers(1,&["A Smith".into()]).unwrap();
    lib
}

#[test]
fn suggestions_fold_cap_count_tracks_and_keep_other_conditions() {
    let lib=library();
    for i in 3..=7 {
        let artist=format!("Álvaro {i}");
        lib.connection().execute("INSERT INTO albums(id,title,album_artist,year) VALUES (?1,'Extra',?2,1960)",rusqlite::params![i,artist]).unwrap();
        lib.connection().execute("INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,added_at) VALUES (?1,1,?2,'Extra',?3,?4,1)",rusqlite::params![i+1,format!("/extra-{i}.flac"),artist,i]).unwrap();
    }
    let reader=crate::Reader::of(lib);
    let result=reader.refine_suggest("-artist:Álvaro year:1960","ALVA");
    let group=&result["groups"][0]; assert_eq!(group["field"],"artist");
    assert_eq!(group["items"].as_array().unwrap().len(),3);
    assert_eq!(group["items"][0],json!({"value":"Álvaro","count":2,"state":"exclude"}));
    assert_eq!(reader.refine_suggest("genre:Jazz","alva")["groups"][0]["items"][0]["count"],2);
    assert_eq!(reader.refine_suggest("genre:Jazz year:1980","alva")["groups"][0]["items"][0]["count"],0);
    assert_eq!(reader.refine_suggest("","blue")["words"],json!({"anywhere":2,"title":2,"album":2}));
    assert_eq!(reader.refine_suggest("artist:Álvaro","alva")["groups"][0]["items"][0]["state"],"include");
    assert_eq!(chilpa_library::store::query::fold("A\u{301}LVARO"),"alvaro");
}

#[test]
fn suggestions_count_or_branches_and_integrated_source_tracks() {
    use crate::library_sources::{SourceIndex,SourceItem};
    let reader=crate::Reader::of(library());
    let source=SourceIndex::new(vec![SourceItem {track:0,album:-1,title:"Blue Source".into(),album_title:"Blue Record".into(),
        artists:vec!["Álvaro".into()],genres:vec!["Jazz".into()],year:Some(1960),tracks:4,..Default::default()}],vec![]);
    let result=reader.suggest_using("genre:Jazz year:1960","alva",&source);
    assert_eq!(result["groups"][0]["items"][0]["count"],6);
    assert_eq!(reader.suggest_using("","blue",&source)["words"],json!({"anywhere":6,"title":6,"album":6}));
    let any=reader.suggest_using("artist:Other | genre:Jazz","other",&source);
    assert_eq!(any["groups"][0]["items"][0]["count"],7);
    let grouped=reader.suggest_using("(artist:Other | genre:Jazz) year:1960","other",&source);
    assert_eq!(grouped["groups"][0]["items"][0]["count"],6);
}

#[test]
fn notes_report_actual_resolutions_and_literal_edits_disable_them() {
    let reader=crate::Reader::of(library());
    for (text,kind,field,literal) in [("A. Smith","name","composer","words:A.,Smith"),
        ("genre:dnb","alias","genre","genre=:dnb"),("genre:Electronika","near","genre","genre=:Electronika")] {
        let notes=reader.refine_notes(text); assert_eq!(notes.len(),1,"{text}");
        let filter=crate::Filter {query:read(text).unwrap(),..Default::default()};
        assert_eq!(reader.refresh(&filter,crate::TrackOrder::Artist,|| true).unwrap().1.totals.tracks,1);

        assert_eq!(notes[0].kind,kind); assert_eq!(notes[0].field,field);
        let edited=reader.refine_edit(text,r#"{"op":"literal","note":0}"#);
        assert_eq!(edited,literal); assert!(reader.refine_notes(&edited).is_empty());
        let parsed=read(&edited).unwrap().unwrap(); assert_eq!(parsed.to_string(),edited);
        let filter=crate::Filter {query:Some(parsed),..Default::default()};
        assert_eq!(reader.lib.query_count(&filter).unwrap(),0);
        assert_eq!(reader.refresh(&filter,crate::TrackOrder::Artist,|| true).unwrap().1.totals.tracks,0);
    }
    let edited=reader.refine_edit("genre:dnb,Electronika",r#"{"op":"literal","note":0}"#);
    assert!(edited.contains("genre=:dnb"));
    assert_eq!(reader.refine_notes(&edited).len(),1,"only the selected value stops widening");
    assert_eq!(reader.refine_edit("genre:dnb",r#"{"op":"literal","note":99}"#),"genre:dnb");
}

#[test]
fn zero_result_hints_are_sorted_and_only_requested_for_zero() {
    let reader=crate::Reader::of(library());
    assert_eq!(reader.refine_without_each("genre:Jazz"),json!([]));
    assert_eq!(reader.refine_without_each("genre:Jazz year:1980"),json!([{"id":"1","count":2},{"id":"0","count":1}]));
    assert_eq!(reader.refine_without_each("(genre:Jazz | artist:Álvaro) year:1980"),json!([{"id":"1","count":2},{"id":"0","count":1}]));
    let source=crate::library_sources::SourceIndex::new(vec![crate::library_sources::SourceItem {
        track:-1,album:-1,year:Some(1980),genres:vec!["Jazz".into()],tracks:1,..Default::default()}],vec![]);
    assert_eq!(reader.without_each_using("genre:Jazz year:1980",&source),json!([]));
}

#[path = "../../../../chilpa-library/tests/support/scale.rs"]
mod scale_fixture;

#[test]
fn suggestions_on_the_scale_fixture_use_one_round_per_keystroke() {
    let reader=crate::Reader::of(scale_fixture::large_library());
    let mut times=Vec::new();
    for typed in ["a","ar","art","arti","artist","artist 0","artist 01"] {
        let start=std::time::Instant::now();
        let result=reader.refine_suggest("year:1960..1980",typed);
        times.push(start.elapsed());
        assert!(result["groups"].as_array().unwrap().iter().all(|g| g["items"].as_array().unwrap().len()<=3));
    }
    eprintln!("refine suggestions: tracks={} samples={times:?}",scale_fixture::TRACKS);
    assert!(times.iter().all(|t| *t<std::time::Duration::from_secs(2)),"{times:?}");
}

#[test]
fn a_literal_note_keeps_groups_and_the_other_notes() {
    let reader=crate::Reader::of(library());
    let text="(A. Smith | genre:dnb) year:1960";
    let notes=reader.refine_notes(text); assert_eq!(notes.len(),2);
    let literal=reader.refine_edit(text,r#"{"op":"literal","note":0}"#);
    assert!(literal.starts_with("(words:A.,Smith | genre:dnb)"));
    assert_eq!(reader.refine_notes(&literal).len(),1);
    let text="A. (Smith)";
    let notes=reader.refine_notes(text); assert_eq!(notes.len(),1);
    let literal=reader.refine_edit(text,r#"{"op":"literal","note":0}"#);
    assert_eq!(literal,"words:A. (words:Smith)");
    assert!(reader.refine_notes(&literal).is_empty());
}

#[test]
fn free_words_share_one_row_and_one_removal() {
    let text=edit("blue",json!({"op":"words","field":"words","word":"train"}));
    assert_eq!(refine(&text)["rows"],json!([{"id":"0","field":"words","kind":"words","values":["blue","train"]}]));
    assert_eq!(edit(&text,json!({"op":"removeValue","id":"0","value":"blue"})),"words:train");
    assert_eq!(edit(&text,json!({"op":"remove","id":"0"})),"");
}

#[test]
fn vocabulary_and_notes_keep_the_engines_eligibility_rules() {
    let lib=library();
    let hidden=lib.source_id("local","hidden").unwrap();
    lib.connection().execute("UPDATE tracks SET source_id=?1 WHERE id=3",[hidden]).unwrap();
    lib.connection().execute("UPDATE sources SET enabled=0 WHERE id=?1",[hidden]).unwrap();
    let mut expected=super::super::library_names(&lib);
    for (_,names) in &mut expected {names.sort();}
    assert_eq!(lib.query_vocabulary().unwrap(),expected);
    let reader=crate::Reader::of(lib);
    assert!(reader.refine_notes("Electronica").is_empty());
    assert_eq!(reader.refine_notes("genre:Electronika")[0].to,"Electronica");
    assert_eq!(reader.refine_suggest("genre:Electronika","alva")["groups"][0]["items"][0]["count"],0);
}

#[test]
fn a_second_name_note_keeps_its_position_after_the_first_resolves() {
    let reader=crate::Reader::of(library());
    let text="A. (Smith (Other | title:blue))";
    let notes=reader.refine_notes(text);
    assert_eq!(notes.iter().map(|n| n.from.as_str()).collect::<Vec<_>>(),["A. Smith","Other"]);
    let edited=reader.refine_edit(text,r#"{"op":"literal","note":1}"#);
    assert!(edited.contains("words:Other"),"{edited}");
    assert_eq!(reader.refine_notes(&edited).len(),1);
}

#[test]
fn word_previews_also_apply_to_complex_queries() {
    let reader=crate::Reader::of(library());
    let text="(genre:Jazz (artist:Álvaro | label:Label))";
    assert!(!refine(text)["complex"].is_null());
    assert_eq!(reader.refine_suggest(text,"absent")["words"],json!({"anywhere":0,"title":0,"album":0}));
}

#[test]
fn moving_a_literal_genre_keeps_its_literal_meaning() {
    let text=edit("genre=:dnb",json!({"op":"value","field":"genre","value":"dnb","state":"exclude"}));
    assert_eq!(text,"-genre=:dnb");
    assert_eq!(edit(&text,json!({"op":"none","field":"genre","on":true})),"genre:none");
}

#[test]
fn the_legacy_focus_view_keeps_flattening_same_mode_groups() {
    let rows=super::super::focus_rows("genre:Jazz (year:1960)");
    assert!(!rows.advanced); assert_eq!(rows.rows.len(),2);
    assert_eq!(super::super::with_value("genre:Jazz (year:1960)","genre","Soul",ValueState::Include),"genre:Jazz,Soul year:1960");
}
