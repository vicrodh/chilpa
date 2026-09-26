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
//! screen at start ("album:7"), for the same reason, and
//! `CHILPA_PLAY_ALBUM=<id>` starts a record playing once the window is up,
//! and `CHILPA_ADD_FOLDER=<path>` adds a folder to the library and scans it.

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
        /// When to take the shot, in milliseconds after start.
        #[qproperty(i32, shot_at)]
        /// The display's period, which QML reads from the screen: frames are
        /// late against it, not against a median that moves with the load.
        #[qproperty(f64, period)]
        /// The screen to open at start, and which record: from "album:7".
        #[qproperty(QString, route)]
        #[qproperty(i64, route_id)]
        /// For "folders:/some/path", the path.
        #[qproperty(QString, route_key)]
        #[qproperty(QString, add_folder)]
        type Dev = super::DevRust;

        /// Called on every presented frame while `frames` is on.
        #[qinvokable]
        fn frame(self: Pin<&mut Dev>);
    }

    impl cxx_qt::Threading for Dev {}
    impl cxx_qt::Initialize for Dev {}
}

use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::QString;
use std::pin::Pin;
use std::time::Instant;

pub struct DevRust {
    frames: bool,
    scroll: i32,
    shot: QString,
    shot_at: i32,
    period: f64,
    route: QString,
    route_id: i64,
    route_key: QString,
    add_folder: QString,
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
            shot_at: var("CHILPA_SHOT_AT").parse().unwrap_or(2500),
            period: 0.0,
            route: QString::from(screen),
            route_id: id.parse().unwrap_or(0),
            route_key: QString::from(if id.parse::<i64>().is_ok() { "" } else { id }),
            add_folder: QString::from(&var("CHILPA_ADD_FOLDER")),
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
        // The display's period, as the screen states it; the median only when
        // it does not.
        let period = if self.period > 0.0 { self.period } else { at(0.5) };
        let late = |k: f64| sorted.iter().filter(|&&t| t > period * k).count();
        eprintln!(
            "frames: {} intervals at a {:.2} ms period, p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms; over 1.5 periods {}, over 2 periods {}",
            sorted.len(),
            period,
            at(0.5),
            at(0.99),
            sorted[sorted.len() - 1],
            late(1.5),
            late(2.0)
        );
        // Where the late ones fell, since the first frame counted: one at
        // the start of a run and one in the middle are different problems.
        let mut elapsed = 0.0;
        let mut when = Vec::new();
        for (i, t) in self.intervals.iter().enumerate() {
            elapsed += t;
            if *t > period * 2.0 && when.len() < 8 {
                when.push(format!("#{i} {t:.0} ms at {:.2} s", elapsed / 1000.0));
            }
        }
        if !when.is_empty() {
            eprintln!("late frames: {}", when.join(", "));
        }
    }
}

impl cxx_qt::Initialize for qobject::Dev {
    fn initialize(self: Pin<&mut Self>) {
        let Some(id) = std::env::var("CHILPA_PLAY_ALBUM").ok().and_then(|v| v.parse::<i64>().ok()) else {
            return;
        };
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let _ = thread.queue(move |_| {
                crate::app::with(|api| {
                    let paths: Vec<String> = api.album_track_rows(id).into_iter().map(|t| t.path).collect();
                    api.play_from(&paths, 0);
                });
            });
        });
    }
}
