//! What the queue's About tab says of a track, read from the facade: the one
//! that plays, or one a list's menu chose.
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
        /// The track `path` names, as JSON; `path` and `rev` only ask again
        /// when what is asked for or what was read changes.
        #[qinvokable]
        fn track(self: &NowAbout, path: &QString, rev: i32) -> QString;
        /// The record it is on, in the same shape.
        #[qinvokable]
        fn album(self: &NowAbout, path: &QString, rev: i32) -> QString;
        #[qinvokable]
        fn refetch_album(self: &NowAbout, path: &QString) -> bool;
        #[qinvokable]
        fn retry(self: &NowAbout, part: &QString, path: &QString) -> bool;
        /// A menu asks the window to show what is known of a track: its
        /// path and the title its row shows.
        #[qsignal]
        fn track_info_requested(self: Pin<&mut NowAbout>, path: &QString, title: &QString);
    }
}
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct NowAboutRust;

impl qobject::NowAbout {
    pub fn refetch_album(&self, path: &QString) -> bool {
        chilpa_qt_kit::app::with(|api| api.refetch_album_about(&path.to_string()))
    }

    pub fn retry(&self, part: &QString, path: &QString) -> bool {
        chilpa_qt_kit::app::with(|api| api.retry_about(&part.to_string(), &path.to_string()))
    }

    pub fn track(&self, path: &QString, _rev: i32) -> QString {
        QString::from(&chilpa_qt_kit::app::with(|api| api.track_about(&path.to_string())))
    }
    pub fn album(&self, path: &QString, _rev: i32) -> QString {
        QString::from(&chilpa_qt_kit::app::with(|api| api.album_about(&path.to_string())))
    }
}
