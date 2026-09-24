//! Keeping a large library fast.
//!
//! The QBZ audit's high finding was the database: every page of the grid
//! aggregated the whole library again, and a query per row did the rest.
//! This test builds a genuinely large library and puts a time ceiling on
//! what the interface queries.
//!
//! The ceilings are generous -- roughly ten times what this machine
//! measures -- but not arbitrary. They are not there to measure the machine:
//! they are there so a query that turns quadratic shows up the day someone
//! writes it, not the day a user opens their library. A ceiling so high that
//! nothing can hit it is not a gate, and these used to all be 1500 ms with
//! everything running in 3.
//!
//! Measured on this machine with fifty thousand tracks, before and after the
//! V6 and V7 migrations (the rollups and FTS5):
//!
//! | what | before | after |
//! | --- | --- | --- |
//! | grid, sorted on an album column | 7 ms | 3 ms |
//! | grid, sorted by plays or by date | 135-155 ms | 3-4 ms |
//! | artists | 78 ms | 7 ms |
//! | search | 80 ms | 12 ms |
//! | most played | 72 ms | 10 ms |
//! | last played | 70 ms | 0.2 ms |
//! | the eight smart lists, counted | 200 ms | 60 ms |
//!
//! That last one is the worst case by construction and not by accident: in
//! this fixture every track has been played, so "Never played" cannot stop
//! at its limit and walks the whole library to find nothing. A real library
//! is the other way round -- most of it has never been played -- and the
//! count stops at two hundred. The number to keep an eye on is this one,
//! because it is the one that cannot get lucky.

use chilpa_library::{AlbumSort, Library, Rule, SmartList, SmartOrder};
use rusqlite::params;
use std::time::{Duration, Instant};

const ALBUMS: i64 = 5_000;
const PER_ALBUM: i64 = 10;
const TRACKS: i64 = ALBUMS * PER_ALBUM;

/// Builds a library with fifty thousand tracks and a play log.
fn large_library() -> Library {
    let lib = Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    let tx = lib.connection().unchecked_transaction().unwrap();
    {
        let mut albums = tx
            .prepare("INSERT INTO albums (id, title, album_artist, year) VALUES (?1, ?2, ?3, ?4)")
            .unwrap();
        let mut tracks = tx
            .prepare(
                "INSERT INTO tracks (id, source_id, external_id, title, artist, album_id,
                                     duration_ms, bit_depth, sample_rate_hz, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )
            .unwrap();
        let mut plays = tx
            .prepare(
                "INSERT INTO play_events (track_id, source_id, started_at, played_ms,
                                          duration_ms, bit_perfect)
                 VALUES (?1, ?2, ?3, ?4, ?4, 1)",
            )
            .unwrap();

        for a in 1..=ALBUMS {
            // Two hundred artists, so the grid by artist really groups and is
            // not one row per artist.
            let artist = format!("Artist {:03}", a % 200);
            albums
                .execute(params![a, format!("Disco {a}"), artist, 1960 + (a % 60) as i32])
                .unwrap();
            for p in 1..=PER_ALBUM {
                let id = (a - 1) * PER_ALBUM + p;
                let bits = if id % 7 == 0 { 24 } else { 16 };
                let rate = if id % 7 == 0 { 96_000 } else { 44_100 };
                tracks
                    .execute(params![
                        id,
                        source,
                        format!("/music/{a}/{p:02}.flac"),
                        format!("Track {p} of {a}"),
                        format!("Artist {:03}", a % 200),
                        a,
                        200_000 + (id % 400_000),
                        bits,
                        rate,
                        1_700_000_000i64 + id
                    ])
                    .unwrap();
                // One in three has been played, some of them several times.
                if id % 3 == 0 {
                    for k in 0..=(id % 4) {
                        plays
                            .execute(params![id, source, 1_700_000_000i64 + id * 10 + k, 200_000])
                            .unwrap();
                    }
                }
            }
        }
    }
    tx.commit().unwrap();
    lib
}

/// Runs something and says how long it took.
fn how_long<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let r = f();
    (r, start.elapsed())
}

fn within(name: &str, took: Duration, ceiling: Duration) {
    // With `--nocapture` the timings show, which is what makes this file
    // useful when something starts getting slow but still passes.
    eprintln!("{name}: {took:?}");
    assert!(
        took <= ceiling,
        "{name} took {took:?}, the ceiling is {ceiling:?}: something turned quadratic"
    );
}

