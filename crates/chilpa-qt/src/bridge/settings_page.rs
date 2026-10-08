//! The rows of one Settings section, with their values (frontend ADR-0010).
//!
//! The rows and what each says now are the facade's
//! (`Chilpa::settings_rows`); this draws them and carries out a change. A change another object owns (the theme, the density, the
//! folders) is handed to it through the registry, and the preference is
//! written here too, so the page shows the new value at once.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        /// The section's id: "audio", "library"...
        #[qproperty(QString, section)]
        /// True when the section has nothing built yet.
        #[qproperty(bool, pending)]
        type SettingsPage = super::SettingsPageRust;

        /// Changes a setting to one of its option values ("true", "dark",
        /// an output's id).
        #[qinvokable]
        fn set(self: Pin<&mut SettingsPage>, key: &QString, value: &QString);

        /// Presses a setting's button or link.
        #[qinvokable]
        fn press(self: Pin<&mut SettingsPage>, key: &QString);

        /// Reads the rows again: a sync moving, a server coming back.
        #[qinvokable]
        fn refresh(self: Pin<&mut SettingsPage>);
        #[qinvokable]
        fn plugins_json(self: &SettingsPage) -> QString;
        #[qinvokable]
        fn plugin_command(self: &SettingsPage, action: &QString, input: &QString);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut SettingsPage>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut SettingsPage>);
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut SettingsPage>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QVector_i32);
        #[inherit]
        fn index(self: &SettingsPage, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &SettingsPage, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &SettingsPage) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &SettingsPage, parent: &QModelIndex) -> i32;
    }

    impl cxx_qt::Initialize for SettingsPage {}
    impl cxx_qt::Threading for SettingsPage {}
}

use crate::{app, registry};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const ROLES: &[&str] = &["rowKind", "key", "title", "description", "control", "action", "value", "options", "optionValues", "marks"];
const USER_ROLE: i32 = 256;

/// What a row shows, as the facade builds it.
type Row = chilpa_api::SettingsRow;

#[derive(Default)]
pub struct SettingsPageRust {
    section: QString,
    pending: bool,
    rows: Vec<Row>,
    /// The last diagnosis of the output, shown under its button.
    report: Option<chilpa_api::WizardReport>,
    /// What the last maintenance press said.
    said: Option<(String, String)>,
    /// The OAuth page currently offered for a plugin. It lives here only
    /// long enough for QML to open it and while the callback is pending.
    plugin_link: Option<(String, String)>,
}

impl SettingsPageRust {
    fn build(&mut self) {
        let notes = chilpa_api::RowNotes { report: self.report.as_ref(), said: self.said.as_ref(), plugin_link: self.plugin_link.as_ref() };
        let section = self.section.to_string();
        (self.rows, self.pending) = app::with(|api| api.settings_rows(&section, &notes));
    }
}

impl qobject::SettingsPage {
    pub fn plugins_json(&self) -> QString { QString::from(app::with(|api| api.plugin_center_json())) }
    pub fn plugin_command(&self, action: &QString, input: &QString) {
        let action = action.to_string();
        if !matches!(action.as_str(), "choose_icon" | "choose_package") {
            return app::with(|api| api.plugin_center_command(&action, &input.to_string()));
        }
        let package = action == "choose_package";
        let title = if package { app::t("Install plugin from ZIP") } else { app::t("Choose plugin icon") };
        let (thread, input) = (self.qt_thread(), input.to_string());
        std::thread::spawn(move || {
            let dialog = rfd::AsyncFileDialog::new().set_title(&title);
            let dialog = if package { dialog.add_filter("ZIP", &["zip"]) }
                else { dialog.add_filter("Images", &["svg", "png", "jpg", "jpeg"]) };
            let picked = pollster::block_on(dialog.pick_file());
            if let (Some(file), Ok(mut input)) = (picked, serde_json::from_str::<serde_json::Value>(&input)) {
                input["path"] = cxx_qt_lib::QUrl::from_local_file(&QString::from(file.path().display().to_string())).to_string().to_string().into();
                let action = if package { "install_local" } else { "icon" };
                let _ = thread.queue(move |_| app::with(|api| api.plugin_center_command(action, &input.to_string())));
            }
        });
    }
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(row) = self.rust().rows.get(index.row().max(0) as usize) else {
            return QVariant::default();
        };
        let text = |s: &str| QVariant::from(&QString::from(s));
        match role - USER_ROLE {
            0 => QVariant::from(&row.kind),
            1 => text(&row.key),
            2 => text(&row.title),
            3 => text(&row.description),
            4 => text(&row.control),
            5 => text(&row.action),
            6 => text(&row.value),
            7 => QVariant::from(&crate::format::strings(row.options.clone().into_iter())),
            8 => QVariant::from(&crate::format::strings(row.values.clone().into_iter())),
            9 => QVariant::from(&crate::format::strings(row.marks.clone().into_iter())),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        crate::format::roles(USER_ROLE, ROLES)
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().rows.len() as i32
    }

