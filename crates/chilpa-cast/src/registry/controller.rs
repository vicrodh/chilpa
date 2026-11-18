//! One owner for every check and search of the registry. The host calls
//! `pump` from its own loop; threads here only ask the network and report.
//! Nothing in this file connects, loads, plays, stops or changes a volume.
use super::{store::{self, Writer}, Absence, EndpointKey, Presence, Registry, Seen, Stored, Unusable, MAX_URL};
use crate::{discovery::Sighting, Device, Error, Kind, Result};
use std::{collections::{HashMap, HashSet, VecDeque}, path::PathBuf, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use std::sync::{atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering}, mpsc::{self, Receiver, SyncSender}, Arc};

/// What the registry may ask of a network. Each call only reads.
pub trait Network: Send + Sync + 'static {
    /// One description GET, over within `limit`: the device it describes.
    fn describe(&self, location: &str, limit: Duration) -> Result<Device>;
    /// The description addresses that answer one SSDP search.
    fn search(&self, window: Duration, running: &AtomicBool) -> Result<Vec<String>>;
    /// Cast receivers as they resolve. Returns once the window has run its
    /// course, or sooner when `found` returns false or `running` clears.
    fn browse(&self, window: Duration, running: &AtomicBool, found: &mut dyn FnMut(Sighting) -> bool) -> Result<()>;
}

/// Description reads in flight, checks and discovery together.
pub const PROBES: usize = 2;
pub const PROBE_LIMIT: Duration = Duration::from_secs(2);
/// One round of checks, waiting for a free probe included.
pub const PASS_LIMIT: Duration = Duration::from_secs(8);
pub const SEARCH_WINDOW: Duration = Duration::from_secs(3);
pub const BROWSE_WINDOW: Duration = Duration::from_secs(5);
pub const CHECK_WINDOW: Duration = Duration::from_secs(2);
/// How long a chosen endpoint may take to answer before the choice fails.
pub const SELECT_LIMIT: Duration = Duration::from_secs(4);
/// The least time between two searches nobody asked for by hand.
pub const COOLDOWN: Duration = Duration::from_secs(30);
pub const EVENTS: usize = 256;
const RECHECK: Duration = Duration::from_secs(5);
const SEARCH_LIMIT: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Purpose { Check, Repair, Discover }

enum Event {
    Loaded(std::result::Result<Option<Vec<Stored>>, Unusable>),
    Answered { operation: u64, generation: u64, result: Result<Device> },
    Located { operation: u64, generation: u64, result: Result<Vec<String>> },
    Sighted { operation: u64, generation: u64, sighting: Sighting },
    Browsed { operation: u64, generation: u64, exhausted: bool, failed: bool },
}
enum Job { Probe { key: EndpointKey, version: u64, url: String }, Describe { url: String, scan: u64, admit: bool, requested: u64 } }
struct Queued { operation: u64, job: Job }
/// `requested` is the operation counter when the search was last asked for:
/// what was forgotten after that stays out of its answers.
struct Run { operation: u64, purpose: Purpose, until: Instant, located: bool, requested: u64 }
#[derive(Default)]
struct Scan { alive: Arc<AtomicBool>, running: Arc<AtomicBool>, run: Option<Run>, want: Option<Purpose>, requested: u64, started: Option<Instant> }
impl Scan {
    fn cancel(&mut self) { self.running.store(false, Ordering::Release); self.run = None; self.want = None; }
    /// A check or a repair joining a search that already waits does not make
    /// that search any newer.
    fn ask(&mut self, purpose: Purpose, at: u64) {
        if purpose == Purpose::Discover || self.want.is_none() { self.requested = at; }
        self.want = self.want.max(Some(purpose));
    }
    fn discovering(&self) -> bool { self.want == Some(Purpose::Discover) || self.run.as_ref().is_some_and(|r| r.purpose == Purpose::Discover) }
}
/// A thread gives its place back when it ends, not when it is cancelled.
struct Flight(Arc<AtomicUsize>);
impl Drop for Flight { fn drop(&mut self) { self.0.fetch_sub(1, Ordering::AcqRel); } }
struct Alive(Arc<AtomicBool>);
impl Drop for Alive { fn drop(&mut self) { self.0.store(false, Ordering::Release); } }

