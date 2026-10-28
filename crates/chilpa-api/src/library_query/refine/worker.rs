//! I answer suggestion rounds off the window's thread.
//!
//! One thread with a read-only reader of its own runs one round at a time;
//! the window asks and goes on drawing, and keeps what it showed until the
//! answer comes back. While a round runs only the latest round asked is
//! kept: the ones asked in between are dropped unanswered, so a burst of
//! keystrokes never queues more than one round behind the one running.
//!
//! Limits (ADR-018): one thread and one connection per facade thread,
//! started by the first round and ended with the handle; a pending slot of
//! one round; nothing runs while nothing is asked.
use crate::{source_catalog::Catalog, Reader};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, Weak};

type Deliver = Box<dyn FnOnce(Value) + Send>;

struct Round { query: String, typed: String, deliver: Deliver }

#[derive(Default)]
struct Inbox { pending: Option<Round>, closed: bool }

/// The slot the asking thread and the answering thread share.
#[derive(Default)]
struct Shared { inbox: Mutex<Inbox>, wake: Condvar }

impl Shared {
    /// Keeps `round` as the next to run, in place of one not yet started.
    fn ask(&self, round: Round) {
        let mut inbox = self.inbox.lock().unwrap_or_else(|e| e.into_inner());
        if inbox.closed { return; }
        inbox.pending = Some(round);
        self.wake.notify_one();
    }

    /// The next round, once there is one; none once closed.
    fn next(&self) -> Option<Round> {
        let mut inbox = self.inbox.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(round) = inbox.pending.take() { return Some(round); }
            if inbox.closed { return None; }
            inbox = self.wake.wait(inbox).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn close(&self) {
        let mut inbox = self.inbox.lock().unwrap_or_else(|e| e.into_inner());
        inbox.closed = true;
        inbox.pending = None;
        self.wake.notify_one();
    }
}

/// The answering thread's handle: dropping it ends the thread after the
/// round it is on, and drops the round waiting unanswered.
pub(crate) struct Suggester { shared: Arc<Shared> }

impl Drop for Suggester {
    fn drop(&mut self) { self.shared.close(); }
}

impl Suggester {
    /// Starts the thread. The reader is opened there, at the first round,
    /// and again at the next one when it could not be; a round without a
    /// reader or without the catalog answers nothing, as a request without
    /// a facade does.
    pub(crate) fn start(path: PathBuf, catalog: Weak<Catalog>) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let inside = shared.clone();
        std::thread::Builder::new().name("chilpa-suggest".into()).spawn(move || {
            let mut reader: Option<Reader> = None;
            while let Some(round) = inside.next() {
                if reader.is_none() {
                    reader = Reader::open(&path).map_err(|e| eprintln!("chilpa-suggest: {e}; will retry on the next round")).ok();
                }
                let answer = match (&mut reader, catalog.upgrade()) {
                    (Some(reader), Some(catalog)) => {
                        // Held only for the round, so the facade can let the
                        // catalog go while the thread waits.
                        reader.catalog = Some(catalog);
                        let answer = reader.refine_suggest(&round.query, &round.typed);
                        reader.catalog = None;
                        answer
                    }
                    _ => super::data::nothing(),
                };
                (round.deliver)(answer);
            }
        })?;
        Ok(Self { shared })
    }

    /// Asks a round; `deliver` gets the answer on the answering thread, or
    /// never, when a later round replaced this one before it started.
    pub(crate) fn ask(&self, query: &str, typed: &str, deliver: impl FnOnce(Value) + Send + 'static) {
        self.shared.ask(Round { query: query.into(), typed: typed.into(), deliver: Box::new(deliver) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    fn round(tag: u8, sent: &std::sync::mpsc::Sender<(u8, Value)>) -> Round {
        let sent = sent.clone();
        Round { query: String::new(), typed: tag.to_string(), deliver: Box::new(move |answer| { let _ = sent.send((tag, answer)); }) }
    }

    #[test]
    fn the_latest_round_asked_replaces_one_not_yet_started() {
        let shared = Shared::default();
        let (sent, received) = channel();
        shared.ask(round(1, &sent));
        shared.ask(round(2, &sent));
        let next = shared.next().unwrap();
        assert_eq!(next.typed, "2", "the round in between is gone");
        (next.deliver)(Value::Null);
        drop(sent);
        assert_eq!(received.recv().unwrap().0, 2);
        assert!(received.recv().is_err(), "the replaced round is never answered");
        shared.close();
        assert!(shared.next().is_none(), "closed, the thread's loop ends");
        shared.ask(round(3, &sent_again()));
        assert!(shared.next().is_none(), "and takes nothing after");
    }

    fn sent_again() -> std::sync::mpsc::Sender<(u8, Value)> { channel().0 }

    fn library(path: &std::path::Path) {
        let lib = chilpa_library::Library::open(path).unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let db = lib.connection();
        db.execute("INSERT INTO albums(id, title, album_artist, year) VALUES (1, 'Blue Record', 'Alvaro', 1960)", []).unwrap();
        db.execute("INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, added_at) VALUES (1, ?1, '/1.flac', 'Blue One', 'Alvaro', 1, 1)", [source]).unwrap();
    }

    #[test]
    fn a_round_is_answered_on_the_other_thread_as_the_reader_would() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.db");
        library(&path);
        let catalog = Catalog::new();
        let worker = Suggester::start(path.clone(), Arc::downgrade(&catalog)).unwrap();
        let (sent, received) = channel();
        worker.ask("", "blue", move |answer| { let _ = sent.send((std::thread::current().id(), answer)); });
        let (thread, answer) = received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_ne!(thread, std::thread::current().id(), "answered off the asking thread");
        assert_eq!(answer, Reader::open(&path).unwrap().refine_suggest("", "blue"));
        assert_eq!(answer["words"]["anywhere"], 1);
        // The facade gone, a round answers nothing, as a request without one does.
        drop(catalog);
        let (sent, received) = channel();
        worker.ask("", "blue", move |answer| { let _ = sent.send(answer); });
        assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), super::super::data::nothing());
    }

    #[test]
    fn a_round_asked_on_a_thread_without_a_facade_answers_nothing_at_once() {
        let (sent, received) = channel();
        super::super::access::suggest_later("", "blue", move |answer| { let _ = sent.send((std::thread::current().id(), answer)); });
        let (thread, answer) = received.try_recv().expect("answered before returning");
        assert_eq!(thread, std::thread::current().id());
        assert_eq!(answer, super::super::data::nothing());
    }
}
