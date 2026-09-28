//! Development traces for the playback path.
//!
//! They are deliberately enabled by default while the player is pre-release.
//! Set `CHILPA_PLAYBACK_TRACE=0` only when a quiet terminal is needed.

use std::fmt;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static STARTED: OnceLock<Instant> = OnceLock::new();
static SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub fn enabled() -> bool {
    std::env::var_os("CHILPA_PLAYBACK_TRACE").as_deref() != Some(std::ffi::OsStr::new("0"))
}

/// Writes one ordered playback event to stderr with elapsed process time and
/// the thread that emitted it. This is for lifecycle events, not audio blocks.
pub fn trace(scope: &str, event: &str, details: fmt::Arguments<'_>) {
    if !enabled() {
        return;
    }
    let elapsed = STARTED.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0;
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let thread = std::thread::current();
    let thread = thread.name().unwrap_or("unnamed");
    eprintln!(
        "[playback +{elapsed:>10.3}ms #{sequence:06} {thread}] {scope}.{event} {details}"
    );
}
