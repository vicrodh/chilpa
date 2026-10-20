//! The one clock everything continuous ticks on.
//!
//! Qt Quick has no dirty-region rendering: any item that changes repaints
//! the whole window, so two animations on two timers of ~30 Hz cost two
//! presents per period, not one. Every continuous animation (the ambient
//! field, the atmosphere drift, the visualizer's frames) advances on the
//! edge of `ms` instead, and the window presents once per period for all
//! of them. Nothing animates on a `Timer`, a `NumberAnimation` or a
//! `FrameAnimation` of its own.
//!
//! The clock sleeps when nobody holds it: a consumer calls `hold()` when it
//! starts moving and `release()` when it stops or is hidden, and with no
//! holder the thread waits. A still window costs no wakeups at all.
//! Slow holders share roughly 100 ms edges until a normal holder needs the
//! producer's period. I do not wake the GUI between those sparse frames.

#[cxx_qt::bridge]
pub mod qobject {
    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// A wrapping millisecond counter. Its value means nothing: a
        /// consumer reacts to the change and keeps its own tick.
        #[qproperty(i32, ms)]
        /// How often it ticks, in milliseconds.
        #[qproperty(i32, period)]
        type Pulse = super::PulseRust;

        /// One more consumer wants the clock running.
        #[qinvokable]
        fn hold(self: Pin<&mut Pulse>);

        /// One consumer stopped. With none left the clock sleeps.
        #[qinvokable]
        fn release(self: Pin<&mut Pulse>);

        /// I need only one edge near every 100 ms, unless another consumer needs more.
        #[qinvokable]
        fn hold_slow(self: Pin<&mut Pulse>);

        /// I no longer need the sparse edges.
        #[qinvokable]
        fn release_slow(self: Pin<&mut Pulse>);
    }

    impl cxx_qt::Threading for Pulse {}
    impl cxx_qt::Initialize for Pulse {}
}

use cxx_qt::{CxxQtType as _, Threading as _};
use std::pin::Pin;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

/// The period by default: the visualizer produces thirty frames a second,
/// so one tick per frame shows every one once and none twice.
const PERIOD_MS: i32 = 33;
/// A day. The counter's value means nothing to anybody, so where it wraps
/// only has to be far away.
const WRAP_MS: i32 = 86_400_000;

/// The period, from `CHILPA_PULSE_MS` when it is set: under 10 ms the
/// presents outrun the data, over 200 ms the motion reads as broken.
fn period_ms() -> i32 {
    std::env::var("CHILPA_PULSE_MS").ok().and_then(|v| v.parse::<i32>().ok()).map_or(PERIOD_MS, |v| v.clamp(10, 200))
}

/// How many consumers want the clock. Never below zero: a release with
/// nothing held is a consumer that forgot its hold, not a debt.
#[derive(Default)]
struct Claims {
    normal: u32,
    slow: u32,
}

impl Claims {
    fn hold(&mut self) -> u32 {
        self.normal += 1;
        self.count()
    }

    fn release(&mut self) -> u32 {
        self.normal = self.normal.saturating_sub(1);
        self.count()
    }

    fn hold_slow(&mut self) { self.slow += 1; }
    fn release_slow(&mut self) { self.slow = self.slow.saturating_sub(1); }
    fn count(&self) -> u32 { self.normal + self.slow }
    fn period(&self, base: i32) -> i32 {
        if self.normal == 0 && self.slow > 0 { base * ((100 + base / 2) / base).max(1) } else { base }
    }
}

/// What the thread waits on: the number of holders, and the signal that
/// it changed.
static GATE: (Mutex<(u32, i32)>, Condvar) = (Mutex::new((0, PERIOD_MS)), Condvar::new());
static STARTED: OnceLock<()> = OnceLock::new();

pub struct PulseRust {
    ms: i32,
    period: i32,
    base_period: i32,
    claims: Claims,
}

impl Default for PulseRust {
    fn default() -> Self {
        let period = period_ms();
        Self { ms: 0, period, base_period: period, claims: Claims::default() }
    }
}

