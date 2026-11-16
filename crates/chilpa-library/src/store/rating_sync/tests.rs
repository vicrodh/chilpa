use super::*;
use crate::store::servers::NewServer;

const ORIGIN: &str = "server:fixture-instance";

/// A library with one server: track 7 (`a`) and track 8 (`b`) on record 1
/// (release `16`), and a local track 9.
fn lab() -> (Library, i64) {
    let lib = Library::in_memory().unwrap();
    let source = lib.add_server(&NewServer { provider: "plex".into(), instance: "fixture-instance".into(), label: "Lab".into(), ..Default::default() }).unwrap();
    let local = lib.source_id("local", "default").unwrap();
    lib.connection()
        .execute_batch(&format!(
            "INSERT INTO albums(id, title, album_artist, version_key) VALUES (1, 'Record', 'Band', '{source}:16');
             INSERT INTO albums(id, title, album_artist, version_key) VALUES (2, 'Loose', 'Band', '{source}:Loose\u{1f}Band');
             INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at) VALUES (7, {source}, 'a', 'One', 1, 0);
             INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at) VALUES (8, {source}, 'b', 'Two', 1, 0);
             INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at) VALUES (9, {local}, 'c.flac', 'Three', NULL, 0);"
        ))
        .unwrap();
    (lib, source)
}

/// A reading on a scale of 0 to 10, two to a star, halves up.
fn read(id: &str, raw: f64, read_ms: i64) -> Incoming {
    Incoming { id: id.into(), local: id.into(), raw, stars: (raw / 2.0).round() as u8, read_ms }
}

fn stars(lib: &Library, track: i64) -> u8 {
    lib.stars_of(RatingTarget::Track(track)).unwrap()
}

fn outbox(lib: &Library) -> i64 {
    lib.connection().query_row("SELECT count(*) FROM rating_outbox", [], |r| r.get(0)).unwrap()
}

#[test]
fn the_first_whole_reading_fills_only_what_has_no_stars_here_and_writes_nothing_out() {
    let (lib, source) = lab();
    lib.rate(RatingTarget::Track(8), 3).unwrap();
    let applied = lib.apply_remote_ratings(ORIGIN, &Holder::Server(source), "track", &[read("a", 7.0, 10), read("b", 4.0, 10)], true).unwrap();
    assert_eq!(applied, Applied { changed: 1, settled: 0 });
    assert_eq!(stars(&lib, 7), 4, "3.5 stars show as 4");
    assert_eq!(stars(&lib, 8), 3, "a rating made here is not replaced on first sight");
    assert_eq!(outbox(&lib), 0, "a reading never queues a write");
    assert_eq!(lib.remote_rating(ORIGIN, "track", "a").unwrap(), Some(7.0), "the raw value is kept beside the stars");
    assert_eq!(lib.remote_rating(ORIGIN, "track", "zz").unwrap(), Some(0.0), "read whole: anything else was unrated");
    assert_eq!(lib.remote_rating(ORIGIN, "album", "16").unwrap(), None, "records were not read yet");
}

#[test]
fn a_change_on_the_source_comes_in_and_an_unchanged_value_never_moves_the_stars() {
    let (lib, source) = lab();
    let holder = Holder::Server(source);
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 7.0, 10), read("b", 4.0, 10)], true).unwrap();
    lib.rate(RatingTarget::Track(7), 2).unwrap();
    let applied = lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 7.0, 20), read("b", 10.0, 20)], true).unwrap();
    assert_eq!(applied.changed, 1);
    assert_eq!(stars(&lib, 7), 2, "the source still says 7: nothing changed there");
    assert_eq!(stars(&lib, 8), 5, "the source changed: it comes in");
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("b", 0.0, 30)], true).unwrap();
    assert_eq!(stars(&lib, 8), 0, "taken off on the source");
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 9.0, 40)], true).unwrap();
    assert_eq!(stars(&lib, 8), 0, "a track the reading does not name keeps what it has");
}

#[test]
fn a_new_rating_on_the_source_after_the_first_reading_is_a_change() {
    let (lib, source) = lab();
    let holder = Holder::Server(source);
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 0.0, 10), read("b", 0.0, 10)], true).unwrap();
    assert_eq!(lib.connection().query_row("SELECT count(*) FROM remote_ratings", [], |r| r.get::<_, i64>(0)).unwrap(), 0, "unrated keeps no row");
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 6.0, 20)], true).unwrap();
    assert_eq!(stars(&lib, 7), 3);
}

