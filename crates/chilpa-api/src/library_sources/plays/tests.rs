use super::*;
use crate::{library_sources::SourceItem, Filter, Reader};
use rusqlite::params;

fn library() -> Library {
    let lib = Library::in_memory().unwrap();
    lib.set_group_album_versions(false).unwrap();
    lib.connection().execute_batch("INSERT INTO sources(id, provider, instance) VALUES (1, 'local', 'default');
        INSERT INTO albums(id, title, album_artist) VALUES (1, 'A', 'Band'), (2, 'B', 'Band'), (3, 'C', 'Band');
        INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at) VALUES
        (1, 1, '/a.flac', 'A', 1, 1), (2, 1, '/b.flac', 'B', 1, 1),
        (3, 1, '/c.flac', 'C', 2, 1), (4, 1, '/d.flac', 'D', 3, 1);").unwrap();
    for (track, count) in [(1, 1), (2, 9), (3, 0), (4, 1)] {
        for at in 0..=count {
            lib.connection().execute("INSERT INTO play_events(track_id, source_id, started_at, played_ms, duration_ms)
                VALUES (?1, 1, ?2, ?3, 1000)", params![track, at, if at == 0 { 499 } else { 500 }]).unwrap();
        }
    }
    lib
}

fn sources() -> SourceIndex {
    SourceIndex::new([(-1, -1, Some(1)), (-2, -1, Some(9)), (-3, -2, Some(0)), (-4, -3, Some(1))]
        .into_iter().map(|(track, album, plays)| SourceItem {
            track, album, plays, title: format!("Source {}", -track), tracks: 1,
            saved_track: true, saved_album: true, ..Default::default()
        }).collect(), Vec::new())
}

#[test]
fn tiers_keep_the_boundaries_and_their_order() {
    assert_eq!(AlbumGroup::from_name("plays"), AlbumGroup::Plays);
    let labels = ["Never played", "Played", "Played", "Played 10 times or more"];
    for (count, expected) in [0, 1, 9, 10].into_iter().zip(labels) {
        assert_eq!(label(count), chilpa_i18n::t(expected));
    }
    assert!(file(10).rank < file(9).rank && file(1).rank < file(0).rank);
}

#[test]
fn records_sum_tracks_and_keep_the_requested_order_within_each_tier() {
    let reader = Reader::of(library());
    let index = sources();
    for ids in [vec![3, -3, 2, -2, 1, -1], vec![-1, 1, -2, 2, -3, 3]] {
        let result = reader.album_groups(ids.clone(), AlbumGroup::Plays, false, true, &index);
        let expected: Vec<_> = [1_i64, 3, 2].iter().flat_map(|album| ids.iter().copied().filter(move |id| id.abs() == *album)).collect();
        assert_eq!(result.ids, expected);
        assert_eq!(result.groups, [(0, label(10)), (2, label(1)), (4, label(0))]);
    }
    let result = reader.albums_view_using("title-desc", "plays", "", &Filter::default(), &SourceIndex::default());
    assert_eq!(result.ids, [1, 3, 2]);
}

#[test]
fn tracks_use_their_own_counts_with_both_sort_directions() {
    let reader = Reader::of(library());
    let index = sources();
    for (order, expected) in [("title", vec![1, 2, 4, -1, -2, -4, 3, -3]),
        ("title-desc", vec![-4, -2, -1, 4, 2, 1, -3, 3]),
        ("plays", vec![2, -2, 1, 4, -1, -4, 3, -3]),
        ("plays-asc", vec![1, 4, -1, -4, 2, -2, 3, -3])] {
        let found = reader.tracks_using(&format!("{order}\u{1f}plays\u{1f}"), &Filter::default(), &index);
        assert_eq!(found.snapshot.ids, expected);
        assert_eq!(found.sections, [(0, label(1)), (6, label(0))]);
        assert_eq!(found.letters, "[]");
        assert_eq!(found.snapshot.totals.tracks, 8);
    }
    reader.lib.connection().execute("INSERT INTO play_events(track_id, source_id, started_at, played_ms, duration_ms)
        VALUES (2, 1, 99, 1000, 1000)", []).unwrap();
    let found = reader.tracks_using("title\u{1f}plays\u{1f}", &Filter::default(), &index);
    assert_eq!(found.snapshot.ids[0], 2);
    assert_eq!(found.sections, [(0, label(10)), (1, label(1)), (6, label(0))]);
    let row = reader.lib.tracks_by_ids(&[2]).unwrap().remove(0);
    assert_eq!(row.plays, 10);
}

#[test]
fn record_groups_agree_with_the_played_query() {
    use crate::library_sources::{query, Stars};
    use chilpa_library::{store::query::parse, TrackOrder};
    let lib = library();
    lib.connection().execute("INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at)
        VALUES (5, 1, '/unplayed.flac', 'Unplayed', 1, 1)", []).unwrap();
    let mut items = sources().items;
    items.push(SourceItem { track: -5, album: -1, plays: Some(0), ..Default::default() });
    items.push(SourceItem { album: -4, ..Default::default() });
    let index = SourceIndex::new(items, Vec::new());
    let node = parse("plays:1..").unwrap();
    let filter = Filter { query: Some(node.clone()), ..Default::default() };
    let mut filtered = lib.browse_ids(&filter, TrackOrder::Title).unwrap().album_ids;
    filtered.extend(index.items.iter().filter(|item| query::holds(&node, item, &Stars::default(), 0)).map(|item| item.album));
    filtered.sort_unstable();
    filtered.dedup();
    let grouped = Reader::of(lib).album_groups(vec![1, 2, 3, -1, -2, -3, -4], AlbumGroup::Plays, false, false, &index);
    assert_eq!(grouped.groups, [(0, label(10)), (2, label(1)), (4, label(0))]);
    assert_eq!(&grouped.ids[4..], &[2, -2, -4], "I include source records without a count in Never played");
    for (at, (start, name)) in grouped.groups.iter().enumerate() {
        let end = grouped.groups.get(at + 1).map_or(grouped.ids.len(), |group| group.0);
        for id in &grouped.ids[*start..end] {
            assert_eq!(filtered.contains(id), *name != label(0), "I compare record {id} in {name} with plays:1..");
        }
    }
}

#[test]
fn grouping_twenty_thousand_tracks_and_fourteen_hundred_records_is_bounded() {
    use std::{io::Write, time::Instant};
    let lib = Library::in_memory().unwrap();
    lib.set_group_album_versions(false).unwrap();
    lib.connection().execute_batch("BEGIN;
        INSERT INTO sources(id, provider, instance) VALUES (1, 'local', 'default');
        WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 1400)
        INSERT INTO albums(id, title, album_artist) SELECT x, printf('Record %04d', x), 'Band' FROM n;
        WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < 20000)
        INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at)
        SELECT x, 1, printf('/%d.flac', x), printf('Track %05d', x), (x - 1) % 1400 + 1, 1 FROM n;
        INSERT INTO track_rollup(track_id, plays) SELECT id, CASE WHEN album_id % 3 = 0 THEN 0
            WHEN album_id % 3 = 1 THEN 1 ELSE 10 END FROM tracks;
        COMMIT;").unwrap();
    let reader = Reader::of(lib);
    let index = SourceIndex::default();
    let before = reader.lib.connection().total_changes();
    let mut albums_ms = Vec::new();
    let mut tracks_ms = Vec::new();
    let mut baseline_ms = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        let baseline = reader.album_groups((1..=1400).collect(), AlbumGroup::Added, false, false, &index);
        baseline_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(baseline.ids.len(), 1400);
        let start = Instant::now();
        let albums = reader.album_groups((1..=1400).collect(), AlbumGroup::Plays, false, false, &index);
        albums_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(albums.ids.len(), 1400);
        let start = Instant::now();
        let tracks = reader.tracks_using("title\u{1f}plays\u{1f}", &Filter::default(), &index);
        tracks_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(tracks.snapshot.ids.len(), 20_000);
        assert_eq!(tracks.sections.len(), 3);
    }
    for samples in [&mut albums_ms, &mut tracks_ms, &mut baseline_ms] { samples.sort_by(f64::total_cmp); }
    writeln!(std::io::stderr(), "plays grouping (debug, median of 5): 1400 records {:.3} ms; 20000 tracks with snapshot {:.3} ms; added grouping {:.3} ms",
        albums_ms[2], tracks_ms[2], baseline_ms[2]).unwrap();
    assert_eq!(reader.lib.connection().total_changes(), before, "I only read while grouping");
    assert!(albums_ms[2] < 2000.0 && tracks_ms[2] < 2000.0, "I keep both medians below two seconds");
}