/// What the host has to tell the person. Each is one flag, never a log.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Notices { pub kept: Option<Unusable>, pub unsaved: bool, pub full: bool, pub search_failed: bool }
/// What a choice made earlier came to.
#[derive(Debug)]
pub enum Outcome { Connect(Device), Unreachable(EndpointKey) }
#[derive(Debug)]
pub enum Selection { Ready(Device), Pending, Unknown }
/// Work in flight, for tests and measurements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Load { pub probes: usize, pub scans: usize, pub queued: usize }

pub struct Controller {
    registry: Registry, network: Arc<dyn Network>, writer: Option<Writer>,
    sender: SyncSender<Event>, events: Receiver<Event>,
    /// The network generation: offline, online and wake each start another.
    /// It is unrelated to any playback generation.
    generation: Arc<AtomicU64>, probing: Arc<AtomicUsize>,
    upnp: Scan, cast: Scan,
    queue: VecDeque<Queued>, flying: HashMap<u64, Job>, operations: u64,
    online: bool, open: bool, loaded: bool, replace: bool,
    /// Nothing was ever stored: the first opening may search by itself.
    /// A list emptied by hand, in this process or an earlier one, may not.
    first_use: bool, emptied: bool,
    /// A write failed and has not been made good; a repair search is due;
    /// when the last Cast browse could not even start.
    unwritten: bool, repair_owed: bool, browse_failed: Option<Instant>,
    pass_until: Option<Instant>, recheck: Option<Instant>, searched: Option<Instant>,
    intent: Option<(EndpointKey, Instant)>, active: Option<EndpointKey>,
    /// Forgotten keys with the operation counter at that moment: nothing
    /// started before it may bring the endpoint back.
    forgotten: HashMap<EndpointKey, u64>,
    clock: Option<(Instant, SystemTime)>, notices: Notices, outcomes: Vec<Outcome>,
}
impl Controller {
    /// With a path the stored list is read on its own thread; rows found
    /// before it arrives are reconciled with it.
    pub fn new(path: Option<PathBuf>, network: Arc<dyn Network>) -> Self {
        let (sender, events) = mpsc::sync_channel(EVENTS);
        let mut notices = Notices::default();
        let mut loaded = path.is_none();
        let writer = path.map(|path| {
            let (report, from) = (sender.clone(), path.clone());
            let spawned = std::thread::Builder::new().name("chilpa-endpoint-load".into()).spawn(move || { let _ = report.send(Event::Loaded(store::load(&from))); });
            if spawned.is_err() { loaded = true; notices.kept = Some(Unusable::Unreadable); }
            Writer::new(path)
        });
        Self { registry: Registry::default(), network, writer, sender, events, generation: Default::default(), probing: Default::default(),
            upnp: Scan::default(), cast: Scan::default(), queue: VecDeque::new(), flying: HashMap::new(), operations: 0,
            online: true, open: false, loaded, replace: false, first_use: loaded, emptied: false, unwritten: false, repair_owed: false, browse_failed: None, pass_until: None, recheck: None, searched: None, intent: None, active: None,
            forgotten: HashMap::new(), clock: None, notices, outcomes: Vec::new() }
    }
    pub fn registry(&self) -> &Registry { &self.registry }
    pub fn notices(&self) -> Notices { self.notices }
    /// The stored list has not been read yet: an empty list is not known.
    pub fn loading(&self) -> bool { !self.loaded }
    pub fn searching(&self) -> bool { self.upnp.discovering() || self.cast.discovering() }
    pub fn pending(&self) -> Option<&EndpointKey> { self.intent.as_ref().map(|(key, _)| key) }
    pub fn load(&self) -> Load {
        Load { probes: self.probing.load(Ordering::Acquire), queued: self.queue.len(),
            scans: usize::from(self.upnp.alive.load(Ordering::Acquire)) + usize::from(self.cast.alive.load(Ordering::Acquire)) }
    }
    fn generation(&self) -> u64 { self.generation.load(Ordering::Acquire) }
    fn scan(&mut self, protocol: Kind) -> &mut Scan { match protocol { Kind::Dlna => &mut self.upnp, Kind::Chromecast => &mut self.cast } }
    fn intends(&self, key: &EndpointKey) -> bool { self.intent.as_ref().is_some_and(|(k, _)| k == key) }

