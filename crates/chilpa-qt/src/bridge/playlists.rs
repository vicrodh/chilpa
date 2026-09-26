//! The playlists' actions, and a revision that moves when any of them
//! changes: the sidebar's list and a playlist's page read again on it.
//!
//! The list itself is a `PlaylistListModel`: a singleton handed whole to a
//! view as its model fails in compiled QML (see `folders`).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Bumped whenever a playlist is made, renamed, deleted or changed.
        #[qproperty(i32, rev)]
        type Playlists = super::PlaylistsRust;

        /// Makes an empty playlist with a name not taken yet and opens it.
        #[qinvokable]
        fn create(self: Pin<&mut Playlists>) -> i64;

        #[qinvokable]
        fn rename(self: Pin<&mut Playlists>, id: i64, name: &QString);

        /// Deletes a playlist. The tracks are not touched.
        #[qinvokable]
        fn remove(self: Pin<&mut Playlists>, id: i64);

        /// Plays a playlist from the top, shuffled or not.
        #[qinvokable]
        fn play(self: Pin<&mut Playlists>, id: i64, shuffled: bool);

        /// Adds a playlist to the end of the queue.
        #[qinvokable]
        fn queue(self: Pin<&mut Playlists>, id: i64);

        /// Keeps what plays now and what comes next as a new playlist.
        #[qinvokable]
        fn save_queue(self: Pin<&mut Playlists>, name: &QString) -> i64;

        /// A playlist's name, for a tab or a header.
        #[qinvokable]
        fn name(self: &Playlists, id: i64, rev: i32) -> QString;

        /// The covers of its first records, as file URLs: its mosaic.
        #[qinvokable]
        fn covers(self: &Playlists, id: i64, rev: i32) -> QStringList;

        /// Our playlists' names and, in the same order, their ids: what a
        /// menu that adds to one lists.
        #[qinvokable]
        fn names(self: &Playlists, rev: i32) -> QStringList;
        #[qinvokable]
        fn ids(self: &Playlists, rev: i32) -> QStringList;
    }

    impl cxx_qt::Threading for Playlists {}
    impl cxx_qt::Initialize for Playlists {}
}

use crate::{app, registry};
use cxx_qt::Threading as _;
use cxx_qt_lib::{QList, QString, QStringList, QUrl};
use std::pin::Pin;

#[derive(Default)]
pub struct PlaylistsRust {
    rev: i32,
}

fn strings(items: impl Iterator<Item = String>) -> QStringList {
    let mut list = QList::<QString>::default();
    for i in items {
        list.append(QString::from(&i));
    }
    QStringList::from(&list)
}

fn paths_of(id: i64) -> Vec<String> {
    app::with(|api| api.playlist_rows(id)).into_iter().map(|t| t.path).collect()
}

impl qobject::Playlists {
    pub fn create(mut self: Pin<&mut Self>) -> i64 {
        let taken: Vec<String> = app::with(|api| api.local_playlists()).into_iter().map(|p| p.name).collect();
        let base = app::t("New playlist");
        let name = (1..)
            .map(|n| if n == 1 { base.clone() } else { format!("{base} {n}") })
            .find(|candidate| !taken.contains(candidate))
            .unwrap_or(base);
        let Some(id) = app::with(|api| api.create_playlist(&name)) else { return 0 };
        self.as_mut().changed();
        registry::navigation(move |nav| nav.go(&QString::from("playlist"), id));
        id
    }

    pub fn rename(mut self: Pin<&mut Self>, id: i64, name: &QString) {
        let name = name.to_string();
        app::with(|api| api.rename_playlist(id, &name));
        self.as_mut().changed();
        // The tab carries the name.
        registry::navigation(|nav| nav.refresh_labels());
    }

    pub fn remove(mut self: Pin<&mut Self>, id: i64) {
        app::with(|api| api.delete_playlist(id));
        self.as_mut().changed();
        registry::navigation(move |nav| nav.left_behind("playlist", id));
    }

    pub fn play(self: Pin<&mut Self>, id: i64, shuffled: bool) {
        let paths = paths_of(id);
        app::with(|api| {
            api.set_shuffle(shuffled);
            api.play_from(&paths, 0);
        });
    }

    pub fn queue(self: Pin<&mut Self>, id: i64) {
        let paths = paths_of(id);
        app::with(|api| api.enqueue_here(&paths));
    }

    pub fn save_queue(self: Pin<&mut Self>, name: &QString) -> i64 {
        let id = app::with(|api| api.save_queue_as(&name.to_string())).unwrap_or(0);
        if id != 0 {
            self.changed();
        }
        id
    }

    pub fn name(&self, id: i64, _rev: i32) -> QString {
        QString::from(&app::with(|api| api.playlist(id).map(|p| p.name)).unwrap_or_default())
    }

    pub fn covers(&self, id: i64, _rev: i32) -> QStringList {
        let mut list = QList::<QString>::default();
        for path in app::with(|api| api.playlist_covers(id)) {
            list.append(QUrl::from_local_file(&QString::from(&path)).to_qstring());
        }
        QStringList::from(&list)
    }

    pub fn names(&self, _rev: i32) -> QStringList {
        strings(app::with(|api| api.local_playlists()).into_iter().map(|p| p.name))
    }

    pub fn ids(&self, _rev: i32) -> QStringList {
        strings(app::with(|api| api.local_playlists()).into_iter().map(|p| p.id.to_string()))
    }

    /// Something about the playlists changed: whoever shows them reads again.
    pub fn changed(mut self: Pin<&mut Self>) {
        let next = self.rev().wrapping_add(1);
        self.as_mut().set_rev(next);
    }
}

impl cxx_qt::Initialize for qobject::Playlists {
    fn initialize(self: Pin<&mut Self>) {
        registry::register_playlists(self.qt_thread());
    }
}
