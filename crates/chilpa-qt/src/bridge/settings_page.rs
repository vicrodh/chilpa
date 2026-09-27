//! The rows of one Settings section, with their values (frontend ADR-0010).
//!
//! The rows come from `settings_schema`; this fills in what each one says
//! now -- the stacks and outputs there are, the chosen theme -- and carries
//! out a change. A change another object owns (the theme, the density, the
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
use chilpa_api::settings_schema;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QList, QModelIndex, QString, QStringList, QVariant, QVector};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const ROLES: &[&str] = &["rowKind", "key", "title", "description", "control", "action", "value", "options", "optionValues", "marks"];
const USER_ROLE: i32 = 256;

/// What a row shows: a group title (0), a setting (1) or a line of a
/// report under one (2).
#[derive(Default)]
struct Row {
    kind: i32,
    key: String,
    title: String,
    description: String,
    control: String,
    action: String,
    value: String,
    options: Vec<String>,
    values: Vec<String>,
    marks: Vec<String>,
}

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

fn list(items: &[String]) -> QVariant {
    let mut l = QList::<QString>::default();
    for i in items {
        l.append(QString::from(i));
    }
    QVariant::from(&QStringList::from(&l))
}

/// The options of a segmented or select setting, as (value, label).
fn fixed(key: &str) -> Vec<(&'static str, String)> {
    // Each label is spelled out as an `app::t` call so the extractor finds it.
    match key {
        "appearance.theme" => {
            vec![("light", app::t("Light")), ("dark", app::t("Dark")), ("system", app::t("System"))]
        }
        "appearance.density" => vec![("compact", app::t("Compact")), ("rich", app::t("Rich"))],
        "appearance.font" => vec![("inter", "Inter".to_string()), ("system", app::t("System"))],
        "navigation.albumClick" => vec![("page", app::t("Album page")), ("quick", app::t("Quick view"))],
        "navigation.playlistArtwork" => {
            vec![("icon", app::t("Playlist icon")), ("collage", app::t("Cover collage"))]
        }
        _ => Vec::new(),
    }
}

/// A toggle's value from its preference, "true" or "false".
fn pref_on(key: &str, default: bool) -> String {
    app::with(|api| api.preference(key)).map_or(default, |v| v == "true").to_string()
}

/// The accents offered (design/05): the theme's own, then four.
const ACCENTS: &[&str] = &["", "#A3161F", "#D8323A", "#3DAEE9", "#5C6166"];

const DEFAULTS: &[(&str, &str)] = &[("appearance.theme", "system"), ("appearance.density", "compact"), ("appearance.font", "inter"), ("navigation.albumClick", "page"), ("navigation.playlistArtwork", "icon")];

impl SettingsPageRust {
    fn build(&mut self) {
        let section = settings_schema::section(&self.section.to_string());
        let mut rows = Vec::new();
        let (output, _) = app::with(|api| api.chosen_output());
        for group in section.groups {
            rows.push(Row { kind: 0, title: app::t(group.title), ..Row::default() });
            for s in group.settings {
                if s.key == "sources.plugins" {
                    let plugins = app::with(|api| api.plugins());
                    if plugins.is_empty() {
                        rows.push(Row { kind: 1, key: s.key.into(), title: app::t(s.title), description: app::t(s.description), control: "none".into(), ..Row::default() });
                    } else {
                        for plugin in plugins {
                            rows.extend(self.source_rows(plugin));
                        }
                    }
                    continue;
                }
                let mut row = Row { kind: 1, key: s.key.into(), title: app::t(s.title), description: if s.description.is_empty() { String::new() } else { app::t(s.description) }, control: s.control.name().into(), action: if s.action.is_empty() { String::new() } else { app::t(s.action) }, ..Row::default() };
                self.fill(&mut row, &output);
                // A row that does not apply to what is chosen is left out.
                if row.control.is_empty() {
                    continue;
                }
                rows.push(row);
                if s.key == "audio.wizard" {
                    for line in self.report.iter().flat_map(|r| &r.rows) {
                        rows.push(Row { kind: 2, title: line.name.clone(), description: line.format.clone(), value: line.verdict.clone(), ..Row::default() });
                    }
                }
            }
        }
        self.pending = section.groups.is_empty();
        self.rows = rows;
    }

