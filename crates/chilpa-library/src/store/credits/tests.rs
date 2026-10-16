use super::*;

fn library_of(names: &[&str]) -> impl Fn(&str) -> bool {
    let known: Vec<String> = names.iter().map(|n| key(n)).collect();
    move |name: &str| known.contains(&key(name))
}

#[test]
fn a_credit_is_split_when_every_part_is_an_artist_of_its_own() {
    let known = library_of(&["May'n", "Megumi Nakajima", "Walk\u{fc}re", "Fire Bomber"]);
    assert_eq!(split_credit("May'n, Megumi Nakajima", &known).unwrap(), ["May'n", "Megumi Nakajima"]);
    assert_eq!(split_credit("May'n & Megumi Nakajima", &known).unwrap(), ["May'n", "Megumi Nakajima"]);
    assert_eq!(split_credit("may'n and megumi nakajima", &known).unwrap(), ["may'n", "megumi nakajima"]);
    assert_eq!(split_credit("May'n, Megumi Nakajima, Walk\u{fc}re", &known).unwrap().len(), 3);
    assert_eq!(split_credit("May'n feat. Fire Bomber", &known).unwrap(), ["May'n", "Fire Bomber"]);
}

#[test]
fn a_name_that_looks_joined_stays_whole() {
    let known = library_of(&["AC/DC", "Emerson, Lake & Palmer", "Echo And The Bunnymen", "McCartney, Paul", "Chris Cornell", "Earth"]);
    for whole in ["AC/DC", "Emerson, Lake & Palmer", "Echo And The Bunnymen", "McCartney, Paul"] {
        assert_eq!(split_credit(whole, &known), None, "{whole}");
    }
    assert_eq!(split_credit("Chris Cornell, Eleven", &known), None, "one part known is not every part");
    assert_eq!(split_credit("Earth, Wind & Fire", &known), None);
    assert_eq!(split_credit("Chris Cornell", &known), None, "one artist is not split into itself");
}

#[test]
fn a_part_that_names_several_is_taken_apart_too() {
    let known = library_of(&["A", "B", "C", "B & C"]);
    assert_eq!(split_credit("A, B & C", &known).unwrap(), ["A", "B", "C"]);
    assert_eq!(split_credit("A, B, a", &known).unwrap(), ["A", "B"], "an artist named twice is one");
}

fn track(lib: &Library, id: i64, album: i64, artist: &str) {
    let source = lib.source_id("local", "default").unwrap();
    lib.connection().execute("INSERT OR IGNORE INTO albums (id, title, album_artist, folder) VALUES (?1, ?2, '', ?2)", params![album, format!("Record {album}")]).unwrap();
    lib.connection()
        .execute("INSERT INTO tracks (id, source_id, external_id, title, artist, album_id, added_at) VALUES (?1, ?2, ?3, 'T', ?4, ?5, 1)", params![id, source, format!("/{id}.flac"), artist, album])
        .unwrap();
}

fn filed_under(lib: &Library, track: i64) -> Vec<String> {
    let mut stmt = lib.connection().prepare("SELECT artist FROM album_members WHERE track_id = ?1 ORDER BY artist").unwrap();
    let rows = stmt.query_map(params![track], |r| r.get(0)).unwrap();
    rows.collect::<rusqlite::Result<_>>().unwrap()
}

#[test]
fn a_matched_credit_files_its_tracks_under_each_artist() {
    let lib = Library::in_memory().unwrap();
    track(&lib, 1, 1, "May'n");
    track(&lib, 2, 1, "Megumi Nakajima");
    track(&lib, 3, 1, "May'n, Megumi Nakajima");
    track(&lib, 4, 2, "AC/DC");
    assert_eq!(filed_under(&lib, 3), ["May'n, Megumi Nakajima"], "until it is matched");

    assert_eq!(lib.match_credits().unwrap(), 1);
    assert_eq!(filed_under(&lib, 3), ["May'n", "Megumi Nakajima"]);
    assert_eq!(filed_under(&lib, 4), ["AC/DC"]);
    assert_eq!(lib.credit_names("May'n, Megumi Nakajima").unwrap(), ["May'n", "Megumi Nakajima"]);
    assert_eq!(lib.credit_names("AC/DC").unwrap(), ["AC/DC"]);
    let artist: String = lib.connection().query_row("SELECT artist FROM tracks WHERE id = 3", [], |r| r.get(0)).unwrap();
    assert_eq!(artist, "May'n, Megumi Nakajima", "the credit is what its tag wrote");
    assert_eq!(lib.match_credits().unwrap(), 0, "and nothing is written when nothing changed");
}

#[test]
fn a_track_that_comes_later_is_filed_by_what_is_already_known() {
    let lib = Library::in_memory().unwrap();
    track(&lib, 1, 1, "A");
    track(&lib, 2, 1, "B");
    track(&lib, 3, 1, "A, B");
    lib.match_credits().unwrap();
    track(&lib, 4, 2, "A, B");
    assert_eq!(filed_under(&lib, 4), ["A", "B"]);
}

#[test]
fn a_match_is_taken_back_when_a_part_is_no_longer_an_artist() {
    let lib = Library::in_memory().unwrap();
    track(&lib, 1, 1, "A");
    track(&lib, 2, 1, "B");
    track(&lib, 3, 1, "A, B");
    lib.match_credits().unwrap();
    lib.connection().execute("UPDATE tracks SET present = 0 WHERE id = 2", []).unwrap();
    assert_eq!(lib.match_credits().unwrap(), 1);
    assert_eq!(filed_under(&lib, 3), ["A, B"]);
}

#[test]
fn what_a_source_states_needs_no_match_and_is_not_undone() {
    let lib = Library::in_memory().unwrap();
    track(&lib, 1, 1, "Chris Cornell, Eleven");
    lib.state_credit("Chris Cornell, Eleven", &["Chris Cornell".into(), "Eleven".into()]).unwrap();
    assert_eq!(filed_under(&lib, 1), ["Chris Cornell", "Eleven"]);
    assert_eq!(lib.match_credits().unwrap(), 0);
    assert_eq!(filed_under(&lib, 1), ["Chris Cornell", "Eleven"]);
    // And an artist a source stated counts as one of the library's.
    track(&lib, 2, 1, "Eleven & Chris Cornell");
    assert_eq!(lib.match_credits().unwrap(), 1);
    assert_eq!(filed_under(&lib, 2), ["Chris Cornell", "Eleven"]);
    lib.state_credit("One", &["One".into()]).unwrap();
    assert_eq!(lib.credit_names("One").unwrap(), ["One"], "one artist is not a joined credit");
}

#[test]
fn a_record_credited_to_several_is_on_the_page_of_each() {
    let lib = Library::in_memory().unwrap();
    track(&lib, 1, 1, "A");
    track(&lib, 2, 2, "B");
    lib.connection().execute("INSERT INTO albums (id, title, album_artist, folder) VALUES (3, 'Both', 'A & B', 'both')", []).unwrap();
    track(&lib, 3, 3, "A & B");
    lib.match_credits().unwrap();
    let mut stmt = lib.connection().prepare("SELECT artist FROM album_artist_rollup WHERE album_id = 3 ORDER BY artist").unwrap();
    let artists: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    assert_eq!(artists, ["A", "B"]);
}
