//! Exact Tracks selections, complete facets, and paired query measurements.
use chilpa_library::{Filter, Library, TrackOrder};
use chilpa_library::store::{query::{Field, Node, Values}, track_sheet::Selection};
use serde_json::json;
use std::time::Instant;

fn filter(value: serde_json::Value) -> Filter {
    Filter { sheet: Selection::parse(&value.to_string()), ..Default::default() }
}

fn fixture(n: usize) -> Library {
    let lib = Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    lib.connection().execute_batch("BEGIN;
        WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<100)
        INSERT INTO albums(id,title,album_artist) SELECT i,printf('Record %03d',i),printf('Artist %03d',i) FROM n;
        INSERT INTO album_artist_rollup(album_id,artist,tracks) SELECT id,album_artist,1000 FROM albums;").unwrap();
    lib.connection().execute(&format!("WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<{n})
        INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,added_at,bit_depth,sample_rate_hz,duration_ms)
        SELECT i,?,printf('/track/%06d.flac',i),printf('Song %06d',i),printf('Artist %03d',i%100+1),i%100+1,1,16,44100,1000 FROM n"), [source]).unwrap();
    lib.connection().execute_batch("COMMIT; ANALYZE;").unwrap();
    lib
}

#[test]
fn exact_names_missing_values_and_other_columns_intersect() {
    let lib = fixture(205);
    lib.connection().execute("UPDATE tracks SET title='100%_quote''',artist='Café',album_id=NULL WHERE id=1", []).unwrap();
    let chosen = filter(json!({"title":["100%_quote'"],"artist":["Café"],"album":[""]}));
    assert_eq!(lib.browse_ids(&chosen, TrackOrder::Title).unwrap().ids, [1]);
    let choices = lib.track_sheet_choices(&chosen, "artist", "cafe", None).unwrap();
    assert_eq!(choices, [("Café".into(),1)]);
    assert!(lib.track_sheet_choices(&chosen, "artist", "CAFÉx", None).unwrap().is_empty());
    lib.connection().execute("UPDATE tracks SET present=0 WHERE id=1", []).unwrap();
    assert!(lib.track_sheet_choices(&chosen, "artist", "cafe", None).unwrap().is_empty());
}

#[test]
fn facets_page_past_one_hundred_and_include_empty_values() {
    let lib = fixture(205);
    lib.connection().execute("UPDATE tracks SET title='' WHERE id=1", []).unwrap();
    let first = lib.track_sheet_choices(&Filter::default(), "title", "", None).unwrap();
    assert_eq!(first.len(),101);
    assert_eq!(first[0].0, "");
    let second = lib.track_sheet_choices(&Filter::default(), "title", "", Some(&first[99].0)).unwrap();
    let third = lib.track_sheet_choices(&Filter::default(), "title", "", Some(&second[99].0)).unwrap();
    assert_eq!(third.len(),5);
    assert_eq!(third.last().unwrap().0, "Song 000205");
    assert_eq!(lib.browse_ids(&filter(json!({"title":[""]})),TrackOrder::Title).unwrap().ids,[1]);
}

#[test]
fn technical_values_match_rows_and_ratings_include_zero() {
    let lib = fixture(3);
    lib.connection().execute("UPDATE tracks SET external_id='/two.mp3',bit_depth=NULL WHERE id=2", []).unwrap();
    lib.connection().execute("UPDATE tracks SET external_id='/three.dsf',sample_rate_hz=2822400 WHERE id=3", []).unwrap();
    lib.connection().execute("INSERT INTO track_ratings(track_id,stars,revision,updated_at) VALUES(1,5,1,1)", []).unwrap();
    lib.connection().execute("INSERT INTO track_favorites(track_id,added_at) VALUES(1,1)", []).unwrap();
    for (selection, ids) in [
        (json!({"format":["mp3"]}),vec![2]), (json!({"quality":["dsd"]}),vec![3]),
        (json!({"rating":[0]}),vec![2,3]), (json!({"rating":[5],"favorite":true}),vec![1]),
        (json!({"source":["unavailable:source"]}),vec![]),
    ] {
        assert_eq!(lib.browse_ids(&filter(selection),TrackOrder::Title).unwrap().ids,ids);
    }
    let source = lib.source_id("local", "default").unwrap();
    lib.connection().execute("UPDATE sources SET enabled=0 WHERE id=?",[source]).unwrap();
    assert!(lib.track_sheet_choices(&Filter::default(),"title","",None).unwrap().is_empty());
}

#[test]
fn technical_selections_keep_numeric_keys_intersections_and_missing_values() {
    let lib = fixture(4);
    lib.connection().execute_batch("UPDATE tracks SET bit_depth=24,sample_rate_hz=96000 WHERE id=2;
        UPDATE tracks SET bit_depth=24,sample_rate_hz=192000 WHERE id=3;
        UPDATE tracks SET bit_depth=NULL,sample_rate_hz=NULL WHERE id=4;").unwrap();
    for (selection, ids) in [
        (json!({"bits":[24],"rate":["96000"]}),vec![2]),
        (json!({"bits":["16","24"],"rate":[44100,192000]}),vec![1,3]),
        (json!({"bits":[""],"rate":[""]}),vec![4]),
        (json!({"bits":["24",""],"rate":[96000]}),vec![2]),
    ] { assert_eq!(lib.browse_ids(&filter(selection),TrackOrder::Title).unwrap().ids,ids); }
    assert_eq!(lib.track_sheet_choices(&filter(json!({"bits":[16]})),"bits","",None).unwrap(),[("".into(),1),("16".into(),1),("24".into(),2)]);
    assert_eq!(lib.track_sheet_choices(&Filter::default(),"rate","",None).unwrap(),[("".into(),1),("44100".into(),1),("96000".into(),1),("192000".into(),1)]);
    assert_eq!(lib.track_sheet_choices(&filter(json!({"bits":[24]})),"rate","96",None).unwrap(),[("96000".into(),1)]);
    for selection in [json!({"bits":["16-bit"]}),json!({"rate":["44.1kHz"]}),json!({"rate":[0]}),json!({"bits":[-1]}),json!({"rate":[44.1]})] {
        assert!(filter(selection).sheet.invalid,"display labels and invalid numbers cannot become query keys");
    }
}

#[test]
fn local_numeric_facets_page_in_numeric_order_without_skipping_values() {
    let lib=fixture(205);
    lib.connection().execute("UPDATE tracks SET sample_rate_hz=id*1000",[]).unwrap();
    let first=lib.track_sheet_choices(&Filter::default(),"rate","",None).unwrap();
    assert_eq!((first.len(),first[0].0.as_str(),first[99].0.as_str()),(101,"1000","100000"));
    let second=lib.track_sheet_choices(&Filter::default(),"rate","",Some(&first[99].0)).unwrap();
    assert_eq!((second.len(),second[0].0.as_str(),second[99].0.as_str()),(101,"101000","200000"));
    let last=lib.track_sheet_choices(&Filter::default(),"rate","",Some(&second[99].0)).unwrap();
    assert_eq!((last.len(),last[4].0.as_str()),(5,"205000"));
}

#[test]
fn exact_titles_keep_multilingual_names_punctuation_empty_names_and_mixed_selections() {
    let names = ["", "!!!", "日本語の歌", "中文音乐", "한국어 노래", "éA", "Aé", "a \"quote\"", "😀", "Song", "song", "nul\0name",
        "À", "Ö", "Ø", "ö", "ø", "ÿ", "ぁ", "ゖ", "ァ", "ヺ", "一", "鿋", "가", "힣", "Ω", "𞤀", "\u{0301}"];
    let lib = fixture(names.len());
    for (at, name) in names.iter().enumerate() {
        lib.connection().execute("UPDATE tracks SET title=? WHERE id=?",rusqlite::params![name,at as i64+1]).unwrap();
    }
    for (at, name) in names.iter().enumerate() {
        assert_eq!(lib.browse_ids(&filter(json!({"title":[name]})),TrackOrder::Title).unwrap().ids,[at as i64+1],"{name:?}");
    }
    let mixed = lib.browse_ids(&filter(json!({"title":["日本語の歌","!!!","","😀"]})),TrackOrder::Title).unwrap();
    assert_eq!(mixed.ids.len(),4);
    assert!(mixed.ids.contains(&1) && mixed.ids.contains(&2) && mixed.ids.contains(&3) && mixed.ids.contains(&9));
    assert!(lib.browse_ids(&filter(json!({"title":["absent title"]})),TrackOrder::Title).unwrap().ids.is_empty());
}

#[test]
fn album_and_source_search_match_unicode_names_under_other_column_selections() {
    let lib = fixture(205);
    lib.connection().execute("UPDATE albums SET title='Café 日本' WHERE id=2",[]).unwrap();
    lib.connection().execute("UPDATE sources SET label='Bibliothèque Café'",[]).unwrap();
    let chosen = filter(json!({"artist":["Artist 002"]}));
    assert_eq!(lib.track_sheet_choices(&chosen,"album","cafe",None).unwrap(),[("Café 日本".into(),3)]);
    assert_eq!(lib.track_sheet_choices(&chosen,"album","日本",None).unwrap(),[("Café 日本".into(),3)]);
    assert_eq!(lib.track_sheet_choices(&chosen,"source","bibliotheque",None).unwrap(),[("local:default".into(),3)]);
    assert!(lib.track_sheet_choices(&chosen,"source","missing",None).unwrap().is_empty());
}

#[test]
fn selections_across_many_pages_keep_more_than_one_thousand_titles() {
    let lib = fixture(2050);
    let names: Vec<_> = (1..=1500).map(|id| format!("Song {id:06}")).collect();
    let result = lib.browse_ids(&filter(json!({"title":names})),TrackOrder::Title).unwrap();
    assert_eq!(result.ids, (1..=1500).collect::<Vec<i64>>());
}

#[test]
fn hundred_thousand_tracks_match_refine_results_and_measure_alternating_pairs() {
    let lib = fixture(100_000);
    lib.connection().execute("UPDATE tracks SET title='日本語の歌' WHERE id=50001",[]).unwrap();
    lib.connection().execute("UPDATE tracks SET title='中文音乐' WHERE id=50002",[]).unwrap();
    lib.connection().execute("UPDATE tracks SET title='한국어 노래' WHERE id=50003",[]).unwrap();
    let sheet = filter(json!({"title":["Song 050000"]}));
    let refine = Filter { query: Some(Node::Is(Field::Title,Values::Any(vec!["Song 050000".into()]))), ..Default::default() };
    let expected = lib.browse_ids(&refine,TrackOrder::Title).unwrap();
    assert_eq!(expected.ids,[50_000]);
    assert_eq!(lib.browse_ids(&sheet,TrackOrder::Title).unwrap().ids, expected.ids);
    let measure = |query: &Filter| {
        let start = Instant::now();
        assert_eq!(lib.browse_ids(query,TrackOrder::Title).unwrap().ids, expected.ids);
        start.elapsed().as_secs_f64()*1000.0
    };
    let facet = |sheet: bool| {
        let start = Instant::now();
        let values = if sheet { lib.track_sheet_choices(&Filter::default(),"artist","Artist 042",None).unwrap() }
            else { lib.query_facet_counts(&Filter::default(),None,Field::Artist,"Artist 042").unwrap().1 };
        assert_eq!(values,[("Artist 042".into(),1000)]);
        start.elapsed().as_secs_f64()*1000.0
    };
    let mut samples = Vec::new();
    for n in 0..7 {
        let (sheet_ms,refine_ms) = if n%2==0 { (measure(&sheet),measure(&refine)) } else { let r=measure(&refine); (measure(&sheet),r) };
        let (sheet_facet_ms,refine_facet_ms) = if n%2==0 { (facet(true),facet(false)) } else { let r=facet(false); (facet(true),r) };
        samples.push(json!({"sheet_ms":sheet_ms,"refine_ms":refine_ms,"sheet_facet_ms":sheet_facet_ms,"refine_facet_ms":refine_facet_ms}));
    }
    let node = |field, value: &str| Node::Is(field, Values::Any(vec![value.into()]));
    let scenarios = [
        ("title_japanese", json!({"title":["日本語の歌"]}), Some(node(Field::Title,"日本語の歌"))),
        ("title_chinese", json!({"title":["中文音乐"]}), Some(node(Field::Title,"中文音乐"))),
        ("title_korean", json!({"title":["한국어 노래"]}), Some(node(Field::Title,"한국어 노래"))),
        ("artist", json!({"artist":["Artist 042"]}), Some(node(Field::Artist,"Artist 042"))),
        ("album", json!({"album":["Record 042"]}), Some(node(Field::Album,"Record 042"))),
        ("rating", json!({"rating":[0]}), Some(node(Field::Rating,"0"))),
        ("bits", json!({"bits":[16]}), Some(node(Field::Bits,"16"))),
        ("rate", json!({"rate":[44100]}), Some(node(Field::Rate,"44100"))),
        ("quality", json!({"quality":["lossless"]}), Some(node(Field::Quality,"cd"))),
        ("intersection", json!({"artist":["Artist 042"],"title":["Song 000041"],"quality":["lossless"]}),
            Some(Node::All(vec![node(Field::Artist,"Artist 042"),node(Field::Title,"Song 000041"),node(Field::Quality,"cd")]))),
        ("source", json!({"source":["local:default"]}), None),
        ("format", json!({"format":["flac"]}), None),
        ("favorite", json!({"favorite":true}), None),
    ];
    let mut cases = Vec::new();
    for (name, selection, query) in scenarios {
        let chosen = filter(selection);
        let baseline = query.map(|query| Filter { query: Some(query), ..Default::default() });
        let expected = lib.browse_ids(&chosen,TrackOrder::Title).unwrap().ids;
        let measure = |filter: &Filter| {
            let at = Instant::now();
            assert_eq!(lib.browse_ids(filter,TrackOrder::Title).unwrap().ids,expected);
            at.elapsed().as_secs_f64()*1000.0
        };
        let mut pairs = Vec::new();
        for n in 0..7 {
            let (sheet_ms,refine_ms) = if n%2==0 { (measure(&chosen),baseline.as_ref().map(measure)) }
                else { let base=baseline.as_ref().map(measure); (measure(&chosen),base) };
            pairs.push(json!({"sheet_ms":sheet_ms,"refine_ms":refine_ms}));
        }
        cases.push(json!({"field":name,"matching_tracks":expected.len(),"samples":pairs}));
    }
    let mut facets = Vec::new();
    for field in ["title","artist","album","source","format","quality","rating","bits","rate","favorite"] {
        let mut pairs = Vec::new();
        let sheet = || {
            let at=Instant::now();
            let rows=lib.track_sheet_choices(&Filter::default(),field,"",None).unwrap();
            assert!(rows.len()<=101);
            (at.elapsed().as_secs_f64()*1000.0,rows.len())
        };
        let refine = || {
            if field!="artist" { return None; }
            let at=Instant::now();
            let rows=lib.query_facet_counts(&Filter::default(),None,Field::Artist,"").unwrap().1;
            Some((at.elapsed().as_secs_f64()*1000.0,rows.len()))
        };
        for n in 0..7 {
            let (sheet,refine)=if n%2==0 { (sheet(),refine()) } else { let base=refine(); (sheet(),base) };
            pairs.push(json!({"sheet_ms":sheet.0,"sheet_values":sheet.1,"refine_ms":refine.map(|v|v.0),"refine_values":refine.map(|v|v.1)}));
        }
        facets.push(json!({"field":field,"search":"","samples":pairs}));
    }
    lib.connection().execute("UPDATE albums SET title='Café 日本' WHERE id=2",[]).unwrap();
    lib.connection().execute("UPDATE sources SET label='Bibliothèque Café'",[]).unwrap();
    let mut search_facets=Vec::new();
    for (field,search) in [("album","cafe"),("album","日本"),("source","bibliotheque")] {
        let mut samples=Vec::new();
        for _ in 0..7 {
            let at=Instant::now();
            let rows=lib.track_sheet_choices(&Filter::default(),field,search,None).unwrap();
            assert_eq!(rows.len(),1);
            samples.push(at.elapsed().as_secs_f64()*1000.0);
        }
        search_facets.push(json!({"field":field,"search":search,"sheet_ms":samples}));
    }
    let mut cold = Vec::new();
    for (name, text) in [("title","Song 050000"),("title_japanese","日本語の歌"),("title_chinese","中文音乐"),("title_korean","한국어 노래")] {
        let sheet = filter(json!({"title":[text]}));
        let refine = Filter { query:Some(node(Field::Title,text)), ..Default::default() };
        let mut pairs=Vec::new();
        let measure = |filter: &Filter| {
            lib.connection().flush_prepared_statement_cache();
            let at=Instant::now();
            assert_eq!(lib.browse_ids(filter,TrackOrder::Title).unwrap().ids.len(),1);
            at.elapsed().as_secs_f64()*1000.0
        };
        for n in 0..7 {
            let (sheet_ms,refine_ms)=if n%2==0 { (measure(&sheet),measure(&refine)) }
                else { let base=measure(&refine); (measure(&sheet),base) };
            pairs.push(json!({"sheet_ms":sheet_ms,"refine_ms":refine_ms}));
        }
        cold.push(json!({"field":name,"samples":pairs}));
    }
    if let Some(path) = std::env::var_os("CHILPA_TRACK_SHEET_BENCH_OUTPUT") {
        std::fs::write(path,serde_json::to_vec_pretty(&json!({"tracks":100000,"profile":"debug","samples":samples,"cases":cases,"facets":facets,"uncached_titles":cold,"search_facets":search_facets,
            "scope":"Same fixture and result IDs for paired snapshots. Repeated sheet queries reuse the existing bounded SQLite statement cache; uncached_titles flushes it before every query. Rating and quality are equivalent on this uniform fixture. Blank artist facets compare 100 values with Refine 3; other facets and source/format/favorite have no equivalent Refine operation."})).unwrap()).unwrap();
    }
}
