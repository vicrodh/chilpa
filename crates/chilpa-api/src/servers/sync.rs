//! Mirroring a server into the library, on a thread of its own (ADR-017 §5).
//!
//! One server at a time, in the order they were asked for. Every page lands
//! whole or not at all; what the server no longer has is taken out only
//! when the whole catalogue was read and the count adds up. A sync that is
//! cut off, or that saw fewer tracks than the server announced, leaves the
//! library as it was.
//!
//! The server's own playlists are read on this thread too (`reading`):
//! after its catalogue, whether the sync finished or ended partial, and
//! alone when the listener asks for them again.

use super::keys::Keys;
use super::net::{self, Network};
use super::reading;
use chilpa_servers::{Failure, Net};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Cover size asked for: the window's largest.
const COVER: u32 = 1200;

/// What a sync read is written when this many tracks wait, or when this
/// long passed since it last wrote. A media server's page is one folder,
/// often one record: written page by page, every few tracks changed the
/// library under the window, which read it again each time.
const WRITE_TRACKS: usize = 500;
const WRITE_EVERY: Duration = Duration::from_secs(5);

/// Pages in a row that bring no track the sync had not seen. Past this the
/// server is listing the same tracks again and again: the sync stops there,
/// keeps what it read and takes nothing out.
const STALE_PAGES: u64 = 5_000;

/// How often a running sync says in the log where it is.
const LOG_EVERY: Duration = Duration::from_secs(30);

/// A sync begun with no word from its thread for this long is stuck: every
/// request it makes has a time limit well below this, and a sync that is
/// still reading says so every `BEAT`. Asked again, it starts over on a new
/// thread.
const STUCK: Duration = Duration::from_secs(180);

/// How often a running sync says where it is when it writes nothing.
const BEAT: Duration = Duration::from_secs(2);

/// What one run of cover fetches may take: covers and time. The rest wait
/// behind whatever else was asked of the thread, and go on after it.
const COVERS_PER_RUN: usize = 200;
const COVERS_TIME: Duration = Duration::from_secs(60);

/// A thread that is gone or stuck is started again at most this often.
const REPLACE_EVERY: Duration = Duration::from_secs(10);

/// Where a sync is, as its thread says it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Reading {
    /// Tracks read so far, and pages of the server's catalogue.
    pub(crate) done: u64,
    pub(crate) pages: u64,
    /// What was read is in the library now.
    pub(crate) wrote: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Event {
    /// Its thread began reading the server's catalogue.
    Started { source: i64 },
    /// Where the sync is.
    Progress { source: i64, reading: Reading },
    /// The catalogue is in. `gone` tracks were taken out.
    Finished { source: i64, gone: usize },
    /// Covers of the server's records came in, or are due: `next` says
    /// where the rest start, and they are fetched after what is queued.
    Covers { source: i64, next: Option<i64> },
    Failed { source: i64, why: String, unreachable: bool },
    /// The server's playlists were read: how many came in, and how many of
    /// their entries name a track the library does not hold.
    Playlists { source: i64, kept: usize, left_out: usize },
    /// Its playlists could not be read. The ones kept stay as they were.
    PlaylistsFailed { source: i64, why: String },
}

/// A sync asked for, as Settings shows it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Running {
    pub(crate) done: u64,
    pub(crate) pages: u64,
    /// When its thread began it; `None` while it waits its turn.
    pub(crate) started: Option<Instant>,
    /// When its thread last said where it is.
    pub(crate) beat: Instant,
}

impl Running {
    pub(crate) fn asked() -> Self {
        Self { done: 0, pages: 0, started: None, beat: Instant::now() }
    }

    /// Begun, and no word from its thread for `STUCK`.
    pub(crate) fn stuck(&self) -> bool {
        self.started.is_some() && self.beat.elapsed() >= STUCK
    }
}

/// What the thread is asked to do for one server.
enum Order {
    /// Its catalogue, then its playlists.
    Catalogue(i64),
    /// Its playlists alone, and its catalogue after them when one of them
    /// names a track the library does not hold.
    Playlists(i64),
    /// One run of its covers, from the record after this one.
    Covers(i64, i64),
}

