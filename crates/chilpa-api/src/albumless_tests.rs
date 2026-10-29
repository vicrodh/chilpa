//! The Library snapshot, its rows and playback share loose track identities.
use crate::{Chilpa, Filter, Reader, TrackOrder};
use std::{path::Path, time::{Duration, Instant}};

fn tagged_wave(path: &Path, album: Option<&str>, title: &str) {
    let mut chunks = b"fmt ".to_vec();
    chunks.extend(16u32.to_le_bytes()); chunks.extend(1u16.to_le_bytes()); chunks.extend(1u16.to_le_bytes());
    chunks.extend(8_000u32.to_le_bytes()); chunks.extend(16_000u32.to_le_bytes());
    chunks.extend(2u16.to_le_bytes()); chunks.extend(16u16.to_le_bytes());
    let mut info = b"INFO".to_vec();
    for (key, value) in [(b"INAM", Some(title)), (b"IART", Some("Solo")), (b"ICRD", Some("1997")), (b"IPRD", album)] {
        if let Some(value) = value {
            let size = value.len() + 1;
            info.extend(key); info.extend((size as u32).to_le_bytes()); info.extend(value.as_bytes()); info.push(0);
            if size % 2 != 0 { info.push(0); }
        }
    }
    chunks.extend(b"LIST"); chunks.extend((info.len() as u32).to_le_bytes()); chunks.extend(info);
    chunks.extend(b"data"); chunks.extend(16_000u32.to_le_bytes());
    for _ in 0..8_000 { chunks.extend(123i16.to_le_bytes()); }
    let mut file = b"RIFF".to_vec(); file.extend((4 + chunks.len() as u32).to_le_bytes()); file.extend(b"WAVE"); file.extend(chunks);
    std::fs::write(path, file).unwrap();
}