    pub fn set(mut self: Pin<&mut Self>, key: &QString, value: &QString) {
        let (key, value) = (key.to_string(), value.to_string());
        // A server's "Remove…" is confirmed by the window first (ADR-017).
        if let (Some(id), "sync") = (key.strip_prefix("sources.server/").and_then(|id| id.parse().ok()), value.as_str()) {
            app::with(|api| api.sync_server(id));
            self.reload();
            return;
        }
        if let Some(id) = key.strip_prefix("sources.token/") {
            if !app::with(|api| api.connect_plugin(id, &value)) {
                let said = app::with(|api| api.now_playing().error.clone());
                self.as_mut().rust_mut().said = Some((format!("sources.plugin/{id}"), said));
            }
            self.reload();
            return;
        }
        if let Some(id) = key.strip_prefix("sources.plugin/") {
            match value.as_str() {
                "approve" => {
                    app::with(|api| api.approve_plugin(id));
                }
                "connect" => match app::with(|api| api.connect_account(id)) {
                    Ok(url) => {
                        self.as_mut().rust_mut().plugin_link = Some((id.to_string(), url));
                        self.as_mut().reload();
                        self.watch_plugin_connection();
                        return;
                    }
                    Err(error) => self.as_mut().rust_mut().said = Some((key.clone(), error)),
                },
                "disconnect" => app::with(|api| api.disconnect_plugin(id)),
                "reactivate" => app::with(|api| api.reactivate_plugin(id)),
                "revoke" => {
                    app::with(|api| api.revoke_plugin(id));
                }
                "uninstall" => {
                    if !app::with(|api| api.uninstall_plugin(id)) {
                        let said = app::with(|api| api.now_playing().error.clone());
                        self.as_mut().rust_mut().said = Some((key.clone(), said));
                    }
                }
                _ => return,
            }
            self.reload();
            return;
        }
        match key.as_str() {
            "audio.backend" => {
                // A stack is chosen through its first output: an output is what
                // gets remembered, and it already names its stack.
                app::with(|api| {
                    if let Some(first) = api.stack_outputs(&value).into_iter().next() {
                        api.use_output_as_chosen(&first.id);
                    }
                });
            }
            "audio.device" => app::with(|api| api.use_output_as_chosen(&value)),
            "audio.alsaPlugin" => app::with(|api| api.set_alsa_plugin(&value)),
            "audio.dsdMode" => app::with(|api| api.set_dsd_mode(&value)),
            "audio.exclusive" => app::with(|api| api.set_exclusive(value == "true")),
            "audio.reserve" => app::with(|api| api.set_reserving(value == "true")),
            "advanced.hwVolume" => {
                app::with(|api| api.set_hardware_volume(value == "true"));
                registry::player(|p| p.read_switches());
            }
            "audio.forceRate" => app::with(|api| api.set_forcing_rate(value == "true")),
            "appearance.theme" => {
                app::with(|api| api.remember_preference(&key, &value));
                chilpa_qt_kit::bridge::theme::ask(move |theme| theme.choose_mode(QString::from(&value)));
            }
            "appearance.density" => {
                app::with(|api| api.remember_preference(&key, &value));
                registry::settings(move |s| s.set_density(QString::from(&value)));
            }
            "appearance.font" | "library.metaSrc" => app::with(|api| api.remember_preference(&key, &value)),
            "appearance.albumHeader" => {
                app::with(|api| api.remember_preference(&key, &value));
                registry::settings(move |s| s.set_album_header(QString::from(&value)));
            }
            "appearance.accent" => chilpa_qt_kit::bridge::theme::ask(move |theme| theme.choose_accent(&QString::from(&value))),
            "playback.gapless" => {
                app::with(|api| api.set_gapless(value == "true"));
                registry::player(|p| p.read_switches());
            }
            "playback.normalize" => {
                app::with(|api| api.set_normalize(value == "true"));
                registry::player(|p| p.read_switches());
            }
            "playback.keepQueue" => app::with(|api| api.set_keep_queue(value == "true")),
            "playback.extra" => {
                app::with(|api| api.remember_preference(&key, &value));
                registry::settings(move |s| s.set_extra_controls(value == "true"));
            }
            "playback.autoHide" => {
                app::with(|api| api.remember_preference(&key, &value));
                registry::settings(move |s| s.set_hide_player(value == "true"));
            }
            "navigation.newtab" => {
                app::with(|api| api.remember_preference(&key, &value));
                registry::settings(move |s| s.set_middle_click_tabs(value == "true"));
            }
            "navigation.restoreTabs" => app::with(|api| api.remember_preference(&key, &value)),
            "navigation.albumClick" => {
                app::with(|api| api.remember_preference(&key, &value));
                registry::settings(move |s| s.set_quick_view_click(value == "quick"));
            }
            "navigation.playlistArtwork" => {
                app::with(|api| api.remember_preference(&key, &value));
                registry::settings(move |s| s.set_playlist_collage(value == "collage"));
            }
            "navigation.folderColors" => {
                app::with(|api| api.remember_preference(&key, &value));
                registry::settings(move |s| s.set_folder_colors(value == "true"));
            }
            "integrations.listenbrainz.enabled" => app::with(|api| api.enable_listenbrainz(value == "true")),
            "integrations.lastfm.enabled" => app::with(|api| api.enable_lastfm(value == "true")),
            "integrations.listenbrainz.account" => {
                app::with(|api| api.connect_listenbrainz(&value));
                self.as_mut().reload();
                self.watch_accounts();
                return;
            }
            // A key the bridge does not know is its feature's: the facade
            // asks around (frontend ADR-0015), and whoever shows the feature
            // is told that something was kept, without the key.
            _ => {
                if app::with(|api| api.adjust(&key, &value)) {
                    chilpa_qt_kit::bridge::prefs::changed();
                    self.as_mut().reload();
                }
                return;
            }
        }
        // Another output or policy can have a volume of its own or not.
        if key.starts_with("audio.") {
            registry::player(|p| p.read_switches());
        }
        self.reload();
    }

