//! Looking a source up does not wait for whoever is writing.
//!
//! The window asks for the local source's id at every track start. The
//! lookup used to begin with a write, so while a scan or a sync held the
//! database it waited as long as the lock allows, on the window's thread:
//! five seconds for a track of the library, twice that for a loose file.

use chilpa_library::Library;
use std::time::{Duration, Instant};

#[test]
fn a_known_source_is_found_while_another_connection_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let local = library.source_id("local", "default").unwrap();

    // A scan or a sync, in the middle of its writing.
    let writer = rusqlite::Connection::open(&path).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE;").unwrap();

    let asked = Instant::now();
    assert_eq!(library.source_id("local", "default").unwrap(), local);
    let took = asked.elapsed();
    assert!(took < Duration::from_secs(1), "it waited {took:?} for a writer it did not need");

    writer.execute_batch("ROLLBACK").unwrap();
    // A source nobody had seen is still added, once the way is clear.
    let other = library.source_id("jellyfin", "home").unwrap();
    assert_ne!(other, local);
    assert_eq!(library.source_id("jellyfin", "home").unwrap(), other);
}
