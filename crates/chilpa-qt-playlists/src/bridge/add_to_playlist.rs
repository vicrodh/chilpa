//! Asks the window's one add-to-playlist picker to open. It is mounted once
//! and outlives the menus that ask for it, as QBZ's is: a menu is closed by
//! the time somebody picks a playlist.
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
        type AddToPlaylist = super::AddToPlaylistRust;

        /// Opens the picker for those library track ids (JSON); with none,
        /// it is "New playlist".
        #[qinvokable]
        fn show(self: Pin<&mut AddToPlaylist>, tracks: &QString);

        #[qsignal]
        fn asked(self: Pin<&mut AddToPlaylist>, tracks: QString);
    }
}

use cxx_qt_lib::QString;
use std::pin::Pin;

#[derive(Default)]
pub struct AddToPlaylistRust;

impl qobject::AddToPlaylist {
    pub fn show(self: Pin<&mut Self>, tracks: &QString) {
        self.asked(tracks.clone());
    }
}
