//! Albums taken out of the library: the question an album's menu raises
//! before one goes, and the list Settings > Library opens to bring them
//! back. The work is the facade's (`chilpa_api` `removed_albums`).

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
        /// Moves when an album went or came back.
        #[qproperty(i32, rev)]
        type RemovedAlbums = super::RemovedAlbumsRust;

        /// An album's menu asked to take it out; the window asks first.
        #[qsignal]
        fn asked(self: Pin<&mut RemovedAlbums>, id: i64, title: &QString);

        /// Settings asked for the list of removed albums.
        #[qsignal]
        fn listed(self: Pin<&mut RemovedAlbums>);

        #[qinvokable]
        fn ask(self: Pin<&mut RemovedAlbums>, id: i64, title: &QString);

        /// Takes the album out; its files stay where they are.
        #[qinvokable]
        fn remove(self: Pin<&mut RemovedAlbums>, id: i64) -> bool;

        /// Brings a removed album back as it was.
        #[qinvokable]
        fn restore(self: Pin<&mut RemovedAlbums>, id: i64) -> bool;

        /// The removed albums, the latest first, as JSON; `rev` only makes
        /// a binding read it again.
        #[qinvokable]
        fn json(self: &RemovedAlbums, rev: i32) -> QString;

        /// Opens the list if Settings asked for it since the last look.
        #[qinvokable]
        fn take_request(self: Pin<&mut RemovedAlbums>);
    }
}

use chilpa_qt_kit::app;
use core::pin::Pin;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct RemovedAlbumsRust {
    rev: i32,
}

impl qobject::RemovedAlbums {
    pub fn ask(self: Pin<&mut Self>, id: i64, title: &QString) {
        if id > 0 {
            self.asked(id, title);
        }
    }
    pub fn remove(mut self: Pin<&mut Self>, id: i64) -> bool {
        let done = app::with(|api| api.remove_album_from_library(id));
        if done {
            let next = self.rev().wrapping_add(1);
            self.as_mut().set_rev(next);
        }
        done
    }
    pub fn restore(mut self: Pin<&mut Self>, id: i64) -> bool {
        let done = app::with(|api| api.restore_removed_album(id));
        if done {
            let next = self.rev().wrapping_add(1);
            self.as_mut().set_rev(next);
        }
        done
    }
    pub fn json(&self, _rev: i32) -> QString {
        QString::from(&app::with(|api| api.removed_albums_json()))
    }
    pub fn take_request(self: Pin<&mut Self>) {
        if app::with(|api| api.take_removed_albums_request()) {
            self.listed();
        }
    }
}
