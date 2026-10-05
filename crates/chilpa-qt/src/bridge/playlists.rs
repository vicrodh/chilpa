//! Playlist actions plus facade-sorted JSON projections for QML.

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
        #[qproperty(i32, rev)]
        type Playlists = super::PlaylistsRust;

        #[qinvokable]
        fn create(self: Pin<&mut Playlists>) -> i64;

        #[qinvokable]
        fn refresh(self: Pin<&mut Playlists>);

        #[qinvokable]
        fn open(self: Pin<&mut Playlists>, id: i64);

        #[qinvokable]
        fn rename(self: Pin<&mut Playlists>, id: i64, name: &QString);

        #[qinvokable]
        fn remove(self: Pin<&mut Playlists>, id: i64);

        #[qinvokable]
        fn play(self: Pin<&mut Playlists>, id: i64, shuffled: bool);

        #[qinvokable]
        fn queue(self: Pin<&mut Playlists>, id: i64);

        #[qinvokable]
        fn save_queue(self: Pin<&mut Playlists>, name: &QString) -> i64;

        #[qinvokable]
        fn name(self: &Playlists, id: i64, rev: i32) -> QString;

        #[qinvokable]
        fn description(self: &Playlists, id: i64, rev: i32) -> QString;

        #[qinvokable]
        fn source_name(self: &Playlists, id: i64, rev: i32) -> QString;

        #[qinvokable]
        fn editable(self: &Playlists, id: i64, rev: i32) -> bool;

        #[qinvokable]
        fn artwork(self: &Playlists, id: i64, rev: i32) -> QString;

        #[qinvokable]
        fn tags(self: &Playlists, id: i64, rev: i32) -> QString;
        #[qinvokable]
        fn folder_id(self: &Playlists, id: i64, rev: i32) -> i64;

        #[qinvokable]
        fn update(self: Pin<&mut Playlists>, id: i64, name: &QString, description: &QString, artwork: &QString, tags: &QString, folder_id: i64) -> bool;

        #[qinvokable]
        fn covers(self: &Playlists, id: i64, rev: i32) -> QStringList;

        #[qinvokable]
        fn names(self: &Playlists, rev: i32) -> QStringList;
        #[qinvokable]
        fn ids(self: &Playlists, rev: i32) -> QStringList;

        #[qinvokable] fn sources_json(self: &Playlists, track_ids_json: &QString) -> QString;
        #[qinvokable] fn write_source(self: Pin<&mut Playlists>, source: &QString, id: i64, name: &QString, description: &QString, track_ids_json: &QString, request: &QString) -> bool;
        #[qinvokable] fn write_state(self: Pin<&mut Playlists>, source: &QString) -> QString;
        #[qinvokable] fn order_folders(self: Pin<&mut Playlists>, ids: &QString);
        #[qinvokable] fn catalog_json(self: &Playlists, query: &QString) -> QString;
        #[qinvokable] fn manager_json(self: &Playlists, query: &QString, filter: &QString, order: &QString, ascending: bool, folder_id: i64) -> QString;
        #[qinvokable] fn picker_json(self: &Playlists, track_ids_json: &QString, query: &QString) -> QString;
        #[qinvokable] fn add_track_ids(self: Pin<&mut Playlists>, playlist_id: i64, track_ids_json: &QString) -> QString;
        #[qinvokable] fn create_full(self: Pin<&mut Playlists>, name: &QString, description: &QString, artwork: &QString, tags: &QString, folder_id: i64, track_ids_json: &QString) -> i64;
        #[qinvokable] fn choose_artwork(self: &Playlists) -> QString;
        #[qinvokable] fn create_folder(self: Pin<&mut Playlists>, name: &QString, color: &QString) -> i64;
        #[qinvokable] fn update_folder(self: Pin<&mut Playlists>, id: i64, name: &QString, color: &QString) -> bool;
        #[qinvokable] fn remove_folder(self: Pin<&mut Playlists>, id: i64);
        #[qinvokable] fn move_to_folder(self: Pin<&mut Playlists>, id: i64, folder_id: i64) -> bool;
        #[qinvokable] fn set_visible(self: Pin<&mut Playlists>, id: i64, visible: bool);
        #[qinvokable] fn set_custom_order(self: Pin<&mut Playlists>, ids_json: &QString);
    }

    impl cxx_qt::Threading for Playlists {}
    impl cxx_qt::Initialize for Playlists {}
}

use crate::format::strings;
use crate::{app, registry};
use cxx_qt::Threading as _;
use cxx_qt_lib::{QList, QString, QStringList, QUrl};
use std::pin::Pin;

#[derive(Default)]
pub struct PlaylistsRust {
    rev: i32,
}

fn paths_of(id: i64) -> Vec<String> {
    app::with(|api| api.playlist_rows(id)).into_iter().filter(|t| t.available).map(|t| t.path).collect()
}

