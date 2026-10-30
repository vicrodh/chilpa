//! A record on a server of mine in the metadata editor: edited through the
//! same calls the window makes, against a mirror a sync left in the
//! library, with no server and no network.
//!
//! One test on purpose: it points the library, the settings and the cache
//! at a temporary directory, and those belong to the whole process.

use chilpa_api::Chilpa;
use chilpa_library::{Library, NewServer, ServerTrack};
use chilpa_meta::{Http, Reply};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A network nothing can go out through.
struct Nowhere;

impl Http for Nowhere {
    fn get(&self, url: &str, _: &str, _: Duration, _: usize) -> Result<Reply, String> {
        panic!("the editor went out to the network: {url}");
    }
}

fn track(id: &str, album: &str) -> ServerTrack {
    ServerTrack {
        external_id: format!("remote:example-server/aaaa/{id}"),
        title: format!("Song {id}"),
        artist: "Band".into(),
        album: album.into(),
        album_artist: "Band".into(),
        album_key: format!("al-{album}"),
        track_number: Some(id.parse().unwrap_or(1)),
        disc_number: Some(1),
        year: Some(2001),
        genres: vec!["Rock".into()],
        duration_ms: 180_000,
        codec: Some("flac".into()),
        bit_depth: Some(16),
        sample_rate_hz: Some(44_100),
        channels: Some(2),
        revision: Some(format!("r1-{id}")),
        ..ServerTrack::default()
    }
}

/// What a sync leaves: the server's tracks as it lists them now.
fn synced(lib: &Library, source: i64, tracks: &[ServerTrack]) {
    let generation = lib.begin_server_sync(source).unwrap();
    lib.save_server_tracks(source, generation, tracks).unwrap();
    lib.finish_server_sync(source, generation).unwrap();
}

