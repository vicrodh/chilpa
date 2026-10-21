use chilpa_library::Library;
use rusqlite::params;

pub const ALBUMS: i64 = 5_000;
pub const PER_ALBUM: i64 = 10;
pub const TRACKS: i64 = ALBUMS * PER_ALBUM;

/// Builds a library with fifty thousand tracks and a play log.
pub fn large_library() -> Library {
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
                .execute(params![a, format!("Record {a}"), artist, 1960 + (a % 60) as i32])
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
    // A real library gets this from its scan; this one was filled by hand.
    // Without it the planner works from the statistics of an empty database,
    // which is what made the smart lists slow before anyone noticed.
    lib.update_statistics().unwrap();
    lib
}
