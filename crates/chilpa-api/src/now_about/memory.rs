//! I serialize on-demand information reads and release the worker at idle.

use serde_json::{json, Value};
use std::{collections::{HashMap, VecDeque}, sync::{Arc, LazyLock, Mutex}, time::{Duration, Instant}};

struct Known { value: Value, expires: Instant, bytes: usize, scope: String, ticket: u64 }
struct Identity { key: String, scope: String, ticket: u64, refresh: bool }
struct Job { identity: Identity, catalog: Arc<crate::source_catalog::Catalog>, read: Box<dyn FnOnce() -> Value + Send> }
#[derive(Default)]
struct State { known: HashMap<String, Known>, queue: VecDeque<Job>, active: Option<String>, running: bool, ticket: u64 }
static STATE: LazyLock<Mutex<State>> = LazyLock::new(Default::default);
const ANSWERS: usize = 256;
const BYTES: usize = 2 * 1024 * 1024;
const JOBS: usize = 2;

pub(super) fn kept(key: &str) -> Option<Value> {
    let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = state.known.get(key).filter(|entry| entry.expires > Instant::now()) { return Some(entry.value.clone()); }
    state.pending(key).then(|| json!({"loading":true}))
}

/// I retry only a failed panel, retaining successful answers and live jobs.
pub(super) fn retry_failed(key: &str) -> bool {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).retry_failed(key)
}

impl State {
    fn pending(&self, key: &str) -> bool {
        self.active.as_deref() == Some(key) || self.queue.iter().any(|job| job.identity.key == key)
    }
    fn retry_failed(&mut self, key: &str) -> bool {
        if self.pending(key) { return false; }
        let failed = self.known.get(key).is_some_and(|entry| entry.value["failed"] == true || entry.value["busy"] == true);
        if failed { self.known.remove(key); }
        failed
    }
    fn finish(&mut self, identity: &Identity, mut found: Value) -> bool {
        let Some(previous) = self.known.get(&identity.key).filter(|entry| entry.ticket == identity.ticket) else { return false; };
        let failed = found["failed"] == true || found["busy"] == true;
        if identity.refresh && failed {
            // I keep the displayed successful answer as well as the permanent database rows.
            let mut retained = previous.value.clone();
            if retained["facts"].is_array() || retained["credits"].is_array() { found = std::mem::take(&mut retained); }
            found["failed"] = json!(true);
            found["refetch_failed"] = json!(true);
        } else if identity.refresh {
            self.known.retain(|key, entry| entry.scope != identity.scope || key == &identity.key);
            self.queue.retain(|job| job.identity.scope != identity.scope);
        }
        found["loading"] = json!(false);
        insert(self, identity.key.clone(), found, Duration::from_secs(300));
        if let Some(entry) = self.known.get_mut(&identity.key) { entry.scope = identity.scope.clone(); }
        trim(self);
        true
    }
}

fn insert(state: &mut State, key: String, mut value: Value, lifetime: Duration) {
    let mut bytes = value.to_string().len() + key.len();
    if bytes > BYTES / 2 {
        value = json!({"loading":false, "failed":true});
        bytes = value.to_string().len() + key.len();
    }
    state.ticket = state.ticket.wrapping_add(1);
    state.known.insert(key, Known { value, expires: Instant::now() + lifetime, bytes, scope: String::new(), ticket: state.ticket });
    trim(state);
}

fn trim(state: &mut State) {
    while state.known.len() > ANSWERS || state.known.values().map(|entry| entry.bytes + entry.scope.len()).sum::<usize>() > BYTES {
        let oldest = state.known.iter().min_by_key(|(_, entry)| entry.expires).map(|(key, _)| key.clone());
        if let Some(key) = oldest { state.known.remove(&key); } else { break; }
    }
}

