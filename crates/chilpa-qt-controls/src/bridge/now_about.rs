//! What the queue's About tab says of what plays, read from the facade.
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
        type NowAbout = super::NowAboutRust;
        /// The track that plays, as JSON; `path` and `rev` only ask again
        /// when what plays or what was read changes.
        #[qinvokable]
        fn track(self: &NowAbout, path: &QString, rev: i32) -> QString;
        /// The record it is on, in the same shape.
        #[qinvokable]
        fn album(self: &NowAbout, path: &QString, rev: i32) -> QString;
    }
}
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct NowAboutRust;

impl qobject::NowAbout {
    pub fn track(&self, _path: &QString, _rev: i32) -> QString {
        QString::from(&chilpa_qt_kit::app::with(|api| api.now_track_about()))
    }
    pub fn album(&self, _path: &QString, _rev: i32) -> QString {
        QString::from(&chilpa_qt_kit::app::with(|api| api.now_album_about()))
    }
}
