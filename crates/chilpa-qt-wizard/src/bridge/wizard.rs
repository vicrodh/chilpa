//! The wizard's document, read from the facade whenever it moved: the modal
//! asks for it while it is open, and when a Settings button may have
//! opened it.
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
        /// Everything the modal shows, as JSON.
        #[qproperty(QString, doc)]
        type HifiWizard = super::HifiWizardRust;

        /// One of the modal's actions, with an index or a text.
        #[qinvokable]
        fn command(self: Pin<&mut HifiWizard>, action: &QString, argument: &QString);
        /// Reads the document again when it moved.
        #[qinvokable]
        fn refresh(self: Pin<&mut HifiWizard>);
    }
}

use chilpa_qt_kit::app;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::QString;
use std::pin::Pin;

pub struct HifiWizardRust {
    doc: QString,
    seen: u64,
}

impl Default for HifiWizardRust {
    fn default() -> Self {
        Self { doc: QString::from("{}"), seen: u64::MAX }
    }
}

impl qobject::HifiWizard {
    pub fn command(self: Pin<&mut Self>, action: &QString, argument: &QString) {
        app::with(|api| api.wizard_command(&action.to_string(), &argument.to_string()));
        self.refresh();
    }

    pub fn refresh(mut self: Pin<&mut Self>) {
        let seen = self.rust().seen;
        if let Some((revision, json)) = app::with(|api| api.wizard_doc(seen)) {
            self.as_mut().rust_mut().seen = revision;
            self.as_mut().set_doc(QString::from(&json));
        }
    }
}