    /// The value, the options and anything the row says about its state.
    fn fill(&self, row: &mut Row, output: &str) {
        match row.key.as_str() {
            "audio.backend" => {
                let (stacks, current) = app::with(|api| (api.stacks(), api.stack_of(output)));
                for s in stacks.iter().filter(|s| s.present) {
                    row.values.push(s.key.clone());
                    row.options.push(s.label.clone());
                }
                if let Some(s) = stacks.iter().find(|s| s.key == current && !s.caveat.is_empty()) {
                    row.description = s.caveat.clone();
                }
                row.value = current;
            }
            "audio.device" => {
                let outputs = app::with(|api| api.outputs_beside(output));
                for o in outputs {
                    row.values.push(o.id);
                    row.options.push(o.label);
                    row.marks.push(if o.bit_perfect { "BP".to_string() } else { String::new() });
                }
                row.value = output.to_string();
            }
            "audio.exclusive" => {
                // Only a stack that shares can be asked not to; a card is
                // always alone.
                let (fidelity, chosen) = app::with(|api| (api.fidelity_of(output), api.exclusive()));
                let fixed = match fidelity {
                    "exact" => Some((true, app::t("This output is always Chilpa's alone."))),
                    "depends" => None,
                    _ => Some((false, app::t("Not through this stack: it mixes by design."))),
                };
                row.value = fixed.as_ref().map_or(chosen, |(on, _)| *on).to_string();
                if let Some((_, why)) = fixed {
                    row.control = "fixed".into();
                    row.description = format!("{} {why}", row.description);
                }
            }
            "audio.forceRate" => {
                let (shown, on, alone) = app::with(|api| (api.stack_of(output) == "pipewire", api.forcing_rate(), api.exclusive()));
                row.value = (on && alone).to_string();
                row.control = if !shown {
                    String::new()
                } else if alone {
                    row.control.clone()
                } else {
                    "fixed".into()
                };
                if shown && !alone {
                    row.description = format!("{} {}", row.description, app::t("Only with exclusive mode on."));
                }
            }
            "advanced.hwVolume" => row.value = app::with(|api| api.hardware_volume()).to_string(),
            "audio.reserve" => {
                let (card, on) = app::with(|api| (!api.alsa_plugin().is_empty(), api.reserving()));
                row.value = on.to_string();
                row.control = if card { row.control.clone() } else { String::new() };
            }
            "audio.alsaPlugin" => {
                let (plugin, plugins) = app::with(|api| (api.alsa_plugin(), api.alsa_plugins()));
                row.value = plugin.to_string();
                // Only a card has a plugin to choose.
                if row.value.is_empty() {
                    row.control.clear();
                }
                for p in plugins {
                    row.values.push(p.to_string());
                    row.options.push(match *p {
                        "hw" => app::t("hw: the card alone"),
                        "plughw" => app::t("plughw: converts what it refuses"),
                        _ => app::t("pcm: shared, as ALSA sets it up"),
                    });
                }
            }
            "audio.wizard" => {
                if let Some(report) = &self.report {
                    row.description = report.summary.clone();
                }
            }
            "integrations.listenbrainz.account" => {
                let status = app::with(|api| api.listenbrainz());
                account(row, &status);
            }
            "appearance.accent" => {
                // The theme's own first -- drawn by the window, which knows
                // which theme is on -- then the design's four.
                row.values = ACCENTS.iter().map(|a| a.to_string()).collect();
                row.options = row.values.clone();
                row.value = app::with(|api| api.preference("appearance.accent")).unwrap_or_default();
            }
            "playback.extra" => row.value = pref_on("playback.extra", false),
            "playback.gapless" => row.value = app::with(|api| api.gapless()).to_string(),
            "playback.normalize" => row.value = app::with(|api| api.normalize()).to_string(),
            "playback.keepQueue" => row.value = app::with(|api| api.keep_queue()).to_string(),
            "navigation.newtab" => row.value = pref_on("navigation.newtab", true),
            "navigation.restoreTabs" => row.value = pref_on("navigation.restoreTabs", true),
            "navigation.folderColors" => row.value = pref_on("navigation.folderColors", false),
            "integrations.listenbrainz.enabled" => row.value = app::with(|api| api.listenbrainz().enabled).to_string(),
            "integrations.lastfm.account" => {
                let status = app::with(|api| api.lastfm());
                if status.checking && !status.link.is_empty() {
                    // Waiting for the person to allow it on the site: the page
                    // opens, and can be opened again from here. "external"
                    // is no control of the schema's: only this state has it.
                    row.description = app::t("Allow Chilpa on Last.fm; this updates by itself.");
                    row.control = "external".into();
                    row.action = app::t("Open Last.fm");
                    row.value = status.link.clone();
                } else {
                    account(row, &status);
                }
            }
            "integrations.lastfm.enabled" => row.value = app::with(|api| api.lastfm().enabled).to_string(),
            "library.folders" => {
                let n = app::with(|api| api.library_folders().len());
                let shown = n.to_string();
                row.description = app::with(|api| api.tf("{} folder", "{} folders", n as i64, &[&shown]));
            }
            "library.analysis" => {
                let status = app::with(|api| api.analysis_status().clone());
                row.action = if status.running { app::t("Cancel") } else { app::t("Analyze") };
                if status.running {
                    if status.total == 0 {
                        row.description = app::t("Counting tracks…");
                    } else {
                        let (done, total) = (status.done.to_string(), status.total.to_string());
                        let progress = app::with(|api| api.t_args("Analyzing {} of {} tracks", &[&done, &total]));
                        row.description = if status.last.is_empty() { progress } else { format!("{progress} · {}", status.last) };
                        row.value = (status.done as f64 / status.total as f64).to_string();
                    }
                } else if status.analyzed + status.without_pulse + status.failed > 0 {
                    let (analyzed, quiet, failed) = (status.analyzed.to_string(), status.without_pulse.to_string(), status.failed.to_string());
                    row.description = app::with(|api| api.t_args("{} analyzed, {} without pulse, {} unreadable", &[&analyzed, &quiet, &failed]));
                }
            }
            key => {
                for (value, label) in fixed(key) {
                    row.values.push(value.to_string());
                    row.options.push(label);
                }
                if !row.values.is_empty() {
                    let default = DEFAULTS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).unwrap_or("");
                    row.value = app::with(|api| api.preference(key)).unwrap_or_else(|| default.to_string());
                }
            }
        }
        if let Some((key, said)) = &self.said {
            if key == &row.key {
                row.description = said.clone();
            }
        }
    }
    /// One status row, plus the token field when that is how the plugin
    /// connects. The available actions are derived from the state so the
    /// menu cannot offer an operation that does not make sense now.
    fn source_rows(&self, plugin: chilpa_api::PluginInfo) -> Vec<Row> {
        let key = format!("sources.plugin/{}", plugin.id);
        let mut details = Vec::new();
        if !plugin.problem.is_empty() {
            details.push(plugin.problem.clone());
        }
        if !plugin.pending.is_empty() {
            let asks = plugin.pending.join(", ");
            details.push(app::with(|api| api.t_args("Requests: {}", &[&asks])));
            if plugin.wants_more {
                details.push(app::t("This update asks for additional access."));
            }
        } else if plugin.connected {
            details.push(app::t("Account connected."));
        } else if plugin.account_expired {
            details.push(app::t("The account expired; connect it again."));
        } else if !plugin.account_kind.is_empty() {
            details.push(app::t("Account not connected."));
        }
        let mut row = Row { kind: 1, key, title: plugin.name.clone(), description: details.join(" "), control: "status".into(), action: app::t(&plugin.state), ..Row::default() };
        if let Some((said_key, said)) = &self.said {
            if said_key == &row.key {
                row.description = said.clone();
            }
        }
        if !plugin.pending.is_empty() {
            row.values.push("approve".into());
            row.options.push(if plugin.wants_more { app::t("Approve new access") } else { app::t("Approve") });
        } else {
            if plugin.state == "quarantined" {
                row.values.push("reactivate".into());
                row.options.push(app::t("Reactivate"));
            }
            if plugin.connected {
                row.values.push("disconnect".into());
                row.options.push(app::t("Disconnect account"));
            } else if plugin.account_kind == "oauth" {
                row.values.push("connect".into());
                let host = if plugin.account_host.is_empty() { app::t("the source") } else { plugin.account_host.clone() };
                row.options.push(app::with(|api| api.t_args("Connect on {}", &[&host])));
            }
            row.values.push("revoke".into());
            row.options.push(app::t("Withdraw approval"));
        }
        row.values.push("uninstall".into());
        row.options.push(app::t("Uninstall"));
        if let Some((id, url)) = &self.plugin_link {
            if id == &plugin.id {
                row.value = url.clone();
            }
        }
        let mut rows = vec![row];
        if plugin.pending.is_empty() && plugin.account_kind == "token" && !plugin.connected {
            rows.push(Row { kind: 1, key: format!("sources.token/{}", plugin.id), title: app::with(|api| api.t_args("Connect {}", &[&plugin.name])), description: app::t("Paste the token supplied by the source. Chilpa keeps it encrypted and the plugin never sees it."), control: "secret".into(), action: app::t("Connect"), ..Row::default() });
        }
        rows
    }
}

