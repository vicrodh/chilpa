//! I exercise the scan/assembly boundary only against an explicitly supplied copy.

#[test]
#[ignore = "reads real media and migrates the explicitly supplied disposable copy"]
fn rescan_release_parts_on_a_disposable_copy() {
    let path = std::env::var("CHILPA_TEST_LIBRARY_COPY").expect("disposable database path");
    let lib = chilpa_library::Library::open(std::path::Path::new(&path)).unwrap();
    let paths = {
        let mut stmt = lib.connection().prepare("SELECT DISTINCT t.release_part FROM tracks t JOIN albums a ON a.id = t.album_id
            WHERE t.present = 1 AND (a.title LIKE '%Piper%40th Anniversary Edition%' OR a.title = 'Original Album Classics'
            OR a.title = 'Stone Deaf Forever' OR a.title LIKE '%Rheingold%' OR a.title LIKE '%Fidelio%'
            OR a.title LIKE '%Songs From Tsongas%')").unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.collect::<rusqlite::Result<Vec<_>>>().unwrap()
    };
    for path in paths {
        let part = std::path::Path::new(&path);
        let root = if chilpa_core::sacd_tracks::is_image(part) { part.parent().unwrap() } else { part };
        let report = chilpa_library::scan::scan_root_with(&lib, root, None, &|_| {}, &chilpa_core::sacd_tracks::DiscImages).unwrap();
        assert!(!report.unreachable, "{}", root.display());
        eprintln!("{}: {} read, {} unchanged", root.display(), report.read, report.unchanged);
        lib.assemble_local_releases().unwrap();
    }
    for (title, count) in [("The Piper At The Gates Of Dawn (40th Anniversary Edition)", 31),
        ("Stone Deaf Forever", 99), ("Wagner: Das Rheingold", 44), ("BEETHOVEN Fidelio", 32),
        ("Songs From Tsongas: Yes 35th Anniversary Concert", 23)] {
        let counts: (i64, i64) = lib.connection().query_row("SELECT count(DISTINCT t.album_id), count(*) FROM tracks t
            JOIN albums a ON a.id = t.album_id WHERE t.present = 1 AND a.title = ?1", [title], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(counts, (1, count), "{title}");
    }
    assert!(lib.check().unwrap().is_empty());
}