#[test]
fn a_partial_first_reading_does_not_make_the_next_one_a_change() {
    let (lib, source) = lab();
    let holder = Holder::Server(source);
    lib.rate(RatingTarget::Track(8), 1).unwrap();
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 0.0, 10)], false).unwrap();
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("b", 8.0, 20)], true).unwrap();
    assert_eq!(stars(&lib, 8), 1, "still first sight: the earlier reading was not whole");
}

#[test]
fn a_change_on_its_way_wins_and_goes_once_the_source_holds_it() {
    let (lib, source) = lab();
    let holder = Holder::Server(source);
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 8.0, 10)], true).unwrap();
    lib.rate(RatingTarget::Track(7), 1).unwrap();
    assert!(lib.queue_rating(ORIGIN, "track", "a", 1, Some(4)).unwrap());
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 10.0, 20)], true).unwrap();
    assert_eq!(stars(&lib, 7), 1, "what the listener did here and has not reached the source wins");
    assert_eq!(outbox(&lib), 1);
    let applied = lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 2.0, 30)], true).unwrap();
    assert_eq!(applied.settled, 1, "the source already holds it: nothing to send");
    assert_eq!(outbox(&lib), 0);
}

#[test]
fn a_reading_asked_for_before_chilpa_s_write_landed_is_not_believed() {
    let (lib, source) = lab();
    let holder = Holder::Server(source);
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 8.0, 10)], true).unwrap();
    lib.rate(RatingTarget::Track(7), 2).unwrap();
    lib.queue_rating(ORIGIN, "track", "a", 2, Some(4)).unwrap();
    let change = lib.due_ratings(i64::MAX, 10).unwrap().remove(0);
    lib.rating_sent(&change, 4.0, 2_000).unwrap();
    assert_eq!(outbox(&lib), 0);
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 8.0, 1_500)], true).unwrap();
    assert_eq!(stars(&lib, 7), 2, "asked for before the write: it says what was there before");
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 4.0, 2_500)], true).unwrap();
    assert_eq!(stars(&lib, 7), 2);
}

#[test]
fn a_value_the_source_already_holds_is_never_queued() {
    let (lib, _) = lab();
    assert!(!lib.queue_rating(ORIGIN, "track", "a", 4, Some(4)).unwrap(), "7 on the source shows as 4: setting 4 sends nothing");
    assert_eq!(outbox(&lib), 0);
    assert!(lib.queue_rating(ORIGIN, "track", "a", 3, Some(4)).unwrap());
    assert!(!lib.queue_rating(ORIGIN, "track", "a", 4, Some(4)).unwrap(), "changed back before it went: the pending one goes too");
    assert_eq!(outbox(&lib), 0);
    assert!(lib.queue_rating(ORIGIN, "track", "a", 0, None).unwrap(), "never read: it goes");
    assert!(lib.queue_rating(ORIGIN, "playlist", "a", 3, None).is_err());
}

#[test]
fn the_last_change_wins_and_a_finished_older_send_does_not_take_it_away() {
    let (lib, _) = lab();
    lib.rate(RatingTarget::Track(7), 2).unwrap();
    lib.queue_rating(ORIGIN, "track", "a", 2, None).unwrap();
    let first = lib.due_ratings(i64::MAX, 10).unwrap().remove(0);
    lib.rate(RatingTarget::Track(7), 5).unwrap();
    lib.queue_rating(ORIGIN, "track", "a", 5, None).unwrap();
    lib.rating_sent(&first, 4.0, 100).unwrap();
    let waiting = lib.due_ratings(i64::MAX, 10).unwrap();
    assert_eq!(waiting.len(), 1, "the newer change still waits");
    assert_eq!((waiting[0].stars, waiting[0].attempts), (5, 0));
    assert!(waiting[0].asked > first.asked);
    lib.rating_failed(&first, "late", 9_999).unwrap();
    assert_eq!(lib.due_ratings(i64::MAX, 10).unwrap()[0].attempts, 0, "a failure of the older one does not count against it");
}

#[test]
fn a_failed_change_waits_its_turn_and_says_why() {
    let (lib, _) = lab();
    lib.queue_rating(ORIGIN, "track", "a", 3, None).unwrap();
    lib.queue_rating("server:other", "album", "16", 4, None).unwrap();
    let change = lib.due_ratings(100, 10).unwrap().into_iter().find(|c| c.origin == ORIGIN).unwrap();
    lib.rating_failed(&change, "the server could not be reached", 500).unwrap();
    assert_eq!(lib.due_ratings(100, 10).unwrap().len(), 1, "not due again before its time");
    assert_eq!(lib.next_rating_due().unwrap(), Some(0));
    assert_eq!(lib.due_ratings(500, 10).unwrap().len(), 2);
    assert_eq!(lib.rating_failures().unwrap(), [(ORIGIN.to_string(), 1, "the server could not be reached".to_string())]);
    lib.drop_rating(&change).unwrap();
    assert!(lib.rating_failures().unwrap().is_empty());
}

