//! Offline music's way to the facade: each action by name, which this
//! bridge hands over without knowing it, and one snapshot of every track's
//! offline state that the rows read in place. Music servers' request for
//! the page is the only thing that leads away from where the listener is.
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
        #[qproperty(i32, rev)]
        #[qproperty(bool, active)]
        type Offline = super::OfflineRust;

        /// Music servers asks for the Offline music page.
        #[qsignal]
        fn requested(self: Pin<&mut Offline>, kind: &QString, selected: &QString);
        /// One of the page's or a menu's actions, answered as JSON. An
        /// action that changes something reads the snapshot again.
        #[qinvokable]
        fn call(self: Pin<&mut Offline>, action: &QString, kind: &QString, selected: &QString) -> QString;
        /// A track's state: empty, queued, downloading, verifying,
        /// removing, ready or failed.
        #[qinvokable]
        fn state(self: &Offline, id: i64, rev: i32) -> QString;
        /// How much of the track's file arrived, or -1 while unknown.
        #[qinvokable]
        fn fraction(self: &Offline, id: i64, rev: i32) -> f64;
        #[qinvokable]
        fn error(self: &Offline, id: i64, rev: i32) -> QString;
        /// What the row's menu button says of the track besides "More".
        #[qinvokable]
        fn tip(self: &Offline, id: i64, rev: i32) -> QString;
        /// How far a selection that is going has got, or -1.
        #[qinvokable]
        fn job(self: &Offline, kind: &QString, selected: &QString, rev: i32) -> f64;
        /// A menu's offline entries, as JSON.
        #[qinvokable]
        fn menu(self: &Offline, kind: &QString, selected: &QString) -> QString;
        /// Reads the snapshot again; `reread` reads the library's copies too.
        #[qinvokable]
        fn refresh(self: Pin<&mut Offline>, reread: bool);
    }
    impl cxx_qt::Initialize for Offline {}
}

use cxx_qt::CxxQtType as _;
use cxx_qt_lib::QString;
use std::pin::Pin;

/// What only reads: everything else is followed by a fresh snapshot.
const READS: [&str; 5] = ["info", "menu", "manager", "status", "removal_info"];

#[derive(Default)]
pub struct OfflineRust {
    rev: i32,
    active: bool,
    values: chilpa_api::OfflineMarks,
}

impl qobject::Offline {
    pub fn call(mut self: Pin<&mut Self>, action: &QString, kind: &QString, selected: &QString) -> QString {
        let action = action.to_string();
        let answer = chilpa_qt_kit::app::with(|api| api.server_offline(&action, &kind.to_string(), &selected.to_string()));
        if !READS.contains(&action.as_str()) {
            self.as_mut().refresh(false);
        }
        QString::from(answer)
    }
    pub fn state(&self, id: i64, _rev: i32) -> QString {
        QString::from(self.rust().values.state(id))
    }
    pub fn fraction(&self, id: i64, _rev: i32) -> f64 {
        self.rust().values.fraction(id)
    }
    pub fn error(&self, id: i64, _rev: i32) -> QString {
        QString::from(self.rust().values.error(id))
    }
    pub fn tip(&self, id: i64, _rev: i32) -> QString {
        QString::from(self.rust().values.tip(id))
    }
    pub fn job(&self, kind: &QString, selected: &QString, _rev: i32) -> f64 {
        self.rust().values.job(&kind.to_string(), &selected.to_string())
    }
    pub fn menu(&self, kind: &QString, selected: &QString) -> QString {
        QString::from(chilpa_qt_kit::app::with(|api| api.server_offline("menu", &kind.to_string(), &selected.to_string())))
    }
    pub fn refresh(mut self: Pin<&mut Self>, reread: bool) {
        let values = chilpa_qt_kit::app::with(|api| api.offline_marks(reread));
        let active = values.active();
        self.as_mut().rust_mut().values = values;
        self.as_mut().set_active(active);
        let next = self.rev().wrapping_add(1);
        self.set_rev(next);
    }
}

impl cxx_qt::Initialize for qobject::Offline {
    fn initialize(self: Pin<&mut Self>) {
        self.refresh(false);
    }
}
