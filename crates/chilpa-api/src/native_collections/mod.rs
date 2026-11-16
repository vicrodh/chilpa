//! One profile broker, one RPC, two pending requests, and a bounded disk window.
mod article;
mod projection;
mod store;
#[cfg(test)]
mod tests;
mod worker;

use crate::{
    natives::Natives,
    source_catalog::{Catalog, collection_window::Lease},
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
};

pub use article::NativeArticleWindow;

const CHUNK_BYTES: usize = 512 * 1024;
const CACHE_BYTES: usize = 4 * 1024 * 1024;
const CACHE_CHUNKS: usize = 8;

enum Command {
    Open(String),
    Seek(usize, usize),
    Refresh(bool),
    Action(String),
    Article(String),
    Image(String, u64),
}
struct Job {
    session: u64,
    epoch: u64,
    request: u64,
    command: Command,
}

#[derive(Default)]
struct State {
    session: u64,
    serial: u64,
    epoch: u64,
    pending: VecDeque<Job>,
    mailbox: Option<Value>,
    working: bool,
    shutdown: bool,
    startup_error: Option<String>,
    action: Option<Lease>,
    article: Option<article::Session>,
    cleanup: bool,
}
impl State {
    fn alive(&self, job: &Job) -> bool {
        !self.shutdown && self.session == job.session && self.epoch == job.epoch
            && match &job.command {
                Command::Image(url, demand) => self.article.as_ref().is_some_and(|article| article.demands.get(url) == Some(demand)),
                _ => true,
            }
    }
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}
impl Shared {
    fn cleanup(&self) {
        self.lock().cleanup = true;
        self.wake.notify_all();
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
    fn alive(&self, job: &Job) -> bool {
        let state = self.lock();
        state.alive(job)
    }
    fn publish(&self, job: &Job, value: Value) {
        let mut state = self.lock();
        while state.mailbox.is_some() && state.alive(job) {
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
        if state.alive(job) {
            state.mailbox = Some(value);
        }
    }
}

pub(crate) struct Broker {
    shared: Arc<Shared>,
    source: Arc<dyn worker::PageSource>,
}
impl Broker {
    fn new(natives: Arc<Natives>, catalog: Arc<Catalog>, path: PathBuf) -> Arc<Self> {
        Self::with_source(Arc::new(worker::NativeSource(natives)), catalog, path)
    }
    fn with_source(
        source: Arc<dyn worker::PageSource>,
        catalog: Arc<Catalog>,
        path: PathBuf,
    ) -> Arc<Self> {
        let shared = Arc::new(Shared::default());
        let worker_shared = shared.clone();
        let worker_source = source.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("native-collections".into())
            .spawn(move || worker::run(worker_shared, worker_source, catalog, path))
        {
            shared.lock().startup_error = Some(error.to_string());
        }
        Arc::new(Self { shared, source })
    }
}
impl Drop for Broker {
    fn drop(&mut self) {
        let action = {
            let mut state = self.shared.lock();
            state.session = 0;
            state.epoch += 1;
            state.pending.clear();
            state.mailbox = None;
            state.article = None;
            state.action.take()
        };
        drop(action);
        self.shared.lock().shutdown = true;
        self.shared.cleanup();
        self.shared.wake.notify_all();
    }
}

/// A view owns its session. Every method is memory-only and nonblocking.
pub struct NativeCollectionWindow {
    broker: Arc<Broker>,
    session: u64,
}

impl crate::Chilpa {
    pub fn native_collection_window(&mut self) -> NativeCollectionWindow {
        let broker = self
            .native_collections
            .get_or_insert_with(|| {
                Broker::new(
                    self.natives.clone(),
                    self.catalog.clone(),
                    chilpa_dirs::cache().join("native-collections.sqlite"),
                )
            })
            .clone();
        NativeCollectionWindow { broker, session: 0 }
    }
}

impl NativeCollectionWindow {
    pub fn open(&mut self, key: &str, request_id: u64) -> u64 {
        self.start(key, request_id, false)
    }

    fn start(&mut self, key: &str, request_id: u64, article: bool) -> u64 {
        self.close(self.session);
        let mut state = self.broker.shared.lock();
        state.serial = state
            .serial
            .checked_add(1)
            .expect("collection session identifiers exhausted");
        self.session = state.serial;
        state.session = self.session;
        state.epoch += 1;
        state.pending.clear();
        state.mailbox = None;
        state.article = None;
        let action = state.action.take();
        let invalid = if key.len() > 32768 {
            Some("The collection address exceeds its limit.".to_string())
        } else {
            state.startup_error.clone()
        };
        if let Some(error) = invalid {
            state.mailbox = Some(if article {
                json!({"kind":"article","sessionId":self.session,"requestId":request_id,"generation":0,"busy":false,"error":error,"document":null})
            } else {
                json!({"sessionId":self.session,"requestId":request_id,"knownCount":0,"busy":false,"error":error})
            });
        } else {
            let job = Job {
                session: self.session,
                epoch: state.epoch,
                request: request_id,
                command: if article { Command::Article(key.to_string()) } else { Command::Open(key.to_string()) },
            };
            state.pending.push_back(job);
        }
        drop(state);
        drop(action);
        self.broker.shared.cleanup();
        self.session
    }

    fn ask(&self, session: u64, request: u64, command: Command) -> bool {
        let mut state = self.broker.shared.lock();
        if session == 0 || session != self.session || session != state.session || state.shutdown {
            return false;
        }
        if matches!(command, Command::Seek(..)) {
            state
                .pending
                .retain(|job| !matches!(job.command, Command::Seek(..)));
        }
        if state.pending.len() >= 2 {
            return false;
        }
        let job = Job {
            session,
            epoch: state.epoch,
            request,
            command,
        };
        state.pending.push_back(job);
        drop(state);
        self.broker.shared.wake.notify_all();
        true
    }
    pub fn seek(&self, session: u64, first: usize, limit: usize, request_id: u64) -> bool {
        if limit == 0 || limit > 100 || first > i32::MAX as usize {
            return false;
        }
        self.ask(session, request_id, Command::Seek(first, limit))
    }
    pub fn refresh(&self, session: u64, manual: bool, request_id: u64) -> bool {
        self.ask(session, request_id, Command::Refresh(manual))
    }
    pub fn prepare_action(&self, session: u64, row_token: &str, request_id: u64) -> bool {
        if row_token.len() > 512 {
            return false;
        }
        self.ask(session, request_id, Command::Action(row_token.to_string()))
    }
    pub fn release_action(&self, lease: u64) {
        let action = {
            let mut state = self.broker.shared.lock();
            if state
                .action
                .as_ref()
                .is_some_and(|action| action.id == lease)
            {
                state.action.take()
            } else {
                None
            }
        };
        drop(action);
        self.broker.shared.cleanup();
    }
    pub fn close(&mut self, session: u64) {
        if session == 0 || session != self.session {
            return;
        }
        let action = {
            let mut state = self.broker.shared.lock();
            if state.session == session {
                state.session = 0;
                state.epoch += 1;
                state.pending.clear();
                state.mailbox = None;
                state.article = None;
                state.action.take()
            } else {
                None
            }
        };
        self.session = 0;
        drop(action);
        self.broker.shared.cleanup();
    }
    pub fn poll(&mut self) -> Option<Value> {
        let mut state = self.broker.shared.lock();
        if self.session == 0 || state.session != self.session {
            return None;
        }
        let value = state.mailbox.take();
        drop(state);
        self.broker.shared.wake.notify_all();
        value
    }
    pub fn busy(&self) -> bool {
        let state = self.broker.shared.lock();
        self.session != 0
            && state.session == self.session
            && (state.working || !state.pending.is_empty() || state.mailbox.is_some())
    }
}
impl Drop for NativeCollectionWindow {
    fn drop(&mut self) {
        self.close(self.session);
    }
}