    /// Reads the page again as the accounts being connected move -- the
    /// page to allow one appears, the answer comes -- and stops when neither
    /// is being connected. Five minutes at most: as long as Last.fm is
    /// waited for.
    fn watch_accounts(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        let done = Arc::new(AtomicBool::new(false));
        let seen = Arc::new(Mutex::new(None));
        std::thread::spawn(move || {
            for _ in 0..1_200 {
                std::thread::sleep(Duration::from_millis(250));
                if done.load(Ordering::Relaxed) {
                    return;
                }
                let (done, seen) = (Arc::clone(&done), Arc::clone(&seen));
                let queued = thread.queue(move |page| {
                    let now = app::with(|api| (api.listenbrainz(), api.lastfm()));
                    let changed = seen.lock().map(|mut last| last.replace(now.clone()) != Some(now.clone())).unwrap_or(true);
                    if changed {
                        page.reload();
                    }
                    if !now.0.checking && !now.1.checking {
                        done.store(true, Ordering::Relaxed);
                    }
                });
                if queued.is_err() {
                    return;
                }
            }
        });
    }
    pub fn press(mut self: Pin<&mut Self>, key: &QString) {
        match key.to_string().as_str() {
            "audio.wizard" => {
                let report = app::with(|api| {
                    let (output, _) = api.chosen_output();
                    api.wizard(&output)
                });
                self.as_mut().rust_mut().report = Some(report);
            }
            "library.folders" => {
                registry::navigation(|nav| nav.go(&QString::from("libraryFolders"), 0));
                return;
            }
            // Pressed only once connected, where it is the way out.
            "integrations.listenbrainz.account" => {
                app::with(|api| api.disconnect_listenbrainz());
                // The scrobbler forgets on its thread; the page follows.
                self.as_mut().watch_accounts();
            }
            // Connect, or -- connected -- disconnect.
            "integrations.lastfm.account" => {
                if app::with(|api| api.lastfm().user.is_empty()) {
                    app::with(|api| api.connect_lastfm());
                } else {
                    app::with(|api| api.disconnect_lastfm());
                }
                self.as_mut().watch_accounts();
            }
            "library.rescan" => {
                chilpa_qt_folders::bridge::folders::ask(|folders| folders.rescan_all());
                return;
            }
            "library.analysis" => {
                let running = app::with(|api| api.analysis_status().running);
                if running {
                    app::with(|api| api.cancel_analysis());
                } else {
                    app::with(|api| api.start_analysis());
                    self.as_mut().watch_analysis();
                }
                self.as_mut().reload();
                return;
            }
            // Any other button is the facade's; a new one needs nothing here.
            pressed => {
                let Some(said) = app::with(|api| api.maintain(pressed)) else { return };
                self.as_mut().rust_mut().said = Some((pressed.into(), said));
            }
        }
        self.reload();
    }

