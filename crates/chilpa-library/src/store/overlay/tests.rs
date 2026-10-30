use super::*;
use crate::store::{NewServer, ServerTrack};
use rusqlite::params;

fn new_server(instance: &str) -> NewServer {
    NewServer {
        provider: "example-server".into(),
        instance: instance.into(),
        label: format!("Server {instance}"),
        icon: "server".into(),
        address: format!("http://{instance}.lan:4533"),
        account: "me".into(),
        identity: String::new(),
        product: "Example 1.0".into(),
        libraries: "[]".into(),
        details: "{}".into(),
    }
}

/// A track as the server lists it, under the entry the queue carries.
fn track(id: &str, album: &str) -> ServerTrack {
    ServerTrack {
        external_id: format!("remote:example-server/aaaa/{id}"),
        title: format!("Song {id}"),
        artist: "Band".into(),
        album: album.into(),
        album_artist: "Band".into(),
        album_key: format!("al-{album}"),
        cover: Some(format!("cover-{album}")),
        track_number: Some(1),
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

fn synced(lib: &Library, source: i64, tracks: &[ServerTrack]) -> usize {
    let generation = lib.begin_server_sync(source).unwrap();
    lib.save_server_tracks(source, generation, tracks).unwrap();
    lib.finish_server_sync(source, generation).unwrap()
}

/// A library with one server and one record of two tracks on it.
fn shelf() -> (Library, i64) {
    let lib = Library::in_memory().unwrap();
    let source = lib.add_server(&new_server("aaaa")).unwrap();
    synced(&lib, source, &[track("1", "One"), track("2", "One")]);
    (lib, source)
}

fn row(lib: &Library, id: &str) -> (i64, String, String, Option<i64>, Option<String>, Option<i64>) {
    lib.db
        .query_row(
            "SELECT t.id, t.title, t.artist, t.present, t.work, t.explicit FROM tracks t WHERE t.external_id = ?1",
            params![format!("remote:example-server/aaaa/{id}")],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap()
}

fn record(lib: &Library, album: &str) -> (i64, String, String, Option<i64>, Option<String>) {
    lib.db
        .query_row(
            "SELECT id, title, album_artist, year, artwork_path FROM albums WHERE version_key LIKE '%:al-' || ?1",
            params![album],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap()
}

fn composers(lib: &Library, track: i64) -> Vec<String> {
    let mut stmt = lib.db.prepare("SELECT c.name FROM track_composers tc JOIN composers c ON c.id = tc.composer_id WHERE tc.track_id = ?1 ORDER BY c.name").unwrap();
    stmt.query_map(params![track], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

fn home(source: i64) -> Home {
    Home::hosted(source, Some("al-One"), "remote:example-server/aaaa/1")
}

#[test]
fn an_overlay_is_laid_over_the_mirror_as_it_is_written_and_read_back() {
    let (lib, source) = shelf();
    let home = home(source);
    assert_eq!(lib.overlay(&home).unwrap(), None, "nothing was changed yet");

    let mut sidecar = Sidecar::new(0);
    sidecar.album.title = Override::Value("One, corrected".into());
    sidecar.track_mut("1").title = Override::Value("First, corrected".into());
    let stored = lib.store_overlay(&home, &sidecar, None, 5).unwrap();
    assert_eq!(stored.tracks.len(), 2, "both tracks of the record were reached");
    assert_eq!(stored.cover, None);

    let read = lib.overlay(&home).unwrap().unwrap();
    assert_eq!(read.album.title, Override::Value("One, corrected".into()));
    assert_eq!(read.tracks["1"].title, Override::Value("First, corrected".into()));
    assert_eq!(read.updated_at, 5);
    assert_eq!(row(&lib, "1").1, "First, corrected");
    assert_eq!(row(&lib, "2").1, "Song 2", "the other track was not named");
    assert_eq!(record(&lib, "One").1, "One, corrected");
    assert_eq!(lib.search("corrected", 10).unwrap().len(), 2, "the search sees the record's new title on both tracks");
    assert!(lib.search("Song 1", 10).unwrap().iter().all(|t| t.title != "Song 1"));
    let first: Vec<String> = lib.search("First", 10).unwrap().into_iter().map(|t| t.title).collect();
    assert_eq!(first, ["First, corrected"], "and the track's own new title");
}

#[test]
fn an_overlay_survives_a_sync_that_changes_the_server_s_title() {
    let (lib, source) = shelf();
    let mut sidecar = Sidecar::new(0);
    sidecar.track_mut("1").title = Override::Value("Mine".into());
    sidecar.album.album_artist = Override::Value("My band".into());
    lib.store_overlay(&home(source), &sidecar, None, 5).unwrap();

    // The server renamed the track and its performers, and the record.
    let mut changed = track("1", "One");
    changed.title = "Theirs now".into();
    changed.artist = "Their band".into();
    changed.album = "One (Deluxe)".into();
    changed.revision = Some("r2-1".into());
    let mut second = track("2", "One");
    second.album = "One (Deluxe)".into();
    synced(&lib, source, &[changed, second]);

    let (_, title, artist, _, _, _) = row(&lib, "1");
    assert_eq!(title, "Mine", "what was changed here wins");
    assert_eq!(artist, "Their band", "what was not follows the server");
    let (_, album_title, album_artist, _, _) = record(&lib, "One");
    assert_eq!(album_title, "One (Deluxe)");
    assert_eq!(album_artist, "My band");
    let credit: String = lib.db.query_row("SELECT album_credit FROM tracks WHERE external_id LIKE '%/2'", [], |r| r.get(0)).unwrap();
    assert_eq!(credit, "My band", "and the record's fields reach every track of it");
    assert_eq!(lib.overlay(&home(source)).unwrap().unwrap(), {
        let mut same = sidecar.clone();
        same.updated_at = 5;
        same
    }, "the sync did not rewrite the overlay");
}

#[test]
fn an_overlay_survives_a_sweep_that_marks_its_track_absent_and_is_laid_again_when_it_is_back() {
    let (lib, source) = shelf();
    let mut sidecar = Sidecar::new(0);
    sidecar.track_mut("2").work = Override::Value("A work".into());
    lib.store_overlay(&home(source), &sidecar, None, 5).unwrap();
    assert_eq!(lib.orphan_overlays(source).unwrap(), 0);

    // A whole sync that no longer lists the record at all.
    synced(&lib, source, &[track("3", "Two")]);
    assert_eq!(row(&lib, "2").3, Some(0), "marked absent, never deleted");
    assert!(lib.overlay(&home(source)).unwrap().is_some(), "and its overlay is kept");
    assert_eq!(lib.orphan_overlays(source).unwrap(), 1, "counted as one the server no longer lists");

    // Back on the server, with another work in its tags.
    synced(&lib, source, &[track("1", "One"), track("2", "One"), track("3", "Two")]);
    let (_, _, _, present, work, _) = row(&lib, "2");
    assert_eq!((present, work.as_deref()), (Some(1), Some("A work")));
    assert_eq!(lib.orphan_overlays(source).unwrap(), 0);
}

#[test]
fn cleared_value_and_absent_lay_three_different_things() {
    let (lib, source) = shelf();
    let (one, _, _, _, _, _) = row(&lib, "1");
    let (two, _, _, _, _, _) = row(&lib, "2");
    assert_eq!(lib.track_genres(one).unwrap(), ["Rock"]);

    let mut sidecar = Sidecar::new(0);
    sidecar.album.genres = Override::Cleared;
    sidecar.album.date = Override::Cleared;
    sidecar.track_mut("1").composers = Override::Value(vec!["Somebody".into(), "Else".into()]);
    sidecar.track_mut("1").explicit = Override::Value(true);
    sidecar.track_mut("1").disc_subtitle = Override::Value("Side A".into());
    sidecar.track_mut("2").track_number = Override::Cleared;
    lib.store_overlay(&home(source), &sidecar, None, 5).unwrap();

    assert!(lib.track_genres(one).unwrap().is_empty(), "cleared: empty whatever the server says");
    assert!(lib.track_genres(two).unwrap().is_empty(), "the record's genres reach every track");
    assert_eq!(record(&lib, "One").3, None, "the year was cleared");
    assert_eq!(composers(&lib, one), ["Else", "Somebody"], "a field no server holds is laid like any other");
    assert_eq!(composers(&lib, two), Vec::<String>::new());
    let (_, _, _, _, _, explicit) = row(&lib, "1");
    assert_eq!(explicit, Some(1));
    let subtitle: Option<String> = lib.db.query_row("SELECT disc_subtitle FROM tracks WHERE id = ?1", params![one], |r| r.get(0)).unwrap();
    assert_eq!(subtitle.as_deref(), Some("Side A"));
    let number: Option<i64> = lib.db.query_row("SELECT track_number FROM tracks WHERE id = ?1", params![two], |r| r.get(0)).unwrap();
    assert_eq!(number, None);
    let (_, title, _, _, _, _) = row(&lib, "1");
    assert_eq!(title, "Song 1", "absent: the server's word stands");

    // The next sync brings the server's genres and year again, and the
    // overlay clears them again.
    synced(&lib, source, &[track("1", "One"), track("2", "One")]);
    assert!(lib.track_genres(one).unwrap().is_empty());
    assert_eq!(record(&lib, "One").3, None);
    assert_eq!(composers(&lib, one), ["Else", "Somebody"]);
}

#[test]
fn an_overlay_with_nothing_left_in_it_is_taken_out() {
    let (lib, source) = shelf();
    let mut sidecar = Sidecar::new(0);
    sidecar.track_mut("1").title = Override::Value("Mine".into());
    lib.store_overlay(&home(source), &sidecar, None, 5).unwrap();
    let rows = |lib: &Library| lib.db.query_row("SELECT count(*) FROM source_overlay", [], |r| r.get::<_, i64>(0)).unwrap();
    assert_eq!(rows(&lib), 1);
    lib.store_overlay(&home(source), &Sidecar::new(0), None, 6).unwrap();
    assert_eq!(rows(&lib), 0);
    assert_eq!(lib.overlay(&home(source)).unwrap(), None);
}

#[test]
fn a_cover_chosen_for_a_server_s_record_outlasts_the_server_s_own() {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open(&dir.path().join("library.db")).unwrap();
    let source = lib.add_server(&new_server("aaaa")).unwrap();
    let mut loose = track("9", "");
    loose.album_key.clear();
    synced(&lib, source, &[track("1", "One"), track("2", "One"), loose.clone()]);
    let home = home(source);
    let covers = lib.overlay_covers_dir(source).unwrap();
    assert_eq!(covers, dir.path().join("source-covers").join(source.to_string()), "beside the library, under the server");

    let stored = lib.store_overlay(&home, &Sidecar::new(0), Some((b"a jpeg", "jpg")), 5).unwrap();
    let cover = stored.cover.expect("the cover was kept");
    assert_eq!(cover, covers.join("al-One.jpg"));
    assert_eq!(std::fs::read(&cover).unwrap(), b"a jpeg", "byte for byte");
    let (album, _, _, _, shown) = record(&lib, "One");
    assert_eq!(shown.as_deref(), Some(cover.to_str().unwrap()));
    let read = lib.overlay(&home).unwrap().unwrap();
    assert_eq!(read.album.cover, Override::Value("al-One.jpg".into()), "the document names the file, not its path");

    // The server's cover arrives afterwards and does not replace it.
    lib.set_server_cover(album, Some("/cache/servers/aaaa/1.jpg")).unwrap();
    assert_eq!(record(&lib, "One").4.as_deref(), Some(cover.to_str().unwrap()));
    assert!(lib.pending_server_covers(source, 10).unwrap().iter().all(|(id, _)| *id != album), "and it is not asked for again");

    // Another kind replaces the first; letting go of it removes the file.
    let stored = lib.store_overlay(&home, &read, Some((b"a png", "png")), 6).unwrap();
    assert_eq!(stored.cover, Some(covers.join("al-One.png")));
    assert!(!covers.join("al-One.jpg").exists());
    let mut none = lib.overlay(&home).unwrap().unwrap();
    none.album.cover = Override::Absent;
    lib.store_overlay(&home, &none, None, 7).unwrap();
    assert!(!covers.join("al-One.png").exists());

    // A track without a record has no cover of its own, and nothing of
    // the save that brought one is kept.
    let mut titled = Sidecar::new(0);
    titled.track_mut("9").title = Override::Value("Loose".into());
    let loose_home = Home::hosted(source, None, &loose.external_id);
    assert!(lib.store_overlay(&loose_home, &titled, Some((b"a jpeg", "jpg")), 8).is_err());
    assert_eq!(lib.overlay(&loose_home).unwrap(), None);
}

#[test]
fn a_cover_has_no_place_in_a_library_that_is_not_a_file() {
    let (lib, source) = shelf();
    assert_eq!(lib.overlay_covers_dir(source), None);
    assert!(lib.store_overlay(&home(source), &Sidecar::new(0), Some((b"a jpeg", "jpg")), 5).is_err());
    assert_eq!(lib.overlay(&home(source)).unwrap(), None, "and nothing was written");
}

#[test]
fn a_server_s_id_cannot_leave_the_covers_folder() {
    assert_eq!(file_safe("al-9_x.1"), "al-9_x.1");
    assert_eq!(file_safe("a b/c"), "a%20b%2Fc");
    assert_eq!(file_safe("..\\up"), "..%5Cup");
    assert_eq!(file_safe(".."), "_..");
    assert_eq!(file_safe("."), "_.");
    assert_eq!(file_safe(""), "_");
}

#[test]
fn a_track_s_entry_finds_the_overlay_laid_over_it() {
    let (lib, source) = shelf();
    let mut sidecar = Sidecar::new(0);
    sidecar.album.musicbrainz_release_id = Override::Value("00000000-0000-4000-8000-000000000001".into());
    lib.store_overlay(&home(source), &sidecar, None, 5).unwrap();
    let found = lib.overlay_of_entry("remote:example-server/aaaa/2").unwrap().expect("its record's overlay");
    assert_eq!(found.album.musicbrainz_release_id, Override::Value("00000000-0000-4000-8000-000000000001".into()));
    assert_eq!(lib.overlay_of_entry("remote:example-server/aaaa/404").unwrap(), None, "a track the library does not have");
    assert_eq!(lib.overlay_of_entry("/music/Record/01.flac").unwrap(), None, "a file is not a server's track");
}

#[test]
fn removing_a_server_takes_its_overlays_with_it() {
    let (lib, source) = shelf();
    let mut sidecar = Sidecar::new(0);
    sidecar.track_mut("1").title = Override::Value("Mine".into());
    lib.store_overlay(&home(source), &sidecar, None, 5).unwrap();
    lib.remove_source(source).unwrap();
    let left: i64 = lib.db.query_row("SELECT count(*) FROM source_overlay", [], |r| r.get(0)).unwrap();
    assert_eq!(left, 0);
    assert!(lib.check().unwrap().is_empty());
}

#[test]
fn a_track_without_a_record_on_the_server_has_an_overlay_of_its_own() {
    let lib = Library::in_memory().unwrap();
    let source = lib.add_server(&new_server("aaaa")).unwrap();
    let mut loose = track("9", "");
    loose.album_key.clear();
    synced(&lib, source, &[loose.clone()]);
    let home = Home::hosted(source, None, &loose.external_id);
    assert_eq!(home, Home::Hosted { source_id: source, scope: Scope::Track, key: "9".into() });

    let mut sidecar = Sidecar::new(0);
    sidecar.track_mut("9").title = Override::Value("Loose, corrected".into());
    sidecar.album.label = Override::Value("A label".into());
    let stored = lib.store_overlay(&home, &sidecar, None, 5).unwrap();
    assert_eq!(stored.tracks.len(), 1);
    let (id, title, _, _, _, _) = row(&lib, "9");
    assert_eq!(title, "Loose, corrected");
    let label: Option<String> = lib.db.query_row("SELECT label FROM tracks WHERE id = ?1", params![id], |r| r.get(0)).unwrap();
    assert_eq!(label.as_deref(), Some("A label"));

    synced(&lib, source, &[loose]);
    assert_eq!(row(&lib, "9").1, "Loose, corrected", "laid again after the sync");
    synced(&lib, source, &[]);
    assert_eq!(lib.orphan_overlays(source).unwrap(), 1);
}

#[test]
fn the_editor_s_tracks_say_which_server_and_record_they_are_on() {
    let (lib, source) = shelf();
    let (album, _, _, _, _) = record(&lib, "One");
    let tracks = lib.edit_tracks(album).unwrap();
    assert_eq!(tracks.len(), 2);
    let hosted = tracks[0].hosted.clone().expect("on a server");
    assert_eq!((hosted.source_id, hosted.album_key.as_deref()), (source, Some("al-One")));
    assert_eq!(tracks[0].home(), home(source));
    assert_eq!(tracks[0].home().key(&tracks[0].external_id), "1");
    assert_eq!(tracks[0].container, None);
}
