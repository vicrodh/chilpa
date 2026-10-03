//! Durable, bounded delivery of output facts to native plugins.
use crate::natives::Natives;
use chilpa_core::playback_events::Observer;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, io::Write, path::{Path, PathBuf}, sync::{Arc, Mutex, Weak, atomic::{AtomicBool, AtomicU64, Ordering}}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

const LIMIT: usize = 1024;
#[derive(Clone, Copy, PartialEq)]
enum Receipt { Accepted, Discard }
#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    #[serde(skip)]
    cancelled: Arc<AtomicBool>,
    plugin: String,
    start: Value,
    latest: Value,
    start_acknowledged: bool,
    ack_sequence: u64,
    // An ambiguous response must retry the exact request, even when the
    // audio thread has supplied a more recent checkpoint meanwhile.
    in_flight: Option<Value>,
}
impl Entry {
    fn next(&mut self) -> Option<Value> {
        if self.in_flight.is_none() {
            self.in_flight = if !self.start_acknowledged { Some(self.start.clone()) }
                else if self.latest["sequence"].as_u64().unwrap_or(0) > self.ack_sequence { Some(self.latest.clone()) }
                else { None };
        }
        self.in_flight.clone()
    }
}

pub(crate) struct Reports {
    entries: Mutex<HashMap<String, Entry>>,
    saving: Mutex<()>,
    revision: AtomicU64,
    saved: AtomicU64,
    path: PathBuf,
    blocked: bool,
}
impl Reports {
    fn load(path: PathBuf) -> Self {
        let loaded = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<HashMap<String, Entry>>(&bytes).map_err(|_| ()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(_) => Err(()),
        };
        let blocked = !loaded.as_ref().is_ok_and(|entries| entries.len() <= LIMIT);
        let mut entries = loaded.unwrap_or_default();
        if blocked { entries.clear(); eprintln!("playback-events: saved queue needs recovery"); }
        for entry in entries.values_mut() {
            if entry.latest["kind"] != "ended" {
                entry.latest["kind"] = json!("ended");
                entry.latest["reason"] = json!("shutdown");
                entry.latest["sequence"] = json!(entry.latest["sequence"].as_u64().unwrap_or(0) + 1);
            }
        }
        Self { entries: Mutex::new(entries), saving: Mutex::new(()), revision: AtomicU64::new(1), saved: AtomicU64::new(0), path, blocked }
    }
    pub(crate) fn start(natives: Weak<Natives>) -> Arc<Self> {
        let reports = Arc::new(Self::load(chilpa_dirs::data().join("native-playback-events.json")));
        let worker = reports.clone();
        let _ = std::thread::Builder::new().name("chilpa-playback-events".into()).spawn(move || {
            let mut retry = HashMap::<String, Instant>::new();
            loop {
                if worker.blocked { break; }
                worker.deliver(&mut retry, |plugin, event| {
                    let natives = natives.upgrade()?;
                    let client = natives.client(plugin)?;
                    let started = event["kind"] == "started";
                    let answer = client.call("playback.event", event, Duration::from_secs(2)).ok()?;
                    if answer["accepted"] == true { if started { natives.history_changed(plugin); } Some(Receipt::Accepted) }
                    else if answer["accepted"] == false && answer["discard"] == true { Some(Receipt::Discard) }
                    else { None }
                });
                if natives.strong_count() == 0 { let _ = worker.persist(); break; }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        reports
    }
    pub(crate) fn forget(&self, plugin: &str) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|_, entry| {
            if entry.plugin != plugin { return true; }
            entry.cancelled.store(true, Ordering::Release);
            false
        });
        self.changed();
        drop(entries);
        let _ = self.persist();
    }
    fn changed(&self) { self.revision.fetch_add(1, Ordering::Release); }
    fn persist(&self) -> std::io::Result<()> {
        let _saving = self.saving.lock().unwrap_or_else(|e| e.into_inner());
        let (revision, entries) = {
            let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            let revision = self.revision.load(Ordering::Acquire);
            if self.saved.load(Ordering::Acquire) == revision { return Ok(()); }
            (revision, entries.clone())
        };
        save(&self.path, &entries)?;
        self.saved.store(revision, Ordering::Release);
        Ok(())
    }
    fn deliver(&self, retry: &mut HashMap<String, Instant>, mut send: impl FnMut(&str, Value) -> Option<Receipt>) {
        let pending: Vec<_> = {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            entries.iter_mut().filter_map(|(id, entry)| {
                if retry.get(id).is_some_and(|at| *at > Instant::now()) { return None; }
                entry.next().map(|event| (id.clone(), entry.plugin.clone(), event))
            }).collect()
        };
        if !pending.is_empty() { self.changed(); }
        if self.persist().is_err() { eprintln!("playback-events: queue could not be saved"); return; }
        for (id, plugin, event) in pending {
            if self.persist().is_err() { return; }
            let Some(receipt) = send(&plugin, event.clone()) else {
                retry.insert(id, Instant::now() + Duration::from_secs(5));
                continue;
            };
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            if receipt == Receipt::Discard {
                if let Some(entry) = entries.get(&id) { entry.cancelled.store(true, Ordering::Release); }
            }
            if receipt == Receipt::Discard || event["kind"] == "ended" { entries.remove(&id); retry.remove(&id); }
            else if let Some(entry) = entries.get_mut(&id) {
                if event["kind"] == "started" { entry.start_acknowledged = true; }
                else { entry.ack_sequence = event["sequence"].as_u64().unwrap_or(0); }
                entry.in_flight = None;
            }
            self.changed();
        }
        if self.persist().is_err() { eprintln!("playback-events: acknowledgement could not be saved"); }
    }
    /// Shutdown waits only for local persistence, not upstream reporting.
    pub(crate) fn flush(&self) {
        if self.blocked { return; }
        let revision = self.revision.load(Ordering::Acquire);
        let until = Instant::now() + Duration::from_secs(3);
        while self.saved.load(Ordering::Acquire) < revision && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        if self.saved.load(Ordering::Acquire) < revision { eprintln!("playback-events: shutdown persistence timed out"); }
    }
    pub(crate) fn observer(self: &Arc<Self>, plugin: &str, resolution: &str) -> Arc<dyn Observer> {
        Arc::new(Playback { reports: self.clone(), plugin: plugin.into(), resolution: resolution.into(),
            id: playback_id(), cancelled: Arc::new(AtomicBool::new(false)), state: Mutex::new(Observed::default()) })
    }
}
fn save(path: &Path, entries: &HashMap<String, Entry>) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| std::io::Error::other("missing state directory"))?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut file, entries)?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}
fn playback_id() -> String {
    let mut bytes = rand::random::<[u8; 16]>();
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}
#[derive(Default)]
struct Observed { played_ms: u64, state: &'static str, sequence: u64, ended: bool, started_at: u64 }
struct Playback { reports: Arc<Reports>, plugin: String, resolution: String, id: String, cancelled: Arc<AtomicBool>, state: Mutex<Observed> }
impl Observer for Playback {
    fn update(&self, frames: u64, rate: u32, state: &'static str, ended: Option<&'static str>) {
        if frames == 0 || rate == 0 || self.reports.blocked || self.cancelled.load(Ordering::Acquire) { return; }
        let played_ms = frames.saturating_mul(1000) / u64::from(rate);
        let mut observed = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if observed.ended { return; }
        if observed.started_at == 0 {
            observed.started_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        }
        if ended.is_none() && observed.sequence > 0 && observed.state == state && played_ms < observed.played_ms + 1000 { return; }
        let mut entries = self.reports.entries.lock().unwrap_or_else(|e| e.into_inner());
        if self.cancelled.load(Ordering::Acquire) || (!entries.contains_key(&self.id) && entries.len() >= LIMIT) { return; }
        observed.played_ms = played_ms.max(observed.played_ms);
        observed.state = state;
        observed.sequence += 1;
        observed.ended = ended.is_some();
        let latest = json!({"kind": if ended.is_some() {"ended"} else {"progress"}, "playback_id":self.id,
            "sequence":observed.sequence, "played_ms":observed.played_ms, "state":state, "reason":ended});
        let entry = entries.entry(self.id.clone()).or_insert_with(|| Entry {
            cancelled: self.cancelled.clone(), plugin: self.plugin.clone(), start: json!({"kind":"started", "playback_id":self.id,
                "resolution_id":self.resolution, "started_at":observed.started_at}), latest: Value::Null,
            start_acknowledged: false, ack_sequence: 0, in_flight: None });
        entry.latest = latest;
        self.reports.changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, Arc<Reports>) {
        let dir = tempfile::tempdir().unwrap();
        let reports = Arc::new(Reports::load(dir.path().join("events.json")));
        (dir, reports)
    }
    #[test]
    fn a_resolved_resource_has_no_events_until_consumed_and_repeat_has_a_new_id() {
        let (_dir, reports) = setup();
        let first = reports.observer("example.source", "ticket-a");
        let second = reports.observer("example.source", "ticket-a");
        assert!(reports.entries.lock().unwrap().is_empty());
        first.update(1000, 1000, "playing", Some("finished"));
        second.update(2000, 1000, "playing", Some("finished"));
        let entries = reports.entries.lock().unwrap();
        assert_eq!(entries.len(), 2);
        for (id, entry) in entries.iter() {
            assert_eq!(id.len(), 36);
            assert_eq!(&id[14..15], "4");
            assert_eq!(entry.start["resolution_id"], "ticket-a");
        }
    }
    #[test]
    fn an_unacknowledged_request_retries_unchanged_before_the_final_checkpoint() {
        let (_dir, reports) = setup();
        let observer = reports.observer("example.source", "ticket-a");
        let mut retry = HashMap::new();
        observer.update(1000, 1000, "playing", None);
        let mut events = Vec::new();
        reports.deliver(&mut retry, |_, event| { events.push(event); None });
        observer.update(12500, 1000, "paused", Some("skipped"));
        retry.clear();
        reports.deliver(&mut retry, |_, event| { events.push(event); Some(Receipt::Accepted) });
        reports.deliver(&mut retry, |_, event| { events.push(event); Some(Receipt::Accepted) });
        assert_eq!(events[0], events[1]);
        assert_eq!(events[2]["kind"], "ended");
        assert_eq!(events[2]["played_ms"], 12500);
        assert!(reports.entries.lock().unwrap().is_empty());
        assert!(Reports::load(reports.path.clone()).entries.lock().unwrap().is_empty());
    }
    #[test]
    fn restart_ends_only_the_last_confirmed_checkpoint_and_preserves_identity() {
        let (_dir, reports) = setup();
        let observer = reports.observer("example.source", "ticket-a");
        observer.update(4321, 1000, "paused", None);
        let mut retry = HashMap::new();
        reports.deliver(&mut retry, |_, _| Some(Receipt::Accepted));
        let restored = Reports::load(reports.path.clone());
        let mut events = Vec::new();
        restored.deliver(&mut retry, |_, event| { events.push(event); Some(Receipt::Accepted) });
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["kind"], "ended");
        assert_eq!(events[0]["played_ms"], 4321);
        assert_eq!(events[0]["reason"], "shutdown");
    }
    #[test]
    fn newer_samples_cannot_change_a_progress_request_awaiting_acknowledgement() {
        let (_dir, reports) = setup();
        let observer = reports.observer("example.source", "ticket-a");
        let mut retry = HashMap::new();
        observer.update(1000, 1000, "playing", None);
        reports.deliver(&mut retry, |_, _| Some(Receipt::Accepted));
        let mut events = Vec::new();
        reports.deliver(&mut retry, |_, event| { events.push(event); None });
        observer.update(2000, 1000, "seeking", None);
        retry.clear();
        reports.deliver(&mut retry, |_, event| { events.push(event); Some(Receipt::Accepted) });
        reports.deliver(&mut retry, |_, event| { events.push(event); Some(Receipt::Accepted) });
        assert_eq!(events[0], events[1]);
        assert_eq!(events[2]["played_ms"], 2000);
        assert!(events[2]["sequence"].as_u64() > events[1]["sequence"].as_u64());
    }
    #[test]
    fn an_invalid_saved_queue_is_preserved_for_recovery() {
        let (dir, _) = setup();
        let path = dir.path().join("events.json");
        std::fs::write(&path, b"broken").unwrap();
        let reports = Arc::new(Reports::load(path.clone()));
        reports.observer("example.source", "ticket").update(5000, 1000, "playing", Some("finished"));
        assert!(reports.blocked);
        assert!(reports.entries.lock().unwrap().is_empty());
        assert_eq!(std::fs::read(path).unwrap(), b"broken");
    }
    #[test]
    fn account_rejection_discards_the_entire_occurrence() {
        let (_dir, reports) = setup();
        let observer = reports.observer("example.source", "old-ticket");
        observer.update(5000, 1000, "playing", None);
        reports.deliver(&mut HashMap::new(), |_, _| Some(Receipt::Discard));
        observer.update(9000, 1000, "playing", Some("finished"));
        assert!(reports.entries.lock().unwrap().is_empty());
    }
    #[test]
    fn events_cross_the_native_rpc_transport_in_order() {
        use std::io::{BufRead, BufReader};
        use std::net::{TcpListener, TcpStream};
        use chilpa_plugin::native::NativeClient;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let recording = std::thread::spawn(move || {
            let mut reader = BufReader::new(peer.try_clone().unwrap());
            let mut events = Vec::new();
            for _ in 0..2 {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], "playback.event");
                events.push(request["params"].clone());
                writeln!(peer, "{}", json!({"jsonrpc":"2.0","id":request["id"],"result":{"accepted":true}})).unwrap();
            }
            events
        });
        let client = NativeClient::start(socket.try_clone().unwrap(), socket, || {}, |_, _| {});
        let (_dir, reports) = setup();
        reports.observer("example.source", "exact-ticket").update(12500, 1000, "playing", Some("finished"));
        let worker = std::thread::spawn(move || {
            let mut retry = HashMap::new();
            for _ in 0..2 {
                reports.deliver(&mut retry, |_, event| {
                    client.call("playback.event", event, Duration::from_secs(2)).ok()
                        .filter(|answer| answer["accepted"] == true).map(|_| Receipt::Accepted)
                });
            }
            assert!(reports.entries.lock().unwrap().is_empty());
            client.stop();
        });
        worker.join().unwrap();
        let events = recording.join().unwrap();
        assert_eq!(events[0]["kind"], "started");
        assert_eq!(events[0]["resolution_id"], "exact-ticket");
        assert_eq!(events[1]["kind"], "ended");
        assert_eq!(events[1]["played_ms"], 12500);
        assert_eq!(events[0]["playback_id"], events[1]["playback_id"]);
    }

}
