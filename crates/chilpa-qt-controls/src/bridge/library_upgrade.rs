//! A library older than this Chilpa, upgraded while the startup cover
//! waits: what the cover says of it and what it can be asked from there.
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
        /// The upgrade as JSON: `active` while the window has to wait for it,
        /// what it is doing and how far, or why it failed.
        #[qproperty(QString, snapshot)]
        type LibraryUpgrade = super::LibraryUpgradeRust;
        /// Reads it again; the cover asks while it waits.
        #[qinvokable]
        fn refresh(self: Pin<&mut LibraryUpgrade>);
        /// Runs a failed upgrade again.
        #[qinvokable]
        fn retry(self: Pin<&mut LibraryUpgrade>);
        /// Quits without the window's question.
        #[qinvokable]
        fn quit(self: Pin<&mut LibraryUpgrade>);
        /// Puts the copy from before a failed upgrade back, and quits.
        #[qinvokable]
        fn restore(self: Pin<&mut LibraryUpgrade>);
    }
}
use chilpa_qt_kit::app;
use cxx_qt_lib::QString;
use std::pin::Pin;

pub struct LibraryUpgradeRust {
    snapshot: QString,
}

impl Default for LibraryUpgradeRust {
    fn default() -> Self {
        Self { snapshot: QString::from(&app::with(|api| api.library_upgrade_view())) }
    }
}

impl qobject::LibraryUpgrade {
    pub fn refresh(self: Pin<&mut Self>) {
        let view = QString::from(&app::with(|api| api.library_upgrade_view()));
        self.set_snapshot(view);
    }

    pub fn retry(self: Pin<&mut Self>) {
        app::with(|api| api.retry_library_upgrade());
        self.refresh();
    }

    pub fn quit(self: Pin<&mut Self>) {
        app::with(|api| api.leave_library_upgrade());
    }

    pub fn restore(self: Pin<&mut Self>) {
        app::with(|api| api.restore_library_copy());
        self.refresh();
    }
}
