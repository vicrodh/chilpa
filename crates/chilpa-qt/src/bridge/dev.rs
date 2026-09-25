//! Instruments for measuring the window, off unless asked for.
//!
//! `CHILPA_SCROLL=<pixels per second>` scrolls the screen's longest view at
//! that speed from top to bottom, records the frames while it moves, and
//! quits: the same scroll every run, so two runs can be compared.
//! `CHILPA_FRAMES=1` records the time between presented frames and prints,
//! on exit, the percentiles and how many frames missed one and two display
//! periods: what the frontend's performance targets are stated in.
//! `CHILPA_SHOT=<file.png>` saves the window once it has settled and quits,
//! so a screen can be looked at without a display. `CHILPA_ROUTE` opens a
//! screen at start ("album:7"), for the same reason.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, frames)]
        #[qproperty(i32, scroll)]
        #[qproperty(QString, shot)]
        /// The screen to open at start, and which record: from "album:7".
        #[qproperty(QString, route)]
        #[qproperty(i64, route_id)]
        type Dev = super::DevRust;

        /// Called on every presented frame while `frames` is on.
        #[qinvokable]
        fn frame(self: Pin<&mut Dev>);
    }
}

use cxx_qt::CxxQtType as _;
use cxx_qt_lib::QString;
use std::pin::Pin;
use std::time::Instant;

pub struct DevRust {
    frames: bool,
    scroll: i32,
    shot: QString,
    route: QString,
    route_id: i64,
    last: Option<Instant>,
    intervals: Vec<f64>,
}

impl Default for DevRust {
    fn default() -> Self {
        let var = |k: &str| std::env::var(k).unwrap_or_default();
        let route = var("CHILPA_ROUTE");
        let (screen, id) = route.split_once(':').unwrap_or((&route, "0"));
        Self {
            frames: !var("CHILPA_FRAMES").is_empty() || !var("CHILPA_SCROLL").is_empty(),
            scroll: var("CHILPA_SCROLL").parse().unwrap_or(0),
            shot: QString::from(&var("CHILPA_SHOT")),
            route: QString::from(screen),
            route_id: id.parse().unwrap_or(0),
            last: None,
            intervals: Vec::new(),
        }
    }
}

impl qobject::Dev {
    pub fn frame(mut self: Pin<&mut Self>) {
        let now = Instant::now();
        let mut this = self.as_mut().rust_mut();
        if let Some(last) = this.last {
            this.intervals.push(now.duration_since(last).as_secs_f64() * 1000.0);
        }
        this.last = Some(now);
    }
}

impl Drop for DevRust {
    fn drop(&mut self) {
        if !self.frames || self.intervals.is_empty() {
            return;
        }
        let mut sorted = self.intervals.clone();
        sorted.sort_by(f64::total_cmp);
        let at = |p: f64| sorted[((sorted.len() - 1) as f64 * p).round() as usize];
        // The display's period is taken from the median frame: with the
        // window busy, most frames arrive on time.
        let period = at(0.5);
        let late = |k: f64| sorted.iter().filter(|&&t| t > period * k).count();
        eprintln!(
            "frames: {} intervals, p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms; over 1.5 periods {}, over 2 periods {}",
            sorted.len(),
            period,
            at(0.99),
            sorted[sorted.len() - 1],
            late(1.5),
            late(2.0)
        );
    }
}
