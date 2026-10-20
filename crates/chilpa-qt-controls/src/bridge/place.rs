//! Where the window was left, for it to open there, and where it is now,
//! for the next run. The rule of what is worth keeping is the facade's.
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
        /// The floating size the window was left at.
        #[qproperty(i32, width)]
        #[qproperty(i32, height)]
        /// Its top left corner, when `placed`.
        #[qproperty(i32, left)]
        #[qproperty(i32, top)]
        #[qproperty(bool, placed)]
        #[qproperty(bool, maximized)]
        /// The smallest the window gets, in the logical pixels it speaks:
        /// the same footprint at every interface size.
        #[qproperty(i32, smallest_width)]
        #[qproperty(i32, smallest_height)]
        type WindowPlace = super::PlaceRust;
        /// A frame the window settled on: "floating", "maximized" or
        /// anything else for one that says nothing. `placed` is whether
        /// the system tells the window where it is.
        #[qinvokable]
        fn keep(self: Pin<&mut WindowPlace>, frame: &QString, width: i32, height: i32, left: i32, top: i32, placed: bool);
        /// The window is shown (not hidden nor minimized) and active: what
        /// the facade reads again in the background waits while it is not.
        #[qinvokable]
        fn seen(self: &WindowPlace, shown: bool, active: bool);
    }
}
use chilpa_api::window_place::Frame;
use chilpa_qt_kit::app;
use cxx_qt_lib::QString;
use std::pin::Pin;

pub struct PlaceRust {
    width: i32,
    height: i32,
    left: i32,
    top: i32,
    placed: bool,
    maximized: bool,
    smallest_width: i32,
    smallest_height: i32,
}

/// Read once, as the window is built: these are where it opens, not where
/// it is.
impl Default for PlaceRust {
    fn default() -> Self {
        let place = app::with(|api| api.window_place());
        let (left, top) = place.at.unwrap_or_default();
        let smallest = chilpa_api::window_place::smallest();
        Self { width: place.width, height: place.height, left, top, placed: place.at.is_some(), maximized: place.maximized, smallest_width: smallest.0, smallest_height: smallest.1 }
    }
}

impl qobject::WindowPlace {
    pub fn keep(self: Pin<&mut Self>, frame: &QString, width: i32, height: i32, left: i32, top: i32, placed: bool) {
        let frame = Frame::named(&frame.to_string());
        app::with(|api| api.keep_window_place(frame, width, height, placed.then_some((left, top))));
    }
    pub fn seen(&self, shown: bool, active: bool) {
        app::with(|api| api.set_window_state(shown, active));
    }
}