    /// Follows the analysis without blocking Settings. The worker owns the
    /// files; this only drains its progress four times a second and refreshes
    /// the player switches when AutoMix may have become available.
    fn watch_analysis(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        let done = Arc::new(AtomicBool::new(false));
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(250));
            let finished = Arc::clone(&done);
            let queued = thread.queue(move |page| {
                let running = app::with(|api| api.analysis_status().running);
                page.reload();
                if !running {
                    registry::player(|player| player.read_switches());
                    finished.store(true, Ordering::Relaxed);
                }
            });
            if queued.is_err() {
                break;
            }
            if done.load(Ordering::Relaxed) {
                break;
            }
        });
    }

    /// Waits for the OAuth loopback without keeping the Settings page or the
    /// GUI thread busy. The regular player tick also advances it, but doing
    /// so here makes account setup independent of whether a player exists.
    fn watch_plugin_connection(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        let done = Arc::new(AtomicBool::new(false));
        std::thread::spawn(move || {
            for _ in 0..1_200 {
                std::thread::sleep(Duration::from_millis(250));
                if done.load(Ordering::Relaxed) {
                    return;
                }
                let finished = Arc::clone(&done);
                let queued = thread.queue(move |mut page| {
                    let connecting = app::with(|api| {
                        api.poll();
                        api.connecting()
                    });
                    if !connecting {
                        page.as_mut().rust_mut().plugin_link = None;
                        page.reload();
                        finished.store(true, Ordering::Relaxed);
                    }
                });
                if queued.is_err() {
                    return;
                }
            }
        });
    }

    pub fn refresh(self: Pin<&mut Self>) {
        self.reload();
    }

    /// Reads the rows again. The same number of rows is the same page with
    /// new values: they change in place, and the control that caused it --
    /// an open menu, a toggle mid-animation -- is not destroyed under the
    /// pointer. A different number is a different page, and resets.
    fn reload(mut self: Pin<&mut Self>) {
        let before = self.rust().rows.len();
        let mut next = SettingsPageRust { section: self.rust().section.clone(), ..SettingsPageRust::default() };
        next.report = self.as_mut().rust_mut().report.take();
        next.said = self.as_mut().rust_mut().said.take();
        next.plugin_link = self.as_mut().rust_mut().plugin_link.take();
        next.build();
        if next.rows.len() == before && before > 0 {
            {
                let mut this = self.as_mut().rust_mut();
                this.rows = std::mem::take(&mut next.rows);
                this.pending = next.pending;
                this.report = next.report.take();
                this.said = next.said.take();
                this.plugin_link = next.plugin_link.take();
            }
            let parent = QModelIndex::default();
            let (top, bottom) = (self.index(0, 0, &parent), self.index(before as i32 - 1, 0, &parent));
            self.as_mut().data_changed(&top, &bottom, &QVector::<i32>::default());
        } else {
            // SAFETY: the reset is begun and ended around the swap, as Qt asks.
            unsafe { self.as_mut().begin_reset_model() };
            {
                let mut this = self.as_mut().rust_mut();
                this.rows = std::mem::take(&mut next.rows);
                this.pending = next.pending;
                this.report = next.report.take();
                this.said = next.said.take();
                this.plugin_link = next.plugin_link.take();
            }
            unsafe { self.as_mut().end_reset_model() };
        }
        let pending = self.rust().pending;
        self.as_mut().set_pending(pending);
    }
}

impl cxx_qt::Initialize for qobject::SettingsPage {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_section_changed(|m| m.reload()).release();
    }
}