/// Whether the playlists are read after this end of a catalogue: after
/// one that finished and after one that ended partial, not after a server
/// that stopped answering.
pub(crate) fn reads_playlists_after(event: &Event) -> bool {
    !matches!(event, Event::Failed { unreachable: true, .. })
}

/// Whether a reading of the playlists alone asks for the catalogue: one of
/// them names a track the library does not hold, which a sync may bring.
/// The reading that follows that sync asks for nothing: what is still not
/// there is left out.
pub(crate) fn asks_for_catalogue(event: &Event) -> bool {
    matches!(event, Event::Playlists { left_out, .. } if *left_out > 0)
}

/// The library a sync's thread writes, opened again for each order.
type Open = Arc<dyn Fn() -> Option<chilpa_library::Library> + Send + Sync>;

/// What a sync's thread works with.
#[derive(Clone)]
struct Tools {
    net: Arc<dyn Net>,
    open: Open,
    keys: Arc<Keys>,
    /// Held while playlists are read, so a change of one never lands in
    /// the middle of a reading.
    gate: Arc<Mutex<()>>,
}

pub(crate) struct Syncer {
    tools: Tools,
    orders: Sender<Order>,
    events: Receiver<Event>,
    /// Asked to stop: the source being removed. Each thread has its own.
    cancel: Arc<Mutex<HashSet<i64>>>,
    pub(crate) running: HashMap<i64, Running>,
    queued: HashSet<i64>,
    /// The servers whose playlists alone are queued or being read.
    reading: HashSet<i64>,
    /// Servers whose sync was stopped to start again: asked again when the
    /// stopped one ends.
    again: HashSet<i64>,
    /// Servers with a run of covers queued or running, and where it starts.
    covering: HashMap<i64, i64>,
    /// When a thread last took the place of one gone or stuck.
    replaced_at: Option<Instant>,
}

impl Syncer {
    pub(crate) fn start(keys: Arc<Keys>, gate: Arc<Mutex<()>>) -> Self {
        let open: Open = Arc::new(open_library);
        Self::with(Tools { net: Arc::new(Network), open, keys, gate })
    }

    fn with(tools: Tools) -> Self {
        let cancel: Arc<Mutex<HashSet<i64>>> = Arc::default();
        let (orders, events) = worker(tools.clone(), Arc::clone(&cancel));
        Self {
            tools,
            orders,
            events,
            cancel,
            running: HashMap::new(),
            queued: HashSet::new(),
            reading: HashSet::new(),
            again: HashSet::new(),
            covering: HashMap::new(),
            replaced_at: None,
        }
    }

    /// Reads the server again from the start: the one running is stopped at
    /// its next page and this one follows it. Asked while one runs, `ask`
    /// would be lost, and the stopped sync would be the last.
    pub(crate) fn restart(&mut self, source: i64) {
        if self.queued.contains(&source) {
            self.cancel(source);
            self.again.insert(source);
        } else {
            self.ask(source);
        }
    }

    /// Queues a sync of that server. One already running goes on, and the
    /// log says so; one stuck starts over on a new thread.
    pub(crate) fn ask(&mut self, source: i64) {
        let running = self.running.get(&source).map(|r| (r.stuck(), r.beat.elapsed().as_secs(), r.done, r.pages));
        if let Some((stuck, quiet, done, pages)) = running {
            if stuck {
                self.replace_thread(&format!("sync of source {source} asked again with no word from it for {quiet}s"));
            } else {
                eprintln!("chilpa-servers: sync of source {source} asked again while it runs ({done} tracks, {pages} pages); it goes on");
            }
            return;
        }
        self.queued.insert(source);
        self.running.insert(source, Running::asked());
        if self.orders.send(Order::Catalogue(source)).is_err() {
            self.replace_thread("the sync thread is gone");
        }
    }

    /// Queues a reading of the server's playlists, unless a sync, which
    /// reads them too, or another reading is queued or running.
    pub(crate) fn ask_playlists(&mut self, source: i64) {
        if !self.queued.contains(&source) && self.reading.insert(source) && self.orders.send(Order::Playlists(source)).is_err() {
            self.replace_thread("the sync thread is gone");
        }
    }

    /// Whether anything of that server is queued or running on the thread:
    /// its catalogue, its playlists or its covers.
    pub(crate) fn has(&self, source: i64) -> bool {
        self.queued.contains(&source) || self.reading.contains(&source) || self.covering.contains_key(&source)
    }

