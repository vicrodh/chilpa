//! That a preference changed, said to whoever shows it.
//!
//! A setting a feature takes in the facade (frontend ADR-0015) is written
//! without the window's bridge knowing its name, so nothing on this side
//! can tell the feature's own objects to read it again. This says only
//! "something changed": each one reads its own keys back on the edge of
//! `rev`. A control that moves a value live before it is kept, like a
//! slider, says what it is showing in `previewKey` and `previewValue`, and
//! clears them when the value is kept.

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
        /// Bumped every time a preference is kept.
        #[qproperty(i32, rev)]
        /// The preference a control is moving live, and the value it shows;
        /// empty when none is.
        #[qproperty(QString, preview_key)]
        #[qproperty(QString, preview_value)]
        /// Whether the playing row's mark is the moving bars.
        #[qproperty(bool, animated_indicator)]
        type Prefs = super::PrefsRust;

        /// A control shows `value` for `key` before keeping it. Empty
        /// arguments end the preview.
        #[qinvokable]
        fn preview(self: Pin<&mut Prefs>, key: &QString, value: &QString);
    }

    impl cxx_qt::Threading for Prefs {}
    impl cxx_qt::Initialize for Prefs {}
}

use cxx_qt::Threading as _;
use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct PrefsRust {
    rev: i32,
    preview_key: QString,
    preview_value: QString,
    animated_indicator: bool,
}

impl qobject::Prefs {
    pub fn preview(mut self: Pin<&mut Self>, key: &QString, value: &QString) {
        self.as_mut().set_preview_key(key.clone());
        self.as_mut().set_preview_value(value.clone());
    }
}

impl cxx_qt::Initialize for qobject::Prefs {
    fn initialize(mut self: Pin<&mut Self>) {
        let animated = crate::app::with(|api| api.play_indicator_animated());
        self.as_mut().set_animated_indicator(animated);
        let thread = self.qt_thread();
        THREAD.with(|slot| *slot.borrow_mut() = Some(thread));
    }
}

thread_local! {
    static THREAD: std::cell::RefCell<Option<cxx_qt::CxxQtThread<qobject::Prefs>>> = const { std::cell::RefCell::new(None) };
}

/// A preference was kept. Dropped before the window is up, when nobody is
/// showing anything yet.
pub fn changed() {
    THREAD.with(|slot| {
        if let Some(thread) = slot.borrow().as_ref() {
            let _ = thread.queue(|mut prefs| {
                let next = prefs.rev().wrapping_add(1);
                prefs.as_mut().set_rev(next);
                let animated = crate::app::with(|api| api.play_indicator_animated());
                prefs.as_mut().set_animated_indicator(animated);
            });
        }
    });
}
