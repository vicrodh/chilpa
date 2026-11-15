//! Opt-in synthetic lifecycle probe, compiled as a child of measurement.
//! Run this exact ignored test alone in the isolated API profile.
use super::*;
use crate::library_sources::Stamp;
use std::time::{Duration, Instant};

type Held = Vec<(Stamp, Arc<SourceIndex>)>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Shared,
    OwnershipControl,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::OwnershipControl => "legacy_ownership_control",
        }
    }
}

// The control uses the retained-index ownership and signature algorithm in
// this binary. It is a model of that path, not an earlier application binary.
fn query(catalog: &Catalog, mode: Mode, held: &mut Held) -> usize {
    let sources = [source()];
    let chosen = vec![("fixture".to_owned(), true, true)];
    let mut replacements = 0;
    for reader in 0..4 {
        let next = match mode {
            Mode::Shared => catalog.integrated_index(&sources, catalog.integration_epoch()),
            Mode::OwnershipControl => {
                let stamp = (catalog.integrated_signature(&chosen), chosen.clone());
                match held.get(reader) {
                    Some((previous, index)) if previous == &stamp => (stamp, index.clone()),
                    _ => (stamp, Arc::new(legacy(catalog, &sources, crate::source_catalog::now()))),
                }
            }
        };
        if let Some(previous) = held.get_mut(reader) {
            replacements += usize::from(!Arc::ptr_eq(&previous.1, &next.1));
            *previous = next;
        } else {
            held.push(next);
            replacements += 1;
        }
    }
    replacements
}

fn refreshes(readers: &[crate::Reader], held: &Held, count: usize) -> String {
    let mut hash = Sha256::new();
    for (reader, (stamp, index)) in readers.iter().zip(held) {
        for order in [crate::TrackOrder::Artist, crate::TrackOrder::Album,
            crate::TrackOrder::Title, crate::TrackOrder::Year,
            crate::TrackOrder::Duration, crate::TrackOrder::Added]
        {
            let (_, snapshot) = reader.refresh_using(&crate::Filter::default(), order,
                || true, (stamp.clone(), index.clone())).unwrap();
            assert_eq!(snapshot.ids.len(), count);
            for id in &snapshot.ids { hash.update(id.to_le_bytes()); }
            for id in &snapshot.album_ids { hash.update(id.to_le_bytes()); }
        }
    }
    format!("{:x}", hash.finalize())
}

fn phase(catalog: &Catalog, mode: Mode, started: Instant, cycle: usize,
    name: &str, baseline_bytes: i64)
{
    println!("SOURCE_INDEX_LIFECYCLE {}", json!({
        "mode":mode.label(), "phase":name, "cycle":cycle,
        "elapsedMs":started.elapsed().as_secs_f64() * 1000.0,
        "rustRequestedLiveBytes":LIVE.load(Ordering::Relaxed),
        "rustRequestedLiveDeltaBytes":LIVE.load(Ordering::Relaxed) - baseline_bytes,
        "rustRequestedPeakDeltaBytes":PEAK.load(Ordering::Relaxed) - baseline_bytes,
        "captures":catalog.index_captures.load(Ordering::Relaxed),
        "publicationSerial":catalog.lock().index_serial,
    }));
}

fn settle(catalog: &Catalog, mode: Mode, held: &mut Held) -> f64 {
    let started = Instant::now();
    query(catalog, mode, held);
    let settle_ms = started.elapsed().as_secs_f64() * 1000.0;
    let captures = catalog.index_captures.load(Ordering::Relaxed);
    let serial = catalog.lock().index_serial;
    // Three requests per persistent Reader exercise the real hot validator.
    for _ in 0..3 { assert_eq!(query(catalog, mode, held), 0); }
    assert_eq!(catalog.index_captures.load(Ordering::Relaxed), captures);
    assert_eq!(catalog.lock().index_serial, serial);
    settle_ms
}

