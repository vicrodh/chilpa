//! The stacks and outputs as last listed, kept for the window.
//!
//! Listing asks every stack what it is and what it offers, and on Linux
//! that starts `pactl` and opens a JACK client, each with a two-second
//! patience. Asked on the window's thread, a slow sound server froze the
//! window for 10 and 18 seconds at a time (sampled 2026-10-30, under
//! `Chilpa::poll`). So one thread of its own lists, and the window reads
//! what it last found: a short lock around a pointer swap, never around the
//! asking. Until the stacks have answered once the snapshot says so, and
//! `Chilpa::device_or_first` falls back to the output that is there without
//! asking.
//!
//! What it costs, declared (ADR-018): one thread, idle unless something is
//! wanted of it; one snapshot of at most `MOST_OUTPUTS_PER_STACK` outputs
//! per stack, the previous one dropped as the next lands; one listing at
//! start, one whenever asked, and one every `REFRESH_EVERY` only while
//! somebody is looking at the list -- the audio settings while shown, the
//! cast picker while open. With nothing looking there is no periodic work.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long a listing stands before it is made again on its own, while
/// somebody is watching. A sink a server added shows up within this; a
/// card coming or going is noticed sooner, by whoever reads `generation`
/// and asks.
const REFRESH_EVERY: Duration = Duration::from_secs(5);

/// What one listing found.
///
/// Availability travels with the content: a snapshot read once says both
/// whether the stacks have answered and what they said. Read as two
/// things, the window once took the empty list of the snapshot before the
/// answer together with the "answered" of the one after it, and played to
/// an output called nothing.
#[derive(Default, PartialEq)]
pub(crate) struct Found {
    /// Whether the stacks have answered. False only in the snapshot that
    /// stands before they have, which is the one empty list that means "not
    /// yet" rather than "nothing".
    pub(crate) listed: bool,
    pub(crate) stacks: Vec<chilpa_core::backends::Stack>,
    pub(crate) outputs: Vec<chilpa_core::Output>,
    /// The stacks whose outputs were cut at the core's cap.
    pub(crate) clipped: Vec<chilpa_core::backends::StackKind>,
}

/// What surveys the stacks: the real thing, or a test's stand-in for it.
pub(crate) type Survey = Arc<dyn Fn() -> Found + Send + Sync + 'static>;

/// What the thread is told, all of it under the one lock its condition
/// waits on, so that nothing told while it is deciding to wait is lost.
#[derive(Default)]
struct Wanted {
    /// The next listing is wanted now rather than on the timer.
    asked: bool,
    /// Somebody is looking at the list, and says so when they stop: the
    /// audio settings while shown.
    watched: bool,
    /// Somebody is looking at the list until then, and says so again while
    /// they keep looking: the cast picker, from the poll. The listing is
    /// made again every `REFRESH_EVERY` while either holds, and only when
    /// asked otherwise.
    watched_until: Option<Instant>,
    stop: bool,
    /// Whether a thread is listing, or waiting to. Set before one is
    /// started and cleared by the thread on its way out, whichever way it
    /// leaves, so a second is never started beside a live one.
    alive: bool,
    /// Why there is no thread when there should be: none could be started,
    /// or the one there was stopped unexpectedly. Cleared when one starts.
    failed: Option<String>,
    /// The thread's handle. Never joined; held so a test can.
    worker: Option<JoinHandle<()>>,
}

impl Wanted {
    fn watched(&self) -> bool {
        self.watched || self.watched_until.is_some_and(|until| Instant::now() < until)
    }
}

struct Shared {
    /// Swapped whole under the lock and read by cloning the pointer, so
    /// the lock is held for nanoseconds on either side.
    found: Mutex<Arc<Found>>,
    /// Told on every publication, for whoever waits on the first.
    answered: Condvar,
    /// Grows by one each time the snapshot's content changes, and not when
    /// a listing finds what the last one found: whoever draws the list
    /// rebuilds on a new number and never on a tick.
    revision: AtomicU64,
    /// How many snapshots have been published, changed or not. Moved after
    /// the snapshot and its revision are in place.
    published: AtomicU64,
    wanted: Mutex<Wanted>,
    wake: Condvar,
}