fn scan(api: &mut Chilpa, root: &Path) {
    assert!(api.start_scan(root));
    let deadline = Instant::now() + Duration::from_secs(20);
    while api.scan_status().running {
        assert!(Instant::now() < deadline, "scan did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn scanned_loose_tracks_refresh_into_and_out_of_records_and_play_from_the_snapshot() {
    const NAME: &str = "albumless_tests::scanned_loose_tracks_refresh_into_and_out_of_records_and_play_from_the_snapshot";
    const CHILD: &str = "CHILPA_ALBUMLESS_TEST";
    if std::env::var(CHILD).as_deref() != Ok(NAME) {
        let profile = tempfile::tempdir().unwrap();
        let temporary = profile.path().join("tmp");
        std::fs::create_dir(&temporary).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"]).env(CHILD, NAME).env("TMPDIR", temporary)
            .envs(chilpa_dirs::profile_environment(profile.path())).output().unwrap();
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
        return;
    }
    let music = tempfile::tempdir().unwrap();
    let path = music.path().join("loose.wav");
    tagged_wave(&path, None, "First title");
    let mut api = Chilpa::new();
    scan(&mut api, music.path());
    let reader = api.reader().unwrap();
    let loose = Filter { albumless: true, ..Default::default() };
    let (facets, before) = reader.refresh(&loose, TrackOrder::Title, || true).unwrap();
    assert_eq!((before.totals.tracks, before.totals.albums), (1, 0));
    assert!(facets.albums.is_empty());
    assert!(facets.artists.iter().any(|entry| entry.key == "Solo"));
    assert!(facets.years.iter().any(|entry| entry.key == "1997"));
    let id = before.ids[0];
    let rows = api.track_rows(&before.ids);
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].id, rows[0].album_id, rows[0].title.as_str()), (id, 0, "First title"));
    assert!(!crate::track_can_go(&rows[0], "library", "", "album"));
    assert!(!crate::track_can_go(&rows[0], "library", "", "metadata"));
    assert!(crate::track_can_go(&rows[0], "library", "", "folder"));
    let paths = reader.paths(&before.ids);
    assert_eq!(paths, [path.to_string_lossy().into_owned()]);
    assert_eq!(api.play_tracks(&paths, "file:capture", false).len(), 1);
    assert_eq!(api.queue_paths(), paths);
    let queued = api.queue_rows();
    assert_eq!((queued[0].id, queued[0].album_id, queued[0].artist.as_str()), (id, 0, "Solo"));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        api.poll();
        if std::fs::metadata(chilpa_core::capture_path()).is_ok_and(|file| file.len() > 0) { break; }
        assert!(Instant::now() < deadline, "no captured audio: {}", api.now_playing().error);
        std::thread::sleep(Duration::from_millis(10));
    }
    api.stop();
    tagged_wave(&path, Some("A real record"), "Second, longer title");
    scan(&mut api, music.path());
    assert!(reader.refresh(&loose, TrackOrder::Title, || true).unwrap().1.ids.is_empty());
    let all = reader.refresh(&Filter::default(), TrackOrder::Title, || true).unwrap().1;
    assert_eq!(all.ids, [id]);
    assert_eq!(all.totals.albums, 1);
    assert!(api.track_rows(&[id])[0].album_id > 0);
    tagged_wave(&path, None, "Third title without a record");
    scan(&mut api, music.path());
    let after = reader.refresh(&loose, TrackOrder::Title, || true).unwrap().1;
    assert_eq!(after.ids, [id]);
    let row = &api.track_rows(&[id])[0];
    assert_eq!((row.album_id, row.title.as_str()), (0, "Third title without a record"));
    let unchanged = reader.lib.read_revision().unwrap();
    assert_eq!(reader.refresh(&loose, TrackOrder::Title, || true).unwrap().1, after);
    assert_eq!(reader.lib.read_revision().unwrap(), unchanged, "repeated read-only refresh does not maintain the database");
    let lib = api.library().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    lib.set_root_reachable(lib.root_id(source, music.path()).unwrap(), false).unwrap();
    assert_eq!(reader.refresh(&loose, TrackOrder::Title, || true).unwrap().1.ids, [id]);
    assert!(!api.track_rows(&[id])[0].available);
    assert!(api.playable(&paths).is_empty());
}

#[test]
fn source_tracks_without_records_share_the_filter_and_do_not_create_album_cards() {
    use crate::library_sources::{SourceIndex, SourceItem};
    let lib = chilpa_library::Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    lib.connection().execute("INSERT INTO tracks(id,source_id,external_id,title,artist,release_year,added_at)
        VALUES(1,?1,'/loose.flac','Local','Solo',1997,1)", [source]).unwrap();
    let reader = Reader::of(lib);
    let index = std::rc::Rc::new(SourceIndex::new(vec![
        SourceItem { track: -2, album: 0, title: "Remote".into(), artists: vec!["Solo".into()], album_artist: "Solo".into(),
            year: Some(1997), plugin: "guest".into(), saved_track: true, tracks: 1, playable: true, ..Default::default() },
        SourceItem { track: -3, album: -4, title: "On record".into(), album_title: "Record".into(), plugin: "guest".into(),
            saved_album: true, tracks: 1, playable: true, ..Default::default() },
    ], Vec::new()));
    let filter = Filter { albumless: true, artist: Some("Solo".into()), year: Some(1997), ..Default::default() };
    let read = |filter: &Filter| reader.refresh_using(filter, TrackOrder::Title, || true,
        ((1, vec![("guest".into(), true, true)]), index.clone())).unwrap();
    let (facets, snapshot) = read(&filter);
    assert_eq!(snapshot.ids, [1, -2]);
    assert_eq!(snapshot.totals.albums, 0);
    assert!(snapshot.album_ids.is_empty() && facets.albums.is_empty());
    let (_, excluded) = read(&Filter { excluded_sources: vec!["plugin:guest".into()], ..filter });
    assert_eq!(excluded.ids, [1]);
}
