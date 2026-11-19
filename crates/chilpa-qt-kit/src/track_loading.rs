//! Tracks query dispatch and opt-in phase timings shared by UI bridges.
use crate::{app, worker};
use chilpa_api::{Filter, FoundTracks};
use std::time::Instant;

#[derive(Default)]
pub struct LatestQuery {
    running: Option<u64>,
    pending: bool,
}

pub struct QueryCompletion {
    pub publish: bool,
    pub restart: bool,
}

impl LatestQuery {
    /// Starts now when idle. While a query runs, the model itself retains the
    /// latest request and this records that it must replace the pending one.
    pub fn start(&mut self, generation: u64) -> bool {
        if self.running.is_some() {
            self.pending = true;
            return false;
        }
        self.running = Some(generation);
        true
    }

    pub fn finish(&mut self, generation: u64, latest: u64) -> QueryCompletion {
        debug_assert_eq!(self.running, Some(generation));
        self.running = None;
        let publish = generation == latest;
        let restart = self.pending || !publish;
        self.pending = false;
        QueryCompletion { publish, restart }
    }
}

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
pub fn submit(query: String, extras: Filter, arrived: impl FnOnce(Option<(FoundTracks, Timing)>) + Send + 'static) {
    let timing = Timing::start();
    app::with(|api| api.prepare_track_libraries());
    let timing = timing.next("prepare");
    let extras = app::with(|api| api.track_sheet_extras(&query, "", extras));
    let timing = timing.next("extras");
    worker::submit_checked(move |reader| {
        let result = reader.ok().map(|reader| {
            let timing = timing.next("worker_wait");
            let mut found = None;
            reader.tracks_progressive(&query, &extras, |value| found = Some(value));
            (found.unwrap_or_default(), timing.next("read"))
        });
        arrived(result);
    });
}

/// Discovery keeps its scoped snapshot; Tracks retains the progressive sheet.
pub fn submit_collection(kind: String, query: String, extras: Filter, arrived: impl FnOnce(Option<(FoundTracks, Timing)>) + Send + 'static) {
    if kind == "tracks" { return submit(query, extras, arrived); }
    let timing = Timing::start();
    worker::submit_checked(move |reader| {
        let result = reader.ok().map(|reader| {
            let mut found = None;
            reader.collection_tracks(&kind, &query, &extras, |value| found = Some(value));
            (found.unwrap_or_default(), timing.next("read"))
        });
        arrived(result);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_bursts_run_the_active_query_and_the_latest_replacement() {
        let queries = std::sync::Arc::new(std::sync::Mutex::new(LatestQuery::default()));
        let mut started = Vec::new();
        if queries.lock().unwrap().start(1) { started.push(1); }
        let (entered, running) = std::sync::mpsc::channel();
        let (release, waiting) = std::sync::mpsc::channel();
        let (finished, completion) = std::sync::mpsc::channel();
        let active = queries.clone();
        let slow = std::thread::spawn(move || {
            entered.send(()).unwrap();
            waiting.recv().unwrap();
            finished.send(active.lock().unwrap().finish(1, 40)).unwrap();
        });
        running.recv().unwrap();
        for generation in 2..=40 {
            if queries.lock().unwrap().start(generation) { started.push(generation); }
        }
        release.send(()).unwrap();
        let first = completion.recv().unwrap();
        slow.join().unwrap();
        assert!(!first.publish && first.restart);
        if queries.lock().unwrap().start(40) { started.push(40); }
        let last = queries.lock().unwrap().finish(40, 40);
        assert!(last.publish && !last.restart);
        assert_eq!(started, [1, 40], "only the active query and latest replacement may run");
    }

    #[test]
    fn a_stale_result_does_not_reset_the_model() {
        let mut queries = LatestQuery::default();
        let mut resets = 0;
        assert!(queries.start(7));
        assert!(!queries.start(8));
        let stale = queries.finish(7, 8);
        if stale.publish { resets += 1; }
        assert_eq!(resets, 0);
        assert!(stale.restart);
    }
}