/// A lock, poisoned or not: nothing here is left half-written by a panic.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    /// Hands the window the new snapshot. The lock covers a pointer clone
    /// and a pointer swap and nothing else: the comparison runs between
    /// them with no lock held, and the old snapshot is dropped after.
    fn publish(&self, found: Arc<Found>) {
        let previous = Arc::clone(&lock(&self.found));
        let changed = *previous != *found;
        drop(previous);
        let old = std::mem::replace(&mut *lock(&self.found), found);
        if changed {
            self.revision.fetch_add(1, Ordering::AcqRel);
        }
        self.published.fetch_add(1, Ordering::AcqRel);
        self.answered.notify_all();
        drop(old);
    }
}

/// The listing, and the thread that keeps it current.
pub(crate) struct Listing {
    shared: Arc<Shared>,
    survey: Survey,
    /// How long a listing stands before it is made again while watched.
    every: Duration,
    /// The last `generation` of the stacks seen, for telling the window
    /// the outputs changed without listing them.
    pub(crate) generation: String,
}

impl Listing {
    /// Starts listing the machine's stacks.
    pub(crate) fn new() -> Self {
        Self::with(Arc::new(|| {
            let found = chilpa_core::inventory();
            Found { listed: true, stacks: found.stacks, outputs: found.outputs, clipped: found.clipped }
        }))
    }

    /// Starts listing with `list`: at once, then whenever asked, and every
    /// `REFRESH_EVERY` while watched, on a thread of its own.
    pub(crate) fn with(list: Survey) -> Self {
        Self::with_every(list, REFRESH_EVERY)
    }

    /// `with`, refreshing every `every` while watched.
    pub(crate) fn with_every(list: Survey, every: Duration) -> Self {
        let shared = Arc::new(Shared {
            found: Mutex::new(Arc::new(Found::default())),
            answered: Condvar::new(),
            revision: AtomicU64::new(0),
            published: AtomicU64::new(0),
            wanted: Mutex::new(Wanted::default()),
            wake: Condvar::new(),
        });
        let listing = Self { shared, survey: list, every, generation: String::new() };
        listing.start(&mut lock(&listing.shared.wanted));
        listing
    }

    /// Starts the thread if there is none. Under the lock, so that two
    /// askers do not start two, and so that `alive` is set before the
    /// thread can clear it.
    fn start(&self, wanted: &mut Wanted) {
        if wanted.alive || wanted.stop {
            return;
        }
        wanted.alive = true;
        wanted.failed = None;
        let (shared, survey, every) = (Arc::clone(&self.shared), Arc::clone(&self.survey), self.every);
        match std::thread::Builder::new().name("chilpa-outputs".into()).spawn(move || run(&shared, &survey, every)) {
            Ok(handle) => {
                wanted.worker = Some(handle);
                // The thread lists at once, and that is the listing asked for.
                wanted.asked = false;
            }
            Err(e) => {
                wanted.alive = false;
                wanted.failed = Some(format!("no thread could be started to list the outputs: {e}"));
            }
        }
    }

    /// What was last found. Says itself whether the stacks have answered.
    pub(crate) fn found(&self) -> Arc<Found> {
        Arc::clone(&lock(&self.shared.found))
    }

    /// What was last found, once the stacks have answered or `patience`
    /// has run out, whichever is first. The one place anything waits on
    /// the listing: a play, where nothing can be named without it.
    pub(crate) fn first_answer(&self, patience: Duration) -> Arc<Found> {
        let deadline = Instant::now() + patience;
        let mut found = lock(&self.shared.found);
        while !found.listed {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            found = self.shared.answered.wait_timeout(found, left).unwrap_or_else(|e| e.into_inner()).0;
        }
        Arc::clone(&found)
    }

    /// Whether the stacks have answered at least once.
    pub(crate) fn listed(&self) -> bool {
        self.found().listed
    }

    /// Grows when the snapshot's content changes. Zero until the stacks
    /// have answered once.
    pub(crate) fn revision(&self) -> u64 {
        self.shared.revision.load(Ordering::Acquire)
    }

