//! I answer suggestion rounds off the window's thread.
//!
//! One thread with a read-only reader of its own runs one round at a time;
//! the window asks and goes on drawing, and keeps what it showed until the
//! answer comes back. Each list on screen is a consumer with a slot of its
//! own: while a round runs, only its latest round waits, and the ones it
//! asked in between are dropped unanswered, so a burst of keystrokes never
//! queues more than one round per list behind the one running, and one
//! list's typing never drops another's round.
//!
//! Limits (ADR-018): one thread and one connection per facade thread,
//! started by the first round and ended with the handle, which the last
//! list to close lets go; one pending round per list on screen; nothing
//! runs while nothing is asked. A round that panics answers nothing, and
//! the reader is opened again at the next.
use crate::{source_catalog::Catalog, Reader};
use serde_json::Value;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, Weak};

type Deliver = Box<dyn FnOnce(Value) + Send>;

struct Round { consumer: u64, query: String, typed: String, deliver: Deliver }

/// One round per consumer, in the order the consumers first asked.
#[derive(Default)]
struct Inbox { pending: Vec<Round>, closed: bool }

/// The slots the asking thread and the answering thread share.
#[derive(Default)]
struct Shared { inbox: Mutex<Inbox>, wake: Condvar }

impl Shared {
    /// Keeps `round` as its consumer's next, in place of one not yet started.
    fn ask(&self, round: Round) {
        let mut inbox = self.inbox.lock().unwrap_or_else(|e| e.into_inner());
        if inbox.closed { return; }
        match inbox.pending.iter_mut().find(|held| held.consumer == round.consumer) {
            Some(held) => *held = round,
            None => inbox.pending.push(round),
        }
        self.wake.notify_one();
    }

    /// Drops the round a consumer has waiting, if any.
    fn forget(&self, consumer: u64) {
        let mut inbox = self.inbox.lock().unwrap_or_else(|e| e.into_inner());
        inbox.pending.retain(|held| held.consumer != consumer);
    }

    /// The next round, once there is one; none once closed.
    fn next(&self) -> Option<Round> {
        let mut inbox = self.inbox.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if !inbox.pending.is_empty() { return Some(inbox.pending.remove(0)); }
            if inbox.closed { return None; }
            inbox = self.wake.wait(inbox).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn close(&self) {
        let mut inbox = self.inbox.lock().unwrap_or_else(|e| e.into_inner());
        inbox.closed = true;
        inbox.pending.clear();
        self.wake.notify_one();
    }
}

/// The answering thread's handle: dropping it ends the thread after the
/// round it is on, and drops the rounds waiting unanswered.
pub(crate) struct Suggester { shared: Arc<Shared> }

impl Drop for Suggester {
    fn drop(&mut self) { self.shared.close(); }
}

/// One round's answer, as the reader gives it; nothing without a reader or
/// without the catalog, as a request without a facade answers.
fn answer(reader: &mut Option<Reader>, catalog: &Weak<Catalog>, round: &Round) -> Value {
    match (reader.as_mut(), catalog.upgrade()) {
        (Some(reader), Some(catalog)) => {
            // Held only for the round, so the facade can let the catalog go
            // while the thread waits.
            reader.catalog = Some(catalog);
            let answer = reader.refine_suggest(&round.query, &round.typed);
            reader.catalog = None;
            answer
        }
        _ => super::data::nothing(),
    }
}

impl Suggester {
    /// Starts the thread. The reader is opened there, at the first round,
    /// and again at the next one when it could not be or a round panicked.
    pub(crate) fn start(path: PathBuf, catalog: Weak<Catalog>) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let inside = shared.clone();
        std::thread::Builder::new().name("chilpa-suggest".into()).spawn(move || {
            let mut reader: Option<Reader> = None;
            while let Some(round) = inside.next() {
                if reader.is_none() {
                    reader = Reader::open(&path).map_err(|e| eprintln!("chilpa-suggest: {e}; will retry on the next round")).ok();
                }
                let found = match catch_unwind(AssertUnwindSafe(|| answer(&mut reader, &catalog, &round))) {
                    Ok(found) => found,
                    Err(_) => {
                        eprintln!("chilpa-suggest: a round panicked; the reader is opened again at the next");
                        reader = None;
                        super::data::nothing()
                    }
                };
                if catch_unwind(AssertUnwindSafe(|| (round.deliver)(found))).is_err() {
                    eprintln!("chilpa-suggest: delivering an answer panicked");
                }
            }
        })?;
        Ok(Self { shared })
    }

