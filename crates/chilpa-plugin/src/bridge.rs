//! A bounded protocol for isolated workers. The guest never chooses its owner.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, Instant};

pub const MAX_FRAME: usize = 4 * 1024 * 1024;
const MAX_SERVICES_PER_CALL: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeError {
    Timeout,
    Disconnected,
    InvalidMessage,
    InvalidSubmission,
    TooLarge,
    ServiceLimit,
    Denied,
    Storage,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostMessage {
    Call { id: u64, operation: String, input: Value },
    ServiceReply { id: u64, result: Value },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerMessage {
    Reply { id: u64, result: Value },
    Service { id: u64, operation: ServiceOperation },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "name", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServiceOperation {
    PrivateGet { key: String },
    PrivateSet { key: String, value: Value },
    Http { request: Value },
}

/// Implementations own an already isolated worker and must enforce deadlines
/// on both reads and writes. `stop` revokes the entire worker, including its
/// descendants; it must also release any blocked transport operations.
pub trait WorkerTransport: Send {
    fn send(&mut self, bytes: &[u8], deadline: Instant) -> Result<(), BridgeError>;
    fn receive(&mut self, deadline: Instant, maximum: usize) -> Result<Vec<u8>, BridgeError>;
    fn stop(&mut self);
}

impl WorkerTransport for Box<dyn WorkerTransport> {
    fn send(&mut self, bytes: &[u8], deadline: Instant) -> Result<(), BridgeError> { (**self).send(bytes, deadline) }
    fn receive(&mut self, deadline: Instant, maximum: usize) -> Result<Vec<u8>, BridgeError> { (**self).receive(deadline, maximum) }
    fn stop(&mut self) { (**self).stop(); }
}

/// A platform backend supplied by the application, never a package command.
pub trait WorkerLauncher: Send + Sync {
    fn launch(&self, guest: &std::path::Path) -> Result<Box<dyn WorkerTransport>, BridgeError>;
    fn guest_root(&self) -> &str;
    /// Empty revisions cannot reuse an inspection from another process.
    fn inspection_revision(&self) -> Option<String> { None }
}

/// Constructed for one approved installation. No method accepts a guest-
/// supplied plugin ID, path, encryption key or another account's credentials.
pub trait WorkerServices: Send {
    fn handle(&mut self, operation: ServiceOperation, deadline: Instant) -> Result<Value, BridgeError>;
}

pub struct BridgeSession<T: WorkerTransport> {
    transport: T,
    next_id: u64,
    last_service: u64,
    stopped: bool,
}

impl<T: WorkerTransport> BridgeSession<T> {
    pub fn new(transport: T) -> Self {
        Self { transport, next_id: 0, last_service: 0, stopped: false }
    }

    /// False once an exchange failed: the transport was stopped for good.
    pub fn alive(&self) -> bool {
        !self.stopped
    }

    pub fn call(
        &mut self,
        operation: &str,
        input: Value,
        timeout: Duration,
        services: &mut dyn WorkerServices,
    ) -> Result<Value, BridgeError> {
        if self.stopped { return Err(BridgeError::Disconnected); }
        let result = self.exchange(operation, input, timeout, services);
        if result.is_err() {
            self.stopped = true;
            self.transport.stop();
        }
        result
    }

    fn exchange(
        &mut self,
        operation: &str,
        input: Value,
        timeout: Duration,
        services: &mut dyn WorkerServices,
    ) -> Result<Value, BridgeError> {
        if operation.is_empty() || operation.len() > 128 { return Err(BridgeError::InvalidMessage); }
        let deadline = Instant::now().checked_add(timeout).ok_or(BridgeError::Timeout)?;
        self.next_id = self.next_id.checked_add(1).ok_or(BridgeError::InvalidMessage)?;
        self.send(&HostMessage::Call { id: self.next_id, operation: operation.into(), input }, deadline)?;
        let mut serviced = 0;
        loop {
            if Instant::now() >= deadline { return Err(BridgeError::Timeout); }
            let bytes = self.transport.receive(deadline, MAX_FRAME)?;
            if Instant::now() >= deadline { return Err(BridgeError::Timeout); }
            if bytes.len() > MAX_FRAME { return Err(BridgeError::TooLarge); }
            let message: WorkerMessage = serde_json::from_slice(&bytes).map_err(|_| BridgeError::InvalidMessage)?;
            match message {
                WorkerMessage::Reply { id, result } if id == self.next_id => return Ok(result),
                WorkerMessage::Reply { .. } => return Err(BridgeError::InvalidMessage),
                WorkerMessage::Service { id, operation } => {
                    if id <= self.last_service { return Err(BridgeError::InvalidMessage); }
                    self.last_service = id;
                    serviced += 1;
                    if serviced > MAX_SERVICES_PER_CALL { return Err(BridgeError::ServiceLimit); }
                    if Instant::now() >= deadline { return Err(BridgeError::Timeout); }
                    let result = match services.handle(operation, deadline) {
                        Ok(value) => serde_json::json!({"ok": true, "value": value}),
                        Err(_) => serde_json::json!({"ok": false}),
                    };
                    self.send(&HostMessage::ServiceReply { id, result }, deadline)?;
                }
            }
        }
    }

    fn send(&mut self, message: &HostMessage, deadline: Instant) -> Result<(), BridgeError> {
        let bytes = serde_json::to_vec(message).map_err(|_| BridgeError::InvalidMessage)?;
        if bytes.len() > MAX_FRAME { return Err(BridgeError::TooLarge); }
        if Instant::now() >= deadline { return Err(BridgeError::Timeout); }
        self.transport.send(&bytes, deadline)
    }
}

impl<T: WorkerTransport> Drop for BridgeSession<T> {
    fn drop(&mut self) {
        if !self.stopped { self.transport.stop(); }
    }
}
