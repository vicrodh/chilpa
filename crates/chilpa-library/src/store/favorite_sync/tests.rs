use super::*;
use crate::store::servers::NewServer;

const ORIGIN: &str = "server:fixture-instance";

/// One server: tracks 7 (`a`) and 8 (`b`) on record 1 (release `16`).
fn lab() -> (Library, i64) {
    let lib = Library::in_memory().unwrap();
    let source = lib.add_server(&NewServer { provider: "subsonic".into(), instance: "fixture-instance".into(), label: "Lab".into(), ..Default::default() }).unwrap();
    lib.connection()
        .execute_batch(&format!(
            "INSERT INTO albums(id, title, album_artist, version_key) VALUES (1, 'Record', 'Band', '{source}:16');
             INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at) VALUES (7, {source}, 'a', 'One', 1, 0);
             INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at) VALUES (8, {source}, 'b', 'Two', 1, 0);"
        ))
        .unwrap();
    (lib, source)
}

fn read(id: &str, on: bool, read_ms: i64) -> FavoriteRead {
    FavoriteRead { id: id.into(), local: id.into(), on, read_ms }
}

fn hearted(lib: &Library, track: i64) -> bool {
    lib.is_favorite(Hearted::Track(track)).unwrap()
}

fn outbox(lib: &Library) -> i64 {
    lib.connection().query_row("SELECT count(*) FROM favorite_outbox", [], |r| r.get(0)).unwrap()
}

#[test]
fn the_first_whole_reading_brings_the_server_s_favorites_and_keeps_chilpa_s_own() {
    let (lib, source) = lab();
    lib.set_favorite(FavoriteTarget::Track(8), true).unwrap();
    let (changed, _) = lib.apply_remote_favorites(ORIGIN, source, "track", &[read("a", true, 10), read("b", false, 10)], true).unwrap();
    assert_eq!(changed, 1);
    assert!(hearted(&lib, 7), "a favorite of the server that Chilpa did not have comes in");
    assert!(hearted(&lib, 8), "a heart made here stays on first sight");
    assert_eq!(outbox(&lib), 0, "and it is not sent: nobody changed it");
    assert_eq!(lib.remote_favorite(ORIGIN, "track", "b").unwrap(), Some(false), "read whole: the rest is not a favorite there");
    assert_eq!(lib.remote_favorite(ORIGIN, "album", "16").unwrap(), None);
}

#[test]
fn a_change_on_the_server_comes_in_and_an_unchanged_one_never_moves_the_heart() {
    let (lib, source) = lab();
    lib.apply_remote_favorites(ORIGIN, source, "track", &[read("a", true, 10)], true).unwrap();
    lib.set_favorite(FavoriteTarget::Track(7), false).unwrap();
    lib.apply_remote_favorites(ORIGIN, source, "track", &[read("a", true, 20)], true).unwrap();
    assert!(!hearted(&lib, 7), "still a favorite there, as before: nothing changed there");
    lib.apply_remote_favorites(ORIGIN, source, "track", &[read("b", true, 30)], true).unwrap();
    assert!(hearted(&lib, 8), "a new favorite there after the first reading is a change");
    lib.apply_remote_favorites(ORIGIN, source, "track", &[read("b", false, 40)], true).unwrap();
    assert!(!hearted(&lib, 8), "taken off there");
}

#[test]
fn a_heart_on_its_way_wins_and_goes_once_the_server_holds_it() {
    let (lib, source) = lab();
    lib.apply_remote_favorites(ORIGIN, source, "track", &[read("a", false, 10)], true).unwrap();
    lib.set_favorite(FavoriteTarget::Track(7), true).unwrap();
    assert!(lib.queue_favorite(ORIGIN, "track", "a", true, Some(false)).unwrap());
    lib.apply_remote_favorites(ORIGIN, source, "track", &[read("a", false, 20)], true).unwrap();
    assert!(hearted(&lib, 7));
    assert_eq!(outbox(&lib), 1);
    lib.apply_remote_favorites(ORIGIN, source, "track", &[read("a", true, 30)], true).unwrap();
    assert_eq!(outbox(&lib), 0, "the server holds it now");
}

#[test]
fn a_reading_from_before_chilpa_s_write_and_a_value_the_server_holds_are_ignored() {
    let (lib, source) = lab();
    assert!(!lib.queue_favorite(ORIGIN, "track", "a", true, Some(true)).unwrap(), "the server already holds it");
    lib.set_favorite(FavoriteTarget::Track(7), true).unwrap();
    lib.queue_favorite(ORIGIN, "track", "a", true, None).unwrap();
    let change = lib.due_favorites(i64::MAX, 10).unwrap().remove(0);
    lib.favorite_sent(&change, 2_000).unwrap();
    lib.apply_remote_favorites(ORIGIN, source, "track", &[read("a", false, 1_500)], true).unwrap();
    assert!(hearted(&lib, 7), "asked for before the write landed");
}

#[test]
fn the_last_change_wins_and_failures_wait_their_turn() {
    let (lib, _) = lab();
    lib.queue_favorite(ORIGIN, "track", "a", true, None).unwrap();
    let first = lib.due_favorites(i64::MAX, 10).unwrap().remove(0);
    lib.queue_favorite(ORIGIN, "track", "a", false, None).unwrap();
    lib.favorite_sent(&first, 100).unwrap();
    let waiting = lib.due_favorites(i64::MAX, 10).unwrap();
    assert_eq!(waiting.len(), 1, "the newer change still waits");
    assert!(!waiting[0].on && waiting[0].asked > first.asked);
    lib.favorite_failed(&waiting[0], "the server could not be reached", 500).unwrap();
    assert!(lib.due_favorites(100, 10).unwrap().is_empty());
    assert_eq!(lib.next_favorite_due().unwrap(), Some(500));
    assert_eq!(lib.favorite_failures().unwrap(), [(ORIGIN.to_string(), 1, "the server could not be reached".to_string())]);
    lib.drop_favorite(&waiting[0]).unwrap();
    assert_eq!(outbox(&lib), 0);
}

#[test]
fn turned_off_nothing_comes_in_and_nothing_waits() {
    let (lib, source) = lab();
    lib.queue_favorite(ORIGIN, "album", "16", true, None).unwrap();
    lib.set_favorite_sync(ORIGIN, false).unwrap();
    assert!(!lib.favorite_sync_enabled(ORIGIN).unwrap());
    assert_eq!(outbox(&lib), 0);
    lib.apply_remote_favorites(ORIGIN, source, "album", &[read("16", true, 10)], true).unwrap();
    assert!(!lib.is_favorite(Hearted::Album(1)).unwrap());
    lib.set_favorite_sync(ORIGIN, true).unwrap();
    lib.apply_remote_favorites(ORIGIN, source, "album", &[read("16", true, 20)], true).unwrap();
    assert!(lib.is_favorite(Hearted::Album(1)).unwrap(), "records come in by their release id");
}

#[test]
fn hearts_and_stars_are_kept_apart() {
    let (lib, source) = lab();
    lib.apply_remote_favorites(ORIGIN, source, "track", &[read("a", true, 10)], true).unwrap();
    assert_eq!(lib.ratings().unwrap().tracks.get(&7), None, "a heart is no rating");
    assert!(lib.due_ratings(i64::MAX, 10).unwrap().is_empty());
    lib.rate(crate::store::ratings::RatingTarget::Track(8), 5).unwrap();
    assert!(!hearted(&lib, 8), "five stars are no heart");
}