    /// The picker opened or closed. Opening shows what is known and checks
    /// what has no fresh answer; closing ends the work that was only for it.
    pub fn set_open(&mut self, open: bool, now: Instant) {
        if self.open == open { return; }
        self.open = open;
        if open { self.recheck = None; self.refresh_known(now); self.first_search(now); return; }
        self.pass_until = None; self.recheck = None;
        let keep = self.intent.as_ref().map(|(key, _)| key.clone());
        self.queue.retain(|q| matches!(&q.job, Job::Probe { key, .. } if Some(key) == keep.as_ref()));
        for protocol in [Kind::Dlna, Kind::Chromecast] {
            if keep.as_ref().is_none_or(|key| key.protocol != protocol) { self.scan(protocol).cancel(); }
        }
        let flying: HashSet<&EndpointKey> = self.flying.values().filter_map(|job| match job { Job::Probe { key, .. } => Some(key), _ => None }).collect();
        let idle: Vec<_> = self.registry.endpoints().iter().filter(|e| e.asked().is_some() && Some(&e.key) != keep.as_ref() && !flying.contains(&e.key)).map(|e| e.key.clone()).collect();
        for key in idle { self.registry.unask(&key); }
    }
    /// Checks every remembered endpoint without a fresh answer, the least
    /// recently checked first. Asking twice asks once.
    pub fn refresh_known(&mut self, now: Instant) {
        if !self.online || !self.open || !self.loaded { return; }
        self.recheck = Some(now + RECHECK);
        let mut stale: Vec<_> = self.registry.endpoints().iter()
            .filter(|e| Some(&e.key) != self.active.as_ref() && !e.fresh(now) && e.asked().is_none())
            .map(|e| (e.checked(), e.key.clone())).collect();
        if stale.is_empty() { return; }
        stale.sort();
        if self.pass_until.is_none_or(|until| now >= until) { self.pass_until = Some(now + PASS_LIMIT); }
        for (_, key) in stale { self.check(&key, false, now); }
    }
    /// A profile that never stored a renderer searches once when the picker
    /// opens. It does not repeat while the picker stays open.
    fn first_search(&mut self, now: Instant) {
        let may = self.online && self.open && self.loaded && self.first_use && !self.emptied && self.registry.endpoints().is_empty();
        if may && self.searched.is_none_or(|at| now.saturating_duration_since(at) >= COOLDOWN) { self.search(now); }
    }
    fn check(&mut self, key: &EndpointKey, first: bool, now: Instant) {
        let Some(endpoint) = self.registry.get(key) else { return };
        if key.protocol == Kind::Chromecast {
            // Identity comes from the receiver's own mDNS answer; an open
            // port on a remembered address would prove nothing.
            if self.browse_failed.is_some_and(|at| now.saturating_duration_since(at) < COOLDOWN) { return; }
            self.registry.ask(key, self.operations);
            self.cast.ask(Purpose::Check, self.operations);
            return;
        }
        let (url, version) = (endpoint.description().map(str::to_string), endpoint.version());
        let Some(url) = url else { self.repair(now); return };
        if self.flying.values().any(|job| matches!(job, Job::Probe { key: k, .. } if k == key)) { return; }
        if let Some(at) = self.queue.iter().position(|q| matches!(&q.job, Job::Probe { key: k, .. } if k == key)) {
            if first { if let Some(queued) = self.queue.remove(at) { self.queue.push_front(queued); } }
            return;
        }
        self.operations += 1;
        let queued = Queued { operation: self.operations, job: Job::Probe { key: key.clone(), version, url } };
        if first { self.queue.push_front(queued); } else { self.queue.push_back(queued); }
        self.registry.ask(key, self.operations);
    }
    /// A renderer stopped answering where it was: one search may find where
    /// it went. Reopening the picker does not shorten the wait.
    fn repair(&mut self, now: Instant) {
        self.repair_owed = !(self.open && self.online && self.upnp.started.is_none_or(|at| now.saturating_duration_since(at) >= COOLDOWN));
        if !self.repair_owed { self.upnp.ask(Purpose::Repair, self.operations); }
    }
    fn search(&mut self, now: Instant) {
        self.searched = Some(now);
        self.notices.search_failed = false;
        // A search already out answers for this one too, unless something
        // was forgotten since it was asked for: that needs a search of its own.
        let (at, again) = (self.operations, !self.forgotten.is_empty());
        for protocol in [Kind::Dlna, Kind::Chromecast] {
            let scan = self.scan(protocol);
            if again || scan.run.as_ref().is_none_or(|run| run.purpose != Purpose::Discover) { scan.ask(Purpose::Discover, at); }
        }
    }
    /// The person asked to search: one bounded pass per protocol, however
    /// often it is asked. A damaged file is set aside for the new list.
    pub fn discover(&mut self, now: Instant) -> bool {
        if !self.online { return false; }
        if self.notices.kept == Some(Unusable::Damaged) { self.notices.kept = None; self.replace = true; self.registry.mark_dirty(); }
        self.notices.full = false;
        self.emptied = false;
        self.search(now);
        true
    }
    /// The person chose this endpoint. With a fresh answer it is ready; if
    /// not, it is asked first and the choice stands until it answers.
    pub fn select(&mut self, key: &EndpointKey, now: Instant) -> Selection {
        self.intent = None;
        let Some(endpoint) = self.registry.get(key) else { return Selection::Unknown };
        if !self.online { return Selection::Unknown; }
        if endpoint.answer() == Presence::Reachable && endpoint.fresh(now) {
            if let Some(device) = endpoint.device() { return Selection::Ready(device.clone()); }
        }
        self.intent = Some((key.clone(), now + SELECT_LIMIT));
        self.check(key, true, now);
        Selection::Pending
    }
    /// The choice was withdrawn: no later answer may connect.
    pub fn deselect(&mut self) { self.intent = None; }
    /// Removes an endpoint nobody is playing on, and its stored record.
    pub fn forget(&mut self, key: &EndpointKey) -> bool {
        if self.active.as_ref() == Some(key) || !self.registry.forget(key) { return false; }
        self.forgotten.insert(key.clone(), self.operations);
        self.operations += 1;
        self.queue.retain(|q| !matches!(&q.job, Job::Probe { key: k, .. } if k == key));
        if self.intends(key) { self.intent = None; }
        self.notices.full = false;
        self.emptied = true;
        true
    }
    /// The endpoint the session plays on is never probed: the session's own
    /// answers are its evidence.
    pub fn set_active(&mut self, key: Option<EndpointKey>) { self.active = key; }
    pub fn session_alive(&mut self, key: &EndpointKey, now: Instant) { self.registry.alive(key, now); }
    pub fn set_online(&mut self, online: bool) {
        if self.online == online { return; }
        self.reset();
        self.online = online;
    }
    /// Another network, or the same one after a sleep: nothing asked before
    /// counts, and nothing is connected because of it.
    pub fn network_changed(&mut self) { if self.online { self.reset(); } }
    fn reset(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.upnp.cancel(); self.cast.cancel();
        // What is still in flight answers for a network that was left.
        self.queue.clear(); self.flying.clear();
        self.repair_owed = false; self.browse_failed = None;
        self.registry.invalidate();
        self.pass_until = None; self.recheck = None;
        if let Some((key, _)) = self.intent.take() { self.outcomes.push(Outcome::Unreachable(key)); }
    }

