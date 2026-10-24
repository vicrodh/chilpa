//! The deck's document, read from the facade whenever it moved: when the
//! window asks what this start shows, when the menu opens What's new, and
//! while the release notes are being read.
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
        /// Everything the deck shows, as JSON.
        #[qproperty(QString, doc)]
        type Onboarding = super::OnboardingRust;

        /// `start` once the window is up; `welcome`, `whatsNew` and `close`.
        #[qinvokable]
        fn command(self: Pin<&mut Onboarding>, action: &QString);
        /// Reads the document again when it moved.
        #[qinvokable]
        fn refresh(self: Pin<&mut Onboarding>);
    }
}

use chilpa_qt_kit::app;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::QString;
use std::pin::Pin;

pub struct OnboardingRust {
    doc: QString,
    seen: u64,
}

impl Default for OnboardingRust {
    fn default() -> Self {
        Self { doc: QString::from("{}"), seen: u64::MAX }
    }
}

impl qobject::Onboarding {
    pub fn command(self: Pin<&mut Self>, action: &QString) {
        let action = action.to_string();
        app::with(|api| if action == "start" { api.onboarding_start() } else { api.onboarding_command(&action) });
        self.refresh();
    }

    pub fn refresh(mut self: Pin<&mut Self>) {
        let seen = self.rust().seen;
        if let Some((revision, json)) = app::with(|api| api.onboarding_doc(seen)) {
            self.as_mut().rust_mut().seen = revision;
            self.as_mut().set_doc(QString::from(&json));
        }
    }
}
