use super::*;
use chilpa_library::{store::query::parse, Filter, TrackOrder};

#[test]
#[ignore = "I measure query fields on the scale fixture without concurrent tests"]
fn query_field_costs() {
    let lib = large_library();
    lib.set_group_album_versions(false).unwrap();
    lib.connection().execute_batch("BEGIN;
        INSERT INTO genres(id, name) VALUES (1, 'Jazz'), (2, 'Rock');
        INSERT INTO track_genres(track_id, genre_id) SELECT id, 1 + id % 2 FROM tracks;
        INSERT INTO composers(id, name) VALUES (1, 'Writer');
        INSERT INTO track_composers(track_id, composer_id) SELECT id, 1 FROM tracks WHERE id % 2 = 0;
        UPDATE tracks SET label = CASE WHEN id % 2 = 0 THEN 'House' ELSE 'Other' END;
        UPDATE albums SET title = title || CASE id % 6 WHEN 0 THEN ' [EP]' WHEN 1 THEN ' - Single'
            WHEN 2 THEN ' (Live at Home)' WHEN 3 THEN ' (Original Soundtrack)' ELSE '' END;
        UPDATE albums SET album_artist = 'Various Artists' WHERE id % 6 = 4;
        UPDATE tracks SET external_id = external_id || '.mp3' WHERE album_id % 4 = 0;
        UPDATE tracks SET external_id = external_id || '.dsf' WHERE album_id % 4 = 1;
        UPDATE tracks SET bit_depth = 16, sample_rate_hz = 48000 WHERE album_id % 4 = 2;
        COMMIT;").unwrap();
    lib.update_statistics().unwrap();
    let queries = ["genre:jazz", "label:House", "composer:Writer", "artist:\"Artist 007\"", "album:Disco", "title:Track",
        "year:1960..2019", "rating:0", "added:1700000000..", "Track", "plays:0", "plays:1..", "plays:1,3",
        "plays:1..2", "plays:..2", "-plays:0", "type:album", "type:single,ep", "-type:compilation",
        "type:live", "type:soundtrack", "quality:lossy", "quality:cd", "quality:hires", "quality:dsd", "quality:cd,hires", "-quality:lossy"];
    const EXISTING: usize = 10;
    let filters: Vec<Filter> = queries.iter().map(|text| Filter { query: Some(parse(text).unwrap()), ..Default::default() }).collect();
    let mut counts = Vec::new();
    for filter in &filters { counts.push(lib.browse_ids(filter, TrackOrder::Title).unwrap().ids.len()); }
    let mut samples = vec![Vec::new(); queries.len()];
    for run in 0..7 {
        for step in 0..queries.len() {
            let at = (step + run * 5) % queries.len();
            let (snapshot, took) = how_long(|| lib.browse_ids(&filters[at], TrackOrder::Title).unwrap());
            assert_eq!(snapshot.ids.len(), counts[at]);
            samples[at].push(took);
        }
    }
    for times in &mut samples { times.sort_unstable(); }
    let slowest = samples[..EXISTING].iter().map(|s| s[3]).max().unwrap();
    eprintln!("query benchmark: {TRACKS} tracks, {ALBUMS} albums, debug={}, seven warm runs, complete title-sorted snapshots", cfg!(debug_assertions));
    for (at, text) in queries.iter().enumerate() {
        let s = &samples[at];
        eprintln!("{text}: rows={} median={:.3} ms min={:.3} ms max={:.3} ms ratio={:.3}", counts[at],
            s[3].as_secs_f64() * 1000.0, s[0].as_secs_f64() * 1000.0, s[6].as_secs_f64() * 1000.0, s[3].as_secs_f64() / slowest.as_secs_f64());
    }
    for (at, times) in samples.iter().enumerate().skip(EXISTING) {
        assert!(times[3] <= slowest * 2, "{}: {:?} exceeds twice the slowest existing field {:?}", queries[at], times[3], slowest);
    }
}