#[test]
fn turned_off_nothing_comes_in_nothing_waits_and_turned_on_it_starts_as_new() {
    let (lib, source) = lab();
    let holder = Holder::Server(source);
    assert!(lib.rating_sync_enabled(ORIGIN).unwrap(), "on unless turned off");
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 8.0, 10)], true).unwrap();
    lib.queue_rating(ORIGIN, "track", "b", 3, None).unwrap();
    lib.set_rating_sync(ORIGIN, false).unwrap();
    assert!(!lib.rating_sync_enabled(ORIGIN).unwrap());
    assert_eq!(outbox(&lib), 0, "what waited is dropped");
    lib.rate(RatingTarget::Track(7), 1).unwrap();
    let applied = lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 10.0, 20)], true).unwrap();
    assert_eq!(applied, Applied::default());
    assert_eq!(stars(&lib, 7), 1);
    lib.set_rating_sync(ORIGIN, true).unwrap();
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[read("a", 10.0, 30)], true).unwrap();
    assert_eq!(stars(&lib, 7), 1, "first sight again: stars here stay");
}

#[test]
fn records_are_found_by_their_release_and_a_track_not_in_the_library_waits() {
    let (lib, source) = lab();
    let holder = Holder::Server(source);
    lib.apply_remote_ratings(ORIGIN, &holder, "album", &[read("16", 10.0, 10)], true).unwrap();
    assert_eq!(lib.stars_of(RatingTarget::Album(1)).unwrap(), 5);
    lib.apply_remote_ratings(ORIGIN, &holder, "track", &[Incoming { id: "new".into(), local: "new".into(), raw: 8.0, stars: 4, read_ms: 10 }], true).unwrap();
    assert_eq!(lib.remote_rating(ORIGIN, "track", "new").unwrap(), Some(0.0), "nothing kept for it: the next reading brings it");
}

#[test]
fn a_plugin_s_items_keep_their_stars_under_its_id_and_each_is_new_until_read() {
    let lib = Library::in_memory().unwrap();
    let holder = Holder::Plugin("test.source".into());
    let origin = "plugin:test.source";
    lib.rate_external("test.source", "track", "track:2", 2).unwrap();
    let plugin_read = |id: &str, raw: f64, ms: i64| Incoming { id: id.into(), local: id.into(), raw, stars: (raw / 2.0).round() as u8, read_ms: ms };
    lib.apply_remote_ratings(origin, &holder, "track", &[plugin_read("track:1", 0.0, 10)], false).unwrap();
    lib.apply_remote_ratings(origin, &holder, "track", &[plugin_read("track:2", 8.0, 20), plugin_read("track:3", 6.0, 20)], false).unwrap();
    assert_eq!(lib.external_stars("test.source", "track", "track:2").unwrap(), 2, "first sight of this one: its stars here stay");
    assert_eq!(lib.external_stars("test.source", "track", "track:3").unwrap(), 3);
    lib.apply_remote_ratings(origin, &holder, "track", &[plugin_read("track:1", 4.0, 30)], false).unwrap();
    assert_eq!(lib.external_stars("test.source", "track", "track:1").unwrap(), 2, "read unrated before: a change");
    assert_eq!(lib.remote_rating(origin, "track", "track:9").unwrap(), None, "a plugin's lists are never whole");
}

#[test]
fn a_rating_here_is_traced_to_the_server_that_holds_the_item() {
    let (lib, source) = lab();
    assert_eq!(lib.rated_on_server(RatingTarget::Track(7)).unwrap(), Some((source, "a".into())));
    assert_eq!(lib.rated_on_server(RatingTarget::Album(1)).unwrap(), Some((source, "16".into())));
    assert_eq!(lib.rated_on_server(RatingTarget::Album(2)).unwrap(), None, "a record the server gave no id for");
    assert_eq!(lib.rated_on_server(RatingTarget::Track(9)).unwrap(), None, "a local file");
    assert_eq!(lib.rated_on_server(RatingTarget::Playlist(1)).unwrap(), None);
}