    /// Why the listing is not being kept, when it is not: no thread could
    /// be started, or the one there was stopped unexpectedly. What stands
    /// is then whatever was last found, or nothing. Asking starts over.
    pub(crate) fn failure(&self) -> Option<String> {
        lock(&self.shared.wanted).failed.clone()
    }

    /// Has the next listing made now. Asked while one is under way, the
    /// next one follows it at once; asked twice, it is made once. A thread
    /// that is gone is started again here, once, and never beside a live one.
    pub(crate) fn ask(&self) {
        let mut wanted = lock(&self.shared.wanted);
        wanted.asked = true;
        self.start(&mut wanted);
        self.shared.wake.notify_one();
    }

    /// Somebody is looking at the list, or has stopped: the audio settings
    /// as they are shown and left. While looked at, the listing is made
    /// again every `REFRESH_EVERY`.
    pub(crate) fn watch(&self, on: bool) {
        let mut wanted = lock(&self.shared.wanted);
        let begins = on && !wanted.watched();
        wanted.watched = on;
        self.start(&mut wanted);
        if begins {
            self.shared.wake.notify_one();
        }
    }

    /// Somebody is looking at the list for that long, and will say so
    /// again while they keep looking: the cast picker, from the poll. Said
    /// again, the longer stands.
    pub(crate) fn watch_for(&self, how_long: Duration) {
        let now = Instant::now();
        let mut wanted = lock(&self.shared.wanted);
        // Woken only when the watch begins: a thread already on its timer
        // reads the longer lease when the timer ends, and a picker saying
        // so four times a second does not wake it four times a second.
        let begins = !wanted.watched();
        if wanted.watched_until.is_none_or(|standing| standing < now + how_long) {
            wanted.watched_until = Some(now + how_long);
        }
        self.start(&mut wanted);
        if begins {
            self.shared.wake.notify_one();
        }
    }

    #[cfg(test)]
    fn published(&self) -> u64 {
        self.shared.published.load(Ordering::Acquire)
    }

    #[cfg(test)]
    fn take_worker(&self) -> Option<JoinHandle<()>> {
        lock(&self.shared.wanted).worker.take()
    }
}

impl Drop for Listing {
    fn drop(&mut self) {
        let mut wanted = lock(&self.shared.wanted);
        wanted.stop = true;
        // Not joined: a listing under way may be waiting on a server for
        // seconds, and the window's shutdown is not where to wait for it.
        // The thread holds its own share of the state and leaves on its own
        // the moment it is back.
        wanted.worker.take();
        drop(wanted);
        self.shared.wake.notify_all();
    }
}

/// Clears `alive` when the thread leaves, however it leaves: a panic in a
/// stack's listing is a thread gone, and said so, not a snapshot that
/// stands forever looking fresh.
struct Leaving<'a>(&'a Shared);

impl Drop for Leaving<'_> {
    fn drop(&mut self) {
        let mut wanted = lock(&self.0.wanted);
        wanted.alive = false;
        if std::thread::panicking() {
            wanted.failed = Some("the thread listing the outputs stopped unexpectedly".to_string());
        }
    }
}

