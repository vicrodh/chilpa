use super::*;
use crate::library_sources::SourceIndex;
use chilpa_library::store::query::parse;

#[test]
fn source_play_counts_use_the_track_value_and_keep_missing_distinct_from_zero() {
    let items = SourceIndex::new([None, Some(0), Some(1), Some(2), Some(10)].into_iter().enumerate()
        .map(|(at, plays)| SourceItem { track: -(at as i64 + 1), plays, ..Default::default() }).collect(), Vec::new());
    let stars = Stars::default();
    for (text, expected) in [("plays:0", vec![1]), ("plays:1,10", vec![2, 4]), ("plays:1..2", vec![2, 3]),
        ("plays:1..", vec![2, 3, 4]), ("plays:..1", vec![1, 2]), ("-plays:0", vec![0, 2, 3, 4]),
        ("-plays:1..", vec![0, 1])] {
        let node = parse(text).unwrap();
        let matches: Vec<usize> = items.items.iter().enumerate().filter(|(_, item)| holds(&node, item, &stars, 0)).map(|(at, _)| at).collect();
        assert_eq!(matches, expected, "{text}");
    }
}

#[test]
fn source_buckets_use_all_record_items_and_missing_data_only_passes_excludes() {
    let index = SourceIndex::new(vec![
        SourceItem { album: -1, track: -10, album_title: "Record".into(), release_type: " EP ".into(), tier: "lossy", tracks: 1, ..Default::default() },
        SourceItem { album: -1, track: -11, album_title: "Record".into(), tier: "hires", tracks: 1, ..Default::default() },
        SourceItem { album: -2, ..Default::default() },
    ], Vec::new());
    for text in ["type:ep", "type:album,ep", "quality:hires", "quality:cd,hires"] {
        let node = parse(text).unwrap();
        assert!(index.items[..2].iter().all(|item| holds(&node, item, &Stars::default(), 0)), "{text}");
        assert!(!holds(&node, &index.items[2], &Stars::default(), 0), "{text}");
        assert!(holds(&parse(&format!("-{text}")).unwrap(), &index.items[2], &Stars::default(), 0), "{text}");
    }
    for text in ["quality:lossy", "quality:cd", "type:single"] {
        assert!(!holds(&parse(text).unwrap(), &index.items[0], &Stars::default(), 0), "{text}");
    }
    let unknown_record = SourceIndex::new(vec![SourceItem { track: -99, tracks: 1, title: "Song".into(),
        album_artist: "Band".into(), ..Default::default() }], Vec::new());
    assert!(!holds(&parse("type:single").unwrap(), &unknown_record.items[0], &Stars::default(), 0));
    assert!(holds(&parse("-type:single").unwrap(), &unknown_record.items[0], &Stars::default(), 0));
    for kind in chilpa_library::release_kind::NAMES {
        let index = SourceIndex::new(vec![SourceItem { album: -1, release_type: kind.to_uppercase(), ..Default::default() }], Vec::new());
        assert!(holds(&parse(&format!("type:{kind}")).unwrap(), &index.items[0], &Stars::default(), 0));
    }
    for (tier, name) in [("lossy", "lossy"), ("lossless", "cd"), ("hires", "hires"), ("dsd", "dsd")] {
        let index = SourceIndex::new(vec![SourceItem { album: -1, tier, ..Default::default() }], Vec::new());
        assert!(holds(&parse(&format!("quality:{name}")).unwrap(), &index.items[0], &Stars::default(), 0));
    }
}

#[test]
fn a_standalone_track_does_not_inherit_an_unrelated_records_buckets() {
    let index = SourceIndex::new(vec![
        SourceItem { album: -1, track: -10, release_type: "ep".into(), tier: "dsd", ..Default::default() },
        SourceItem { album: 0, track: -1, tracks: 1, tier: "lossy", ..Default::default() },
    ], Vec::new());
    let item = &index.items[1];
    for text in ["-type:ep", "quality:lossy", "-quality:dsd"] {
        assert!(holds(&parse(text).unwrap(), item, &Stars::default(), 0), "{text}");
    }
    assert!(!holds(&parse("type:single").unwrap(), item, &Stars::default(), 0));
}
