use super::*;
use crate::library_sources::{query, SourceItem, Stars};
use chilpa_library::{store::query::parse, Filter, Library, TrackOrder};
use rusqlite::params;

fn compare(reader: &crate::Reader, index: &SourceIndex, field: &str, buckets: &[(&str, &str)]) {
    let mut ids = reader.lib.browse_ids(&Filter::default(), TrackOrder::Title).unwrap().album_ids;
    ids.extend(index.items.iter().map(|i| i.album));
    ids.sort_unstable();
    ids.dedup();
    let groups = reader.album_groups(ids, AlbumGroup::from_name(field), false, false, index);
    for (name, label) in buckets {
        let node = parse(&format!("{field}:{name}")).unwrap();
        let filter = Filter { query: Some(node.clone()), ..Default::default() };
        let mut filtered = reader.lib.browse_ids(&filter, TrackOrder::Title).unwrap().album_ids;
        filtered.extend(index.items.iter().filter(|item| query::holds(&node, item, &Stars::default(), 0)).map(|i| i.album));
        filtered.sort_unstable();
        filtered.dedup();
        let mut grouped = Vec::new();
        for (at, (start, title)) in groups.groups.iter().enumerate() {
            if *title == chilpa_i18n::t(label) {
                let end = groups.groups.get(at + 1).map_or(groups.ids.len(), |g| g.0);
                grouped.extend_from_slice(&groups.ids[*start..end]);
            }
        }
        grouped.sort_unstable();
        assert!(!grouped.is_empty(), "the fixture must cover {name}");
        assert_eq!(filtered, grouped, "{field}:{name}");
    }
}

#[test]
fn release_type_queries_agree_with_local_and_source_record_groups() {
    let lib = Library::in_memory().unwrap();
    lib.set_group_album_versions(false).unwrap();
    let source = lib.source_id("local", "default").unwrap();
    let mut items = Vec::new();
    for (at, (title, artist, count, duration)) in [
        ("Full Record", "Band", 7, 200_000), ("Short Record", "Band", 3, 200_000),
        ("Small Record", "Band", 4, 200_000), ("Six Short Songs", "Band", 6, 299_999),
        ("Six Long Songs", "Band", 6, 300_000), ("Live at Home", "Band", 7, 200_000),
        ("Record [EP]", "Band", 8, 200_000), ("Record - Single", "Band", 8, 200_000),
        ("Hits", "Various Artists", 8, 200_000), ("Film (Original Soundtrack)", "Band", 8, 200_000),
    ].into_iter().enumerate() {
        let album = at as i64 + 1;
        lib.connection().execute("INSERT INTO albums(id, title, album_artist) VALUES (?1, ?2, ?3)", params![album, title, artist]).unwrap();
        for n in 0..count {
            lib.connection().execute("INSERT INTO tracks(source_id, external_id, title, album_id, duration_ms, added_at)
                VALUES (?1, ?2, 'Song', ?3, ?4, 1)", params![source, format!("/{album}/{n}.flac"), album, duration]).unwrap();
            items.push(SourceItem { album: -album, track: -(album * 100 + n), album_title: title.into(), album_artist: artist.into(),
                tracks: 1, duration_ms: duration as u64, ..Default::default() });
        }
    }
    items.push(SourceItem { album: -999, ..Default::default() });
    let index = SourceIndex::new(items, Vec::new());
    compare(&crate::Reader::of(lib), &index, "type", &[("album", "Albums"), ("single", "Singles"), ("ep", "EPs"),
        ("compilation", "Compilations"), ("live", "Live"), ("soundtrack", "Soundtracks")]);
}

#[test]
fn quality_queries_agree_with_local_and_source_record_groups_at_the_boundaries() {
    let lib = Library::in_memory().unwrap();
    lib.set_group_album_versions(false).unwrap();
    let source = lib.source_id("local", "default").unwrap();
    let cases = [("a.mp3", None, Some(32), Some(96_000), "lossy"), ("a.oga", None, Some(32), Some(96_000), "lossy"),
        ("a.flac", None, Some(23), Some(48_000), "lossless"), ("a.flac", None, Some(24), Some(48_000), "hires"),
        ("a.flac", None, Some(16), Some(48_001), "hires"), ("a.m4a", None, None, Some(44_100), "lossy"),
        ("a.m4a", None, Some(16), Some(44_100), "lossless"), ("a.dsf", None, None, None, "dsd"),
        ("a.unknown", None, None, None, "unknown"), ("a.unknown", None, Some(16), Some(48_000), "lossless"),
        ("a.wav", None, Some(16), Some(2_822_399), "hires"), ("a.wav", None, Some(16), Some(2_822_400), "dsd"),
        ("remote:test/instance/1", Some("alac"), None, None, "lossless"), ("remote:test/instance/2", Some("opus"), Some(32), Some(96_000), "lossy")];
    let mut items = Vec::new();
    for (at, (path, codec, bits, rate, tier)) in cases.into_iter().enumerate() {
        let album = at as i64 + 1;
        lib.connection().execute("INSERT INTO albums(id, title, album_artist) VALUES (?1, ?2, 'Band')", params![album, format!("Record {album}")]).unwrap();
        let path = if path.starts_with("remote:") { path.to_string() } else { format!("/{album}/{path}") };
        lib.connection().execute("INSERT INTO tracks(source_id, external_id, title, album_id, bit_depth, sample_rate_hz, codec, added_at)
            VALUES (?1, ?2, 'Song', ?3, ?4, ?5, ?6, 1)", params![source, path, album, bits, rate, codec]).unwrap();
        items.push(SourceItem { album: -album, track: -album, tier, tracks: 1, ..Default::default() });
    }
    // I give a lossy record a better file, then an absent DSD file.
    lib.connection().execute("INSERT INTO tracks(source_id, external_id, title, album_id, bit_depth, sample_rate_hz, added_at)
        VALUES (?1, '/mixed.flac', 'Better', 1, 24, 96000, 1)", [source]).unwrap();
    lib.connection().execute("INSERT INTO tracks(source_id, external_id, title, album_id, present, added_at)
        VALUES (?1, '/missing.dsf', 'Absent', 1, 0, 1)", [source]).unwrap();
    items.push(SourceItem { album: -1, track: -100, tier: "hires", tracks: 1, ..Default::default() });
    compare(&crate::Reader::of(lib), &SourceIndex::new(items, Vec::new()), "quality",
        &[("lossy", "Lossy"), ("cd", "Lossless"), ("hires", "Hi-Res"), ("dsd", "DSD")]);
}