/// An account's row: checking it, or who it is with a way out and how many
/// listens wait, or what went wrong.
fn account(row: &mut Row, status: &chilpa_api::ScrobbleStatus) {
    if status.checking {
        row.description = app::t("Checking the account…");
    } else if !status.user.is_empty() {
        row.title = app::with(|api| api.t_args("Connected as {}", &[&status.user]));
        row.control = "button".into();
        row.action = app::t("Disconnect");
        row.description = if status.pending > 0 {
            let n = status.pending.to_string();
            app::with(|api| api.tf("{} listen waiting to be sent", "{} listens waiting to be sent", status.pending as i64, &[&n]))
        } else {
            String::new()
        };
    }
    if !status.error.is_empty() {
        row.description = status.error.clone();
    }
}

impl qobject::SettingsPage {
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
            7 => list(&row.options),
            8 => list(&row.values),
            9 => list(&row.marks),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (i, name) in ROLES.iter().enumerate() {
            roles.insert(USER_ROLE + i as i32, QByteArray::from(*name));
        }
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().rows.len() as i32
    }

    pub fn set(mut self: Pin<&mut Self>, key: &QString, value: &QString) {
        let (key, value) = (key.to_string(), value.to_string());
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
            "audio.exclusive" => app::with(|api| api.set_exclusive(value == "true")),
            "audio.reserve" => app::with(|api| api.set_reserving(value == "true")),
            "advanced.hwVolume" => {
                app::with(|api| api.set_hardware_volume(value == "true"));
                registry::player(|p| p.read_switches());
            }
            "audio.forceRate" => app::with(|api| api.set_forcing_rate(value == "true")),
            "appearance.theme" => {
                app::with(|api| api.remember_preference(&key, &value));
                registry::theme(move |theme| theme.choose_mode(QString::from(&value)));
            }
            "appearance.density" => {
                app::with(|api| api.remember_preference(&key, &value));
                registry::settings(move |s| s.set_density(QString::from(&value)));
            }
            "appearance.font" => app::with(|api| api.remember_preference(&key, &value)),
            "appearance.accent" => registry::theme(move |theme| theme.choose_accent(&QString::from(&value))),
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
            _ => return,
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
                registry::folders(|folders| folders.rescan_all());
                return;
            }
            "library.repair" => {
                let said = if app::with(|api| api.repair_library()) { app::t("Repaired. Nothing was lost.") } else { app::with(|api| api.now_playing().error.clone()) };
                self.as_mut().rust_mut().said = Some(("library.repair".into(), said));
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
            _ => return,
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
