//! Opt-in synthetic retention probe. Run alone with private CHILPA/XDG paths.
use super::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicI64, Ordering};
use sha2::{Digest, Sha256};

struct Counted;
static LIVE: AtomicI64 = AtomicI64::new(0);
static PEAK: AtomicI64 = AtomicI64::new(0);

fn added(bytes: i64) {
    let live = LIVE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() { added(layout.size() as i64); }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() { added(layout.size() as i64); }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout); }
        added(-(layout.size() as i64));
    }
    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, size: usize) -> *mut u8 {
        let resized = unsafe { System.realloc(pointer, old, size) };
        if !resized.is_null() { added(size as i64 - old.size() as i64); }
        resized
    }
}

#[global_allocator]
static ALLOCATOR: Counted = Counted;

fn number(name: &str, fallback: usize) -> usize {
    std::env::var(name).ok().and_then(|text| text.parse().ok()).unwrap_or(fallback)
}

fn large_fixture(count: usize) -> Arc<Catalog> {
    let catalog = Catalog::new();
    let mut inner = catalog.lock();
    let mut albums = Vec::new();
    let mut tracks = Vec::new();
    for n in 0..count {
        if n % 15 == 0 {
            albums.push(inner.store.take_item("fixture", &json!({"kind":"album", "ref":format!("album:{}", n/15),
                "title":format!("Synthetic album {}", n/15), "artist":format!("Synthetic artist {}", n%37),
                "genres":["Heavy Metal", "Symphonic Metal"], "label":"Fixture label", "year":2001}), 1).unwrap());
        }
        let album = *albums.last().unwrap();
        let id = inner.store.take_item_track("fixture", &json!({"kind":"track", "ref":format!("track:{n}"),
            "title":format!("Synthetic track {n}"), "artist":format!("Synthetic artist {}", n%37),
            "composer":"Synthetic writer", "duration_ms":180000}), None, 1).unwrap();
        inner.store.tracks.get_mut(&id).unwrap().album_id = album;
        inner.store.cards.get_mut(&album).unwrap().tracks.get_or_insert_with(Vec::new).push(id);
        tracks.push(id);
    }
    inner.store.shelves.insert("fixture section:albums".into(), Shelf { cards:albums, ..Default::default() });
    inner.store.shelves.insert("fixture section:tracks".into(), Shelf { tracks, ..Default::default() });
    drop(inner);
    catalog
}

fn mark(stage: &str) {
    println!("SOURCE_INDEX_STAGE {stage}");
    let millis = number("CHILPA_SOURCE_INDEX_PAUSE_MS", 0).min(5000);
    std::thread::sleep(std::time::Duration::from_millis(millis as u64));
    println!("SOURCE_INDEX_STAGE active");
}

fn digest(index: &SourceIndex) -> String {
    // Sorted versions keep randomized HashMap iteration out of the comparison.
    let mut versions: Vec<_> = index.versions.iter().collect();
    versions.sort_unstable_by_key(|(id, _)| **id);
    let mut hash = Sha256::new();
    for item in &index.items { hash.update(format!("{item:?}")); }
    for key in &index.keys { hash.update(format!("{key:?}")); }
    for version in versions { hash.update(format!("{version:?}")); }
    hash.update(format!("{:?}", index.sources));
    format!("{:x}", hash.finalize())
}

fn mutation_cost() -> serde_json::Value {
    use crate::source_catalog::shared_map::SharedMap;
    const ROWS: usize = 1000;
    let mut baseline = std::collections::BTreeMap::<usize, Arc<usize>>::new();
    let started = std::time::Instant::now();
    for id in 0..ROWS { baseline.insert(id, Arc::new(id)); }
    for id in 0..ROWS { *Arc::make_mut(baseline.get_mut(&id).unwrap()) += 1; }
    let baseline_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut generated = SharedMap::<usize, usize>::default();
    let started = std::time::Instant::now();
    for id in 0..ROWS { generated.insert(id, id); }
    for id in 0..ROWS { *generated.get_mut(&id).unwrap() += 1; }
    let generation_ms = started.elapsed().as_secs_f64() * 1000.0;
    for id in 0..ROWS { assert_eq!(generated.get(&id), baseline.get(&id).map(Arc::as_ref)); }
    json!({"rows":ROWS,"operations":2 * ROWS,"baselineMs":baseline_ms,"generationMs":generation_ms})
}

