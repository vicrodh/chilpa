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

        /// The folders and playlists a search and a filter leave, in an
        /// order, inside a folder or at the top: as JSON.
        #[qinvokable]
        fn json(self: &PlaylistManager, query: &QString, filter: &QString, order: &QString, ascending: bool, folder_id: i64) -> QString;
        #[qinvokable]
        fn group_sources(self: &PlaylistManager, grouped: bool);
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
    pub fn json(&self, query: &QString, filter: &QString, order: &QString, ascending: bool, folder_id: i64) -> QString {
        let rows = app::with(|api| api.managed_playlists(&query.to_string(), &filter.to_string(), &order.to_string(), ascending, (folder_id > 0).then_some(folder_id)));
        QString::from(&serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into()))
    }
}
