//! The About window's document as QML reads it: the facade's, with this
//! build's date and commit, which only a build script can know.
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
        type About = super::AboutRust;

        /// What the About window shows, as one document; `rev` is the
        /// language's, so its labels follow it.
        #[qinvokable]
        fn document(self: &About, rev: i32) -> QString;

        /// Where the menu's "documentation" and "issue" rows go.
        #[qinvokable]
        fn link(self: &About, name: &QString) -> QString;
    }
}

use chilpa_qt_kit::app;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct AboutRust;

impl qobject::About {
    pub fn document(&self, _rev: i32) -> QString {
        QString::from(&app::with(|api| api.about_document(env!("CHILPA_BUILD_DATE"), env!("CHILPA_BUILD_COMMIT"))))
    }

    pub fn link(&self, name: &QString) -> QString {
        QString::from(match name.to_string().as_str() {
            "documentation" => chilpa_api::about::DOCUMENTATION,
            "issue" => chilpa_api::about::NEW_ISSUE,
            _ => "",
        })
    }
}
