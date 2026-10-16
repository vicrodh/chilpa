//! A process plugin's channel: JSON-RPC 2.0, one object per line, over its
//! standard input and output.
//!
//! Every call carries an id and waits only for its own answer, so a slow
//! call never holds a fast one behind it, and any thread can ask at once.
//! Nothing here starts the process: the sandbox that launches it hands in
//! its pipes and a way to end it.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Longest line read from a plugin. A longer one ends the channel: a reply
/// cannot be cut and still be the same reply.
pub const MAX_LINE: usize = 32 * 1024 * 1024;
/// Lines waiting to be written. A plugin that stops reading fills this, and
/// the next call fails at once instead of blocking whoever asked.
const OUT_QUEUE: usize = 256;

#[derive(Clone, Debug, PartialEq)]
pub enum NativeError {
    /// The plugin answered with an error of its own. `data` is what it
    /// added, such as how long to wait before asking again.
    Rpc { code: i64, message: String, data: Option<Value> },
    Timeout,
    /// The process ended, or its pipes did.
    Closed,
    /// The plugin is not reading what it is sent.
    Busy,
}

impl std::fmt::Display for NativeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NativeError::Rpc { message, .. } => f.write_str(message),
            NativeError::Timeout => f.write_str("the plugin did not answer in time"),
            NativeError::Closed => f.write_str("the plugin is not running"),
            NativeError::Busy => f.write_str("the plugin is not reading its input"),
        }
    }
}

type Waiter = Sender<Result<Value, NativeError>>;

pub struct NativeClient {
    out: SyncSender<Vec<u8>>,
    next_id: AtomicU64,
    waiting: Arc<Mutex<HashMap<u64, Waiter>>>,
    closed: Arc<AtomicBool>,
    end: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl NativeClient {
    /// Starts the reader and writer threads over a plugin's pipes. What the
    /// plugin sends on its own (a log line) goes to `notice`, on the reader
    /// thread, so it has to be short.
    pub fn start(
        input: impl Write + Send + 'static,
        output: impl Read + Send + 'static,
        end: impl FnOnce() + Send + 'static,
        notice: impl Fn(&str, &Value) + Send + 'static,
    ) -> Arc<Self> {
        let (out, lines) = mpsc::sync_channel::<Vec<u8>>(OUT_QUEUE);
        let waiting: Arc<Mutex<HashMap<u64, Waiter>>> = Arc::default();
        let closed = Arc::new(AtomicBool::new(false));
        let client = Arc::new(Self { out, next_id: AtomicU64::new(1), waiting: waiting.clone(), closed: closed.clone(),
            end: Mutex::new(Some(Box::new(end))) });

        let writer_closed = closed.clone();
        let _ = std::thread::Builder::new().name("chilpa-native-out".into()).spawn(move || {
            let mut input = input;
            for line in lines {
                if input.write_all(&line).and_then(|()| input.write_all(b"\n")).and_then(|()| input.flush()).is_err() {
                    break;
                }
            }
            writer_closed.store(true, Ordering::SeqCst);
        });

        let _ = std::thread::Builder::new().name("chilpa-native-in".into()).spawn(move || {
            let mut reader = BufReader::new(output);
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.by_ref().take(MAX_LINE as u64 + 1).read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line.len() > MAX_LINE => break,
                    Ok(_) => {}
                }
                let Ok(message) = serde_json::from_slice::<Value>(&line) else { continue };
                match message.get("id").and_then(Value::as_u64) {
                    Some(id) if message.get("method").is_none() => {
                        let answer = match message.get("error") {
                            Some(error) => Err(NativeError::Rpc {
                                code: error.get("code").and_then(Value::as_i64).unwrap_or(-32000),
                                message: error.get("message").and_then(Value::as_str).unwrap_or("").to_string(),
                                data: error.get("data").cloned(),
                            }),
                            None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                        };
                        let waiter = waiting.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                        if let Some(waiter) = waiter {
                            let _ = waiter.send(answer);
                        }
                    }
                    _ => {
                        if let Some(method) = message.get("method").and_then(Value::as_str) {
                            notice(method, message.get("params").unwrap_or(&Value::Null));
                        }
                    }
                }
            }
            closed.store(true, Ordering::SeqCst);
            // Whoever still waits learns now, not at their timeout.
            for (_, waiter) in waiting.lock().unwrap_or_else(|e| e.into_inner()).drain() {
                let _ = waiter.send(Err(NativeError::Closed));
            }
        });
        client
    }

    pub fn alive(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }

    /// Asks the plugin and waits for its answer only. Other calls run at the
    /// same time, from any thread.
    pub fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, NativeError> {
        if !self.alive() {
            return Err(NativeError::Closed);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.waiting.lock().unwrap_or_else(|e| e.into_inner()).insert(id, tx);
        let line = serde_json::to_vec(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .unwrap_or_default();
        let forget = || { self.waiting.lock().unwrap_or_else(|e| e.into_inner()).remove(&id); };
        match self.out.try_send(line) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => { forget(); return Err(NativeError::Busy); }
            Err(TrySendError::Disconnected(_)) => { forget(); return Err(NativeError::Closed); }
        }
        match rx.recv_timeout(timeout) {
            Ok(answer) => answer,
            Err(mpsc::RecvTimeoutError::Timeout) => { forget(); Err(NativeError::Timeout) }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(NativeError::Closed),
        }
    }

    /// Tells the plugin something without waiting for anything back.
    pub fn notify(&self, method: &str, params: Value) {
        let line = serde_json::to_vec(&json!({"jsonrpc": "2.0", "method": method, "params": params})).unwrap_or_default();
        let _ = self.out.try_send(line);
    }

    /// Ends the process. Every call still waiting fails as closed.
    pub fn stop(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Some(end) = self.end.lock().unwrap_or_else(|e| e.into_inner()).take() {
            end();
        }
    }
}

