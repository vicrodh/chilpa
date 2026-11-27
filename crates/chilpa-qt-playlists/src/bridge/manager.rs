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
        /// The music servers by the source their playlists carry: name,
        /// icon and whether they are on, as JSON.
        #[qinvokable]
        fn servers_json(self: &PlaylistManager) -> QString;
        /// What a playlist page shows of a server's own playlist, as JSON;
        /// `{}` for any other.
        #[qinvokable]
        fn server_playlist(self: &PlaylistManager, id: i64, rev: i32) -> QString;
        /// Reads every server's playlists again.
        #[qinvokable]
        fn refresh_servers(self: &PlaylistManager);
        /// Asks where with the system's save dialog and writes the list
        /// there: a playlist of the library (`id` above 0) or a source's
        /// (below), with its files or as plain data. `exported` says how it
        /// went.
        #[qinvokable]
        fn export_list(self: Pin<&mut PlaylistManager>, id: i64, plain: bool);
        /// What writing a list left, as JSON: written, left_out and error.
        #[qsignal]
        fn exported(self: Pin<&mut PlaylistManager>, result: &QString);
        /// Whether a name carries a character a source may refuse.
        #[qinvokable]
        fn name_is_unusual(self: &PlaylistManager, name: &QString) -> bool;
    }

    impl cxx_qt::Threading for PlaylistManager {}
}

use chilpa_qt_kit::app;
use core::pin::Pin;
use cxx_qt::Threading;
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
    pub fn servers_json(&self) -> QString {
        QString::from(&app::with(|api| api.playlist_servers_json()))
    }
    pub fn server_playlist(&self, id: i64, _rev: i32) -> QString {
        QString::from(&app::with(|api| api.server_playlist_json(id)))
    }
    pub fn export_list(self: Pin<&mut Self>, id: i64, plain: bool) {
        let suggested = app::with(|api| api.export_file_name(id, plain));
        let filter = |name: String, extension: &str| (name, vec![extension.to_string()]);
        let mut filters = vec![filter(app::t("JSON playlist"), "json"), filter(app::t("XSPF playlist"), "xspf")];
        if !plain && id > 0 {
            filters.insert(0, filter(app::t("M3U8 playlist"), "m3u8"));
        }
        let ask = chilpa_qt_kit::dialogs::Ask::Save { suggested: chilpa_qt_kit::dialogs::file_name(&suggested), filters };
        chilpa_qt_kit::dialogs::ask(self.qt_thread(), app::t("Export playlist"), ask, move |manager, path| {
            let result = app::with(|api| api.export_list(id, &path, plain)).json();
            manager.exported(&QString::from(&result));
        });
    }
    pub fn name_is_unusual(&self, name: &QString) -> bool {
        chilpa_api::playlist_name_is_unusual(&name.to_string())
    }
    pub fn refresh_servers(&self) {
        app::with(|api| api.refresh_server_playlists());
    }
}
