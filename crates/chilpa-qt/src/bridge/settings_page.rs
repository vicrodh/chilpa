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
        fn data_changed(
            self: Pin<&mut SettingsPage>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );
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

use crate::settings_schema;
use crate::{app, registry};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QList, QModelIndex, QString, QStringList, QVariant, QVector};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const ROLES: &[&str] = &[
    "rowKind", "key", "title", "description", "control", "action", "value", "options", "optionValues",
    "marks",
];
const USER_ROLE: i32 = 256;

/// What a row shows: a group title (0), a setting (1) or a line of a
/// report under one (2).
#[derive(Default)]
struct Row {
    kind: i32,
    key: &'static str,
    title: String,
    description: String,
    control: &'static str,
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
    said: Option<(&'static str, String)>,
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
        _ => Vec::new(),
    }
}

/// A toggle's value from its preference, "true" or "false".
fn pref_on(key: &str, default: bool) -> String {
    app::with(|api| api.preference(key)).map_or(default, |v| v == "true").to_string()
}

/// The accents offered (design/05): the theme's own, then four.
const ACCENTS: &[&str] = &["", "#A3161F", "#D8323A", "#3DAEE9", "#5C6166"];

const DEFAULTS: &[(&str, &str)] = &[
    ("appearance.theme", "system"),
    ("appearance.density", "compact"),
    ("appearance.font", "inter"),
    ("navigation.albumClick", "page"),
];

impl SettingsPageRust {
    fn build(&mut self) {
        let section = settings_schema::section(&self.section.to_string());
        let mut rows = Vec::new();
        let (output, bit_perfect) = app::with(|api| api.chosen_output());
        for group in section.groups {
            rows.push(Row { kind: 0, title: app::t(group.title), ..Row::default() });
            for s in group.settings {
                let mut row = Row {
                    kind: 1,
                    key: s.key,
                    title: app::t(s.title),
                    description: if s.description.is_empty() { String::new() } else { app::t(s.description) },
                    control: s.control.name(),
                    action: if s.action.is_empty() { String::new() } else { app::t(s.action) },
                    ..Row::default()
                };
                self.fill(&mut row, &output, bit_perfect);
                // A row that does not apply to what is chosen is left out.
                if row.control.is_empty() {
                    continue;
                }
                rows.push(row);
                if s.key == "audio.wizard" {
                    for line in self.report.iter().flat_map(|r| &r.rows) {
                        rows.push(Row {
                            kind: 2,
                            title: line.name.clone(),
                            description: line.format.clone(),
                            value: line.verdict.clone(),
                            ..Row::default()
                        });
                    }
                }
            }
        }
        self.pending = section.groups.is_empty();
        self.rows = rows;
    }

    /// The value, the options and anything the row says about its state.
    fn fill(&self, row: &mut Row, output: &str, bit_perfect: bool) {
        match row.key {
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
            "audio.bitperfect" => row.value = bit_perfect.to_string(),
            "audio.alsaPlugin" => {
                row.value = app::with(|api| api.alsa_plugin()).to_string();
                // Only a card has a plugin to choose.
                if row.value.is_empty() {
                    row.control = "";
                }
                row.values = vec!["hw".to_string(), "plughw".to_string()];
                row.options = vec![app::t("hw: the card alone"), app::t("plughw: converts what it refuses")];
            }
            "audio.wizard" => {
                if let Some(report) = &self.report {
                    row.description = report.summary.clone();
                }
            }
            "listenbrainz.account" => {
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
            "navigation.newtab" => row.value = pref_on("navigation.newtab", true),
            "navigation.restoreTabs" => row.value = pref_on("navigation.restoreTabs", true),
            "listenbrainz.enabled" => row.value = app::with(|api| api.listenbrainz().enabled).to_string(),
            "lastfm.account" => {
                let status = app::with(|api| api.lastfm());
                if status.checking && !status.link.is_empty() {
                    // Waiting for the person to allow it on the site: the page
                    // opens, and can be opened again from here. "external"
                    // is no control of the schema's: only this state has it.
                    row.description = app::t("Allow Chilpa on Last.fm; this updates by itself.");
                    row.control = "external";
                    row.action = app::t("Open Last.fm");
                    row.value = status.link.clone();
                } else {
                    account(row, &status);
                }
            }
            "lastfm.enabled" => row.value = app::with(|api| api.lastfm().enabled).to_string(),
            "library.folders" => {
                let n = app::with(|api| api.library_folders().len());
                let shown = n.to_string();
                row.description = app::with(|api| api.tf("{} folder", "{} folders", n as i64, &[&shown]));
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
            if *key == row.key {
                row.description = said.clone();
            }
        }
    }
}

/// An account's row: checking it, or who it is with a way out and how many
/// listens wait, or what went wrong.
fn account(row: &mut Row, status: &chilpa_api::ScrobbleStatus) {
    if status.checking {
        row.description = app::t("Checking the account…");
    } else if !status.user.is_empty() {
        row.title = app::with(|api| api.t_args("Connected as {}", &[&status.user]));
        row.control = "button";
        row.action = app::t("Disconnect");
        row.description = if status.pending > 0 {
            let n = status.pending.to_string();
            app::with(|api| {
                api.tf("{} listen waiting to be sent", "{} listens waiting to be sent", status.pending as i64, &[&n])
            })
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
        let Some(row) = self.rust().rows.get(index.row().max(0) as usize) else { return QVariant::default() };
        let text = |s: &str| QVariant::from(&QString::from(s));
        match role - USER_ROLE {
            0 => QVariant::from(&row.kind),
            1 => text(row.key),
            2 => text(&row.title),
            3 => text(&row.description),
            4 => text(row.control),
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
            "audio.alsaPlugin" => app::with(|api| api.set_alsa_plugin(value == "plughw")),
            "audio.bitperfect" => {
                app::with(|api| api.set_bit_perfect(value == "true"));
                // The player bar's options show it too.
                registry::player(|p| p.read_switches());
            }
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
            "listenbrainz.enabled" => app::with(|api| api.enable_listenbrainz(value == "true")),
            "lastfm.enabled" => app::with(|api| api.enable_lastfm(value == "true")),
            "listenbrainz.account" => {
                app::with(|api| api.connect_listenbrainz(&value));
                self.as_mut().reload();
                self.watch_accounts();
                return;
            }
            _ => return,
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
            "listenbrainz.account" => {
                app::with(|api| api.disconnect_listenbrainz());
                // The scrobbler forgets on its thread; the page follows.
                self.as_mut().watch_accounts();
            }
            // Connect, or -- connected -- disconnect.
            "lastfm.account" => {
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
                let said = if app::with(|api| api.repair_library()) {
                    app::t("Repaired. Nothing was lost.")
                } else {
                    app::with(|api| api.now_playing().error.clone())
                };
                self.as_mut().rust_mut().said = Some(("library.repair", said));
            }
            _ => return,
        }
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
        next.build();
        if next.rows.len() == before && before > 0 {
            {
                let mut this = self.as_mut().rust_mut();
                this.rows = std::mem::take(&mut next.rows);
                this.pending = next.pending;
                this.report = next.report.take();
                this.said = next.said.take();
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
