//! Validated settings over an isolated worker channel.

use crate::{bridge::{BridgeError, BridgeSession, WorkerServices, WorkerTransport}, settings::Form};
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Duration};

/// Owns one worker and its installation-bound services. No caller supplies
/// a storage owner or replaces the approved services during a session.
pub struct WorkerClient<T: WorkerTransport, S: WorkerServices> {
    session: BridgeSession<T>,
    services: S,
    current: Option<(u64, Form)>,
}

impl<T: WorkerTransport, S: WorkerServices> WorkerClient<T, S> {
    pub fn new(transport: T, services: S) -> Self {
        Self { session: BridgeSession::new(transport), services, current: None }
    }

    pub fn alive(&self) -> bool {
        self.session.alive()
    }

    pub fn call(&mut self, operation: &str, input: Value, timeout: Duration) -> Result<Value, BridgeError> {
        if operation == "settings.submit" {
            let revision = input.get("revision").and_then(Value::as_u64).ok_or(BridgeError::InvalidSubmission)?;
            let action = input.get("action").and_then(Value::as_str).ok_or(BridgeError::InvalidSubmission)?;
            let values: BTreeMap<String, Value> = serde_json::from_value(input.get("values").cloned().unwrap_or(json!({})))
                .map_err(|_| BridgeError::InvalidSubmission)?;
            let (current, form) = self.current.as_ref().ok_or(BridgeError::InvalidSubmission)?;
            if *current != revision { return Err(BridgeError::InvalidSubmission); }
            form.validate_submission(action, &values).map_err(|_| BridgeError::InvalidSubmission)?;
        }
        let settings = matches!(operation, "settings.open" | "settings.submit");
        if settings { self.current = None; }
        let mut result = self.session.call(operation, input, timeout, &mut self.services)?;
        if settings && result.get("error").is_none() {
            let revision = result.get("revision").and_then(Value::as_u64).filter(|n| *n > 0)
                .ok_or(BridgeError::InvalidMessage)?;
            let form: Form = serde_json::from_value(result.get("form").cloned().ok_or(BridgeError::InvalidMessage)?)
                .map_err(|_| BridgeError::InvalidMessage)?;
            form.validate().map_err(|_| BridgeError::InvalidMessage)?;
            let values = serde_json::from_value(result.get("values").cloned().unwrap_or(json!({})))
                .map_err(|_| BridgeError::InvalidMessage)?;
            result["values"] = serde_json::to_value(form.public_values(&values)).map_err(|_| BridgeError::InvalidMessage)?;
            self.current = Some((revision, form));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::ServiceOperation;
    use std::{sync::{Arc, atomic::{AtomicUsize, Ordering}}, time::Instant};

    struct Transport(Arc<AtomicUsize>);
    impl WorkerTransport for Transport {
        fn send(&mut self, _: &[u8], _: Instant) -> Result<(), BridgeError> {
            self.0.fetch_add(1, Ordering::SeqCst); Ok(())
        }
        fn receive(&mut self, _: Instant, _: usize) -> Result<Vec<u8>, BridgeError> {
            Ok(serde_json::to_vec(&json!({"type":"reply","id":self.0.load(Ordering::SeqCst),"result":{
                "revision":1, "form":{"fields":[{"key":"password","label":"Password","description":"",
                "required":true,"control":{"type":"secret","max_length":256}}],
                "actions":[{"key":"connect","label":"Connect","fields":["password"]}],"unsupported":[]},
                "values":{"password":"must not leave the adapter"}
            }})).unwrap())
        }
        fn stop(&mut self) {}
    }
    struct Deny;
    impl WorkerServices for Deny {
        fn handle(&mut self, _: ServiceOperation, _: Instant) -> Result<Value, BridgeError> { Err(BridgeError::Denied) }
    }

    #[test]
    fn stale_and_malformed_submissions_never_reach_the_worker() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut client = WorkerClient::new(Transport(calls.clone()), Deny);
        let timeout = Duration::from_secs(1);
        let reply = client.call("settings.open", json!({"page":"settings.html"}), timeout).unwrap();
        assert_eq!(reply["values"], json!({}));
        for input in [json!({"revision":2,"action":"connect","values":{"password":"secret"}}),
            json!({"revision":1,"action":"connect","values":{"password":123}}),
            json!({"revision":1,"action":"invented","values":{}})] {
            assert_eq!(client.call("settings.submit", input, timeout), Err(BridgeError::InvalidSubmission));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(client.call("settings.submit", json!({"revision":1,"action":"connect","values":{"password":"secret"}}), timeout).is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