fn run(shared: &Shared, list: &Survey, every: Duration) {
    let _leaving = Leaving(shared);
    while !lock(&shared.wanted).stop {
        shared.publish(Arc::new(list()));
        let deadline = Instant::now() + every;
        let mut wanted = lock(&shared.wanted);
        while !wanted.stop && !wanted.asked {
            if wanted.watched() {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                wanted = shared.wake.wait_timeout(wanted, left).unwrap_or_else(|e| e.into_inner()).0;
            } else {
                wanted = shared.wake.wait(wanted).unwrap_or_else(|e| e.into_inner());
            }
        }
        wanted.asked = false;
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

    pub(crate) fn answered(outputs: Vec<chilpa_core::Output>) -> Found {
        Found { listed: true, stacks: Vec::new(), outputs, clipped: Vec::new() }
    }

    /// A survey that counts its calls and waits at the gate on each one.
    fn counted(gate: &Arc<Gate>, calls: &Arc<AtomicUsize>) -> Survey {
        let (gate, calls) = (Arc::clone(gate), Arc::clone(calls));
        Arc::new(move || {
            calls.fetch_add(1, Ordering::SeqCst);
            gate.wait();
            answered(vec![output("alsa:hw:1,0", "Card")])
        })
    }

    /// A short period, so that a timer that runs when it should not shows
    /// within a test's patience instead of five seconds later.
    const SHORT: Duration = Duration::from_millis(20);

    /// A listing made every `SHORT` while watched, counting its publications
    /// through `published`.
    fn quick() -> Listing {
        Listing::with_every(Arc::new(|| answered(Vec::new())), SHORT)
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
        assert!(wait_until(|| listing.published() == 1, Duration::from_secs(5)), "the stacks answered and nothing was kept");
        assert!(listing.listed());
        assert_eq!(listing.found().outputs.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(), ["alsa:hw:1,0"]);
    }

    #[test]
    fn availability_and_outputs_travel_in_one_snapshot() {
        // The window used to read the list and "answered" separately, and
        // between the two reads the stacks answered: an empty list said
        // "answered", and the first output was called nothing. Now a
        // snapshot taken before the answer says it is not one, whole, and a
        // snapshot taken after carries the outputs, whole: there is no
        // pair of reads to disagree.
        let (gate, calls) = (Arc::new(Gate::default()), Arc::new(AtomicUsize::new(0)));
        let listing = Listing::with(counted(&gate, &calls));
        assert!(wait_until(|| calls.load(Ordering::SeqCst) == 1, Duration::from_secs(5)));
        let before = listing.found();
        gate.open();
        assert!(wait_until(|| listing.published() == 1, Duration::from_secs(5)));
        let after = listing.found();
        assert!(!before.listed && before.outputs.is_empty(), "the snapshot before the answer still says so");
        assert!(after.listed && !after.outputs.is_empty(), "the snapshot after carries what was found");
        assert_eq!(listing.revision(), 1, "the first answer is the first change");
    }

    #[test]
    fn a_wait_for_the_first_answer_ends_when_it_lands_or_when_patience_does() {
        let (gate, calls) = (Arc::new(Gate::default()), Arc::new(AtomicUsize::new(0)));
        let listing = Listing::with(counted(&gate, &calls));
        let asked = Instant::now();
        let early = listing.first_answer(Duration::from_millis(100));
        assert!(!early.listed, "patience ran out before the stacks answered");
        assert!(asked.elapsed() >= Duration::from_millis(100) && asked.elapsed() < Duration::from_secs(2), "{:?}", asked.elapsed());
        gate.open();
        let late = listing.first_answer(Duration::from_secs(5));
        assert!(late.listed && !late.outputs.is_empty());
        let again = Instant::now();
        assert!(listing.first_answer(Duration::from_secs(5)).listed);
        assert!(again.elapsed() < Duration::from_millis(100), "an answer already there is not waited for");
    }

    #[test]
    fn asking_lists_again_at_once() {
        let (gate, calls) = (Arc::new(Gate::default()), Arc::new(AtomicUsize::new(0)));
        gate.open();
        let listing = Listing::with(counted(&gate, &calls));
        assert!(wait_until(|| listing.published() == 1, Duration::from_secs(5)));
        // The timer is seconds away, and nobody is watching for it anyway.
        listing.ask();
        assert!(wait_until(|| listing.published() == 2, Duration::from_secs(2)), "asking did not list again");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn nothing_lists_on_its_own_while_nobody_watches() {
        // Twenty milliseconds a period: a timer that ran unwatched would
        // have published a dozen more by the time this looks.
        let listing = quick();
        assert!(wait_until(|| listing.published() == 1, Duration::from_secs(5)));
        std::thread::sleep(SHORT * 15);
        assert_eq!(listing.published(), 1, "it listed on its own while nobody was watching");
    }

    #[test]
    fn the_revision_moves_only_when_what_was_found_does() {
        let (gate, calls) = (Arc::new(Gate::default()), Arc::new(AtomicUsize::new(0)));
        gate.open();
        let listing = Listing::with(counted(&gate, &calls));
        assert!(wait_until(|| listing.published() == 1, Duration::from_secs(5)));
        assert_eq!(listing.revision(), 1);
        listing.ask();
        assert!(wait_until(|| listing.published() == 2, Duration::from_secs(2)));
        assert_eq!(listing.revision(), 1, "the same list again is not a change for the window to redraw");
    }

    #[test]
    fn watching_lists_on_the_timer_while_the_page_is_shown_and_stops_when_it_is_left() {
        let listing = quick();
        assert!(wait_until(|| listing.published() == 1, Duration::from_secs(5)));
        listing.watch(true);
        assert!(wait_until(|| listing.published() >= 5, Duration::from_secs(2)), "watching did not list again on the timer");
        listing.watch(false);
        // A listing the timer started just before the page was left lands.
        std::thread::sleep(SHORT * 5);
        let settled = listing.published();
        std::thread::sleep(SHORT * 15);
        assert_eq!(listing.published(), settled, "it kept listing after the page was left");
    }

    #[test]
    fn a_short_watch_lists_on_the_timer_and_ends_on_its_own() {
        let listing = quick();
        assert!(wait_until(|| listing.published() == 1, Duration::from_secs(5)));
        listing.watch_for(SHORT * 10);
        assert!(wait_until(|| listing.published() >= 4, Duration::from_secs(2)), "the lease did not list on the timer");
        assert!(wait_until(|| !lock(&listing.shared.wanted).watched(), Duration::from_secs(2)));
        std::thread::sleep(SHORT * 5);
        let settled = listing.published();
        std::thread::sleep(SHORT * 15);
        assert_eq!(listing.published(), settled, "it kept listing after the lease ended");
    }

    #[test]
    fn dropping_the_listing_stops_its_thread_at_once() {
        let (gate, calls) = (Arc::new(Gate::default()), Arc::new(AtomicUsize::new(0)));
        gate.open();
        let listing = Listing::with(counted(&gate, &calls));
        assert!(wait_until(|| listing.published() == 1, Duration::from_secs(5)));
        let worker = listing.take_worker().expect("the thread started");
        // The thread is waiting with no timer: a wakeup lost here is a
        // thread that never leaves, not one that leaves five seconds late.
        let dropped = Instant::now();
        drop(listing);
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || done.send(worker.join().is_ok()));
        assert_eq!(finished.recv_timeout(Duration::from_secs(5)), Ok(true), "the thread did not leave after the drop");
        assert!(dropped.elapsed() < Duration::from_secs(1), "it left late: {:?}", dropped.elapsed());
        assert_eq!(calls.load(Ordering::SeqCst), 1, "it listed again on its way out");
    }

    #[test]
    fn a_listing_under_way_at_the_drop_finishes_and_the_thread_leaves() {
        let (gate, calls) = (Arc::new(Gate::default()), Arc::new(AtomicUsize::new(0)));
        let listing = Listing::with(counted(&gate, &calls));
        assert!(wait_until(|| calls.load(Ordering::SeqCst) == 1, Duration::from_secs(5)));
        let worker = listing.take_worker().expect("the thread started");
        drop(listing);
        gate.open();
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || done.send(worker.join().is_ok()));
        assert_eq!(finished.recv_timeout(Duration::from_secs(5)), Ok(true));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_thread_that_stops_unexpectedly_is_said_so_and_started_again_when_asked() {
        // A stack's listing panics: the thread is gone, and without this the
        // empty snapshot stood forever looking like a machine with nothing.
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        let listing = Listing::with_every(
            Arc::new(move || {
                if counted.fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("a stack could not be asked");
                }
                answered(vec![output("alsa:hw:1,0", "Card")])
            }),
            SHORT,
        );
        assert!(wait_until(|| listing.failure().is_some(), Duration::from_secs(5)), "the thread's end went unsaid");
        assert!(!listing.listed());
        assert!(!lock(&listing.shared.wanted).alive);
        listing.ask();
        assert!(wait_until(|| listing.published() == 1, Duration::from_secs(5)), "asking did not start over");
        assert!(listing.listed());
        assert_eq!(listing.failure(), None, "the failure stayed after the thread was back");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        // Asked again with the thread alive, no second one is started: one
        // more listing, and then nothing, since nobody is watching.
        listing.ask();
        assert!(wait_until(|| listing.published() == 2, Duration::from_secs(2)));
        std::thread::sleep(SHORT * 10);
        assert_eq!(listing.published(), 2);
    }
}
