use super::*;
use crate::{Reader, TrackOrder};
use rusqlite::params;
use std::rc::Rc;

/// Two records of the library and, from a source, a saved record whose
/// tracks were read, a saved record whose tracks were not, and a saved
/// track of a record that is not saved.
fn fixture() -> (Reader, SourceIndex) {
    let lib = chilpa_library::Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    lib.set_group_album_versions(false).unwrap();
    for (id, title, artist, year) in [(1, "Blue Train", "Coltrane", 1958), (2, "Moanin'", "Blakey", 1959)] {
        lib.connection().execute("INSERT INTO albums(id, title, album_artist, year) VALUES (?1, ?2, ?3, ?4)", params![id, title, artist, year]).unwrap();
    }
    for (id, album, number, title, genre, label) in [(10, 1, 1, "Blue Train", "Jazz", "Blue Note"), (11, 1, 2, "Moment's Notice", "Jazz", "Blue Note"), (20, 2, 1, "Moanin'", "Hard Bop", "Blue Note")] {
        lib.connection().execute("INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, track_number, label, duration_ms, added_at, bit_depth, sample_rate_hz)
            VALUES (?1, ?2, ?3, ?4, 'x', ?5, ?6, ?7, 1000, 1, 16, 44100)", params![id, source, format!("/{id}.flac"), title, album, number, label]).unwrap();
        lib.set_track_genres(id, &[genre.to_string()]).unwrap();
    }
    let reader = Reader::of(lib);
    let item = |track: i64, album: i64, title: &str, artist: &str, record: &str, year: i32, genre: &str| SourceItem {
        track, album, plugin: "guest".into(), identity: format!("ref:{track}:{album}"), album_identity: format!("album:{album}"), album_artist: artist.into(), album_title: record.into(),
        title: title.into(), artists: vec![artist.into()], year: Some(year), genres: vec![genre.into()], label: "Impulse!".into(),
        composers: vec!["John Coltrane".into()], tier: "hires", format: "flac", tracks: 1, playable: true, number: Some(1),
        duration_ms: 2000, ..SourceItem::default()
    };
    let index = SourceIndex::new(vec![
            SourceItem { saved_album: true, ..item(-1, -10, "Acknowledgement", "Coltrane", "A Love Supreme", 1965, "jazz") },
            SourceItem { saved_album: true, track: 0, tracks: 6, ..item(0, -11, "Ascension", "Coltrane", "Ascension", 1966, "Free Jazz") },
            SourceItem { saved_track: true, ..item(-2, -12, "So What", "Davis", "Kind of Blue", 1959, "Modal") },
        ],
        vec![FacetEntry { key: "plugin:guest".into(), label: "Guest".into(), albums: 8, year: None, icon: String::new() }]);
    (reader, index)
}

fn labels(entries: &[FacetEntry]) -> Vec<(&str, usize)> {
    entries.iter().map(|e| (e.label.as_str(), e.albums)).collect()
}

fn refresh(reader: &Reader, index: SourceIndex, filter: &Filter, order: TrackOrder) -> (crate::Facets, crate::Snapshot) {
    reader.refresh_using(filter, order, || true, ((1, vec![("guest".into(), true, true)]), Rc::new(index))).unwrap()
}

#[test]
fn the_columns_count_the_sources_with_the_library_and_names_meet_whatever_their_case() {
    let (reader, index) = fixture();
    let (facets, _) = refresh(&reader, index, &Filter::default(), TrackOrder::Artist);
    assert_eq!(labels(&facets.genres), [("Free Jazz", 1), ("Hard Bop", 1), ("Jazz", 2), ("Modal", 1)], "jazz joins Jazz");
    assert_eq!(labels(&facets.labels), [("Blue Note", 2), ("Impulse!", 3)]);
    assert_eq!(facets.years.iter().map(|e| e.year.unwrap()).collect::<Vec<_>>(), [1966, 1965, 1959, 1958], "newest first");
    assert_eq!(facets.years.iter().find(|e| e.year == Some(1959)).unwrap().albums, 2);
    assert!(facets.sources.iter().any(|s| s.key == "plugin:guest"));
    assert_eq!(facets.albums.len(), 5);
}

#[test]
fn a_choice_in_one_column_narrows_the_ones_to_its_right_and_the_result() {
    let (reader, index) = fixture();
    let mut filter = Filter::default();
    filter.choose(1, &["Jazz".into()]);
    let (facets, snapshot) = refresh(&reader, index, &filter, TrackOrder::Artist);
    assert_eq!(labels(&facets.labels), [("Blue Note", 1), ("Impulse!", 1)], "only the Jazz records' labels");
    assert_eq!(labels(&facets.genres).len(), 4, "the column chosen in still offers every genre");
    assert_eq!(snapshot.album_ids, [1, -10], "Coltrane's two, by artist");
    assert_eq!(snapshot.ids, [10, 11, -1]);
    assert_eq!(snapshot.totals.albums, 2);
}

#[test]
fn a_record_whose_tracks_were_not_read_still_counts_and_shows() {
    let (reader, index) = fixture();
    let mut filter = Filter::default();
    filter.choose(0, &["1966".into()]);
    let (_, snapshot) = refresh(&reader, index, &filter, TrackOrder::Artist);
    assert_eq!(snapshot.album_ids, [-11]);
    assert!(snapshot.ids.is_empty(), "no track of it is known to play");
    assert_eq!(snapshot.totals.tracks, 6, "the summary says what the source said it holds");
}

#[test]
fn sources_left_out_quality_and_composer_filter_the_sources_as_they_filter_files() {
    let (reader, index) = fixture();
    let lossless = Filter { qualities: vec!["lossless".into()], ..Filter::default() };
    let (_, snapshot) = refresh(&reader, fixture().1, &lossless, TrackOrder::Album);
    assert!(snapshot.album_ids.iter().all(|id| *id > 0), "the source's records are hi-res");
    let away = Filter { excluded_sources: vec!["plugin:guest".into()], ..Filter::default() };
    assert!(refresh(&reader, fixture().1, &away, TrackOrder::Album).1.album_ids.iter().all(|id| *id > 0));
    let mut written = Filter::default();
    written.choose(3, &["john coltrane".into()]);
    assert_eq!(refresh(&reader, index, &written, TrackOrder::Album).1.album_ids.len(), 3, "a composer is the same whatever its case");
}

#[test]
fn the_sources_take_their_place_in_every_order() {
    let (reader, index) = fixture();
    let (_, by_title) = refresh(&reader, index, &Filter::default(), TrackOrder::Title);
    assert_eq!(by_title.ids, [-1, 10, 20, 11, -2], "Acknowledgement, Blue Train, Moanin', Moment's Notice, So What");
    let (_, by_year) = refresh(&reader, fixture().1, &Filter::default(), TrackOrder::YearDescending);
    assert_eq!(by_year.album_ids, [-11, -10, -12, 2, 1], "Kind of Blue before Moanin', both of 1959");
}

#[test]
fn tracks_takes_the_saved_tracks_only_and_says_where_each_letter_starts() {
    let (reader, index) = fixture();
    let found = reader.tracks_using("title\u{1f}name\u{1f}", &Filter::default(), &index);
    assert_eq!(found.snapshot.ids, [10, 20, 11, -2], "a track of a saved record is not a saved track");
    assert_eq!(found.letters, r#"[["B",0],["M",1],["S",3]]"#);
    let rated = reader.tracks_using("title\u{1f}off\u{1f}\u{1f}5", &Filter::default(), &index);
    assert!(rated.snapshot.ids.is_empty(), "nothing has five stars");
}

#[test]
fn source_track_metadata_names_every_new_group() {
    let item = SourceItem { year: Some(1999), genres: vec!["Zulu".into(), "alpha".into()],
        label: " House ".into(), composers: vec!["Writer, Z".into(), "author".into()], ..Default::default() };
    let key = merge::key_of(&item);
    for (by, expected) in [("year", "1999"), ("decade", "1990s"), ("genre", "alpha"), ("label", "House"), ("composer", "author")] {
        assert_eq!(merge::group_label(&key, by), expected, "{by}");
    }
    for year in [None, Some(0), Some(-1)] {
        let key = merge::key_of(&SourceItem { year, ..Default::default() });
        for (by, missing) in [("year", "Unknown year"), ("decade", "Unknown year"), ("genre", "No genre"), ("label", "No label"), ("composer", "No composer")] {
            assert_eq!(merge::group_label(&key, by), chilpa_i18n::t(missing), "{by}");
        }
    }
}

#[test]
fn saved_tracks_join_their_genre_and_the_missing_genre_group() {
    let (reader, mut index) = fixture();
    index.items[2].genres = vec!["Zulu".into(), "jazz".into()];
    index.items.push(SourceItem { track: -3, title: "A missing genre".into(), saved_track: true, ..Default::default() });
    reader.lib.set_track_genres(20, &[]).unwrap();
    let index = SourceIndex::new(index.items, index.sources);
    let found = reader.tracks_using("title\u{1f}genre\u{1f}", &Filter::default(), &index);
    assert_eq!(found.snapshot.ids, [10, 11, -2, -3, 20]);
    assert_eq!(found.sections, [(0, "Jazz".into()), (3, chilpa_i18n::t("No genre"))]);
    assert_eq!(crate::section_at(&found.sections, 2), Some("Jazz"), "case variants share one section");
    let descending = reader.tracks_using("title-desc\u{1f}genre\u{1f}", &Filter::default(), &index);
    assert_eq!(descending.snapshot.ids, [-2, 11, 10, 20, -3], "only the tracks inside each group reverse");
    assert_eq!(descending.sections.len(), 2);
}

#[test]
fn saved_tracks_merge_into_dates_labels_and_composers() {
    let (reader, index) = fixture();
    for (by, expected, sections) in [
        ("year", vec![20, -2, 10, 11], vec![(0, "1959".to_string()), (2, "1958".into())]),
        ("decade", vec![10, 20, 11, -2], vec![(0, "1950s".into())]),
        ("label", vec![10, 20, 11, -2], vec![(0, "Blue Note".into()), (3, "Impulse!".into())]),
        ("composer", vec![-2, 10, 20, 11], vec![(0, "John Coltrane".into()), (1, chilpa_i18n::t("No composer"))]),
    ] {
        let found = reader.tracks_using(&format!("title\u{1f}{by}\u{1f}"), &Filter::default(), &index);
        assert_eq!(found.snapshot.ids, expected, "{by}");
        assert_eq!(found.sections, sections, "{by}");
    }
}

/// I measure the complete Tracks answer on a read-only library copy.
#[test]
#[ignore]
fn measure_tracks_groups_on_a_library_copy() {
    let path = std::env::var("CHILPA_LIBRARY_COPY").expect("set CHILPA_LIBRARY_COPY to a library copy");
    let reader = Reader::of(chilpa_library::Library::open_read_only(std::path::Path::new(&path)).unwrap());
    let index = SourceIndex::default();
    let groups = ["off", "year", "decade", "genre", "label", "composer"];
    let mut samples = vec![Vec::new(); groups.len()];
    let mut counts = vec![0; groups.len()];
    for round in 0..4 {
        for offset in 0..groups.len() {
            let at = if round % 2 == 0 { offset } else { groups.len() - 1 - offset };
            let query = format!("album\u{1f}{}\u{1f}", groups[at]);
            let start = std::time::Instant::now();
            let found = reader.tracks_using(&query, &Filter::default(), &index);
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            assert!(!found.snapshot.ids.is_empty(), "the query must return tracks");
            counts[at] = found.snapshot.ids.len();
            if round > 0 { samples[at].push(ms); }
        }
    }
    assert!(counts.iter().all(|count| *count == counts[0]));
    for (at, group) in groups.iter().enumerate() {
        let mut sorted = samples[at].clone();
        sorted.sort_by(f64::total_cmp);
        println!("Tracks {group:8}: {} rows, best {:.2} ms, median {:.2} ms, samples {:?} ms", counts[at], sorted[0], sorted[1], samples[at]);
    }
}

#[test]
fn albums_merges_the_saved_records_in_its_order_and_groups_them_with_the_library() {
    let (reader, index) = fixture();
    let found = reader.albums_view_using("year", "", "", &Filter::default(), &index);
    assert_eq!(found.ids, [-11, -10, 2, 1], "a saved track's record is not a saved record");
    let by_genre = reader.albums_view_using("artist", "genre", "", &Filter::default(), &index);
    assert_eq!(by_genre.groups.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), ["Free Jazz", "Hard Bop", "Jazz"]);
    let typed = reader.albums_view_using("artist", "", "supreme", &Filter::default(), &index);
    assert_eq!(typed.ids, [-10]);
}

#[test]
fn a_record_keeps_its_stars_whether_or_not_its_tracks_were_read() {
    let (reader, index) = fixture();
    reader.lib.rate_external("guest", "album", "album:-10", 4).unwrap();
    reader.lib.rate_external("guest", "album", "album:-11", 2).unwrap();
    reader.lib.rate_external("guest", "track", "ref:-2:-12", 5).unwrap();
    let stars = Stars::read(&reader.lib, &index);
    assert_eq!((stars.albums.get(&-10), stars.albums.get(&-11), stars.tracks.get(&-2)), (Some(&4), Some(&2), Some(&5)));
    let mut four = Filter::default();
    four.choose(4, &["4".into()]);
    assert_eq!(refresh(&reader, index, &four, TrackOrder::Artist).1.album_ids, [-10], "a record read track by track keeps its own stars");
}

#[test]
fn a_filter_reads_an_item_as_it_reads_a_track() {
    let (_, index) = fixture();
    let stars = Stars { albums: [(-10, 4)].into_iter().collect(), tracks: Default::default() };
    let item = &index.items[0];
    let mut f = Filter::default();
    assert!(passes(item, &f, &stars));
    f.queries[6] = "love".into();
    assert!(passes(item, &f, &stars), "a header search");
    f.queries[0] = "1965".into();
    assert!(passes(item, &f, &stars));
    f.ratings = vec![4];
    assert!(passes(item, &f, &stars));
    f.ratings = vec![5];
    assert!(!passes(item, &f, &stars));
    let performer = Filter { performer: Some("Coltrane".into()), ..Filter::default() };
    assert!(!passes(item, &performer, &stars), "an artist page reads the library only");
    let words = Filter { text: "acknow coltrane".into(), ..Filter::default() };
    assert!(passes(item, &words, &stars));
    let format = Filter { formats: vec!["FLAC".into()], ..Filter::default() };
    assert!(passes(item, &format, &stars));
}

/// Times a Library refresh on a copy of a real library, with no source and
/// with 6,000 integrated items: `CHILPA_LIBRARY_COPY=<file> cargo test -p
/// chilpa-api --lib measured_on_a_real_library -- --ignored --nocapture`.
#[test]
#[ignore]
fn measured_on_a_real_library() {
    let Ok(path) = std::env::var("CHILPA_LIBRARY_COPY") else { return };
    let reader = Reader::of(chilpa_library::Library::open_read_only(std::path::Path::new(&path)).unwrap());
    let genres: Vec<String> = reader.lib.facet_genres(&Filter::default()).unwrap().into_iter().take(40).map(|g| g.label).collect();
    let items: Vec<SourceItem> = (1..=6000).map(|n| SourceItem {
        track: if n % 3 == 0 { 0 } else { -n }, album: -(n / 12 + 1), plugin: "guest".into(), identity: n.to_string(),
        album_artist: format!("Artist {}", n % 400), album_title: format!("Record {}", n / 12), title: format!("Song {n}"),
        artists: vec![format!("Artist {}", n % 400)], year: Some(1960 + (n % 60) as i32), genres: vec![genres[n as usize % genres.len()].clone()],
        label: format!("Label {}", n % 90), composers: vec![format!("Writer {}", n % 300)], tier: "lossless", format: "flac",
        tracks: if n % 3 == 0 { 10 } else { 1 }, saved_track: n % 2 == 0, saved_album: n % 2 == 1, playable: true, ..SourceItem::default()
    }).collect();
    let index = Rc::new(SourceIndex::new(items, Vec::new()));
    let median = |run: &dyn Fn() -> ()| {
        let mut times: Vec<u128> = (0..5).map(|_| { let t = std::time::Instant::now(); run(); t.elapsed().as_micros() }).collect();
        times.sort();
        times[2] as f64 / 1000.0
    };
    let mut cases = vec![("all", Filter::default())];
    let mut one = Filter::default(); one.choose(1, &[genres[0].clone()]); cases.push(("one genre", one.clone()));
    let mut two = one.clone(); two.choose(1, &[genres[0].clone(), genres[1].clone()]); cases.push(("two genres", two));
    let mut decade = Filter::default(); decade.choose(0, &(1990..2000).map(|y| y.to_string()).collect::<Vec<_>>()); cases.push(("a decade", decade));
    for (name, filter) in &cases {
        // A fresh reader per run: the refresh cache would answer the repeats.
        let local = median(&|| { reader.forget_refreshes(); reader.refresh_using(filter, TrackOrder::Artist, || true, Default::default()).unwrap(); });
        let mixed = median(&|| { reader.forget_refreshes(); reader.refresh_using(filter, TrackOrder::Artist, || true, ((1, vec![("guest".into(), true, true)]), index.clone())).unwrap(); });
        println!("refresh {name:12} library only {local:8.1} ms   with 6,000 source items {mixed:8.1} ms");
    }
    let all = reader.lib.browse_ids(&Filter::default(), TrackOrder::Artist).unwrap();
    let rows = median(&|| { let _ = all.ids.chunks(500).flat_map(|ids| reader.lib.tracks_by_ids(ids).unwrap()).count(); });
    let plain = median(&|| { let _ = reader.lib.grouped_snapshot(&Filter::default(), TrackOrder::Artist).unwrap(); });
    let keyed = median(&|| { let _ = reader.lib.keyed_snapshot(&Filter::default(), TrackOrder::Artist).unwrap(); });
    println!("snapshot of the whole library: plain {plain:.1} ms, with keys {keyed:.1} ms");
    let tracks = median(&|| { let _ = reader.tracks_using("artist\u{1f}off\u{1f}", &Filter::default(), &index); });
    println!("tracks merge: reading every row {rows:.1} ms (before); keyed snapshot {keyed:.1} ms; whole Tracks answer {tracks:.1} ms ({} tracks)", all.ids.len());
    let albums = median(&|| { let _ = reader.albums_view_using("artist", "genre", "", &Filter::default(), &index); });
    println!("albums view grouped by genre with sources {albums:.1} ms");
}