    /// Takes what the threads reported, starts what may start and returns
    /// what earlier choices came to. Cheap when there is nothing to do.
    pub fn pump(&mut self, now: Instant, wall: SystemTime) -> Vec<Outcome> {
        if let Some((before, clock)) = self.clock.replace((now, wall)) {
            // The wall clock ran ahead of the monotonic one: the machine slept.
            if wall.duration_since(clock).unwrap_or_default() > now.saturating_duration_since(before) + COOLDOWN { self.network_changed(); }
        }
        let seconds = wall.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let orphaned = self.probing.load(Ordering::Acquire) == 0 && !self.flying.is_empty();
        let mut sightings = Vec::new();
        let mut browsed = Vec::new();
        let mut arrived = false;
        // A search thread that had already ended reported everything it ever will.
        let ended = |scan: &Scan| scan.run.as_ref().filter(|_| !scan.alive.load(Ordering::Acquire)).map(|run| run.operation);
        let (upnp_ended, cast_ended) = (ended(&self.upnp), ended(&self.cast));
        let events: Vec<_> = self.events.try_iter().take(EVENTS * 2).collect();
        for event in events {
            match event {
                Event::Loaded(result) => {
                    self.loaded = true;
                    self.first_use = !matches!(result, Ok(Some(_)));
                    match result {
                        Ok(stored) => { let forgotten = &self.forgotten; self.registry.restore(stored.unwrap_or_default(), |key| forgotten.contains_key(key)); }
                        Err(why) => self.notices.kept = Some(why),
                    }
                    self.recheck = None;
                    arrived = true;
                }
                Event::Answered { operation, generation, result } => {
                    let Some(job) = self.flying.remove(&operation) else { continue };
                    if generation != self.generation() { continue; }
                    match job {
                        Job::Describe { admit, requested, .. } => if let Ok(device) = result { sightings.push((device, None, admit, requested)); },
                        Job::Probe { key, version, .. } => {
                            if self.registry.get(&key).is_none_or(|e| e.version() != version) { continue; }
                            match result {
                                Ok(device) if EndpointKey::of(&device) == key => sightings.push((device, None, false, operation)),
                                // Somebody else answers at that address now.
                                Ok(device) => { self.fail(&key, Absence::WrongIdentity, now); sightings.push((device, None, false, operation)); }
                                Err(Error::Description) => self.fail(&key, Absence::NotRenderer, now),
                                Err(_) => self.fail(&key, Absence::NoAnswer, now),
                            }
                        }
                    }
                }
                Event::Located { operation, generation, result } => {
                    if self.upnp.run.as_ref().is_none_or(|run| run.operation != operation) { continue; }
                    if generation != self.generation() { self.upnp.run = None; continue; }
                    let (admit, requested) = self.upnp.run.as_mut().map(|run| { run.located = true; (run.purpose == Purpose::Discover, run.requested) }).unwrap_or_default();
                    let Ok(mut locations) = result else { self.notices.search_failed |= admit; continue };
                    locations.sort(); locations.dedup(); locations.truncate(SEARCH_LIMIT);
                    for url in locations {
                        // Only an endpoint that just answered there settles an address: a
                        // stranger at a remembered one still has to be read.
                        let settled = self.registry.endpoints().iter().any(|e| e.description() == Some(url.as_str()) && e.answer() == Presence::Reachable && e.fresh(now));
                        if settled || url.len() > MAX_URL { continue; }
                        self.operations += 1;
                        self.queue.push_back(Queued { operation: self.operations, job: Job::Describe { url, scan: operation, admit, requested } });
                    }
                }
                Event::Sighted { operation, generation, sighting } => {
                    let Some(run) = self.cast.run.as_ref().filter(|run| run.operation == operation && generation == self.generation()) else { continue };
                    sightings.push((sighting.device, Some(sighting.fullname), run.purpose == Purpose::Discover, run.requested));
                }
                Event::Browsed { operation, generation, exhausted, failed } => {
                    let Some(run) = self.cast.run.take_if(|run| run.operation == operation) else { continue };
                    if generation == self.generation() { browsed.push((run, exhausted, failed)); }
                }
            }
        }
        // Candidates of one round enter in key order, whatever order the
        // network answered in.
        sightings.sort_by(|a, b| EndpointKey::of(&a.0).cmp(&EndpointKey::of(&b.0)));
        for (device, fullname, admit, operation) in sightings {
            let key = EndpointKey::of(&device);
            if self.forgotten.get(&key).is_some_and(|at| operation <= *at) { continue; }
            match self.registry.observe(device, fullname.as_deref(), admit, now, seconds) {
                Seen::Full => self.notices.full = true,
                Seen::Updated | Seen::Added => self.settle(&key, true),
                Seen::Unknown | Seen::Invalid => {}
            }
        }
        for (run, exhausted, failed) in browsed {
            self.notices.search_failed |= failed && run.purpose == Purpose::Discover;
            if failed { self.browse_failed = Some(now); }
            let asked: Vec<_> = self.registry.endpoints().iter().filter(|e| e.key.protocol == Kind::Chromecast && e.asked().is_some_and(|at| at < run.operation)).map(|e| e.key.clone()).collect();
            // Only a whole window without an answer says a receiver is away.
            for key in asked { if exhausted { self.fail(&key, Absence::NoAnswer, now); } else { self.registry.unask(&key); } }
            let waiting = self.registry.endpoints().iter().any(|e| e.key.protocol == Kind::Chromecast && e.asked().is_some());
            if waiting && !failed { self.cast.ask(Purpose::Check, self.operations); }
        }
        if cast_ended.is_some() && self.cast.run.as_ref().map(|run| run.operation) == cast_ended { self.cast.run = None; self.unask_receivers(); }
        if upnp_ended.is_some() && self.upnp.run.as_ref().filter(|run| !run.located).map(|run| run.operation) == upnp_ended { self.upnp.run = None; }
        if orphaned {
            // Every probe thread had ended before this round: what is still
            // listed as in flight will never report.
            for (_, job) in self.flying.drain() { if let Job::Probe { key, .. } = job { self.registry.unask(&key); } }
        }
        self.expire(now);
        if self.open && self.recheck.is_none_or(|at| now >= at) { self.refresh_known(now); }
        if arrived { self.first_search(now); }
        if self.repair_owed && self.open { self.repair(now); }
        for protocol in [Kind::Dlna, Kind::Chromecast] { self.launch(protocol, now); }
        self.dispatch();
        self.persist(false);
        let idle = self.flying.is_empty() && self.queue.is_empty() && self.load().scans == 0 && [&self.upnp, &self.cast].iter().all(|scan| scan.run.is_none() && scan.want.is_none());
        if idle && self.loaded { self.forgotten.clear(); }
        std::mem::take(&mut self.outcomes)
    }
    fn fail(&mut self, key: &EndpointKey, why: Absence, now: Instant) {
        let was = self.registry.get(key).map(|e| e.answer());
        self.registry.absent(key, why, now);
        if key.protocol == Kind::Dlna && !matches!(was, Some(Presence::Unavailable(_))) { self.repair(now); }
        self.settle(key, false);
    }
    /// An answer about the chosen endpoint ends the choice, one way or the other.
    fn settle(&mut self, key: &EndpointKey, reachable: bool) {
        if !self.intends(key) { return; }
        self.intent = None;
        let device = self.registry.get(key).and_then(|e| e.device().cloned()).filter(|_| reachable);
        self.outcomes.push(match device { Some(device) => Outcome::Connect(device), None => Outcome::Unreachable(key.clone()) });
    }
    fn expire(&mut self, now: Instant) {
        if self.pass_until.is_some_and(|until| now >= until) {
            // What was not reached stays to be checked on the next demand;
            // not being reached says nothing about the endpoint.
            self.pass_until = None;
            let mut kept = VecDeque::new();
            for queued in std::mem::take(&mut self.queue) {
                match &queued.job {
                    Job::Probe { key, .. } if !self.intends(key) => self.registry.unask(key),
                    _ => kept.push_back(queued),
                }
            }
            self.queue = kept;
        }
        if let Some(run) = self.upnp.run.as_ref().filter(|run| run.located) {
            let operation = run.operation;
            if now >= run.until { self.queue.retain(|q| !matches!(&q.job, Job::Describe { scan, .. } if *scan == operation)); }
            let describing = |job: &Job| matches!(job, Job::Describe { scan, .. } if *scan == operation);
            if !self.queue.iter().any(|q| describing(&q.job)) && !self.flying.values().any(describing) { self.upnp.run = None; }
        }
        if self.intent.as_ref().is_some_and(|(_, until)| now >= *until) {
            if let Some((key, _)) = self.intent.take() { self.outcomes.push(Outcome::Unreachable(key)); }
        }
    }
    /// Starts the search a protocol is waiting for, once the thread of the
    /// previous one has really ended: a cancelled search still holds its place.
    fn launch(&mut self, protocol: Kind, now: Instant) {
        let online = self.online;
        let scan = self.scan(protocol);
        let Some(purpose) = scan.want else { return };
        if !online { scan.want = None; return; }
        if scan.alive.load(Ordering::Acquire) { return; }
        scan.want = None;
        let requested = scan.requested;
        self.operations += 1;
        let operation = self.operations;
        let mut wanted = None;
        if protocol == Kind::Chromecast {
            if purpose == Purpose::Check {
                let ids: HashSet<String> = self.registry.endpoints().iter().filter(|e| e.key.protocol == Kind::Chromecast && e.asked().is_some()).map(|e| e.key.stable_id.clone()).collect();
                if ids.is_empty() { return; }
                wanted = Some(ids);
            } else {
                let stale: Vec<_> = self.registry.endpoints().iter().filter(|e| e.key.protocol == Kind::Chromecast && Some(&e.key) != self.active.as_ref() && !e.fresh(now)).map(|e| e.key.clone()).collect();
                for key in stale { self.registry.ask(&key, operation - 1); }
            }
        } else {
            // A new search replaces what the previous one still had to describe.
            self.queue.retain(|q| !matches!(&q.job, Job::Describe { .. }));
        }
        let (network, sender, generation, mine) = (self.network.clone(), self.sender.clone(), self.generation.clone(), self.generation());
        let scan = self.scan(protocol);
        let running = Arc::new(AtomicBool::new(true));
        scan.running = running.clone();
        scan.alive.store(true, Ordering::Release);
        let alive = Alive(scan.alive.clone());
        let name = if protocol == Kind::Dlna { "chilpa-endpoint-search" } else { "chilpa-endpoint-browse" };
        let spawned = std::thread::Builder::new().name(name.into()).spawn(move || {
            let _alive = alive;
            let current = || generation.load(Ordering::Acquire) == mine;
            if protocol == Kind::Dlna {
                let result = if current() { network.search(SEARCH_WINDOW, &running) } else { Ok(Vec::new()) };
                let _ = sender.send(Event::Located { operation, generation: mine, result });
                return;
            }
            let window = if purpose == Purpose::Check { CHECK_WINDOW } else { BROWSE_WINDOW };
            let mut stopped = !current();
            let result = if stopped { Ok(()) } else {
                network.browse(window, &running, &mut |sighting| {
                    if let Some(ids) = &mut wanted { ids.remove(&sighting.device.id); }
                    let more = sender.send(Event::Sighted { operation, generation: mine, sighting }).is_ok() && wanted.as_ref().is_none_or(|ids| !ids.is_empty());
                    stopped |= !more;
                    more
                })
            };
            let exhausted = result.is_ok() && !stopped && current() && running.load(Ordering::Acquire);
            let _ = sender.send(Event::Browsed { operation, generation: mine, exhausted, failed: result.is_err() });
        });
        // A thread that never started dropped its guard with the closure.
        if spawned.is_err() {
            self.notices.search_failed |= purpose == Purpose::Discover;
            if protocol == Kind::Chromecast { self.unask_receivers(); }
            return;
        }
        let scan = self.scan(protocol);
        scan.started = Some(now);
        scan.run = Some(Run { operation, purpose, until: now + PASS_LIMIT, located: false, requested });
    }
    fn unask_receivers(&mut self) {
        let asked: Vec<_> = self.registry.endpoints().iter().filter(|e| e.key.protocol == Kind::Chromecast && e.asked().is_some()).map(|e| e.key.clone()).collect();
        for key in asked { self.registry.unask(&key); }
    }
    fn dispatch(&mut self) {
        while self.probing.load(Ordering::Acquire) < PROBES {
            let Some(Queued { operation, job }) = self.queue.pop_front() else { break };
            let url = match &job { Job::Probe { url, .. } | Job::Describe { url, .. } => url.clone() };
            self.probing.fetch_add(1, Ordering::AcqRel);
            let flight = Flight(self.probing.clone());
            let (network, sender, generation, mine) = (self.network.clone(), self.sender.clone(), self.generation.clone(), self.generation());
            let spawned = std::thread::Builder::new().name("chilpa-endpoint-probe".into()).spawn(move || {
                let _flight = flight;
                let result = if generation.load(Ordering::Acquire) == mine { network.describe(&url, PROBE_LIMIT) } else { Err(Error::Network) };
                let _ = sender.send(Event::Answered { operation, generation: mine, result });
            });
            match spawned {
                Ok(_) => { self.flying.insert(operation, job); }
                Err(_) => if let Job::Probe { key, .. } = &job { self.registry.unask(key); },
            }
        }
    }
    /// Writes only what changed for good; answers alone change nothing stored.
    /// A write that failed is tried again with the next change, and once
    /// more when the registry goes (`last`): never on every round.
    fn persist(&mut self, last: bool) {
        let Some(writer) = &self.writer else { return };
        match writer.outcome() {
            store::SAVED => { self.notices.unsaved = false; self.unwritten = false; }
            store::FAILED => { self.notices.unsaved = true; self.unwritten = true; }
            store::TOO_LARGE => { self.notices.unsaved = true; self.notices.full = true; self.unwritten = true; }
            _ => {}
        }
        if !self.loaded || self.notices.kept.is_some() { return; }
        let changed = self.registry.take_dirty();
        if changed || (last && self.unwritten) { writer.save(self.registry.stored(), std::mem::take(&mut self.replace)); }
    }
}
impl Drop for Controller {
    /// Threads end on their own deadlines; nobody waits for the network here.
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.upnp.cancel(); self.cast.cancel();
        self.persist(true);
    }
}
