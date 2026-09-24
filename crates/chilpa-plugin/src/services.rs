//! What the host lends a plugin.
//!
//! A wasm guest has no network, no disk and no clock: only what Chilpa hands
//! it exists. These are v1's four doors, and there is no fifth (ADR-007).
//!
//! The rule that governs the rest: **a plugin never sees a credential.** Not
//! because it is asked not to keep one, but because the host makes the
//! request and the token is added on this side. It was never in the guest's
//! memory.
//!
//! This crate knows neither how to speak HTTP nor how to open a database, on
//! purpose: whoever uses it implements that. That way the plugin host drags
//! in no network stack, and the tests can look at exactly what a plugin
//! asked for.

/// How important a log line is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn from_code(n: i32) -> Self {
        match n {
            0 => Level::Debug,
            2 => Level::Warn,
            3 => Level::Error,
            _ => Level::Info,
        }
    }
}

/// What the host does for a plugin.
///
/// Everything carries the plugin's id because what is kept, asked for and
/// noted is its own and nobody else's.
pub trait HostServices: Send {
    /// A line to the log, with the plugin in front.
    fn log(&mut self, plugin: &str, level: Level, message: &str);

    /// The time, in unix seconds.
    fn now_unix(&mut self) -> i64;

    /// Reads from the plugin's store. `None` if there is nothing under that key.
    fn store_get(&mut self, plugin: &str, key: &str) -> Option<Vec<u8>>;

    /// Writes to the plugin's store. False if it could not be done.
    fn store_set(&mut self, plugin: &str, key: &str, value: &[u8]) -> bool;

    /// Makes a request on the plugin's behalf.
    ///
    /// It takes and returns JSON. **Whoever implements this supplies the
    /// credential**; the plugin only said where to and with what body. And
    /// it cannot wait forever: the guest can be cut off by fuel or by the
    /// clock, but a host call that hangs hangs the thread.
    fn http(&mut self, plugin: &str, request: &[u8]) -> Vec<u8>;

    /// A place where whoever implements this can keep whatever they need
    /// to renew credentials, if they handle any.
    ///
    /// It is on the trait because the implementor arrives here as a `dyn
    /// HostServices` and its type is no longer known. Returning `None` is
    /// the normal case: a test host has no accounts to renew.
    fn auth_map(&self) -> Option<std::sync::Arc<std::sync::Mutex<AuthMap>>> {
        None
    }
}

/// How each plugin connects its account, by id.
pub type AuthMap = std::collections::BTreeMap<String, chilpa_auth::Auth>;

/// A host that allows nothing. It serves plugins that asked for no
/// capability at all, and the tests.
#[derive(Default)]
pub struct NoServices;

impl HostServices for NoServices {
    fn log(&mut self, _plugin: &str, _level: Level, _message: &str) {}
    fn now_unix(&mut self) -> i64 {
        0
    }
    fn store_get(&mut self, _plugin: &str, _key: &str) -> Option<Vec<u8>> {
        None
    }
    fn store_set(&mut self, _plugin: &str, _key: &str, _valor: &[u8]) -> bool {
        false
    }
    fn http(&mut self, _plugin: &str, _request: &[u8]) -> Vec<u8> {
        br#"{"error":"este plugin no tiene permiso de red"}"#.to_vec()
    }
}

/// A fake host that notes down everything it was asked for.
///
/// It is what lets the hostile plugin suite claim not only that Chilpa
/// survived, but that the plugin never reached where it should not.
///
/// It can be cloned: one copy goes to the host and the test keeps another
/// to look at what happened. That is why what is noted lives behind its own
/// lock and not the one wrapping the host.
#[derive(Default, Clone)]
pub struct TestHost(std::sync::Arc<std::sync::Mutex<Recorded>>);

/// Everything a plugin asked the test host for.
#[derive(Default)]
pub struct Recorded {
    pub logs: Vec<(String, Level, String)>,
    pub reads: Vec<(String, String)>,
    pub writes: Vec<(String, String, Vec<u8>)>,
    pub requests: Vec<(String, String)>,
    /// What `store_get` answers, by key.
    pub store: std::collections::HashMap<String, Vec<u8>>,
    /// What `http` answers. Empty gives a filler reply.
    pub http_reply: Vec<u8>,
    pub clock: i64,
}

