//! The stacks and outputs as last listed, kept for the window.
//!
//! Listing asks every stack what it is and what it offers, and on Linux
//! that starts `pactl` and opens a JACK client, each with a two-second
//! patience. Asked on the window's thread, a slow sound server froze the
//! window for 10 and 18 seconds at a time (sampled 2026-10-30, under
//! `Chilpa::poll`). So one thread of its own lists, and the window reads
//! what it last found: a short lock around a pointer swap, never around the
//! asking. Until the stacks have answered once the lists are empty and
//! `Chilpa::device_or_first` falls back to the output that is there without
//! asking.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long a listing stands before it is made again on its own. A sink a
/// server added shows up within this; a card coming or going is noticed
/// sooner, by whoever reads `generation` and asks.
const REFRESH_EVERY: Duration = Duration::from_secs(5);

/// More outputs than this are not kept. A machine has a handful, and a
/// server with a runaway list of virtual sinks does not get to grow the
/// window's memory with it.
const MOST_OUTPUTS: usize = 256;

/// What one listing found.
#[derive(Default)]
pub(crate) struct Found {
    pub(crate) stacks: Vec<chilpa_core::backends::Stack>,
    pub(crate) outputs: Vec<chilpa_core::Output>,
}

/// What surveys the stacks: the real thing, or a test's stand-in for it.
pub(crate) type Survey = Box<dyn Fn() -> Found + Send + 'static>;

struct Shared {
    /// Swapped whole under the lock and read by cloning the pointer, so
    /// the lock is held for nanoseconds on either side.
    found: Mutex<Arc<Found>>,
    /// Whether the stacks have answered at least once.
    listed: AtomicBool,
    /// Set by whoever wants the next listing now rather than on the timer.
    asked: Mutex<bool>,
    wake: Condvar,
    stop: AtomicBool,
}

/// The listing, and the thread that keeps it current.
pub(crate) struct Listing {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    /// The last `generation` of the stacks seen, for telling the window
    /// the outputs changed without listing them.
    pub(crate) generation: String,
}

impl Listing {
    /// Starts listing the machine's stacks.
    pub(crate) fn new() -> Self {
        Self::with(Box::new(|| {
            let (stacks, outputs) = chilpa_core::inventory();
            Found { stacks, outputs: outputs.into_iter().take(MOST_OUTPUTS).collect() }
        }))
    }

    /// Starts listing with `list`: at once, then every `REFRESH_EVERY` and
    /// whenever asked, on a thread of its own.
    pub(crate) fn with(list: Survey) -> Self {
        let shared = Arc::new(Shared {
            found: Mutex::new(Arc::new(Found::default())),
            listed: AtomicBool::new(false),
            asked: Mutex::new(false),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
        });
        let on_thread = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name("chilpa-outputs".into())
            .spawn(move || run(&on_thread, &list))
            .ok();
        Self { shared, worker, generation: String::new() }
    }

