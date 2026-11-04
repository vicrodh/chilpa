//! Extra presentation controls reuse the facade's authoritative transport.
#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type TransportActions = super::TransportActionsRust;
        #[qinvokable]
        fn stop(self: &TransportActions);
        #[qinvokable]
        fn position(self: &TransportActions, revision: i32) -> QString;
    }
}
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct TransportActionsRust;

impl qobject::TransportActions {
    pub fn stop(&self) { chilpa_qt_kit::app::with(|api| api.stop()); }

    pub fn position(&self, _revision: i32) -> QString {
        chilpa_qt_kit::app::with(|api| {
            let now = api.now_playing();
            let position = if now.queue_len == 0 { 0 } else { now.index.saturating_add(1) };
            QString::from(&format!("{position}/{}", now.queue_len))
        })
    }
}