impl TestHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Looks at what was noted without being able to change it.
    pub fn examine<T>(&self, f: impl FnOnce(&Recorded) -> T) -> T {
        let save = self.0.lock().expect("el candado del host de prueba");
        f(&save)
    }

    /// Sets something up before running.
    pub fn prepare(&self, f: impl FnOnce(&mut Recorded)) {
        let mut save = self.0.lock().expect("el candado del host de prueba");
        f(&mut save);
    }
}

impl HostServices for TestHost {
    fn log(&mut self, plugin: &str, level: Level, message: &str) {
        self.prepare(|a| a.logs.push((plugin.to_string(), level, message.to_string())));
    }
    fn now_unix(&mut self) -> i64 {
        self.examine(|a| a.clock)
    }
    fn store_get(&mut self, plugin: &str, key: &str) -> Option<Vec<u8>> {
        let mut save = self.0.lock().expect("el candado del host de prueba");
        save.reads.push((plugin.to_string(), key.to_string()));
        save.store.get(key).cloned()
    }
    fn store_set(&mut self, plugin: &str, key: &str, value: &[u8]) -> bool {
        let mut save = self.0.lock().expect("el candado del host de prueba");
        save.writes.push((plugin.to_string(), key.to_string(), value.to_vec()));
        save.store.insert(key.to_string(), value.to_vec());
        true
    }
    fn http(&mut self, plugin: &str, request: &[u8]) -> Vec<u8> {
        let mut save = self.0.lock().expect("el candado del host de prueba");
        save
            .requests
            .push((plugin.to_string(), String::from_utf8_lossy(request).to_string()));
        if save.http_reply.is_empty() {
            br#"{"status":200,"body":""}"#.to_vec()
        } else {
            save.http_reply.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_no_services_the_network_answers_no() {
        let mut s = NoServices;
        let r = s.http("com.a.b", b"{}");
        assert!(String::from_utf8_lossy(&r).contains("no tiene permiso"));
        assert_eq!(s.store_get("com.a.b", "x"), None);
        assert!(!s.store_set("com.a.b", "x", b"y"), "no deja escribir");
    }

    #[test]
    fn the_test_host_remembers_everything() {
        let mut h = TestHost::new();
        h.prepare(|a| a.clock = 1234);
        h.log("com.a.b", Level::Warn, "algo");
        h.store_set("com.a.b", "token", b"secreto");
        assert_eq!(h.store_get("com.a.b", "token").as_deref(), Some(&b"secreto"[..]));
        h.http("com.a.b", br#"{"url":"https://api.ejemplo.com"}"#);

        h.examine(|a| {
            assert_eq!(a.logs.len(), 1);
            assert_eq!(a.logs[0].1, Level::Warn);
            assert_eq!(a.writes.len(), 1);
            assert_eq!(a.reads.len(), 1);
            assert_eq!(a.requests.len(), 1);
            assert!(a.requests[0].1.contains("api.ejemplo.com"));
        });
        assert_eq!(h.now_unix(), 1234);
    }

    #[test]
    fn a_clone_of_the_test_host_sees_the_same() {
        let mut h = TestHost::new();
        let copy = h.clone();
        h.log("com.a.b", Level::Info, "algo");
        assert_eq!(copy.examine(|a| a.logs.len()), 1, "la copia mira el mismo cuaderno");
    }

    #[test]
    fn unknown_levels_fall_back_to_info() {
        assert_eq!(Level::from_code(0), Level::Debug);
        assert_eq!(Level::from_code(1), Level::Info);
        assert_eq!(Level::from_code(2), Level::Warn);
        assert_eq!(Level::from_code(3), Level::Error);
        assert_eq!(Level::from_code(99), Level::Info, "lo raro no se pierde, se degrada");
    }
}
