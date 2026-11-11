//! The listener's rating changes on their way to their sources, on a thread
//! of its own that sleeps until the window wakes it.
//!
//! Each run takes the changes due, oldest first, at most `PER_RUN`, and
//! sends each one: to a server through its protocol (`servers::ratings`),
//! to a plugin with `rating.set`. One that could not go is tried again
//! later, each time later (`WAITS`); one whose item or source is gone is
//! dropped. A change replaced by a newer one while it was on its way does
//! not take the newer one with it.

use crate::servers::keys::Keys;
use chilpa_library::store::rating_sync::Pending;
use chilpa_servers::Net;
use serde_json::{json, Value};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

/// Changes sent in one run; the rest are due again at once.
const PER_RUN: usize = 100;

/// How long after each failure a change is tried again, in seconds.
const WAITS: [i64; 5] = [30, 120, 600, 1_800, 3_600];

/// How long a plugin has to take a rating.
const PLUGIN_TIME: Duration = Duration::from_secs(30);

/// How one change went.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Delivery {
    /// Taken, as this value of the source's scale (0 when taken off).
    Sent(f64),
    /// Not now, and why.
    Later(String),
    /// Never: the item or its source is gone.
    Never(String),
}

/// What sending to a plugin needs, which a test fakes.
pub(crate) trait Plugins: Send + Sync {
    /// Its `rating` capability, as it declared it.
    fn capability(&self, plugin: &str) -> Value;
    fn call(&self, plugin: &str, method: &str, params: Value) -> Result<Value, String>;
    /// The source took the value: its lists say so from now on.
    fn took(&self, plugin: &str, kind: &str, identity: &str, value: f64);
}

struct Running {
    natives: Arc<crate::natives::Natives>,
    catalog: Arc<crate::source_catalog::Catalog>,
}

impl Plugins for Running {
    fn capability(&self, plugin: &str) -> Value {
        self.natives.capability(plugin, "rating")
    }
    fn call(&self, plugin: &str, method: &str, params: Value) -> Result<Value, String> {
        self.natives.call(plugin, method, params, PLUGIN_TIME)
    }
    fn took(&self, plugin: &str, kind: &str, identity: &str, value: f64) {
        self.catalog.set_source_rating(plugin, kind, identity, value);
    }
}

/// What the thread works with.
pub(crate) struct Tools {
    pub(crate) net: Arc<dyn Net>,
    pub(crate) keys: Arc<Keys>,
    pub(crate) plugins: Arc<dyn Plugins>,
    pub(crate) open: Arc<dyn Fn() -> Option<chilpa_library::Library> + Send + Sync>,
}

impl Tools {
    pub(crate) fn new(keys: Arc<Keys>, natives: Arc<crate::natives::Natives>, catalog: Arc<crate::source_catalog::Catalog>) -> Tools {
        Tools {
            net: Arc::new(crate::servers::ratings::network()),
            keys,
            plugins: Arc::new(Running { natives, catalog }),
            open: Arc::new(crate::servers::ratings::open_library),
        }
    }
}

/// How a run went.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Outcome {
    pub(crate) sent: usize,
    pub(crate) dropped: usize,
    /// Changes waiting after failing at least once, across every source.
    pub(crate) failing: usize,
    /// When the next change is due, in unix seconds.
    pub(crate) next_due: Option<i64>,
}

pub(crate) struct Writer {
    wake: Sender<()>,
    done: Receiver<Outcome>,
}

impl Writer {
    pub(crate) fn start(tools: Tools) -> Writer {
        let (wake, inbox) = channel::<()>();
        let (outbox, done) = channel::<Outcome>();
        let spawned = std::thread::Builder::new().name("chilpa-rating-writes".into()).spawn(move || {
            while inbox.recv().is_ok() {
                while inbox.try_recv().is_ok() {}
                let later = || Outcome { next_due: Some(crate::now_unix() + WAITS[0]), ..Outcome::default() };
                // A run that panics still says how it went: the window waits
                // for that before it wakes the thread again.
                let outcome = match (tools.open)() {
                    Some(lib) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&lib, &tools, crate::now_unix()))).unwrap_or_else(|_| {
                        eprintln!("ratings: a run of the writer panicked; it is tried again later");
                        later()
                    }),
                    None => later(),
                };
                if outbox.send(outcome).is_err() {
                    break;
                }
            }
        });
        if let Err(error) = spawned {
            eprintln!("ratings: the writer thread could not start: {error}");
        }
        Writer { wake, done }
    }

    /// False when the thread is gone.
    pub(crate) fn wake(&self) -> bool {
        self.wake.send(()).is_ok()
    }

    pub(crate) fn poll(&self) -> Vec<Outcome> {
        self.done.try_iter().collect()
    }

    /// A writer whose runs are what the test sends, with no thread.
    #[cfg(test)]
    pub(crate) fn for_tests(done: Receiver<Outcome>) -> Writer {
        Writer { wake: channel().0, done }
    }
}

/// One run: the changes due by `now`, each sent and kept as it went.
pub(crate) fn run(lib: &chilpa_library::Library, tools: &Tools, now: i64) -> Outcome {
    let mut outcome = Outcome::default();
    for change in lib.due_ratings(now, PER_RUN).unwrap_or_default() {
        let delivery = deliver(lib, tools, &change);
        let kept = match &delivery {
            Delivery::Sent(value) => {
                outcome.sent += 1;
                lib.rating_sent(&change, *value, super::sync::now_ms())
            }
            Delivery::Later(why) => {
                let wait = WAITS[(change.attempts.max(0) as usize).min(WAITS.len() - 1)];
                eprintln!("ratings: a change for {} was not taken ({why}); tried again in {wait}s", change.origin);
                lib.rating_failed(&change, why, now.saturating_add(wait))
            }
            Delivery::Never(why) => {
                outcome.dropped += 1;
                eprintln!("ratings: a change for {} was dropped: {why}", change.origin);
                lib.drop_rating(&change)
            }
        };
        if let Err(error) = kept {
            eprintln!("ratings: how a change went could not be kept: {error}");
        }
    }
    outcome.failing = lib.rating_failures().unwrap_or_default().iter().map(|(_, count, _)| count).sum();
    outcome.next_due = lib.next_rating_due().ok().flatten();
    outcome
}

fn deliver(lib: &chilpa_library::Library, tools: &Tools, change: &Pending) -> Delivery {
    if let Some(instance) = change.origin.strip_prefix("server:") {
        return crate::servers::ratings::deliver(lib, tools.net.as_ref(), &tools.keys, instance, change);
    }
    let Some(plugin) = change.origin.strip_prefix("plugin:") else {
        return Delivery::Never(format!("unknown origin {}", change.origin));
    };
    let Some((scale, kinds)) = super::scale::declared(&tools.plugins.capability(plugin)) else {
        return Delivery::Later(chilpa_i18n::t("the plugin is not running"));
    };
    if !kinds.contains(&change.kind) {
        return Delivery::Never(format!("{plugin} no longer rates a {}", change.kind));
    }
    let value = scale.value(change.stars);
    match tools.plugins.call(plugin, "rating.set", json!({"ref": change.id, "kind": change.kind, "value": value})) {
        Ok(_) => {
            tools.plugins.took(plugin, &change.kind, &change.id, value.unwrap_or(0.0));
            Delivery::Sent(value.unwrap_or(0.0))
        }
        Err(why) => Delivery::Later(why),
    }
}