#[test]
#[ignore = "synthetic allocator/RSS probe, run alone in an isolated test profile"]
fn measured_source_index_and_six_refreshes() {
    let count = number("CHILPA_SOURCE_INDEX_TRACKS", 250);
    let readers_count = number("CHILPA_SOURCE_INDEX_READERS", 1);
    assert!(matches!(readers_count, 1 | 2 | 4));
    assert!(matches!(count, 250 | 67163));
    let mode = std::env::var("CHILPA_SOURCE_INDEX_MODE").unwrap_or_else(|_| "shared".into());
    assert!(matches!(mode.as_str(), "shared" | "baseline"));
    let mutation_cost = mutation_cost();
    let catalog = large_fixture(count);
    let sources = [source()];
    let capture_start = std::time::Instant::now();
    let captured = inputs::Inputs::capture(&catalog, &sources, super::super::super::now());
    let capture_ms = capture_start.elapsed().as_secs_f64() * 1000.0;
    let hash_start = std::time::Instant::now();
    std::hint::black_box(captured.fingerprint());
    let fingerprint_ms = hash_start.elapsed().as_secs_f64() * 1000.0;
    drop(captured);
    let readers = (0..readers_count).map(|_| crate::Reader::of(chilpa_library::Library::in_memory().unwrap())).collect::<Vec<_>>();
    let mut held = Vec::with_capacity(readers_count);
    mark("before_index");
    let before = LIVE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let started = std::time::Instant::now();
    for _ in &readers {
        let index = if mode == "shared" { catalog.integrated_index(&sources, catalog.integration_epoch()).1 }
            else { Arc::new(legacy(&catalog, &sources, super::super::super::now())) };
        held.push(index);
    }
    let build_ms = started.elapsed().as_secs_f64() * 1000.0;
    let index_bytes = LIVE.load(Ordering::Relaxed) - before;
    let peak_bytes = PEAK.load(Ordering::Relaxed) - before;
    let expected = digest(&held[0]);
    assert!(held.iter().all(|index| index.items.len() == count && digest(index) == expected));
    mark("index_retained");
    let warm_iterations = 3;
    let chosen = vec![("fixture".into(), true, true)];
    let serial_before = catalog.lock().index_serial;
    let before_warm = LIVE.load(Ordering::Relaxed);
    let warm_started = std::time::Instant::now();
    for _ in 0..warm_iterations {
        for retained in &held {
            let validated = if mode == "shared" {
                catalog.integrated_index(&sources, catalog.integration_epoch()).1
            } else {
                std::hint::black_box(catalog.integrated_signature(&chosen));
                retained.clone()
            };
            assert!(Arc::ptr_eq(&validated, retained), "warm validation must reuse the retained index");
            std::hint::black_box(validated);
        }
    }
    let warm_validation_ms = warm_started.elapsed().as_secs_f64() * 1000.0;
    let warm_retained_delta = LIVE.load(Ordering::Relaxed) - before_warm;
    assert_eq!(warm_retained_delta, 0, "warm validation must leave no additional Rust allocations live");
    assert_eq!(catalog.lock().index_serial, serial_before, "warm validation must not publish another generation");
    assert!(held.iter().all(|index| digest(index) == expected));
    let stamp = (1, chosen);
    let mut snapshots = Sha256::new();
    for (reader, index) in readers.iter().zip(&held) {
        for order in [crate::TrackOrder::Artist, crate::TrackOrder::Album, crate::TrackOrder::Title,
            crate::TrackOrder::Year, crate::TrackOrder::Duration, crate::TrackOrder::Added] {
            let (_, snapshot) = reader.refresh_using(&crate::Filter::default(), order, || true, (stamp.clone(), index.clone())).unwrap();
            assert_eq!(snapshot.ids.len(), count);
            for id in &snapshot.ids { snapshots.update(id.to_le_bytes()); }
            for id in &snapshot.album_ids { snapshots.update(id.to_le_bytes()); }
        }
    }
    mark("six_refreshes_retained");
    let with_refreshes = LIVE.load(Ordering::Relaxed);
    for reader in &readers { reader.forget_refreshes(); }
    let refresh_bytes = with_refreshes - LIVE.load(Ordering::Relaxed);
    assert!(refresh_bytes >= (readers_count * 6 * count * std::mem::size_of::<i64>()) as i64);
    let weak = Arc::downgrade(&held[0]);
    held.clear();
    catalog.clear_integrated_index();
    assert!(weak.upgrade().is_none(), "Readers and their refresh stamps must not retain an old index");
    mark("indexes_released_readers_alive");
    println!("SOURCE_INDEX_MEASUREMENT {}", json!({"mode":mode,"tracks":count,"readers":readers_count,"mutations":mutation_cost,
        "indexRustRequestedRetainedBytes":index_bytes,"indexRustRequestedPeakBytes":peak_bytes,
        "sixRefreshEntriesRustRequestedBytes":refresh_bytes,
        "warmValidationIterations":warm_iterations,"warmValidationCalls":warm_iterations * readers_count,
        "warmValidationMs":warm_validation_ms,"warmValidationRustRetainedDeltaBytes":warm_retained_delta,
        "captureMs":capture_ms,"fingerprintMs":fingerprint_ms,"buildMs":build_ms,"digest":expected,"snapshotDigest":format!("{:x}", snapshots.finalize())}));
}