fn enqueue(state: &mut State, catalog: Arc<crate::source_catalog::Catalog>, key: String, scope: String,
    refresh: bool, read: impl FnOnce() -> Value + Send + 'static) -> Value {
    if state.queue.len() >= JOBS {
        if let Some(old) = state.queue.pop_front() { state.known.remove(&old.identity.key); }
    }
    let mut loading = if refresh { state.known.get(&key).map(|entry| entry.value.clone()).unwrap_or_default() } else { Value::Null };
    if !loading.is_object() { loading = json!({}); }
    loading["loading"] = json!(true);
    loading["failed"] = json!(false);
    loading["refetch_failed"] = json!(false);
    insert(state, key.clone(), loading.clone(), Duration::from_secs(300));
    let ticket = state.ticket;
    if let Some(entry) = state.known.get_mut(&key) { entry.scope = scope.clone(); }
    trim(state);
    state.queue.push_back(Job { identity: Identity { key: key.clone(), scope, ticket, refresh }, catalog, read: Box::new(read) });
    if !state.running {
        state.running = true;
        if std::thread::Builder::new().name("chilpa-about".into()).spawn(|| loop {
            let job = {
                let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
                match state.queue.pop_front() {
                    Some(job) => { state.active = Some(job.identity.key.clone()); job },
                    None => { state.running = false; return; }
                }
            };
            let found = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job.read))
                .unwrap_or_else(|_| json!({"failed":true}));
            let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
            state.active = None;
            let changed = state.finish(&job.identity, found);
            drop(state);
            if changed { job.catalog.changed(); }
        }).is_err() {
            state.running = false;
            while let Some(job) = state.queue.pop_front() { state.finish(&job.identity, json!({"failed":true})); }
            return state.known.get(&key).map(|entry| entry.value.clone()).unwrap_or_else(|| json!({"loading":false,"busy":true}));
        }
    }
    loading
}

pub(super) fn read(catalog: Arc<crate::source_catalog::Catalog>, key: String, scope: String, read: impl FnOnce() -> Value + Send + 'static) -> Value {
    if key.len() > 16 * 1024 || scope.len() > 4096 { return json!({"loading":false, "failed":true}); }
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = state.known.get(&key).filter(|entry| entry.expires > Instant::now()) { return entry.value.clone(); }
    if state.pending(&key) { return json!({"loading":true}); }
    enqueue(&mut state, catalog, key, scope, false, read)
}

pub(super) fn refresh(catalog: Arc<crate::source_catalog::Catalog>, key: String, scope: String, read: impl FnOnce() -> Value + Send + 'static) -> bool {
    if key.len() > 16 * 1024 || scope.len() > 4096 { return false; }
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if state.pending(&key) { return false; }
    enqueue(&mut state, catalog, key, scope, true, read);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_invalidates_only_a_failed_panel_and_never_duplicates_an_active_lookup() {
        let key = "retry:failure";
        let good = "retry:success";
        let mut state = State::default();
        insert(&mut state, key.into(), json!({"loading":false,"failed":true}), Duration::from_secs(300));
        insert(&mut state, good.into(), json!({"loading":false,"facts":["known"]}), Duration::from_secs(300));
        state.active = Some(key.into());
        assert!(!state.retry_failed(key));
        state.active = None;
        assert!(!state.retry_failed(good));
        assert!(state.retry_failed(key));
        assert!(!state.retry_failed(key));
        assert!(state.known.contains_key(good));
    }

    #[test]
    fn oversized_answers_finish_with_an_error_and_retained_answers_stay_within_budget() {
        let mut state = State::default();
        let lifetime = Duration::from_secs(300);
        insert(&mut state, "large".into(), json!({"loading":true}), lifetime);
        insert(&mut state, "large".into(), json!({"credits":"x".repeat(BYTES)}), lifetime);
        assert_eq!(state.known["large"].value["loading"], false);
        assert_eq!(state.known["large"].value["failed"], true);
        for index in 0..ANSWERS + 20 {
            insert(&mut state, index.to_string(), json!({"loading":false, "facts":[]}), lifetime);
        }
        assert!(state.known.len() <= ANSWERS);
        assert!(state.known.values().map(|entry| entry.bytes).sum::<usize>() <= BYTES);
    }
    #[test]
    fn refresh_keeps_previous_facts_on_failure_and_invalidates_only_its_album_on_success() {
        let mut state = State::default();
        for (key, scope) in [("album", "one"), ("track", "one"), ("other", "two")] {
            insert(&mut state, key.into(), json!({"facts":[["Label","Original"]],"loading":true}), Duration::from_secs(300));
            state.known.get_mut(key).unwrap().scope = scope.into();
        }
        let identity = Identity { key:"album".into(), scope:"one".into(), ticket:state.known["album"].ticket, refresh:true };
        assert!(state.finish(&identity, json!({"failed":true})));
        assert_eq!(state.known["album"].value["facts"][0][1], "Original");
        assert_eq!(state.known["album"].value["refetch_failed"], true);
        assert!(state.known.contains_key("track"));
        assert!(!state.finish(&identity, json!({"facts":["obsolete"]})), "an old ticket cannot replace a newer answer");
        let identity = Identity { ticket:state.known["album"].ticket, ..identity };
        assert!(state.finish(&identity, json!({"facts":["updated"]})));
        assert!(!state.known.contains_key("track"));
        assert!(state.known.contains_key("other"));
        assert_eq!(state.known["album"].value["loading"], false);
    }

}
