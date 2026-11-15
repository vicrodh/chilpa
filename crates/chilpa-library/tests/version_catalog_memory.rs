//! Opt-in comparison of catalog retention and query cost on synthetic data.
//! The allocator counts requested Rust bytes. SQLite and the system allocator's
//! own bookkeeping are outside that metric. A separate process sampler can
//! measure RSS during the explicit, opt-in observation pauses.

use chilpa_library::{Filter, Library, TrackOrder};
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

struct CountingAllocator;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);

fn added(bytes: usize) {
    let live = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(live, Ordering::Relaxed);
    CALLS.fetch_add(1, Ordering::Relaxed);
}

// SAFETY: every allocation and its unchanged layout are forwarded to System.
// The counters never allocate and do not affect ownership or returned pointers.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() { added(layout.size()); }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() { added(layout.size()); }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, layout, size) };
        if !next.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            added(size);
        }
        next
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn seed(library: &Library, tracks: usize) -> usize {
    let albums = if tracks == 250 { 25 } else { 4_654 };
    let local = library.source_id("local", "fixture").unwrap();
    let remote = library.source_id("server", "fixture").unwrap();
    library.connection().execute("UPDATE sources SET kind = 'server' WHERE id = ?1", [remote]).unwrap();
    let transaction = library.connection().unchecked_transaction().unwrap();
    {
        let mut insert_album = transaction.prepare("INSERT INTO albums(id,title,album_artist,folder,year)
            VALUES (?1,?2,?3,?4,?5)").unwrap();
        for album in 1..=albums {
            let group = (album - 1) / 2;
            insert_album.execute(params![album, format!("Record {group:05}"), format!("Artist {:03}", group % 200),
                format!("/synthetic/record-{album:05}"), 1970 + group % 50]).unwrap();
        }
        let mut insert_track = transaction.prepare("INSERT INTO tracks(id,source_id,external_id,title,artist,album_id,
            duration_ms,added_at,bit_depth,sample_rate_hz,codec,track_number,reachable,explicit)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'flac',?11,?12,?13)").unwrap();
        for index in 0..tracks {
            // Seventeen is coprime to both fixture album counts (25 and 4,654),
            // so every album receives tracks in an order unrelated to track ids.
            let album = (index * 17 % albums as usize + 1) as i64;
            let group = (album - 1) / 2;
            let id = index as i64 * 17 + 3;
            let source = if album % 2 == 0 { remote } else { local };
            let bits = if album % 2 == 0 { Some(24) } else { Some(16) };
            let explicit = if index % 11 == 0 { None } else { Some(index % 7 == 0) };
            insert_track.execute(params![id, source, format!("/synthetic/record-{album:05}/{id}.flac"),
                format!("Track {index:06}"), format!("Artist {:03}", group % 200), album,
                180_000 + index as i64 % 80_000, 1_700_000_000 + index as i64,
                bits, if album % 2 == 0 { 96_000 } else { 44_100 }, (index % 15 + 1) as i64,
                index % 19 != 0, explicit]).unwrap();
        }
    }
    transaction.commit().unwrap();
    library.update_statistics().unwrap();
    let active: i64 = library.connection().query_row("SELECT count(DISTINCT album_id) FROM tracks WHERE present=1", [], |row| row.get(0)).unwrap();
    let sources: i64 = library.connection().query_row("SELECT count(DISTINCT source_id) FROM tracks WHERE present=1", [], |row| row.get(0)).unwrap();
    assert_eq!(active, albums, "every seeded album must have eligible tracks");
    assert_eq!(sources, 2, "the fixture must exercise local and server copies");
    albums as usize
}

fn fixture_hash(path: &std::path::Path) -> String {
    use std::io::Read;
    let mut file = std::fs::File::open(path).unwrap();
    let mut buffer = [0_u8; 64 * 1024];
    let mut digest = Sha256::new();
    loop {
        let read = file.read(&mut buffer).unwrap();
        if read == 0 { break; }
        digest.update(&buffer[..read]);
    }
    format!("{:x}", digest.finalize())
}

fn fixture_name(tracks: usize) -> String { format!("version-catalog-synthetic-{tracks}.sqlite") }