impl Drop for NativeClient {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A process plugin as the registry sees it: listed, approved and watched
/// like any other. Callers that must not wait for the registry's lock ask
/// [`NativePlugin::client`] directly, outside it.
pub struct NativePlugin {
    manifest: crate::Manifest,
    client: Arc<NativeClient>,
}

impl NativePlugin {
    pub fn new(manifest: crate::Manifest, client: Arc<NativeClient>) -> Self {
        Self { manifest, client }
    }

    pub fn client(&self) -> Arc<NativeClient> {
        self.client.clone()
    }
}

impl crate::RuntimePlugin for NativePlugin {
    fn manifest(&self) -> &crate::Manifest {
        &self.manifest
    }

    /// An operation of the shared protocol is a method of the same name.
    fn call(
        &mut self,
        _: &crate::Host,
        _: &crate::Services,
        operation: &str,
        input: &[u8],
        limits: crate::Limits,
    ) -> Result<crate::Reply, crate::PluginError> {
        let params: Value = serde_json::from_slice(input).unwrap_or(Value::Null);
        match self.client.call(operation, params, limits.timeout) {
            Ok(result) => Ok(crate::Reply::Ok(serde_json::to_vec(&result).unwrap_or_default())),
            Err(NativeError::Rpc { code: -32601, .. }) => Err(crate::PluginError::MissingExport(operation.into())),
            Err(NativeError::Rpc { message, .. }) => Ok(crate::Reply::Error(message)),
            Err(e) => Err(crate::PluginError::Crashed(e.to_string())),
        }
    }

    fn alive(&self) -> bool {
        self.client.alive()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    use std::time::Instant;

    /// A plugin on a thread: `slow` answers after 3 s, `echo` at once,
    /// `fail` with an error, `busy` with an error that asks for a pause,
    /// `quit` closes its end.
    fn plugin() -> Arc<NativeClient> {
        let (host, guest) = UnixStream::pair().unwrap();
        let reply_to = guest.try_clone().unwrap();
        std::thread::spawn(move || {
            let reply = Arc::new(Mutex::new(reply_to));
            for line in BufReader::new(guest).lines() {
                let Ok(line) = line else { break };
                let message: Value = serde_json::from_str(&line).unwrap();
                let id = message["id"].clone();
                let method = message["method"].as_str().unwrap_or("").to_string();
                if method == "quit" {
                    let _ = reply.lock().unwrap().shutdown(std::net::Shutdown::Both);
                    break;
                }
                let reply = reply.clone();
                std::thread::spawn(move || {
                    let answer = match method.as_str() {
                        "slow" => { std::thread::sleep(Duration::from_secs(3)); json!({"jsonrpc": "2.0", "id": id, "result": "slow"}) }
                        "fail" => json!({"jsonrpc": "2.0", "id": id, "error": {"code": 7, "message": "no"}}),
                        "busy" => json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": "busy", "data": {"retry_after_ms": 1500}}}),
                        _ => json!({"jsonrpc": "2.0", "id": id, "result": message["params"]}),
                    };
                    let mut out = reply.lock().unwrap();
                    let _ = writeln!(out, "{answer}");
                });
            }
        });
        let output = host.try_clone().unwrap();
        NativeClient::start(host, output, || {}, |_, _| {})
    }

    #[test]
    fn a_slow_call_does_not_hold_a_fast_one_behind_it() {
        let client = plugin();
        let slow = { let client = client.clone(); std::thread::spawn(move || client.call("slow", Value::Null, Duration::from_secs(10))) };
        std::thread::sleep(Duration::from_millis(50));
        let started = Instant::now();
        assert_eq!(client.call("echo", json!({"n": 1}), Duration::from_secs(1)).unwrap(), json!({"n": 1}));
        assert!(started.elapsed() < Duration::from_millis(50), "the fast call waited {:?}", started.elapsed());
        assert_eq!(slow.join().unwrap().unwrap(), json!("slow"));
    }

    #[test]
    fn an_error_and_a_timeout_reach_only_their_caller() {
        let client = plugin();
        assert_eq!(client.call("fail", Value::Null, Duration::from_secs(1)), Err(NativeError::Rpc { code: 7, message: "no".into(), data: None }));
        assert_eq!(client.call("slow", Value::Null, Duration::from_millis(100)), Err(NativeError::Timeout));
        assert_eq!(client.call("echo", json!(2), Duration::from_secs(1)).unwrap(), json!(2));
        assert_eq!(
            client.call("busy", Value::Null, Duration::from_secs(1)),
            Err(NativeError::Rpc { code: -32000, message: "busy".into(), data: Some(json!({"retry_after_ms": 1500})) }),
            "what the plugin added to its error reaches the caller"
        );
    }

    #[test]
    fn a_plugin_that_ends_fails_its_waiting_calls_at_once() {
        let client = plugin();
        let slow = { let client = client.clone(); std::thread::spawn(move || client.call("slow", Value::Null, Duration::from_secs(10))) };
        std::thread::sleep(Duration::from_millis(50));
        let started = Instant::now();
        let _ = client.call("quit", Value::Null, Duration::from_millis(200));
        assert_eq!(slow.join().unwrap(), Err(NativeError::Closed));
        assert!(started.elapsed() < Duration::from_secs(2), "it waited for its timeout instead");
        assert!(!client.alive());
    }
}