fn tell(holders: u32, period: i32) {
    let (count, changed) = &GATE;
    let mut demand = count.lock().unwrap_or_else(|e| e.into_inner());
    let wake = (demand.0 == 0) != (holders == 0) || demand.1 != period;
    *demand = (holders, period);
    if wake { changed.notify_all(); }
}

impl qobject::Pulse {
    pub fn hold(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().claims.hold();
        self.as_mut().refresh();
        self.start();
    }

    pub fn release(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().claims.release();
        self.refresh();
    }

    pub fn hold_slow(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().claims.hold_slow();
        self.as_mut().refresh();
        self.start();
    }

    pub fn release_slow(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().claims.release_slow();
        self.refresh();
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let period = self.rust().claims.period(self.rust().base_period);
        let holders = self.rust().claims.count();
        self.as_mut().set_period(period);
        tell(holders, period);
    }

    /// The thread, once: it sleeps while nobody holds the clock, and while
    /// somebody does it bumps `ms` every period on the window's thread.
    fn start(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        STARTED.get_or_init(|| {
            std::thread::Builder::new()
                .name("chilpa-pulse".into())
                .spawn(move || loop {
                    let period = {
                        let (count, changed) = &GATE;
                        let mut demand = count.lock().unwrap_or_else(|e| e.into_inner());
                        while demand.0 == 0 {
                            demand = changed.wait(demand).unwrap_or_else(|e| e.into_inner());
                        }
                        let period = demand.1;
                        let (_demand, timeout) = changed.wait_timeout(demand, Duration::from_millis(period as u64))
                            .unwrap_or_else(|e| e.into_inner());
                        if !timeout.timed_out() { continue; }
                        period
                    };
                    if thread.queue(move |mut pulse| {
                        if pulse.rust().claims.count() == 0 { return; }
                        let next = (*pulse.ms() + period) % WRAP_MS;
                        pulse.as_mut().set_ms(next);
                    }).is_err() {
                        return;
                    }
                })
                .expect("the pulse thread starts");
        });
    }
}

impl cxx_qt::Initialize for qobject::Pulse {
    fn initialize(self: Pin<&mut Self>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holders_are_counted_and_never_go_below_zero() {
        let mut claims = Claims::default();
        assert_eq!(claims.hold(), 1);
        assert_eq!(claims.hold(), 2);
        assert_eq!(claims.release(), 1);
        assert_eq!(claims.release(), 0);
        assert_eq!(claims.release(), 0, "a release with nothing held is not a debt");
        assert_eq!(claims.hold(), 1);
    }

    #[test]
    fn the_period_is_the_producers_and_the_environment_stays_in_range() {
        assert_eq!(PERIOD_MS, 33);
        std::env::set_var("CHILPA_PULSE_MS", "4");
        assert_eq!(period_ms(), 10, "under ten the presents outrun the data");
        std::env::set_var("CHILPA_PULSE_MS", "900");
        assert_eq!(period_ms(), 200, "over two hundred the motion reads as broken");
        std::env::set_var("CHILPA_PULSE_MS", "not a number");
        assert_eq!(period_ms(), PERIOD_MS);
        std::env::remove_var("CHILPA_PULSE_MS");
        assert_eq!(period_ms(), PERIOD_MS);
    }

    #[test]
    fn slow_holders_yield_to_producers_and_release_independently() {
        let mut claims = Claims::default();
        claims.hold_slow();
        claims.hold_slow();
        assert_eq!(claims.count(), 2);
        for (base, slow) in [(10, 100), (33, 99), (100, 100), (200, 200)] {
            assert_eq!(claims.period(base), slow);
            claims.hold();
            assert_eq!(claims.period(base), base, "a producer always keeps its cadence");
            claims.release();
        }
        claims.release_slow();
        assert_eq!(claims.period(33), 99);
        claims.hold();
        claims.release_slow();
        claims.release_slow();
        assert_eq!(claims.count(), 1, "a slow release cannot release a producer");
        assert_eq!(claims.period(33), 33);
        assert_eq!(claims.release(), 0);
    }
}
