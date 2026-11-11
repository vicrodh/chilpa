//! The Offline music page's way to the facade: each action by name, which
//! this bridge hands over without knowing it, and the menus' request for the
//! page.
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
        type Offline = super::OfflineRust;

        /// A menu or Music servers asks for the page, with what it chose:
        /// `tracks`, `album` or `playlist` and their ids, or `downloads`.
        #[qsignal]
        fn requested(self: Pin<&mut Offline>, kind: &QString, selected: &QString);
        /// One of the page's actions, answered as JSON.
        #[qinvokable]
        fn call(self: &Offline, action: &QString, kind: &QString, selected: &QString) -> QString;
    }
}

use cxx_qt_lib::QString;

#[derive(Default)]
pub struct OfflineRust;

impl qobject::Offline {
    pub fn call(&self, action: &QString, kind: &QString, selected: &QString) -> QString {
        QString::from(chilpa_qt_kit::app::with(|api| api.server_offline(&action.to_string(), &kind.to_string(), &selected.to_string())))
    }
}
