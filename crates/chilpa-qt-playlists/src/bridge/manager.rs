//! What the playlist manager lists, read from the facade.
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
        type PlaylistManager = super::PlaylistManagerRust;

        /// The folders and playlists a search, a filter and a source leave,
        /// in an order, inside a folder or at the top: as JSON.
        #[qinvokable]
        fn json(self: &PlaylistManager, query: &QString, filter: &QString, source: &QString, order: &QString, ascending: bool, folder_id: i64) -> QString;
        /// The sources the playlists come from, for the source filter.
        #[qinvokable]
        fn sources_json(self: &PlaylistManager) -> QString;
        #[qinvokable]
        fn group_sources(self: &PlaylistManager, grouped: bool);
        /// The order a playlist's tracks were last shown in, and keeping a new one.
        #[qinvokable]
        fn track_order(self: &PlaylistManager, id: i64) -> QString;
        #[qinvokable]
        fn set_track_order(self: &PlaylistManager, id: i64, order: &QString);
    }
}

use chilpa_qt_kit::app;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct PlaylistManagerRust;

impl qobject::PlaylistManager {
    pub fn group_sources(&self, grouped: bool) {
        app::with(|api| api.group_playlists_by_source(grouped));
        chilpa_qt_kit::bridge::prefs::changed();
    }
    pub fn track_order(&self, id: i64) -> QString {
        QString::from(&app::with(|api| api.playlist_track_order(id)))
    }
    pub fn set_track_order(&self, id: i64, order: &QString) {
        app::with(|api| api.set_playlist_track_order(id, &order.to_string()));
    }
    pub fn json(&self, query: &QString, filter: &QString, source: &QString, order: &QString, ascending: bool, folder_id: i64) -> QString {
        let rows = app::with(|api| api.managed_playlists(&query.to_string(), &filter.to_string(), &source.to_string(), &order.to_string(), ascending, (folder_id > 0).then_some(folder_id)));
        QString::from(&serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into()))
    }
    pub fn sources_json(&self) -> QString {
        QString::from(&serde_json::to_string(&app::with(|api| api.managed_playlist_sources())).unwrap_or_else(|_| "[]".into()))
    }
}