    /// Asks a round for `consumer`; `deliver` gets the answer on the
    /// answering thread, or never, when the consumer asked again before it
    /// started.
    pub(crate) fn ask(&self, consumer: u64, query: &str, typed: &str, deliver: impl FnOnce(Value) + Send + 'static) {
        self.shared.ask(Round { consumer, query: query.into(), typed: typed.into(), deliver: Box::new(deliver) });
    }

    /// A consumer is gone: the round it had waiting is dropped.
    pub(crate) fn forget(&self, consumer: u64) { self.shared.forget(consumer); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    fn round(consumer: u64, tag: u8, sent: &std::sync::mpsc::Sender<(u8, Value)>) -> Round {
        let sent = sent.clone();
        Round { consumer, query: String::new(), typed: tag.to_string(), deliver: Box::new(move |answer| { let _ = sent.send((tag, answer)); }) }
    }

    #[test]
    fn a_consumer_keeps_one_round_waiting_and_never_drops_another_consumers() {
        let shared = Shared::default();
        let (sent, received) = channel();
        shared.ask(round(1, 1, &sent));
        shared.ask(round(2, 2, &sent));
        shared.ask(round(1, 3, &sent));
        let first = shared.next().unwrap();
        assert_eq!((first.consumer, first.typed.as_str()), (1, "3"), "the first consumer's later round, in the first consumer's place");
        let second = shared.next().unwrap();
        assert_eq!((second.consumer, second.typed.as_str()), (2, "2"), "the other consumer's round is still there");
        (first.deliver)(Value::Null);
        (second.deliver)(Value::Null);
        drop(sent);
        let answered: Vec<u8> = received.iter().map(|(tag, _)| tag).collect();
        assert_eq!(answered, [3, 2], "the replaced round is never answered");
        let (sent, _received) = channel();
        shared.ask(round(2, 4, &sent));
        shared.forget(2);
        shared.close();
        assert!(shared.next().is_none(), "closed, the thread's loop ends");
    }

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
        worker.ask(1, "", "blue", move |answer| { let _ = sent.send((std::thread::current().id(), answer)); });
        let (thread, answer) = received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_ne!(thread, std::thread::current().id(), "answered off the asking thread");
        assert_eq!(answer, Reader::open(&path).unwrap().refine_suggest("", "blue"));
        assert_eq!(answer["words"]["anywhere"], 1);
        // The facade gone, a round answers nothing, as a request without one does.
        drop(catalog);
        let (sent, received) = channel();
        worker.ask(1, "", "blue", move |answer| { let _ = sent.send(answer); });
        assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), super::super::data::nothing());
    }

    #[test]
    fn a_round_that_panics_does_not_take_the_thread_with_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("library.db");
        library(&path);
        let catalog = Catalog::new();
        let worker = Suggester::start(path, Arc::downgrade(&catalog)).unwrap();
        let (entered, running) = channel();
        worker.ask(1, "", "blue", move |_| { let _ = entered.send(()); panic!("a delivery that fails") });
        running.recv_timeout(Duration::from_secs(5)).unwrap();
        let (sent, received) = channel();
        worker.ask(1, "", "blue", move |answer| { let _ = sent.send(answer); });
        let answer = received.recv_timeout(Duration::from_secs(5)).expect("the next round is still answered");
        assert_eq!(answer["words"]["anywhere"], 1, "and by the same reader");
    }

    #[test]
    fn a_round_asked_on_a_thread_without_a_facade_answers_nothing_at_once() {
        let (sent, received) = channel();
        super::super::access::suggest_later(1, "", "blue", move |answer| { let _ = sent.send((std::thread::current().id(), answer)); });
        let (thread, answer) = received.try_recv().expect("answered before returning");
        assert_eq!(thread, std::thread::current().id());
        assert_eq!(answer, super::super::data::nothing());
    }
}