impl qobject::Playlists {
    pub fn create(mut self: Pin<&mut Self>) -> i64 {
        let taken: Vec<String> = app::with(|api| api.local_playlists()).into_iter().map(|p| p.name).collect();
        let base = app::t("New playlist");
        let name = (1..).map(|n| if n == 1 { base.clone() } else { format!("{base} {n}") }).find(|candidate| !taken.contains(candidate)).unwrap_or(base);
        let Some(id) = app::with(|api| api.create_playlist(&name)) else {
            return 0;
        };
        self.as_mut().changed();
        registry::navigation(move |nav| nav.go(&QString::from("playlist"), id));
        id
    }

    pub fn refresh(mut self: Pin<&mut Self>) {
        app::with(|api| {
            api.refresh_playlist_sources();
            api.playlists();
        });
        self.as_mut().changed();
    }

    pub fn open(mut self: Pin<&mut Self>, id: i64) {
        if id < 0 {
            registry::navigation(move |nav| nav.go(&QString::from("album"), id));
            return;
        }
        let playlist = app::with(|api| api.playlist(id));
        if let Some(playlist) = playlist {
            if !playlist.source.is_empty() {
                app::with(|api| {
                    api.open_source_playlist(&playlist.source, &playlist.external_id);
                });
                self.as_mut().changed();
            }
            registry::navigation(move |nav| nav.go(&QString::from("playlist"), id));
        }
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
        if id < 0 { app::with(|api| api.album_action(&[id], "play", shuffled)); return; }
        let paths = paths_of(id);
        app::with(|api| {
            api.set_shuffle(shuffled);
            api.count_playlist_play(id);
            api.play_from(&paths, 0);
        });
    }