#[test]
fn a_fifty_thousand_track_library_does_not_crawl() {
    let (lib, built) = how_long(large_library);
    eprintln!("building {TRACKS} tracks: {built:?}");
    assert_eq!(lib.track_count(false).unwrap(), TRACKS as usize);

    // The grid: a page of a hundred albums with their play counts. This is
    // what aggregated the whole library per page in QBZ.
    //
    // The orderings that only look at `albums` columns trim the page
    // before counting anything and come out in about ten milliseconds;
    // the ones sorting by plays or by date added have to aggregate, and
    // run around a hundred and fifty. That is why the ceiling is the same
    // for all: what is watched is that none of them turns quadratic.
    for order in AlbumSort::all() {
        let (page, t) = how_long(|| lib.albums_sorted(order, 100, 0).unwrap());
        assert_eq!(page.len(), 100, "{}", order.name());
        within(&format!("grid by {}", order.name()), t, Duration::from_millis(50));
    }

    // A page from deep in: it must not cost more than the first one just
    // for being far away.
    let (_, t) = how_long(|| lib.albums_sorted(AlbumSort::Artist, 100, 4_000).unwrap());
    within("grid on page 40", t, Duration::from_millis(80));

    // The artists.
    let (artists, t) = how_long(|| lib.artists(500, 0).unwrap());
    assert_eq!(artists.len(), 200, "two hundred artists");
    within("artists", t, Duration::from_millis(80));

    // The search, which walks titles, artists, albums and genres.
    let (found, t) = how_long(|| lib.search("Track 7", 200).unwrap());
    assert!(!found.is_empty());
    within("search", t, Duration::from_millis(120));

    // The stats, which sum the whole log.
    let (stats, t) = how_long(|| lib.stats().unwrap());
    assert!(stats.plays > 0);
    // The only one that still reads the whole log, and the only one that
    // has to: a total is a total. It is one view, opened on purpose, not
    // something that runs while someone scrolls.
    within("statistics", t, Duration::from_millis(300));

    let (_, t) = how_long(|| lib.top_tracks(20).unwrap());
    within("most played", t, Duration::from_millis(100));
    let (_, t) = how_long(|| lib.recently_played(20).unwrap());
    within("last played", t, Duration::from_millis(50));

    // The smart lists. Counting them is what the panel does -- eight side
    // by side, every time the tab opens -- and resolving one is what
    // happens when somebody presses play on it. The first has to be
    // instant; the second can take what a query takes.
    let (_, t) = how_long(|| {
        chilpa_library::smart_builtins()
            .iter()
            .map(|l| lib.smart_count(l).unwrap())
            .sum::<usize>()
    });
    within("the eight lists, counted", t, Duration::from_millis(200));

    for list in chilpa_library::smart_builtins() {
        let (_, t) = how_long(|| lib.smart_tracks(&list).unwrap());
        within(&format!("list \"{}\"", list.name), t, Duration::from_millis(400));
    }

    // And a list with several rules at once.
    let composite = SmartList::new(
        "compound",
        vec![Rule::HiRes, Rule::PlayedAtLeast(2), Rule::LongerThanMs(300_000)],
        SmartOrder::MostPlayed,
        200,
    );
    let (_, t) = how_long(|| lib.smart_tracks(&composite).unwrap());
    within("list with three rules", t, Duration::from_millis(200));
}

/// What a scan pays per track, which nothing measured before.
///
/// Everything that makes reading fast is a write: the rollups and the FTS5
/// index are maintained by triggers, so each of them is work at insert time.
/// It is worth what it costs, but only as long as someone knows what it
/// costs.
///
/// Measured on this machine, inserting into one transaction:
///
/// | | per track |
/// | --- | --- |
/// | the insert on its own | 5.5 us |
/// | with the rollup triggers | 15.6 us |
/// | with FTS5 as well | 55.9 us |
///
/// So the indexes add about fifty microseconds to a track, and a hundred
/// thousand of them add five seconds to a scan that takes minutes, because
/// a scan also opens every file. An incremental rescan pays none of it:
/// what does not change is not written.
///
/// Those are the numbers with nothing else running. Inside the suite this
/// test shares the machine with the other two and reads around 150 us, so
/// the ceiling is 300: it is there for the day someone adds a third trigger
/// without measuring, not to time the machine.
#[test]
fn a_scan_does_not_pay_too_much_for_the_indexes() {
    const N: i64 = 20_000;
    let lib = Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    for a in 1..=(N / 10) {
        lib.connection()
            .execute(
                "INSERT INTO albums (id, title, album_artist, year)
                 VALUES (?1, ?2, 'Someone', 1970)",
                params![a, format!("Album {a}")],
            )
            .unwrap();
    }
    let tx = lib.connection().unchecked_transaction().unwrap();
    let (_, took) = how_long(|| {
        let mut stmt = tx
            .prepare(
                "INSERT INTO tracks (id, source_id, external_id, title, artist, album_id,
                                     duration_ms, added_at)
                 VALUES (?1, ?2, ?3, ?4, 'Someone', ?5, 200000, ?1)",
            )
            .unwrap();
        for i in 1..=N {
            stmt.execute(params![
                i,
                source,
                format!("/music/{i}.flac"),
                format!("Track {i}"),
                (i - 1) / 10 + 1
            ])
            .unwrap();
        }
    });
    drop(tx);
    let each = took / N as u32;
    eprintln!("inserting a track: {each:?} ({took:?} for {N})");
    assert!(
        each <= Duration::from_micros(300),
        "a track costs {each:?} to insert; it was 56 us alone, 150 in the suite, \
         when the indexes went in"
    );
}

#[test]
fn the_features_of_one_page_come_out_in_a_single_query() {
    let lib = large_library();
    let ids: Vec<i64> = (1..=200).collect();
    for id in &ids {
        lib.save_features(*id, "chilpa", 2, Some(120.0), Some(0.8), None, Some(-12.0)).unwrap();
    }
    let (map, t) = how_long(|| lib.features_for(&ids, "chilpa").unwrap());
    assert_eq!(map.len(), 200);
    within("features for two hundred tracks", t, Duration::from_millis(50));
}
