//! Keeping a large library fast.
//!
//! The QBZ audit's high finding was the database: every page of the grid
//! aggregated the whole library again, and a query per row did the rest.
//! This test builds a genuinely large library and puts a time ceiling on
//! what the interface queries.
//!
//! The ceilings are generous on purpose. They are not there to measure the
//! machine: they are there so a query that turns quadratic shows up the day
//! someone writes it, not the day a user opens their library.

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
            let artist = format!("Artista {:03}", a % 200);
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
                        format!("/musica/{a}/{p:02}.flac"),
                        format!("Pista {p} del {a}"),
                        format!("Artista {:03}", a % 200),
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
        "{name} tardó {took:?}, el techo es {ceiling:?}: algo se volvió cuadrático"
    );
}

#[test]
fn a_fifty_thousand_track_library_does_not_crawl() {
    let (lib, built) = how_long(large_library);
    eprintln!("armar {TRACKS} pistas: {built:?}");
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
        within(&format!("reja por {}", order.name()), t, Duration::from_millis(1500));
    }

    // A page from deep in: it must not cost more than the first one just
    // for being far away.
    let (_, t) = how_long(|| lib.albums_sorted(AlbumSort::Artist, 100, 4_000).unwrap());
    within("reja en la página 40", t, Duration::from_millis(1500));

    // The artists.
    let (artists, t) = how_long(|| lib.artists(500, 0).unwrap());
    assert_eq!(artists.len(), 200, "doscientos artistas");
    within("artistas", t, Duration::from_millis(1500));

    // The search, which walks titles, artists, albums and genres.
    let (found, t) = how_long(|| lib.search("Pista 7", 200).unwrap());
    assert!(!found.is_empty());
    within("búsqueda", t, Duration::from_millis(1500));

    // The stats, which sum the whole log.
    let (stats, t) = how_long(|| lib.stats().unwrap());
    assert!(stats.plays > 0);
    within("estadísticas", t, Duration::from_millis(1500));

    let (_, t) = how_long(|| lib.top_tracks(20).unwrap());
    within("más escuchadas", t, Duration::from_millis(1500));
    let (_, t) = how_long(|| lib.recently_played(20).unwrap());
    within("lo último que sonó", t, Duration::from_millis(1500));

    // The smart lists, which is what gets resolved every time they are
    // looked at.
    for list in chilpa_library::smart_builtins() {
        let (_, t) = how_long(|| lib.smart_tracks(&list).unwrap());
        within(&format!("lista \"{}\"", list.name), t, Duration::from_millis(1500));
    }

    // And a list with several rules at once.
    let composite = SmartList::new(
        "compuesta",
        vec![Rule::HiRes, Rule::PlayedAtLeast(2), Rule::LongerThanMs(300_000)],
        SmartOrder::MostPlayed,
        200,
    );
    let (_, t) = how_long(|| lib.smart_tracks(&composite).unwrap());
    within("lista con tres reglas", t, Duration::from_millis(1500));
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
    within("features de doscientas pistas", t, Duration::from_millis(500));
}
