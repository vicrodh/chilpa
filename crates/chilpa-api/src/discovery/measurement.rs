//! Reproducible discovery stages on synthetic data, with no plugins or Qt.
//! The optional report contains timings and counts, never library metadata.
use super::*;
use std::time::Instant;

fn fixture() -> Reader {
    let lib = chilpa_library::Library::in_memory().unwrap();
    lib.connection().execute_batch("BEGIN;
        INSERT INTO sources(id,provider,instance) VALUES (1,'local','default');
        WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<2000)
        INSERT INTO albums(id,title,album_artist,artwork_path)
        SELECT i,printf('Record %04d',i),printf('Artist %04d',i),printf('/cover/%d',i) FROM n;
        WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<20000)
        INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,added_at,bit_depth,sample_rate_hz)
        SELECT i,1,printf('/track/%05d.flac',i),'Song','Player',(i-1)/10+1,1,16,44100 FROM n;
        INSERT INTO genres(id,name) VALUES (1,'Jazz'),(2,'Technical Death Metal');
        INSERT INTO track_genres(track_id,genre_id) SELECT id,(album_id%2)+1 FROM tracks;
        COMMIT; ANALYZE;").unwrap();
    Reader::of(lib)
}

fn timed<T>(samples: &mut Vec<Value>, round: usize, stage: &str, read: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let result = read();
    samples.push(json!({"round":round,"stage":stage,"ms":start.elapsed().as_secs_f64()*1000.0}));
    result
}

#[test]
fn populated_genre_pages_keep_counts_and_release_request_temporaries() {
    let mut samples = Vec::new();
    for round in 0..3 {
        let reader = fixture();
        let revision = reader.lib.read_revision().unwrap();
        let (_, source) = timed(&mut samples, round, "source_index_without_plugins", || reader.source_index());
        assert!(source.items.is_empty());
        let tags: Vec<_> = timed(&mut samples, round, "discovery_names", || reader.discovery_names(false))
            .into_iter().map(|(tag, _)| tag).collect();
        let inventory = timed(&mut samples, round, "inventory", || genres::Inventory::new(&tags));
        let names = inventory.browse(None, "", "all");
        let summaries = timed(&mut samples, round, "summaries_cold_versions", || reader.genre_summaries(&names, &inventory).unwrap());
        assert_eq!(summaries.iter().map(|summary| summary.albums).sum::<usize>(), 2000);
        assert!(summaries.iter().all(|summary| summary.loose_tracks == 0 && summary.images.is_empty()));
        let warm = timed(&mut samples, round, "page_warm_versions", || reader.discovery_page_options("genres", "", "", "{}", 0));
        let again = timed(&mut samples, round, "page_repeated", || reader.discovery_page_options("genres", "", "", "{}", 0));
        assert_eq!(warm, again);
        assert_eq!(reader.lib.read_revision().unwrap(), revision);
        let cold = fixture();
        let first = timed(&mut samples, round, "page_cold_versions", || cold.discovery_page_options("genres", "", "", "{}", 0));
        assert_eq!(first, warm);
        // An empty search page must not scan releases or initialize versions.
        let absent = timed(&mut samples, round, "empty_search_page", || cold.discovery_page_options("genres", "", "no-match-for-this-search", "{}", 0));
        assert_eq!(absent["total"], 0);
        assert_eq!(absent["items"], json!([]));
    }
    if let Some(directory) = std::env::var_os("CHILPA_DISCOVERY_MEASUREMENT_DIR") {
        std::fs::write(std::path::Path::new(&directory).join("api.json"),
            serde_json::to_vec_pretty(&json!({"tracks":20000,"albums":2000,
                "mode":"test-profile, in-memory SQLite; cold means a new Reader/version catalog; taxonomy may be initialized by other tests",
                "samples":samples})).unwrap()).unwrap();
    }
}