    /// Stops the sync of a server that is being removed, at the next page.
    pub(crate) fn cancel(&mut self, source: i64) {
        self.again.remove(&source);
        if !self.queued.contains(&source) && !self.reading.contains(&source) && !self.covering.contains_key(&source) {
            return;
        }
        if let Ok(mut s) = self.cancel.lock() {
            s.insert(source);
        }
    }

    /// A new thread in place of one that is gone or stuck, handed again
    /// everything asked of the old one. The old one, if it ever moves again,
    /// stops at its next page, and nothing it says is read.
    fn replace_thread(&mut self, why: &str) {
        if self.replaced_at.is_some_and(|at| at.elapsed() < REPLACE_EVERY) {
            return;
        }
        self.replaced_at = Some(Instant::now());
        eprintln!(
            "chilpa-servers: {why}; a new sync thread takes {} syncs and {} readings",
            self.queued.len(),
            self.reading.len()
        );
        let old = std::mem::take(&mut self.cancel);
        if let Ok(mut stop) = old.lock() {
            stop.extend(self.queued.iter().chain(self.reading.iter()).chain(self.covering.keys()).copied());
        }
        let (orders, events) = worker(self.tools.clone(), Arc::clone(&self.cancel));
        self.orders = orders;
        self.events = events;
        self.again.clear();
        for source in self.queued.clone() {
            self.running.insert(source, Running::asked());
            let _ = self.orders.send(Order::Catalogue(source));
        }
        for source in self.reading.clone() {
            let _ = self.orders.send(Order::Playlists(source));
        }
        for (source, after) in self.covering.clone() {
            let _ = self.orders.send(Order::Covers(source, after));
        }
    }

    pub(crate) fn poll(&mut self) -> Vec<Event> {
        let mut events: Vec<Event> = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(event) => events.push(event),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                // Its thread ended: whatever was asked of it is asked again.
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if !(self.queued.is_empty() && self.reading.is_empty() && self.covering.is_empty()) {
                        self.replace_thread("the sync thread is gone");
                    }
                    break;
                }
            }
        }
        for event in &events {
            match event {
                Event::Started { source } => {
                    let running = self.running.entry(*source).or_insert_with(Running::asked);
                    running.started = Some(Instant::now());
                    running.beat = Instant::now();
                }
                Event::Progress { source, reading } => {
                    if let Some(running) = self.running.get_mut(source) {
                        running.done = reading.done;
                        running.pages = reading.pages;
                        running.beat = Instant::now();
                    }
                }
                Event::Finished { source, .. } | Event::Failed { source, .. } => {
                    self.running.remove(source);
                    self.queued.remove(source);
                    if self.again.remove(source) {
                        self.ask(*source);
                    }
                }
                Event::Playlists { source, .. } | Event::PlaylistsFailed { source, .. } => {
                    self.reading.remove(source);
                }
                // Covers due while a run of them is on its way: that run
                // covers them.
                Event::Covers { source, next: Some(0) } if self.covering.contains_key(source) => {}
                // The rest of its covers wait behind what is queued now.
                Event::Covers { source, next: Some(after) } => {
                    self.covering.insert(*source, *after);
                    if self.orders.send(Order::Covers(*source, *after)).is_err() {
                        self.replace_thread("the sync thread is gone");
                    }
                }
                Event::Covers { source, next: None } => {
                    self.covering.remove(source);
                }
            }
        }
        events
    }
}

/// The library, opened for one of the servers' threads. The window opened
/// it first and brought it up to date: this one does not migrate it, and a
/// library another writer holds right now is opened again a little later
/// instead of failing the sync (`background::retried`).
pub(crate) fn open_library() -> Option<chilpa_library::Library> {
    let path = chilpa_library::Library::default_path();
    super::background::retried("opening the library for a server's thread", &super::background::WAITS, || {
        chilpa_library::Library::open_existing(&path)
    })
    .ok()
}

