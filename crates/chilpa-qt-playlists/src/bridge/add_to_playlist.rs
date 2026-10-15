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

        #[qinvokable]
        fn finish(self: Pin<&mut AddToPlaylist>);

        #[qsignal]
        fn asked(self: Pin<&mut AddToPlaylist>, tracks: QString);
    }
}

use cxx_qt_lib::QString;
use cxx_qt::CxxQtType as _;
use std::pin::Pin;

#[derive(Default)]
pub struct AddToPlaylistRust {
    hold: Option<chilpa_api::SelectionHold>,
}

impl qobject::AddToPlaylist {
    pub fn show(mut self: Pin<&mut Self>, tracks: &QString) {
        let ids: Vec<i64> = serde_json::from_str(&tracks.to_string()).unwrap_or_default();
        self.as_mut().rust_mut().hold = Some(chilpa_qt_kit::app::with(|api| api.hold_playlist_tracks(&ids)));
        self.asked(tracks.clone());
    }
    pub fn finish(self: Pin<&mut Self>) { self.rust_mut().hold = None; }
}
