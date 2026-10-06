//! The timer that stops playback and the A-B loop, as the queue's foot,
//! the playback options and the seek bar read them.
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
        /// What is left of the timer, ready to show. Empty while it is off.
        #[qproperty(QString, sleep_left)]
        /// 0 with no loop, 1 with a start waiting for its end, 2 looping.
        #[qproperty(i32, loop_state)]
        #[qproperty(f64, loop_start)]
        #[qproperty(f64, loop_end)]
        type Controls = super::ControlsRust;
        #[qinvokable]
        fn refresh(self: Pin<&mut Controls>);
        /// Stops playback after that many minutes; zero turns the timer off.
        #[qinvokable]
        fn sleep_in(self: Pin<&mut Controls>, minutes: i32);
        /// The start of the loop, then its end, then lets it go.
        #[qinvokable]
        fn mark_loop(self: Pin<&mut Controls>);
        #[qinvokable]
        fn clear_loop(self: Pin<&mut Controls>);
        /// Whether closing the window should hide it to the tray instead.
        #[qinvokable]
        fn closes_to_tray(self: &Controls) -> bool;
        /// The tray's menu, in the language chosen now.
        #[qinvokable]
        fn relabel_tray(self: &Controls);
    }
}
use chilpa_qt_kit::app;
use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct ControlsRust {
    sleep_left: QString,
    loop_state: i32,
    loop_start: f64,
    loop_end: f64,
}

impl qobject::Controls {
    pub fn refresh(mut self: Pin<&mut Self>) {
        let (left, (state, start, end)) = app::with(|api| (api.sleep_left(), api.ab_state()));
        self.as_mut().set_sleep_left(QString::from(&left));
        self.as_mut().set_loop_state(state);
        self.as_mut().set_loop_start(start);
        self.set_loop_end(end);
    }

    pub fn sleep_in(mut self: Pin<&mut Self>, minutes: i32) {
        app::with(|api| if minutes > 0 { api.sleep_in_minutes(minutes) } else { api.set_sleep_timer(None) });
        self.as_mut().refresh();
    }

    pub fn mark_loop(mut self: Pin<&mut Self>) {
        app::with(|api| api.ab_mark());
        self.as_mut().refresh();
    }

    pub fn clear_loop(mut self: Pin<&mut Self>) {
        app::with(|api| api.ab_clear());
        self.as_mut().refresh();
    }

    pub fn closes_to_tray(&self) -> bool {
        app::with(|api| api.closes_to_tray())
    }

    pub fn relabel_tray(&self) {
        app::with(|api| api.relabel_tray());
    }
}
