//! What the lines of the lyrics ask the facade about how they are drawn.
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
        type LyricsView = super::LyricsViewRust;
        /// How lit a line is, from 0 to 1: `position` is its place among
        /// the lines and `current` the place of the one being sung, or -1.
        #[qinvokable]
        fn dim(self: &LyricsView, mode: &QString, synced: bool, position: i32, current: i32) -> f64;
    }
}
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct LyricsViewRust;

impl qobject::LyricsView {
    pub fn dim(&self, mode: &QString, synced: bool, position: i32, current: i32) -> f64 {
        chilpa_api::lyric_dimming(&mode.to_string(), synced, position, current)
    }
}
