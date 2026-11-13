//! Opt-in measurements of first and repeated Tracks snapshots, without Qt.

use super::*;
use serde_json::json;
use std::time::Instant;

fn fixture() -> Reader {
    let lib = chilpa_library::Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    lib.connection().execute_batch("BEGIN;
        WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<1000)
        INSERT INTO albums(id,title,album_artist)
        SELECT i,printf('Record %04d',i),printf('Artist %04d',i) FROM n;
        INSERT INTO album_artist_rollup(album_id,artist,tracks) SELECT id,album_artist,100 FROM albums;").unwrap();
    lib.connection().execute("WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<100000)
        INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,added_at,bit_depth,sample_rate_hz,duration_ms)
        SELECT i,?,printf('/track/%06d.flac',i),printf('Song %06d',i),printf('Artist %04d',i%1000+1),
            i%1000+1,1,16,44100,1000 FROM n", [source]).unwrap();
    lib.connection().execute_batch("COMMIT; ANALYZE;").unwrap();
    Reader::of(lib)
}

#[test]
#[ignore = "100k Tracks latency diagnostic; run in an isolated profile with --nocapture"]
fn empty_sheet_and_legacy_snapshot_latency() {
    let mut samples = Vec::new();
    // Fresh readers give both protocols the same cold edition-catalog work.
    // The fixture and its ids live only for this iteration, never in a cache.
    for sheet in [false, true] {
        let reader = fixture();
        let started = Instant::now();
        let (_, index) = reader.source_index();
        samples.push(json!({"sheet":sheet,"stage":"source_index","ms":started.elapsed().as_secs_f64()*1000.0}));
        for round in 0..3 {
            for (order, group) in [("album", "off"), ("title", "off"), ("album", "album"), ("title", "artist")] {
                let key = format!("{order}\u{1f}{group}\u{1f}\u{1f}{}", if sheet { "\u{1f}{}" } else { "" });
                let started = Instant::now();
                let found = reader.tracks_using(&key, &Filter::default(), &index);
                samples.push(json!({"sheet":sheet,"round":round,"order":order,"group":group,
                    "stage":"tracks","ms":started.elapsed().as_secs_f64()*1000.0,"count":found.snapshot.ids.len()}));
                assert_eq!(found.snapshot.ids.len(), 100_000);
                assert_eq!(found.snapshot.totals.duration_ms, 100_000_000);
                if group == "off" { assert!(found.sections.is_empty()); }
            }
        }
        let legacy = reader.tracks_using("album\u{1f}off\u{1f}\u{1f}", &Filter::default(), &index);
        let sheet = reader.tracks_using("album\u{1f}off\u{1f}\u{1f}\u{1f}{}", &Filter::default(), &index);
        assert_eq!(legacy.snapshot.ids, sheet.snapshot.ids);
        assert_eq!(legacy.snapshot.album_ids, sheet.snapshot.album_ids);
    }
    println!("tracks-snapshot-latency {}", json!({"tracks":100_000,"samples":samples}));
}