/// Polls until the editor is in that state.
fn until(chilpa: &mut Chilpa, state: &str) {
    let limit = Instant::now() + Duration::from_secs(20);
    while Instant::now() < limit {
        if chilpa.metadata_poll().state == state {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the editor never got to \"{state}\": {:?}", chilpa.metadata_view());
}

fn until_idle(chilpa: &mut Chilpa) {
    let limit = Instant::now() + Duration::from_secs(20);
    while Instant::now() < limit {
        let view = chilpa.metadata_poll();
        if !view.artwork.loading {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the cover never came back");
}

/// A record's field as the form holds it: its value, and whether the
/// overlay lays it.
fn field(view: &chilpa_api::MetadataView, name: &str) -> (String, bool) {
    view.album.iter().find(|f| f.name == name).map(|f| (f.value.clone(), f.overridden)).unwrap_or_else(|| panic!("no record field {name}"))
}

fn track_field(view: &chilpa_api::MetadataView, at: usize, name: &str) -> (String, bool) {
    view.tracks[at].fields.iter().find(|f| f.name == name).map(|f| (f.value.clone(), f.overridden)).unwrap_or_else(|| panic!("no track field {name}"))
}

/// A 16 by 16 PNG, written where it is asked.
fn image_of(path: &Path) {
    image::RgbImage::from_fn(16, 16, |x, y| image::Rgb([(x * 16) as u8, (y * 16) as u8, 90])).save(path).unwrap();
}

#[test]
fn a_record_on_a_server_is_edited_into_an_overlay_kept_on_this_device() {
    let dir = std::env::temp_dir().join("chilpa-api-metadata-hosted");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));

    // The mirror of a server, as a sync leaves it.
    let path = Library::default_path();
    let (source, album, first) = {
        let lib = Library::open(&path).unwrap();
        let source = lib
            .add_server(&NewServer {
                provider: "example-server".into(),
                instance: "aaaa".into(),
                label: "Lab".into(),
                icon: "server".into(),
                address: "http://lab.lan".into(),
                libraries: "[]".into(),
                details: "{}".into(),
                ..Default::default()
            })
            .unwrap();
        synced(&lib, source, &[track("1", "One"), track("2", "One")]);
        let album: i64 = lib.connection().query_row("SELECT id FROM albums WHERE title = 'One'", [], |r| r.get(0)).unwrap();
        let first: i64 = lib.connection().query_row("SELECT id FROM tracks WHERE external_id LIKE '%/1'", [], |r| r.get(0)).unwrap();
        (source, album, first)
    };

    let mut chilpa = Chilpa::new();
    chilpa.metadata_network(Arc::new(Nowhere));

    // It opens from the library alone, and says where a save goes.
    assert!(chilpa.metadata_open(album, 0));
    until(&mut chilpa, "ready");
    let view = chilpa.metadata_view().clone();
    assert!(view.save.hosted, "{:?}", view.save);
    assert!(!view.save.files_allowed && !view.save.files_refused.is_empty());
    assert!(!view.save.xml_allowed);
    assert_eq!(view.layers.files, 0, "no file was read");
    assert_eq!(field(&view, "title").0, "One");
    assert_eq!(field(&view, "genres").0, "Rock");
    assert_eq!(view.tracks.len(), 2);
    assert_eq!(view.tracks[0].file, "1", "a track goes by its id on the server");
    assert_eq!(track_field(&view, 0, "title").0, "Song 1");
    assert!(!view.dirty);
    assert!(!chilpa.metadata_set_mode("files"), "a file mode has nothing to write into");
    assert!(!chilpa.metadata_view().error.is_empty());
    assert!(!chilpa.metadata_set_mode("xml"));
    assert!(chilpa.metadata_set_mode("sidecar"));

    // Fields a server holds and fields no server holds, edited alike.
    assert!(chilpa.metadata_set_album("title", "One, corrected"));
    assert!(chilpa.metadata_set_album("genres", "Blues; Soul"));
    assert!(chilpa.metadata_set_album("catalog_number", "CAT-1"));
    assert!(chilpa.metadata_set_track(first, "title", "First, corrected"));
    assert!(chilpa.metadata_set_track(first, "composers", "Somebody; Else"));
    assert!(chilpa.metadata_set_track(first, "work", "A work"));
    assert!(chilpa.metadata_set_track(first, "disc_subtitle", "Side A"));
    assert!(chilpa.metadata_set_track(first, "explicit", "true"));
    assert!(chilpa.metadata_set_track(first, "musicbrainz_recording_id", "3d3207f9-c4c2-425c-a364-f359b4320805"));
    let picked = dir.join("picked.png");
    image_of(&picked);
    assert!(chilpa.metadata_artwork_from_file(&picked));
    until_idle(&mut chilpa);
    assert!(chilpa.metadata_view().cover.waiting, "{}", chilpa.metadata_view().error);
    assert!(chilpa.metadata_view().dirty);
    assert!(chilpa.metadata_save());
    until(&mut chilpa, "closed");
    let after = chilpa.metadata_view().clone();
    assert!(after.error.is_empty(), "{}", after.error);
    assert!(!after.notice.is_empty());
    assert_eq!(after.album_id, album, "the record keeps its id");

    // The library shows the overlay on top of the mirror, where every
    // list and the search read it.
    let rows = chilpa.album_track_rows(album);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["First, corrected", "Song 2"]);
    assert_eq!(chilpa.album(album).map(|a| a.title), Some("One, corrected".to_string()));
    assert_eq!(chilpa.album_detail(album).map(|d| d.genre), Some("Blues".to_string()));
    let cover = chilpa.album(album).map(|a| a.artwork_path).unwrap_or_default();
    let covers = dir.join("data").join("chilpa").join("source-covers").join(source.to_string());
    assert_eq!(Path::new(&cover), covers.join("al-One.png"), "beside the library: {cover}");
    assert_eq!(std::fs::read(&cover).unwrap(), std::fs::read(&picked).unwrap(), "byte for byte");
    {
        let lib = Library::open_read_only(&path).unwrap();
        assert_eq!(lib.search("corrected", 10).unwrap().len(), 2, "the search finds the record's new title on both tracks");
        assert!(lib.search("Song 1", 10).unwrap().iter().all(|t| t.title != "Song 1"));
        let (work, subtitle, explicit, recording): (String, String, i64, String) = lib
            .connection()
            .query_row("SELECT work, disc_subtitle, explicit, recording_mbid FROM tracks WHERE id = ?1", [first], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap();
        assert_eq!((work.as_str(), subtitle.as_str(), explicit, recording.as_str()), ("A work", "Side A", 1, "3d3207f9-c4c2-425c-a364-f359b4320805"));
        let composers: i64 = lib.connection().query_row("SELECT count(*) FROM track_composers WHERE track_id = ?1", [first], |r| r.get(0)).unwrap();
        assert_eq!(composers, 2);
        let (rows, document): (i64, String) = lib
            .connection()
            .query_row("SELECT count(*), max(document) FROM source_overlay WHERE source_id = ?1 AND scope = 'album' AND key = 'al-One'", [source], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(rows, 1, "one overlay for the record");
        assert!(document.contains("\"catalog_number\": \"CAT-1\""), "a field the library has no column for is kept all the same: {document}");
        assert!(document.contains("\"cover\": \"al-One.png\""), "{document}");
        assert_eq!(lib.orphan_overlays(source).unwrap(), 0);
    }

    // The server renames the track and the record, and stops listing the
    // second track: what was changed here wins, the rest follows.
    {
        let lib = Library::open(&path).unwrap();
        let mut theirs = track("1", "One");
        theirs.title = "Theirs now".into();
        theirs.artist = "Their band".into();
        theirs.album = "One (Deluxe)".into();
        theirs.revision = Some("r2-1".into());
        synced(&lib, source, &[theirs]);
    }
    assert!(chilpa.metadata_open(album, first));
    until(&mut chilpa, "ready");
    let view = chilpa.metadata_view().clone();
    assert_eq!(view.tracks.len(), 1, "the track the server no longer lists is not open");
    assert_eq!(field(&view, "title"), ("One, corrected".to_string(), true));
    assert_eq!(field(&view, "catalog_number").0, "CAT-1");
    assert_eq!(track_field(&view, 0, "title"), ("First, corrected".to_string(), true));
    assert_eq!(track_field(&view, 0, "artist_credit").0, "Their band", "what nobody changed is the server's");
    assert_eq!(track_field(&view, 0, "composers").0, "Somebody; Else");
    assert!(!view.dirty);

    // Typing a field back to empty clears it, and the server's word for it
    // does not come back; the server not answering changes nothing here.
    {
        let lib = Library::open(&path).unwrap();
        lib.set_source_reachable(source, false).unwrap();
    }
    assert!(chilpa.metadata_set_track(first, "composers", ""));
    assert!(chilpa.metadata_set_album("date", ""));
    assert!(chilpa.metadata_save());
    until(&mut chilpa, "closed");
    assert!(chilpa.metadata_view().error.is_empty(), "{}", chilpa.metadata_view().error);
    {
        let lib = Library::open_read_only(&path).unwrap();
        let composers: i64 = lib.connection().query_row("SELECT count(*) FROM track_composers WHERE track_id = ?1", [first], |r| r.get(0)).unwrap();
        assert_eq!(composers, 0);
        let year: Option<i64> = lib.connection().query_row("SELECT year FROM albums WHERE id = ?1", [album], |r| r.get(0)).unwrap();
        assert_eq!(year, None);
        assert_eq!(std::fs::read(&cover).unwrap(), std::fs::read(&picked).unwrap(), "the cover was left alone");
    }

    let _ = std::fs::remove_dir_all(dir);
}
