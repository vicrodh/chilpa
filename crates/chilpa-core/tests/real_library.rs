//! A rescan of a real library, on a copy of its database, run on purpose.
//!
//! CHILPA_SMOKE_LIBRARY=/path/to/a/copy/library.db \
//!   cargo test -p chilpa-core --test real_library -- --ignored --nocapture
//!
//! It rescans every root the copy knows, the way the window does, and says
//! how many tracks had no record before and after, and where the ones left
//! are. Never point it at the library in use: it writes.

use std::path::PathBuf;

fn orphans(lib: &chilpa_library::Library) -> Vec<String> {
    lib.connection()
        .prepare("SELECT external_id FROM tracks WHERE present = 1 AND album_id IS NULL AND source_id = 1 ORDER BY external_id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
#[ignore = "rescans a real library; run it on purpose, on a copy"]
fn a_rescan_gives_every_track_it_can_a_record() {
    let Some(copy) = std::env::var_os("CHILPA_SMOKE_LIBRARY").map(PathBuf::from) else {
        eprintln!("smoke: set CHILPA_SMOKE_LIBRARY to a copy of a library to run this");
        return;
    };
    let lib = chilpa_library::Library::open(&copy).unwrap();
    let roots: Vec<PathBuf> = lib
        .connection()
        .prepare("SELECT path FROM roots ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|p| PathBuf::from(p.unwrap()))
        .collect();
    let before = orphans(&lib);
    eprintln!("smoke: {} tracks without a record before", before.len());
    for root in &roots {
        let started = std::time::Instant::now();
        let report = chilpa_library::scan::scan_root_with(&lib, root, None, &|_| {}, &chilpa_core::sacd_tracks::DiscImages).unwrap();
        eprintln!(
            "smoke: {} read {} unchanged {} missing {} in {:.1} s",
            root.display(), report.read, report.unchanged, report.missing, started.elapsed().as_secs_f64()
        );
    }
    let after = orphans(&lib);
    eprintln!("smoke: {} tracks without a record after", after.len());
    for path in &after {
        eprintln!("smoke:   left {path}");
    }
    let mut records = std::collections::BTreeMap::<(String, String, Option<i32>), usize>::new();
    for path in &before {
        let found: Option<(String, String, Option<i32>)> = lib
            .connection()
            .query_row(
                "SELECT a.title, a.album_artist, a.year FROM tracks t JOIN albums a ON a.id = t.album_id WHERE t.external_id = ?1",
                [path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();
        if let Some(key) = found {
            *records.entry(key).or_default() += 1;
        }
    }
    for ((title, artist, year), tracks) in &records {
        eprintln!("smoke:   now on {title} | {artist} | {year:?} | {tracks} tracks");
    }
    assert!(after.len() < before.len() || before.is_empty(), "the rescan gave no orphan a record");
}