/// Starts a thread that serves orders until its sender is dropped. If it
/// cannot start, its channels are closed already, and the next `poll` or
/// `ask` tries again.
fn worker(tools: Tools, stop: Arc<Mutex<HashSet<i64>>>) -> (Sender<Order>, Receiver<Event>) {
    let (orders, inbox) = channel::<Order>();
    let (outbox, events) = channel::<Event>();
    let spawned = std::thread::Builder::new().name("chilpa-server-sync".into()).spawn(move || {
        while let Ok(order) = inbox.recv() {
            let (source, owes) = match order {
                Order::Catalogue(source) => (source, OWE_CATALOGUE),
                Order::Playlists(source) => (source, OWE_PLAYLISTS),
                Order::Covers(source, _) => (source, OWE_COVERS),
            };
            // A panic still ends the order, once: the server would show as
            // syncing for as long as the window stays open, and the thread
            // would take no other order.
            let owed = AtomicU8::new(owes);
            let served = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| serve(order, &tools, &stop, &outbox, &owed)));
            if served.is_err() {
                eprintln!("chilpa-servers: sync of source {source} panicked; nothing was taken out");
                let why = chilpa_i18n::t("the sync stopped on an error; nothing was taken out, sync again");
                let end = match owed.load(Ordering::Relaxed) {
                    OWE_CATALOGUE => Some(Event::Failed { source, why, unreachable: false }),
                    OWE_PLAYLISTS => Some(Event::PlaylistsFailed { source, why }),
                    OWE_COVERS => Some(Event::Covers { source, next: None }),
                    _ => None,
                };
                if let Some(end) = end {
                    let _ = outbox.send(end);
                }
            }
            if let Ok(mut s) = stop.lock() {
                s.remove(&source);
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("chilpa-servers: the sync thread could not start: {e}");
    }
    (orders, events)
}

/// Which end an order still owes the window, should it panic: none, its
/// playlists' or its catalogue's.
const OWE_NOTHING: u8 = 0;
const OWE_PLAYLISTS: u8 = 1;
const OWE_CATALOGUE: u8 = 2;
const OWE_COVERS: u8 = 3;

/// One order, on the sync's thread. `owed` follows which end is still owed.
fn serve(order: Order, tools: &Tools, stop: &Mutex<HashSet<i64>>, outbox: &Sender<Event>, owed: &AtomicU8) {
    let (source, catalogue, after) = match order {
        Order::Catalogue(source) => (source, true, None),
        Order::Playlists(source) => (source, false, None),
        Order::Covers(source, after) => (source, false, Some(after)),
    };
    let asked_to_stop = || stop.lock().map(|s| s.contains(&source)).unwrap_or(false);
    let say = |event: Event| {
        let _ = outbox.send(event);
    };
    let Some(lib) = (tools.open)() else {
        eprintln!("chilpa-servers: sync of source {source} not begun: the library could not be opened");
        let why = chilpa_i18n::t("the library could not be opened");
        owed.store(OWE_NOTHING, Ordering::Relaxed);
        say(match (catalogue, after) {
            (true, _) => Event::Failed { source, why, unreachable: false },
            (false, None) => Event::PlaylistsFailed { source, why },
            (false, Some(_)) => Event::Covers { source, next: None },
        });
        return;
    };
    let net = tools.net.as_ref();
    if let Some(after) = after {
        let next = covers(&lib, net, &tools.keys, source, after, &asked_to_stop);
        owed.store(OWE_NOTHING, Ordering::Relaxed);
        say(Event::Covers { source, next: next.filter(|_| !asked_to_stop()) });
        return;
    }
    let wanted = if catalogue {
        true
    } else {
        let event = reading::read(&lib, net, &tools.keys, source, &tools.gate, &asked_to_stop);
        let asks = asks_for_catalogue(&event);
        owed.store(OWE_NOTHING, Ordering::Relaxed);
        say(event);
        asks && !asked_to_stop()
    };
    if !wanted {
        return;
    }
    owed.store(OWE_CATALOGUE, Ordering::Relaxed);
    say(Event::Started { source });
    let progress = |reading: Reading| say(Event::Progress { source, reading });
    let event = run(&lib, net, &tools.keys, source, &progress, &asked_to_stop);
    // What came in has its covers fetched, whether the sync finished or
    // ended partial: a server read in part still shows its records whole.
    let goes_on = reads_playlists_after(&event);
    owed.store(OWE_NOTHING, Ordering::Relaxed);
    say(event);
    if goes_on && !asked_to_stop() {
        say(reading::read(&lib, net, &tools.keys, source, &tools.gate, &asked_to_stop));
    }
    // Its covers are fetched in runs of their own, behind what is queued.
    if goes_on && !asked_to_stop() {
        say(Event::Covers { source, next: Some(0) });
    }
}

/// Reads the whole catalogue of one server into the library.
pub(crate) fn run(
    lib: &chilpa_library::Library,
    net: &dyn Net,
    keys: &Keys,
    source: i64,
    progress: &dyn Fn(Reading),
    stopped: &dyn Fn() -> bool,
) -> Event {
    let failed = |failure: Failure| Event::Failed { source, unreachable: failure.is_unreachable(), why: failure.to_string() };
    let Some(stored) = lib.server(source).ok().flatten() else {
        return Event::Failed { source, why: chilpa_i18n::t("that server is no longer in the library"), unreachable: false };
    };
    if !stored.enabled {
        return Event::Failed { source, why: chilpa_i18n::t("that server is disabled"), unreachable: false };
    }
    let Some(mut server) = net::server(&stored) else {
        return Event::Failed { source, why: chilpa_i18n::t("that server is no longer in the library"), unreachable: false };
    };
    // A media server added by a name may give its files by its address:
    // what the name resolves to now is its own too.
    if server.kind == chilpa_servers::Kind::Dlna {
        for name in super::lan::other_names(&server.address) {
            if !server.details.hosts.contains(&name) {
                server.details.hosts.push(name);
            }
        }
    }
    let Some(key) = net::key(keys, &stored) else {
        return Event::Failed { source, why: chilpa_i18n::t_args("sign in to {} again", &[&stored.label]), unreachable: false };
    };
    let Ok(generation) = lib.begin_server_sync(source) else {
        return Event::Failed { source, why: chilpa_i18n::t("the library could not be written"), unreachable: false };
    };
    let known = lib.server_revisions(source).unwrap_or_default();
    let mut seen: HashSet<String> = HashSet::new();
    // What was kept or saved in this sync: only that counts as read.
    let mut sealed: usize = 0;
    // Something listed could not be read; nothing is taken out.
    let mut partial = false;
    let mut announced: HashMap<String, u64> = HashMap::new();
    let mut cursor: Option<String> = None;
    // Read and not written yet: it goes in together, every `WRITE_TRACKS`
    // or `WRITE_EVERY`, and before the sync ends.
    let (mut keep, mut save): (Vec<String>, Vec<chilpa_library::ServerTrack>) = (Vec::new(), Vec::new());
    let (started, mut wrote, mut logged, mut beat) = (Instant::now(), Instant::now(), Instant::now(), Instant::now());
    let (mut pages, mut stale): (u64, u64) = (0, 0);
    // Containers of a media server that could not be read and were left out.
    let mut skipped: u64 = 0;
    let unwritten = || Event::Failed { source, why: chilpa_i18n::t("the library could not be written"), unreachable: false };
    eprintln!("chilpa-servers: sync of source {source} ({}) started", stored.provider);
    loop {
        // What a stopped sync read is not written: its server is going.
        if stopped() {
            eprintln!("chilpa-servers: sync of source {source} stopped after {pages} pages, {} tracks", seen.len());
            return Event::Failed { source, why: chilpa_i18n::t("the sync was stopped"), unreachable: false };
        }
        let asked_at = Instant::now();
        let page = match chilpa_servers::page(net, &server, &key, cursor.as_deref()) {
            Ok(page) => page,
            Err(failure) => {
                eprintln!(
                    "chilpa-servers: sync of source {source} failed at page {} after {:.1}s, {} tracks read: {failure}",
                    pages + 1,
                    started.elapsed().as_secs_f32(),
                    seen.len()
                );
                if !write(lib, source, generation, &mut keep, &mut save) {
                    return unwritten();
                }
                return failed(failure);
            }
        };
        pages += 1;
        if asked_at.elapsed() > Duration::from_secs(5) {
            eprintln!("chilpa-servers: sync of source {source}: page {pages} took {:.1}s", asked_at.elapsed().as_secs_f32());
        }
        if let Some(total) = page.total {
            announced.insert(page.scope.clone(), total);
        }
        partial |= page.partial;
        skipped += page.skipped;
        let before = seen.len();
        let mut ask = Vec::new();
        for track in page.tracks {
            let entry = net::entry(&stored, &track.id);
            if !seen.insert(entry.clone()) {
                continue;
            }
            let unchanged = track.revision.is_some() && known.get(&entry).is_some_and(|r| r == &track.revision);
            if unchanged {
                keep.push(entry);
            } else if track.complete {
                save.push(net::track(&stored, track));
            } else {
                ask.push(track.id);
            }
        }
        for batch in ask.chunks(chilpa_servers::DETAIL_BATCH) {
            match chilpa_servers::details(net, &server, &key, batch) {
                Ok(found) => {
                    // A track listed and then not described was deleted in
                    // between, or the answer was cut: either way this sync
                    // cannot tell what is gone.
                    let described: HashSet<&str> = found.iter().map(|t| t.id.as_str()).collect();
                    partial |= batch.iter().any(|id| !described.contains(id.as_str()));
                    save.extend(found.into_iter().filter(|t| batch.contains(&t.id)).map(|t| net::track(&stored, t)));
                }
                Err(failure) => {
                    eprintln!("chilpa-servers: sync of source {source}: details failed at page {pages}: {failure}");
                    if !write(lib, source, generation, &mut keep, &mut save) {
                        return unwritten();
                    }
                    return failed(failure);
                }
            }
        }
        stale = if seen.len() > before { 0 } else { stale + 1 };
        let last = page.next.is_none();
        let due = last || stale >= STALE_PAGES || keep.len() + save.len() >= WRITE_TRACKS || wrote.elapsed() >= WRITE_EVERY;
        if due && !(keep.is_empty() && save.is_empty()) {
            sealed += keep.len() + save.len();
            if !write(lib, source, generation, &mut keep, &mut save) {
                return unwritten();
            }
            wrote = Instant::now();
            beat = Instant::now();
            progress(Reading { done: seen.len() as u64, pages, wrote: true });
        } else if beat.elapsed() >= BEAT {
            beat = Instant::now();
            progress(Reading { done: seen.len() as u64, pages, wrote: false });
        }
        if logged.elapsed() >= LOG_EVERY {
            logged = Instant::now();
            eprintln!(
                "chilpa-servers: sync of source {source}: {pages} pages, {} tracks, {:.0}s",
                seen.len(),
                started.elapsed().as_secs_f32()
            );
        }
        if stale >= STALE_PAGES {
            eprintln!("chilpa-servers: sync of source {source}: {stale} pages in a row brought no new track; stopped at {} tracks", seen.len());
            let read = seen.len().to_string();
            return Event::Failed {
                source,
                why: chilpa_i18n::t_args("the server kept listing tracks already read; the sync stopped at {} tracks and took nothing out", &[&read]),
                unreachable: false,
            };
        }
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    eprintln!(
        "chilpa-servers: sync of source {source} read {pages} pages, {} tracks in {:.0}s; {skipped} containers left out{}",
        seen.len(),
        started.elapsed().as_secs_f32(),
        if partial { ", partial" } else { "" }
    );
    // Paging without a tie-breaker can repeat a track and skip another. A
    // sync that kept fewer than the server has, or could not read all it
    // listed, takes nothing out. Subsonic says its total only when asked,
    // after the last page, so a record skipped between pages shows.
    let mut expected: u64 = announced.values().sum();
    match chilpa_servers::census(net, &server, &key) {
        Ok(Some(total)) => expected = expected.max(total),
        Ok(None) => {}
        Err(failure) => return failed(failure),
    }
    // A folder of a media server that could not be read is named by how
    // many there were: the tracks read are in, and the rest is said.
    if skipped > 0 {
        let (read, left) = (seen.len().to_string(), skipped.to_string());
        return Event::Failed {
            source,
            why: chilpa_i18n::tf(
                "{} tracks came in; {} folder of the server could not be read and was left out. Nothing was taken out; sync again.",
                "{} tracks came in; {} folders of the server could not be read and were left out. Nothing was taken out; sync again.",
                skipped as i64,
                &[&read, &left],
            ),
            unreachable: false,
        };
    }
    if partial {
        return Event::Failed {
            source,
            why: chilpa_i18n::t("the server did not describe every track it listed; nothing was taken out, sync again"),
            unreachable: false,
        };
    }
    if (sealed as u64) < expected {
        let (saw, total) = ((sealed as u64).to_string(), expected.to_string());
        return Event::Failed {
            source,
            why: chilpa_i18n::t_args("the server listed {} of its {} tracks; nothing was taken out, sync again", &[&saw, &total]),
            unreachable: false,
        };
    }
    match lib.finish_server_sync(source, generation) {
        Ok(gone) => {
            let _ = lib.set_source_reachable(source, true);
            Event::Finished { source, gone }
        }
        Err(_) => Event::Failed { source, why: chilpa_i18n::t("the library could not be written"), unreachable: false },
    }
}

/// Writes what a sync read and has not written, and empties it. Each part
/// lands whole or not at all. False when the library could not be written.
fn write(
    lib: &chilpa_library::Library,
    source: i64,
    generation: i64,
    keep: &mut Vec<String>,
    save: &mut Vec<chilpa_library::ServerTrack>,
) -> bool {
    let kept = keep.is_empty() || lib.keep_server_tracks(source, generation, keep.as_slice()).is_ok();
    let saved = kept && (save.is_empty() || lib.save_server_tracks(source, generation, save.as_slice()).is_ok());
    keep.clear();
    save.clear();
    saved
}

/// Where a server's covers are kept: removing the server removes the folder.
pub(crate) fn covers_dir(instance: &str) -> std::path::PathBuf {
    chilpa_dirs::cache().join("servers").join(instance)
}

/// Fetches covers of the server's records that have none yet, after the
/// record `after`: at most `COVERS_PER_RUN` of them, for at most
/// `COVERS_TIME`. Where the next run starts, or `None` when none is left
/// to ask for this time.
///
/// A cover the server says it does not have (404, 410), or that is not an
/// image, is not asked for again until the server names another. One it
/// could not give now (another status, no answer) is asked for again at a
/// later sync, up to `COVER_TRIES` times.
pub(crate) fn covers(lib: &chilpa_library::Library, net: &dyn Net, keys: &Keys, source: i64, after: i64, stopped: &dyn Fn() -> bool) -> Option<i64> {
    let stored = lib.server(source).ok().flatten().filter(|s| s.enabled)?;
    let (server, key) = (net::server(&stored)?, net::key(keys, &stored)?);
    let dir = covers_dir(&stored.instance);
    let _ = std::fs::create_dir_all(&dir);
    let pending = lib.pending_server_covers_after(source, after, COVERS_PER_RUN).unwrap_or_default();
    let (began, full) = (Instant::now(), pending.len() >= COVERS_PER_RUN);
    let (mut last, mut missed) = (after, 0);
    for (album, cover) in pending {
        if stopped() {
            return None;
        }
        if began.elapsed() >= COVERS_TIME {
            eprintln!("chilpa-servers: covers of source {source}: run ended after {}s at record {last}", COVERS_TIME.as_secs());
            return Some(last);
        }
        last = album;
        let request = chilpa_servers::cover(&server, &key, &cover, COVER);
        let _ = match net.send(&request) {
            Ok(reply) if reply.status == 200 => {
                let kind = reply.headers.get("content-type").cloned().unwrap_or_default();
                let ext = image_kind(&kind, &reply.body);
                let path = dir.join(format!("{album}.{ext}"));
                let saved = (!ext.is_empty() && std::fs::write(&path, &reply.body).is_ok()).then(|| path.display().to_string());
                lib.set_server_cover(album, saved.as_deref())
            }
            Ok(reply) if matches!(reply.status, 404 | 410) => lib.set_server_cover(album, None),
            Ok(_) | Err(_) => {
                missed += 1;
                lib.missed_server_cover(album)
            }
        };
    }
    if missed > 0 {
        eprintln!("chilpa-servers: covers of source {source}: {missed} not given now, asked again at a later sync");
    }
    full.then_some(last)
}

/// The extension a cover is kept with: from its type, or, when a server
/// sends none or a generic one (some media servers do), from its first
/// bytes. Empty when it is not an image.
fn image_kind(kind: &str, body: &[u8]) -> &'static str {
    if body.starts_with(&[0x89, b'P', b'N', b'G']) || kind.contains("png") {
        "png"
    } else if body.starts_with(&[0xFF, 0xD8, 0xFF]) || kind.starts_with("image/") {
        "jpg"
    } else {
        ""
    }
}

#[cfg(test)]
mod tests;