    /// What was last found. Empty until the stacks have answered once.
    pub(crate) fn found(&self) -> Arc<Found> {
        Arc::clone(&self.shared.found.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Whether the stacks have answered at least once.
    pub(crate) fn listed(&self) -> bool {
        self.shared.listed.load(Ordering::Acquire)
    }

    /// Has the next listing made now. Asked while one is under way, the
    /// next one follows it at once; asked twice, it is made once.
    pub(crate) fn ask(&self) {
        *self.shared.asked.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.shared.wake.notify_one();
    }
}

impl Drop for Listing {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.wake.notify_one();
        // Not joined: a listing under way may be waiting on a server for
        // seconds, and the window's shutdown is not where to wait for it.
        // The thread holds its own share of the state and leaves on its own
        // the moment it is back.
        self.worker.take();
    }
}

fn run(shared: &Shared, list: &Survey) {
    loop {
        if shared.stop.load(Ordering::Acquire) {
            return;
        }
        let found = Arc::new(list());
        *shared.found.lock().unwrap_or_else(|e| e.into_inner()) = found;
        shared.listed.store(true, Ordering::Release);
        let mut asked = shared.asked.lock().unwrap_or_else(|e| e.into_inner());
        let deadline = Instant::now() + REFRESH_EVERY;
        while !*asked && !shared.stop.load(Ordering::Acquire) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            asked = shared.wake.wait_timeout(asked, left).unwrap_or_else(|e| e.into_inner()).0;
        }
        *asked = false;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// Holds a listing until opened, so a test can look while it is under way.
    #[derive(Default)]
    pub(crate) struct Gate {
        open: Mutex<bool>,
        opened: Condvar,
    }

    impl Gate {
        pub(crate) fn wait(&self) {
            let mut open = self.open.lock().unwrap();
            while !*open {
                open = self.opened.wait(open).unwrap();
            }
        }
        pub(crate) fn open(&self) {
            *self.open.lock().unwrap() = true;
            self.opened.notify_all();
        }
    }

    pub(crate) fn wait_until(what: impl Fn() -> bool, most: Duration) -> bool {
        let deadline = Instant::now() + most;
        while Instant::now() < deadline {
            if what() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        what()
    }

    pub(crate) fn output(id: &str, label: &str) -> chilpa_core::Output {
        chilpa_core::Output { id: id.into(), label: label.into(), detail: String::new(), bit_perfect: true }
    }

    /// A survey that counts its calls and waits at the gate on each one.
    fn counted(gate: &Arc<Gate>, calls: &Arc<AtomicUsize>) -> Survey {
        let (gate, calls) = (Arc::clone(gate), Arc::clone(calls));
        Box::new(move || {
            calls.fetch_add(1, Ordering::SeqCst);
            gate.wait();
            Found { stacks: Vec::new(), outputs: vec![output("alsa:hw:1,0", "Card")] }
        })
    }

    #[test]
    fn reading_returns_at_once_while_the_stacks_are_still_being_asked() {
        let (gate, calls) = (Arc::new(Gate::default()), Arc::new(AtomicUsize::new(0)));
        let listing = Listing::with(counted(&gate, &calls));
        assert!(wait_until(|| calls.load(Ordering::SeqCst) == 1, Duration::from_secs(5)), "the listing did not start");
        let before = Instant::now();
        assert!(listing.found().outputs.is_empty(), "nothing is found until the stacks answer");
        assert!(!listing.listed());
        assert!(before.elapsed() < Duration::from_secs(1), "reading waited on the listing");
        gate.open();
        assert!(wait_until(|| listing.listed(), Duration::from_secs(5)), "the stacks answered and nothing was kept");
        assert_eq!(listing.found().outputs.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(), ["alsa:hw:1,0"]);
    }

    #[test]
    fn asking_lists_again_without_waiting_for_the_timer() {
        let (gate, calls) = (Arc::new(Gate::default()), Arc::new(AtomicUsize::new(0)));
        gate.open();
        let listing = Listing::with(counted(&gate, &calls));
        assert!(wait_until(|| listing.listed(), Duration::from_secs(5)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // The timer is seconds away; without the ask this would stay at one.
        listing.ask();
        assert!(wait_until(|| calls.load(Ordering::SeqCst) == 2, Duration::from_secs(2)), "asking did not list again");
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(calls.load(Ordering::SeqCst), 2, "nothing lists on its own between the timer's ticks");
    }

    #[test]
    fn dropping_the_listing_stops_its_thread() {
        let (gate, calls) = (Arc::new(Gate::default()), Arc::new(AtomicUsize::new(0)));
        gate.open();
        let mut listing = Listing::with(counted(&gate, &calls));
        assert!(wait_until(|| listing.listed(), Duration::from_secs(5)));
        let worker = listing.worker.take().expect("the thread started");
        drop(listing);
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || done.send(worker.join().is_ok()));
        assert_eq!(finished.recv_timeout(Duration::from_secs(5)), Ok(true), "the thread did not leave after the drop");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "it listed again on its way out");
    }

    #[test]
    fn a_listing_under_way_at_the_drop_finishes_and_the_thread_leaves() {
        let (gate, calls) = (Arc::new(Gate::default()), Arc::new(AtomicUsize::new(0)));
        let mut listing = Listing::with(counted(&gate, &calls));
        assert!(wait_until(|| calls.load(Ordering::SeqCst) == 1, Duration::from_secs(5)));
        let worker = listing.worker.take().expect("the thread started");
        drop(listing);
        gate.open();
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || done.send(worker.join().is_ok()));
        assert_eq!(finished.recv_timeout(Duration::from_secs(5)), Ok(true));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
