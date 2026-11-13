//! Tracks query dispatch and opt-in phase timings shared by UI bridges.
use crate::{app, worker};
use chilpa_api::{Filter, FoundTracks};
use std::time::Instant;

/// One timestamp per in-flight query; disabled timing allocates nothing.
#[derive(Clone, Copy)]
pub struct Timing(Option<Instant>);
impl Timing {
    fn start() -> Self { Self(std::env::var_os("CHILPA_TRACK_TIMINGS").map(|_| Instant::now())) }
    pub fn next(self, phase: &str) -> Self {
        Self(self.0.map(|started| {
            eprintln!("tracks stage={phase} elapsed_us={}", started.elapsed().as_micros());
            Instant::now()
        }))
    }
    pub fn published(self, rows: i32) {
        if let Some(started) = self.0 { eprintln!("tracks stage=show elapsed_us={} rows={rows}", started.elapsed().as_micros()); }
    }
}

/// Reuses the existing reader queue; the callback returns its result to Qt.
pub fn submit(query: String, extras: Filter, mut emit: impl FnMut(FoundTracks, Timing) + Send + 'static) {
    let timing = Timing::start();
    app::with(|api| api.prepare_track_libraries());
    let timing = timing.next("prepare");
    let extras = app::with(|api| api.track_sheet_extras(&query, "", extras));
    let timing = timing.next("extras");
    worker::submit(move |reader| {
        let timing = timing.next("worker_wait");
        reader.tracks_progressive(&query, &extras, |found| emit(found, timing.next("read")));
    });
}