    pub fn queue(self: Pin<&mut Self>, id: i64) {
        if id < 0 { app::with(|api| api.album_action(&[id], "queue", false)); return; }
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

    pub fn description(&self, id: i64, _rev: i32) -> QString {
        QString::from(&app::with(|api| api.playlist(id).map(|playlist| playlist.description)).unwrap_or_default())
    }

    pub fn source_name(&self, id: i64, _rev: i32) -> QString {
        QString::from(&app::with(|api| api.playlist(id).map(|playlist| playlist.provider)).unwrap_or_default())
    }

    pub fn editable(&self, id: i64, _rev: i32) -> bool {
        app::with(|api| api.playlist(id).is_some_and(|playlist| playlist.mixed))
    }

    pub fn artwork(&self, id: i64, _rev: i32) -> QString {
        QString::from(&app::with(|api| api.playlist(id).map(|playlist| playlist.artwork_path)).unwrap_or_default())
    }

    pub fn tags(&self, id: i64, _rev: i32) -> QString {
        QString::from(&app::with(|api| api.playlist(id).map(|playlist| playlist.tags.join(", "))).unwrap_or_default())
    }

    pub fn folder_id(&self, id: i64, _rev: i32) -> i64 {
        app::with(|api| api.playlist(id).and_then(|playlist| playlist.folder_id)).unwrap_or_default()
    }

    pub fn update(mut self: Pin<&mut Self>, id: i64, name: &QString, description: &QString, artwork: &QString, tags: &QString, folder_id: i64) -> bool {
        let tags: Vec<String> = tags.to_string().split(',').map(str::trim).filter(|tag| !tag.is_empty()).map(str::to_string).collect();
        let artwork = artwork.to_string();
        let changed = app::with(|api| api.update_playlist(id, &name.to_string(), &description.to_string(), (!artwork.is_empty()).then_some(artwork.as_str()), &tags, (folder_id > 0).then_some(folder_id)));
        if changed {
            self.as_mut().changed();
            registry::navigation(|nav| nav.refresh_labels());
        }
        changed
    }

    pub fn covers(&self, id: i64, _rev: i32) -> QStringList {
        let mut list = QList::<QString>::default();
        if let Some(custom) = app::with(|api| api.playlist(id).map(|playlist| playlist.artwork_path)) {
            if !custom.is_empty() {
                if custom.contains("://") {
                    list.append(QString::from(&custom));
                } else {
                    list.append(QUrl::from_local_file(&QString::from(&custom)).to_qstring());
                }
            }
        }
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

    pub fn sources_json(&self, track_ids_json: &QString) -> QString {
        QString::from(&app::with(|api| api.playlist_sources_json(&ids_from_json(track_ids_json))))
    }
    pub fn write_source(self: Pin<&mut Self>, source: &QString, id: i64, name: &QString, description: &QString, track_ids_json: &QString, request: &QString) -> bool {
        app::with(|api| api.write_source_playlist(&source.to_string(), id, &name.to_string(), &description.to_string(), &ids_from_json(track_ids_json), &request.to_string()))
    }
    pub fn write_state(mut self: Pin<&mut Self>, source: &QString) -> QString {
        let state = app::with(|api| api.playlist_write_state(&source.to_string()));
        if !state.contains("\"pending\":true") { self.as_mut().changed(); }
        QString::from(&state)
    }
    pub fn order_folders(mut self: Pin<&mut Self>, ids: &QString) {
        if app::with(|api| api.order_playlist_folders(&ids_from_json(ids))) { self.as_mut().changed(); }
    }

    pub fn catalog_json(&self, query: &QString) -> QString {
        QString::from(&app::with(|api| api.sidebar_playlists(&query.to_string())))
    }

    pub fn manager_json(&self, query: &QString, filter: &QString, order: &QString, ascending: bool, folder_id: i64) -> QString {
        let rows = app::with(|api| api.managed_playlists(&query.to_string(), &filter.to_string(), &order.to_string(), ascending, (folder_id > 0).then_some(folder_id)));
        QString::from(&serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into()))
    }

    pub fn picker_json(&self, track_ids_json: &QString, query: &QString) -> QString {
        let track_ids = ids_from_json(track_ids_json);
        QString::from(&serde_json::to_string(&app::with(|api| api.playlist_targets(&track_ids, &query.to_string()))).unwrap_or_else(|_| "[]".into()))
    }

    pub fn add_track_ids(mut self: Pin<&mut Self>, playlist_id: i64, track_ids_json: &QString) -> QString {
        let track_ids = ids_from_json(track_ids_json);
        let result = app::with(|api| api.add_track_ids_to_playlist(playlist_id, &track_ids));
        if result.added > 0 { self.as_mut().changed(); }
        QString::from(&serde_json::to_string(&result).unwrap_or_else(|_| "{}".into()))
    }

    pub fn create_full(mut self: Pin<&mut Self>, name: &QString, description: &QString, artwork: &QString, tags: &QString, folder_id: i64, track_ids_json: &QString) -> i64 {
        let track_ids = ids_from_json(track_ids_json);
        let tags: Vec<String> = tags.to_string().split(',').map(str::trim).filter(|tag| !tag.is_empty()).map(str::to_string).collect();
        let artwork = artwork.to_string();
        let id = app::with(|api| api.create_playlist_with(&name.to_string(), &description.to_string(), (!artwork.is_empty()).then_some(artwork.as_str()), &tags, (folder_id > 0).then_some(folder_id))).unwrap_or(0);
        if id > 0 {
            let added = app::with(|api| api.add_track_ids_to_playlist(id, &track_ids));
            if added.rejected {
                app::with(|api| api.delete_playlist(id));
                return 0;
            }
            self.as_mut().changed();
        }
        id
    }

    pub fn choose_artwork(&self) -> QString {
        pollster::block_on(rfd::AsyncFileDialog::new().set_title(&app::t("Choose playlist artwork")).add_filter("Images", &["png", "jpg", "jpeg", "webp"]).pick_file()).map(|file| QString::from(&file.path().display().to_string())).unwrap_or_default()
    }

    pub fn create_folder(mut self: Pin<&mut Self>, name: &QString, color: &QString) -> i64 { let id = app::with(|api| api.create_playlist_folder(&name.to_string(), &color.to_string())).unwrap_or(0); if id > 0 { self.as_mut().changed(); } id }
    pub fn update_folder(mut self: Pin<&mut Self>, id: i64, name: &QString, color: &QString) -> bool {
        let changed = app::with(|api| api.update_playlist_folder(id, &name.to_string(), &color.to_string()));
        if changed { self.as_mut().changed(); }
        changed
    }
    pub fn remove_folder(mut self: Pin<&mut Self>, id: i64) { if app::with(|api| api.delete_playlist_folder(id)) { self.as_mut().changed(); } }
    pub fn move_to_folder(mut self: Pin<&mut Self>, id: i64, folder_id: i64) -> bool { let moved = app::with(|api| api.move_playlist_to_folder(id, (folder_id > 0).then_some(folder_id))); if moved { self.as_mut().changed(); } moved }
    pub fn set_visible(mut self: Pin<&mut Self>, id: i64, visible: bool) { if app::with(|api| api.set_playlist_visible(id, visible)) { self.as_mut().changed(); } }
    pub fn set_custom_order(mut self: Pin<&mut Self>, ids_json: &QString) { let ids = ids_from_json(ids_json); if app::with(|api| api.set_playlist_custom_order(&ids)) { self.as_mut().changed(); } }

    /// Something about the playlists changed: whoever shows them reads again.
    pub fn changed(mut self: Pin<&mut Self>) {
        let next = self.rev().wrapping_add(1);
        self.as_mut().set_rev(next);
    }
}

fn ids_from_json(value: &QString) -> Vec<i64> {
    serde_json::from_str(&value.to_string()).unwrap_or_default()
}

impl cxx_qt::Initialize for qobject::Playlists {
    fn initialize(self: Pin<&mut Self>) {
        registry::register_playlists(self.qt_thread());
        app::with(|api| {
            api.refresh_playlist_sources();
            api.playlists();
        });
    }
}