fn prepare_fixture(path: &std::path::Path, tracks: usize) {
    use std::io::Write;
    assert_eq!(path.file_name().and_then(|name| name.to_str()), Some(fixture_name(tracks).as_str()));
    let marker = path.with_extension("fixture.json");
    assert!(!marker.exists(), "fixture preparation requires a fresh marker path");
    // create_new reserves a fresh file before Library opens it for seeding.
    // Preparation never replaces an existing database or marker.
    drop(std::fs::OpenOptions::new().write(true).create_new(true).open(path).unwrap());
    let writer = Library::open(path).unwrap();
    let albums = seed(&writer, tracks);
    drop(writer);
    let document = serde_json::json!({"kind":"chilpa-version-catalog-fixture-v1", "tracks":tracks,
        "seededAlbums":albums,"sha256":fixture_hash(path)});
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(marker).unwrap();
    file.write_all(document.to_string().as_bytes()).unwrap();
    file.sync_all().unwrap();
    eprintln!("VERSION_FIXTURE {document}");
}

fn validate_fixture(path: &std::path::Path, tracks: usize) -> usize {
    assert_eq!(path.file_name().and_then(|name| name.to_str()), Some(fixture_name(tracks).as_str()));
    let marker = path.with_extension("fixture.json");
    assert!(std::fs::metadata(&marker).unwrap().len() <= 1024, "invalid synthetic fixture marker");
    let document: serde_json::Value = serde_json::from_slice(&std::fs::read(marker).unwrap()).unwrap();
    assert_eq!(document["kind"], "chilpa-version-catalog-fixture-v1");
    assert_eq!(document["tracks"].as_u64(), Some(tracks as u64));
    let albums = if tracks == 250 { 25 } else { 4_654 };
    assert_eq!(document["seededAlbums"].as_u64(), Some(albums as u64));
    assert_eq!(document["sha256"].as_str(), Some(fixture_hash(path).as_str()));
    // Only a fixture with the exact marker/hash above reaches a read-only open.
    let library = Library::open_read_only(path).unwrap();
    let population: (i64, i64, i64) = library.connection().query_row(
        "SELECT count(*),count(DISTINCT t.album_id),count(DISTINCT t.source_id)
         FROM tracks t JOIN sources s ON s.id=t.source_id WHERE t.present=1 AND s.enabled=1", [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap();
    assert_eq!(population, (tracks as i64, albums as i64, 2));
    let total: i64 = library.connection().query_row("SELECT count(*) FROM tracks", [], |row| row.get(0)).unwrap();
    assert_eq!(total, tracks as i64);
    albums
}

fn number(name: &str, default: usize) -> usize {
    std::env::var(name).map(|value| value.parse().unwrap()).unwrap_or(default)
}

fn observe(phase: &str) {
    use std::io::Write;
    eprintln!("VERSION_MEMORY_PHASE {phase}");
    std::io::stderr().flush().unwrap();
    let pause = number("CHILPA_VERSION_SCALE_PAUSE_MS", 0);
    if pause != 0 { std::thread::sleep(std::time::Duration::from_millis(pause as u64)); }
    eprintln!("VERSION_MEMORY_PHASE work");
}

#[test]
#[ignore = "explicit synthetic allocation and timing comparison"]
fn measured_catalog_retention() {
    let tracks = number("CHILPA_VERSION_SCALE_TRACKS", 67_163);
    let readers = number("CHILPA_VERSION_SCALE_READERS", 1);
    assert!([250, 67_163].contains(&tracks));
    assert!([1, 2, 4].contains(&readers));
    let supplied = std::env::var_os("CHILPA_VERSION_SCALE_FIXTURE").map(std::path::PathBuf::from);
    if std::env::var("CHILPA_VERSION_SCALE_PREPARE").is_ok_and(|value| value == "1") {
        prepare_fixture(supplied.as_deref().expect("preparation requires an explicit fresh fixture path"), tracks);
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = supplied.clone().unwrap_or_else(|| directory.path().join("synthetic.sqlite"));
    let seeded_albums = if supplied.is_some() { validate_fixture(&path, tracks) } else {
        let writer = Library::open(&path).unwrap();
        let albums = seed(&writer, tracks);
        drop(writer);
        albums
    };
    let fixture_sha256 = fixture_hash(&path);
    let libraries: Vec<_> = (0..readers).map(|_| Library::open_read_only(&path).unwrap()).collect();
    let mut catalogs = Vec::with_capacity(readers);
    observe("before_catalog");
    let before = LIVE.load(Ordering::Relaxed);
    let calls_before = CALLS.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let started = Instant::now();
    for library in &libraries { catalogs.push(library.version_catalog().unwrap()); }
    let build_micros = started.elapsed().as_micros();
    let retained = LIVE.load(Ordering::Relaxed).saturating_sub(before);
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(before);
    let allocations = CALLS.load(Ordering::Relaxed) - calls_before;
    observe("catalogs_retained");
    for (library, catalog) in libraries.iter().zip(&catalogs) {
        assert!(std::sync::Arc::ptr_eq(catalog, &library.version_catalog().unwrap()));
    }
    for pair in catalogs.windows(2) { assert!(!std::sync::Arc::ptr_eq(&pair[0], &pair[1])); }
    let active_versions = catalogs[0].versions.len();
    assert!(catalogs.iter().all(|catalog| catalog.versions.len() == seeded_albums), "every Reader must include every populated edition");
    let (grouped_albums, multiple_copy_groups) = {
        let mut groups = std::collections::HashMap::<&str, usize>::new();
        for version in catalogs[0].versions.values() { *groups.entry(&version.group).or_default() += 1; }
        (groups.len(), groups.values().filter(|count| **count > 1).count())
    };
    assert_eq!(grouped_albums, seeded_albums.div_ceil(2));
    assert_eq!(multiple_copy_groups, seeded_albums / 2, "adjacent local/server editions must populate the same groups");
    assert!(catalogs[0].choices(1).len() > 1, "the public version selector must expose both copies");
    let mut digest = Sha256::new();
    let mut album_ids: Vec<_> = catalogs[0].versions.keys().copied().collect();
    album_ids.sort_unstable();
    for id in album_ids {
        digest.update(format!("{:?}", catalogs[0].versions[&id]).as_bytes());
        digest.update(catalogs[0].selected(id).to_le_bytes());
    }
    let mut timings = Vec::new();
    for order in [TrackOrder::Album, TrackOrder::TitleDescending, TrackOrder::Duration] {
        for excluded in [false, true] {
            let filter = Filter { excluded_sources: if excluded { vec!["server:fixture".into()] } else { Vec::new() }, ..Default::default() };
            let started = Instant::now();
            let grouped = libraries[0].grouped_snapshot(&filter, order).unwrap();
            let grouped_micros = started.elapsed().as_micros();
            assert_eq!(grouped.album_ids.len(), grouped_albums, "grouped rows must retain every populated album group");
            let started = Instant::now();
            let (keyed, keys) = libraries[0].keyed_snapshot(&filter, order).unwrap();
            let keyed_micros = started.elapsed().as_micros();
            assert_eq!(keyed, grouped);
            assert_eq!(keys.len(), grouped.ids.len());
            for id in &grouped.ids { digest.update(id.to_le_bytes()); }
            for id in &grouped.album_ids { digest.update(id.to_le_bytes()); }
            digest.update(grouped.totals.duration_ms.to_le_bytes());
            timings.push(serde_json::json!({"order":format!("{order:?}"),"excluded":excluded,
                "groupedMicros":grouped_micros,"keyedMicros":keyed_micros,
                "tracks":grouped.ids.len(),"albums":grouped.album_ids.len()}));
        }
    }
    let weak_catalogs: Vec<_> = catalogs.iter().map(std::sync::Arc::downgrade).collect();
    drop(catalogs);
    drop(libraries);
    assert!(weak_catalogs.iter().all(|catalog| catalog.upgrade().is_none()));
    observe("readers_dropped");
    assert_eq!(fixture_hash(&path), fixture_sha256, "all measured readers must leave the fixture unchanged");
    eprintln!("VERSION_MEMORY {}", serde_json::json!({"tracks":tracks,"readers":readers,
        "seededAlbums":seeded_albums,"activeVersions":active_versions,"fixtureSha256":fixture_sha256,"groupedAlbums":grouped_albums,"multipleCopyGroups":multiple_copy_groups,
        "metric":"requested Rust allocation bytes; excludes SQLite allocations and allocator overhead",
        "catalogRetainedBytes":retained,"catalogPeakBytes":peak,"catalogAllocationCalls":allocations,
        "buildMicros":build_micros,"snapshotDigest":format!("{:x}",digest.finalize()),"queries":timings}));
}