fn bursts(catalog: &Catalog, mode: Mode, held: &mut Held, album: i64, rounds: usize) {
    let expected = digest(&held[0].1);
    for kind in ["cached_art", "image", "read_at", "image_and_read_at"] {
        let captures_before = catalog.index_captures.load(Ordering::Relaxed);
        let serial_before = catalog.lock().index_serial;
        let mut replacements = 0;
        let mut latencies = Vec::with_capacity(rounds);
        // Sequential interleaving makes every mutation visible to a request.
        // Concurrent builder/publication races are covered by the unit tests.
        for round in 0..rounds {
            {
                let mut inner = catalog.lock();
                if kind == "cached_art" {
                    let image = inner.store.cards.get(&album).unwrap().image.clone();
                    let path = if round % 2 == 0 { "synthetic-art-a" } else { "synthetic-art-b" };
                    assert!(inner.store.cards.cache_art_for_image(&image, path));
                } else {
                    let card = inner.store.cards.get_mut(&album).unwrap();
                    if kind != "read_at" {
                        card.image = if round % 2 == 0 {
                            "https://example.invalid/cover-a.jpg"
                        } else {
                            "https://example.invalid/cover-b.jpg"
                        }.into();
                        card.art.clear();
                    }
                    if kind != "image" { card.read_at += 1; }
                }
            }
            let started = Instant::now();
            replacements += query(catalog, mode, held);
            latencies.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        let settle_ms = settle(catalog, mode, held);
        assert!(held.iter().all(|(_, index)| digest(index) == expected));
        if mode == Mode::Shared {
            assert_eq!(replacements, 0, "nonmusical changes preserve the publication");
            assert_eq!(catalog.lock().index_serial, serial_before);
            let captures = catalog.index_captures.load(Ordering::Relaxed) - captures_before;
            assert_eq!(captures, if kind == "cached_art" { 0 } else { rounds as u64 });
        }
        latencies.sort_by(f64::total_cmp);
        println!("SOURCE_INDEX_LIFECYCLE_BURST {}", json!({
            "mode":mode.label(), "kind":kind, "mutations":rounds,
            "queryCalls":4 * rounds, "settlingCalls":4, "warmCalls":12,
            "captures":catalog.index_captures.load(Ordering::Relaxed) - captures_before,
            "replacedReaderIndexes":replacements,
            "publicationChanges":catalog.lock().index_serial - serial_before,
            "totalQueryMs":latencies.iter().sum::<f64>(),
            "medianQueryMs":latencies[rounds / 2],
            "p95QueryMs":latencies[(rounds * 95).div_ceil(100) - 1],
            "maxQueryMs":latencies[rounds - 1], "settleMs":settle_ms,
            "digest":expected,
        }));
    }
}

#[test]
#[ignore = "synthetic lifecycle and allocator probe; run alone with an isolated profile"]
fn persistent_readers_release_indexes_across_lifecycle_and_bursts() {
    let count = number("CHILPA_SOURCE_INDEX_TRACKS", 250);
    assert!(matches!(count, 250 | 67163));
    let seconds = number("CHILPA_SOURCE_INDEX_LIFECYCLE_SECONDS", 5);
    assert!((1..=1800).contains(&seconds));
    let interval = number("CHILPA_SOURCE_INDEX_CYCLE_SECONDS", 30);
    assert!((1..=60).contains(&interval));
    let burst_rounds = number("CHILPA_SOURCE_INDEX_BURST_ROUNDS", 3);
    assert!((1..=16).contains(&burst_rounds));
    let mode = match std::env::var("CHILPA_SOURCE_INDEX_MODE").as_deref() {
        Ok("shared") | Err(_) => Mode::Shared,
        Ok("ownership_control") => Mode::OwnershipControl,
        _ => panic!("mode must be shared or ownership_control"),
    };
    let catalog = large_fixture(count);
    let readers = (0..4).map(|_| crate::Reader::of(chilpa_library::Library::in_memory().unwrap()))
        .collect::<Vec<_>>();
    let (album, track) = {
        let mut inner = catalog.lock();
        let album = inner.store.shelves.get("fixture section:albums").unwrap().cards[0];
        let track = inner.store.shelves.get("fixture section:tracks").unwrap().tracks[0];
        inner.store.cards.get_mut(&album).unwrap().image = "https://example.invalid/lifecycle-cover.jpg".into();
        (album, track)
    };
    let mut held = Vec::with_capacity(4);
    let baseline_bytes = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline_bytes, Ordering::Relaxed);
    let started = Instant::now();
    let budget = Duration::from_secs(seconds as u64);
    let mut cycle = 0;
    let mut last_digest: String;
    loop {
        cycle += 1;
        query(&catalog, mode, &mut held);
        assert!(held.iter().all(|(_, index)| index.items.len() == count));
        if mode == Mode::Shared {
            assert!(held.iter().all(|(_, index)| Arc::ptr_eq(index, &held[0].1)));
        }
        phase(&catalog, mode, started, cycle, "indexed_before_refreshes", baseline_bytes);
        let integrated_digest = refreshes(&readers, &held, count);
        phase(&catalog, mode, started, cycle, "integrated_with_six_refreshes", baseline_bytes);
        settle(&catalog, mode, &mut held);
        if cycle == 1 { bursts(&catalog, mode, &mut held, album, burst_rounds); }

        let in_flight = held[0].1.clone();
        let previous = Arc::downgrade(&in_flight);
        {
            let mut inner = catalog.lock();
            // The legacy signature consumes composer, so both modes observe
            // this musical change in the same timestamp tick.
            inner.store.tracks.get_mut(&track).unwrap().metadata["composer"] =
                json!(if cycle % 2 == 1 { "Lifecycle writer A" } else { "Lifecycle writer B" });
        }
        assert_eq!(query(&catalog, mode, &mut held), 4);
        assert!(previous.upgrade().is_some(), "an in-flight query may retain its old index");
        drop(in_flight);
        assert!(previous.upgrade().is_none(), "persistent Readers must release the retired index");
        last_digest = digest(&held[0].1);
        assert!(held.iter().all(|(_, index)| digest(index) == last_digest));
        let expected = legacy(&catalog, &[source()], crate::source_catalog::now());
        assert_eq!(digest(&expected), last_digest);
        drop(expected);
        assert_eq!(refreshes(&readers, &held, count), integrated_digest);
        phase(&catalog, mode, started, cycle, "musical_change_with_six_refreshes", baseline_bytes);

        let retired = held.iter().map(|(_, index)| Arc::downgrade(index)).collect::<Vec<_>>();
        held.clear();
        let empty = catalog.integrated_index(&[], catalog.integration_epoch());
        assert!(empty.1.items.is_empty());
        assert!(retired.iter().all(|weak| weak.upgrade().is_none()));
        // Normal refresh invalidation at the empty-source stamp replaces all
        // six entries while keeping every Reader alive throughout the trial.
        for reader in &readers {
            for order in [crate::TrackOrder::Artist, crate::TrackOrder::Album,
                crate::TrackOrder::Title, crate::TrackOrder::Year,
                crate::TrackOrder::Duration, crate::TrackOrder::Added]
            {
                let (_, snapshot) = reader.refresh_using(&crate::Filter::default(), order,
                    || true, empty.clone()).unwrap();
                assert!(snapshot.ids.is_empty());
            }
        }
        drop(empty);
        drop(retired);
        phase(&catalog, mode, started, cycle, "opted_out_readers_alive", baseline_bytes);
        if started.elapsed() >= budget { break; }
        std::thread::sleep(Duration::from_secs(interval as u64).min(budget.saturating_sub(started.elapsed())));
        if started.elapsed() >= budget { break; }
    }
    phase(&catalog, mode, started, cycle, "finished_readers_alive", baseline_bytes);
    println!("SOURCE_INDEX_LIFECYCLE_RESULT {}", json!({
        "mode":mode.label(), "tracks":count, "readers":4, "cycles":cycle,
        "requestedSeconds":seconds, "elapsedSeconds":started.elapsed().as_secs_f64(),
        "burstRounds":burst_rounds, "digest":last_digest,
        "captures":catalog.index_captures.load(Ordering::Relaxed),
        "rustRequestedLiveDeltaBytes":LIVE.load(Ordering::Relaxed) - baseline_bytes,
        "rustRequestedPeakDeltaBytes":PEAK.load(Ordering::Relaxed) - baseline_bytes,
    }));
    drop(held);
    drop(readers);
    phase(&catalog, mode, started, cycle, "readers_dropped", baseline_bytes);
}
