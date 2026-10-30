//! The rows of one Settings section, with what each says now (frontend
//! ADR-0010).
//!
//! The rows come from `settings_schema`; this fills in what each one says
//! -- the stacks and outputs there are, the chosen theme, the state of each
//! plugin and server -- in plain data. It lived in the shell's bridge, and
//! it is logic: moved here so the shell only draws it (ADR-001, and the
//! room the servers of ADR-017 needed in the shell's budget).

use crate::{settings_schema, Chilpa};

fn cache_size(bytes: u64) -> String {
    if bytes < 1024 * 1024 { format!("{} KiB", bytes.div_ceil(1024)) }
    else { format!("{:.1} MiB", bytes as f64 / (1024 * 1024) as f64) }
}

/// One row: a group title (0), a setting (1) or a line of a report under
/// one (2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsRow {
    pub kind: i32,
    pub key: String,
    pub title: String,
    pub description: String,
    pub control: String,
    pub action: String,
    pub value: String,
    pub options: Vec<String>,
    pub values: Vec<String>,
    pub marks: Vec<String>,
}

type Row = SettingsRow;

fn pipewire_dsd_row(row: &mut Row, exclusive: bool, mode: &str) {
    for (value, label) in fixed("audio.dsdMode") {
        if value == "dop" { continue; }
        row.values.push(value.into());
        row.options.push(label);
    }
    row.control = if exclusive { "select" } else { "fixed" }.into();
    row.value = if exclusive && mode == "native" { "native" } else { "pcm" }.into();
    row.description = chilpa_i18n::t("Native DSD needs exclusive mode and a compatible output. Otherwise playback converts DSD to PCM.");
}

/// What the page holds that the facade does not: the last diagnosis of the
/// output, what the last press said, and a plugin's sign-in page while it is
/// being waited for.
#[derive(Default)]
pub struct RowNotes<'a> {
    pub report: Option<&'a crate::WizardReport>,
    pub said: Option<&'a (String, String)>,
    pub plugin_link: Option<&'a (String, String)>,
}

/// The options of a segmented or select setting, as (value, label).
pub(crate) fn fixed(key: &str) -> Vec<(&'static str, String)> {
    // Each label is spelled out as a `chilpa_i18n::t` call so the extractor
    // finds it.
    match key {
        // Each language is named in itself, so whoever cannot read the
        // current one still finds their own.
        "appearance.language" => vec![
            ("", chilpa_i18n::t("System")),
            ("en", "English".into()),
            ("es", "Español".into()),
            ("fr", "Français".into()),
            ("de", "Deutsch".into()),
            ("pt", "Português".into()),
            ("ru", "Русский".into()),
            ("ja", "日本語".into()),
            ("nl", "Nederlands".into()),
        ],
        crate::ambient::MODE => vec![
            ("off", chilpa_i18n::t("Off")),
            ("ambient", chilpa_i18n::t("Ambient")),
            ("blurred", chilpa_i18n::t("Blurred art")),
            ("wallpaper", chilpa_i18n::t("Wallpaper")),
            ("wallpaper-blurred", chilpa_i18n::t("Wallpaper, blurred")),
        ],
        crate::mini::DEFAULT_VIEW => vec![
            ("remember", chilpa_i18n::t("Remember")),
            ("micro", chilpa_i18n::t("Micro")),
            ("compact", chilpa_i18n::t("Compact")),
            ("artwork", chilpa_i18n::t("Artwork")),
            ("queue", chilpa_i18n::t("Queue")),
            ("lyrics", chilpa_i18n::t("Lyrics")),
        ],
        crate::immersive::DEFAULT_VIEW => vec![
            ("remember", chilpa_i18n::t("Remember")),
            ("reactive", chilpa_i18n::t("Album Reactive")),
            ("static", chilpa_i18n::t("Static")),
            ("coverflow", chilpa_i18n::t("Coverflow")),
            ("spectrum", chilpa_i18n::t("Spectrum")),
            ("lyrics", chilpa_i18n::t("Lyrics")),
            ("queue", chilpa_i18n::t("Queue")),
        ],
        crate::immersive::SEARCH_ACTION => vec![
            ("disabled", chilpa_i18n::t("Disabled")),
            ("replace", chilpa_i18n::t("Replace current queue")),
            ("next", chilpa_i18n::t("Play next")),
            ("queue", chilpa_i18n::t("Add to queue")),
        ],
        "appearance.density" => vec![("compact", chilpa_i18n::t("Compact")), ("rich", chilpa_i18n::t("Rich"))],
        "appearance.font" => vec![("inter", "Inter".to_string()), ("line-seed-jp", "LINE Seed JP".to_string()), ("system", chilpa_i18n::t("System"))],
        "appearance.albumHeader" => vec![("full", chilpa_i18n::t("Full")), ("colors", chilpa_i18n::t("Colors")), ("off", chilpa_i18n::t("Off"))],
        "navigation.albumClick" => vec![("page", chilpa_i18n::t("Album page")), ("quick", chilpa_i18n::t("Quick view"))],
        "navigation.playerLayout" => vec![("floating", chilpa_i18n::t("Floating")), ("bottom", chilpa_i18n::t("Full width, bottom")), ("top", chilpa_i18n::t("Full width, top"))],
        "navigation.playlistArtwork" => {
            vec![("icon", chilpa_i18n::t("Playlist icon")), ("collage", chilpa_i18n::t("Cover collage"))]
        }
        "storage.remoteLimitMb" => vec![("100", "100 MiB".into()), ("200", "200 MiB".into()), ("500", "500 MiB".into()), ("1024", "1 GiB".into())],
        "library.metaSrc" => vec![("musicbrainz", "MusicBrainz".to_string()), ("discogs", "Discogs".to_string()), ("embedded", chilpa_i18n::t("Embedded tags only"))],
        "audio.dsdMode" => vec![("native", chilpa_i18n::t("Native DSD")), ("dop", chilpa_i18n::t("DSD over PCM (DoP)")), ("pcm", chilpa_i18n::t("Convert to PCM"))],
        "desktop.trayIcon" => vec![("auto", chilpa_i18n::t("Auto")), ("mono-light", chilpa_i18n::t("Mono light")), ("mono-dark", chilpa_i18n::t("Mono dark")), ("color", chilpa_i18n::t("Color"))],
        _ => Vec::new(),
    }
}


const DEFAULTS: &[(&str, &str)] = &[("appearance.density", "compact"), ("appearance.font", "inter"), ("appearance.albumHeader", "full"), (crate::ambient::MODE, "off"), (crate::mini::DEFAULT_VIEW, "remember"), (crate::immersive::DEFAULT_VIEW, "remember"), (crate::immersive::SEARCH_ACTION, "replace"), ("navigation.albumClick", "page"), ("navigation.playlistArtwork", "icon"), ("library.metaSrc", "musicbrainz")];

impl Chilpa {
    /// Does what a button of Settings asks for, whole, and answers what its
    /// row says afterwards. `None` for a button nobody here takes.
    ///
    /// Each part of Chilpa answers for its own buttons, next to the code
    /// that does the work, and this only asks them in turn. A new button is
    /// its entry in the schema and an arm where its feature lives: no list
    /// of keys grows here, and nothing changes in the window's bridge.
    pub fn maintain(&mut self, key: &str) -> Option<String> {
        if key == "storage.refresh" {
            crate::storage_cache::request_scan();
            return Some(chilpa_i18n::t("Updating usage…"));
        }
        if let Some(kind) = key.strip_prefix("storage.clear.") {
            if let Some(said) = self.lyrics_maintenance(key) { return Some(said); }
            return crate::storage_cache::clear(kind).then(|| chilpa_i18n::t("Clearing in the background…"));
        }
        if let Some(said) = self.library_maintenance(key) {
            return Some(said);
        }
        self.lyrics_maintenance(key).or_else(|| self.updates_maintenance(key)).or_else(|| self.wizard_maintenance(key)).or_else(|| self.audio_output_maintenance(key)).or_else(||self.discogs_maintenance(key))
    }

    /// A setting's new value, for the feature it belongs to. Like
    /// [`Self::maintain`], this asks each feature in turn and keeps no list
    /// of keys: the window's bridge hands over what it does not know by
    /// name. `false` for a key nobody takes.
    pub fn adjust(&mut self, key: &str, value: &str) -> bool {
        if self.discogs_setting(key,value) || self.updates_setting(key,value) {return true;}
        if key == "storage.remoteLimitMb" {
            if !crate::storage_cache::set_limit(value) { return false }
            self.remember_preference(key, value);
            return true;
        }
        if key == "audio.hwVolume" {
            self.set_hardware_volume(value == "true");
            return true;
        }
        #[cfg(target_os = "linux")]
        if key == "audio.hwVolumeControl" {
            if let Err(error) = self.choose_alsa_hardware_volume(value) { self.now.error = error; }
            return true;
        }
        if key == "integrations.musicbrainz.enabled" {
            self.set_musicbrainz_enabled(value == "true");
            return true;
        }
        if matches!(key, "integrations.offline" | "integrations.proxy.enabled" | "integrations.lyrics.enabled" | "integrations.lyrics.service" | "integrations.lyrics.translate")
            || key.starts_with("integrations.lyrics.provider.") {
            self.remember_preference(key, value);
            return true;
        }
        self.track_credits_setting(key, value) || self.playlist_grouping_setting(key, value) || self.search_preview_setting(key, value) || self.device_sync_setting(key, value) || self.tray_setting(key, value) || self.discord_setting(key, value) || self.ambient_setting(key, value) || self.mini_setting(key, value) || self.immersive_setting(key, value) || self.play_indicator_setting(key, value) || self.graphics_setting(key, value) || self.normalization_setting(key, value)
    }

    /// A toggle's value from its preference, "true" or "false".
    fn pref_on(&self, key: &str, default: bool) -> String {
        self.preference(key).map_or(default, |v| v == "true").to_string()
    }

    /// The rows of one section, with what each says now, and whether the
    /// section has nothing built yet.
    pub fn settings_rows(&mut self, section: &str, notes: &RowNotes) -> (Vec<SettingsRow>, bool) {
        if section == "storage" && !crate::storage_cache::usage().ready {
            crate::storage_cache::request_scan();
        }
        // The audio rows show the stacks and outputs as last listed, and
        // somebody opening them wants a device plugged in a moment ago: the
        // next listing is made now, the page is told when it lands, and the
        // listing keeps itself current for a while, since the page is
        // looked at and does not say so again.
        if section == "audio" {
            self.outputs_listing.ask();
            self.outputs_listing.watch_for(crate::outputs::WATCH);
        }
        let section = settings_schema::section(section);
        let mut rows = Vec::new();
        let (output, _) = self.chosen_output();
        for group in section.groups {
            // A group whose every row was left out goes with them: a heading
            // over nothing reads as something that failed to load.
            if rows.last().is_some_and(|r: &Row| r.kind == 0) {
                rows.pop();
            }
            rows.push(Row { kind: 0, title: chilpa_i18n::t(group.title), ..Row::default() });
            for s in group.settings {
                if s.key == "plugins.installed" {
                    let plugins = self.plugins();
                    if plugins.is_empty() {
                        rows.push(Row { kind: 1, key: s.key.into(), title: chilpa_i18n::t(s.title), description: chilpa_i18n::t(s.description), control: "none".into(), ..Row::default() });
                    } else {
                        for plugin in plugins {
                            rows.extend(self.source_rows(plugin, notes));
                        }
                    }
                    continue;
                }
                if s.key == "sources.servers" {
                    rows.extend(self.server_rows(notes));
                    continue;
                }
                let mut row = Row { kind: 1, key: s.key.into(), title: chilpa_i18n::t(s.title), description: if s.description.is_empty() { String::new() } else { chilpa_i18n::t(s.description) }, control: s.control.name().into(), action: if s.action.is_empty() { String::new() } else { chilpa_i18n::t(s.action) }, ..Row::default() };
                self.fill(&mut row, &output, notes);
                // An unavailable row stays absent when offline restrictions apply.
                if row.control.is_empty() { continue; }
                let local_account_action=row.key=="integrations.discogs.account" && row.control=="button" && {
                    let status=self.discogs();!status.user.is_empty() || status.checking
                };
                let lyrics_off = self.preference("integrations.lyrics.enabled").as_deref() == Some("false");
                if (self.offline() && row.key.starts_with("integrations.") && row.key != "integrations.offline" && row.key != "integrations.lyrics.enabled" && !local_account_action)
                    || (lyrics_off && row.key.starts_with("integrations.lyrics.") && row.key != "integrations.lyrics.enabled") {
                    row.control = "fixed".into();
                }
                rows.push(row);
                if s.key == "audio.wizard" {
                    for line in notes.report.iter().filter(|r|r.device == output).flat_map(|r| &r.rows) {
                        rows.push(Row { kind: 2, title: line.name.clone(), description: line.format.clone(), value: line.verdict.clone(), ..Row::default() });
                    }
                }
            }
        }
        if rows.last().is_some_and(|r: &Row| r.kind == 0) {
            rows.pop();
        }
        (rows, section.groups.is_empty())
    }

    /// The value, the options and anything the row says about its state.
    fn fill(&mut self, row: &mut Row, output: &str, notes: &RowNotes) {
        if self.discogs_row(row) || self.updates_settings_row(row) {return;}
        if self.graphics_row(row) || self.normalization_row(row) || self.wizard_row(row) || self.theme_row(row) {
            return;
        }
        match row.key.as_str() {
            "appearance.language" => {
                row.value = chilpa_core::settings::load().language;
                for (value, label) in fixed(&row.key) {
                    row.values.push(value.into()); row.options.push(label);
                }
            }
            "storage.total" | "storage.remote" | "storage.embedded" | "storage.derivatives" | "storage.other" => {
                let usage = crate::storage_cache::usage();
                row.control = "none".into();
                if usage.ready {
                    let count = match row.key.as_str() {
                        "storage.remote" => usage.remote,
                        "storage.embedded" => usage.embedded,
                        "storage.derivatives" => usage.derivatives,
                        "storage.other" => usage.other,
                        _ => crate::storage_cache::Count { bytes: usage.total(), files: usage.remote.files + usage.embedded.files + usage.derivatives.files + usage.other.files },
                    };
                    row.value = cache_size(count.bytes);
                    row.description = format!("{} · {}", row.description, chilpa_i18n::t_args("{} files", &[&count.files.to_string()]));
                    if row.key == "storage.remote" && usage.pinned.files > 0 {
                        row.description = format!("{} · {}", row.description,
                            chilpa_i18n::t_args("{} protected by the library", &[&cache_size(usage.pinned.bytes)]));
                    }
                } else {
                    row.value = chilpa_i18n::t("Calculating…");
                }
            }
            "storage.remoteLimitMb" => {
                row.value = crate::storage_cache::limit_mb().to_string();
                for (value, label) in fixed(&row.key) {
                    row.values.push(value.into());
                    row.options.push(label);
                }
            }
            "storage.open" => {
                row.value = url::Url::from_directory_path(chilpa_dirs::cache()).map(|url| url.to_string()).unwrap_or_default();
            }
            "devices.original" | "devices.discovery" => row.control = "none".into(),
            "audio.backend" => {
                let (stacks, current) = (self.stacks(), self.stack_of(output));
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
                let outputs = self.outputs_beside(output);
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
                let (fidelity, chosen) = (self.fidelity_of(output), self.exclusive());
                let fixed = match fidelity {
                    "exact" => Some((true, chilpa_i18n::t("This output is always Chilpa's alone."))),
                    "depends" => None,
                    _ => Some((false, chilpa_i18n::t("Not through this stack: it mixes by design."))),
                };
                row.value = fixed.as_ref().map_or(chosen, |(on, _)| *on).to_string();
                if let Some((_, why)) = fixed {
                    row.control = "fixed".into();
                    row.description = format!("{} {why}", row.description);
                }
            }
            "audio.forceRate" => {
                let (shown, on, alone) = (self.stack_of(output) == "pipewire", self.forcing_rate(), self.exclusive());
                row.value = (on && alone).to_string();
                row.control = if !shown {
                    String::new()
                } else if alone {
                    row.control.clone()
                } else {
                    "fixed".into()
                };
                if shown && !alone {
                    row.description = format!("{} {}", row.description, chilpa_i18n::t("Only with exclusive mode on."));
                }
            }
            "audio.hwVolume" => {
                if self.stack_of(output) != "alsa" || self.alsa_plugin() != "hw" {
                    row.control.clear();
                } else {
                    let available = chilpa_core::volume_of(output).available;
                    row.value = (available && self.hardware_volume()).to_string();
                    if !available {
                        row.control = "fixed".into();
                        row.description = chilpa_i18n::t("This output has no unambiguous, safe playback volume control.");
                    }
                }
            }
            "audio.hwVolumeControl" => {
                if self.stack_of(output) != "alsa" || self.alsa_plugin() != "hw" {
                    row.control.clear();
                } else {
                    #[cfg(target_os = "linux")]
                    {
                        let choices = chilpa_core::alsa_hardware_volume_choices(output);
                        if choices.is_empty() || (choices.len() == 1 && choices[0].selected) { row.control.clear(); }
                        else {
                            row.values.push(String::new());
                            row.options.push(chilpa_i18n::t("Choose a playback control"));
                            for choice in choices {
                                if choice.selected { row.value = choice.id.clone(); }
                                row.values.push(choice.id);
                                row.options.push(choice.label);
                            }
                        }
                    }
                    #[cfg(not(target_os = "linux"))]
                    { row.control.clear(); }
                }
            }
            crate::ambient::MODE | crate::ambient::IMAGE | crate::ambient::BLUR => self.ambient_row(row),
            crate::mini::DEFAULT_VIEW => self.mini_row(row),
            crate::immersive::DEFAULT_VIEW | crate::immersive::SEARCH_ACTION => self.immersive_row(row),
            crate::tray::SHOWN => row.value = self.tray_wanted().to_string(),
            crate::tray::CONFIRM_QUIT => row.value = self.confirms_quit().to_string(),
            // Nothing to close to, and no glyph to choose, while the icon is off.
            crate::tray::CLOSE_TO_TRAY => {
                row.value = self.preference(&row.key).is_none_or(|v| v == "true").to_string();
                if !self.tray_wanted() {
                    row.control.clear();
                }
            }
            crate::tray::GLYPH => {
                for (value, label) in fixed(&row.key) {
                    row.values.push(value.to_string());
                    row.options.push(label);
                }
                row.value = self.tray_glyph();
                if !self.tray_wanted() {
                    row.control.clear();
                }
            }
            "audio.reserve" => {
                let (card, on) = (!self.alsa_plugin().is_empty(), self.reserving());
                row.value = on.to_string();
                row.control = if card { row.control.clone() } else { String::new() };
            }
            "audio.alsaPlugin" => {
                let (plugin, plugins) = (self.alsa_plugin(), self.alsa_plugins());
                row.value = plugin.to_string();
                // Only a card has a plugin to choose.
                if row.value.is_empty() {
                    row.control.clear();
                }
                for p in plugins {
                    row.values.push(p.to_string());
                    row.options.push(match *p {
                        "hw" => chilpa_i18n::t("hw: the card alone"),
                        "plughw" => chilpa_i18n::t("plughw: converts what it refuses"),
                        _ => chilpa_i18n::t("pcm: shared, as ALSA sets it up"),
                    });
                }
            }
            "audio.dsdMode" => {
                if self.stack_of(output) == "pipewire" {
                    pipewire_dsd_row(row, self.exclusive(), self.dsd_mode().key());
                    return;
                }
                if !matches!(self.stack_of(output).as_str(), "alsa" | "coreaudio") {
                    row.control.clear();
                    return;
                }
                // Only a card on hw, or a Mac's output alone, takes DSD
                // untouched. Anywhere else it plays converted, and the row
                // says so instead of going away: a hidden row left the
                // choice made for the card looking like the one in force.
                // A Mac has no native DSD to offer, only DoP.
                let mac = self.stack_of(output) == "coreaudio";
                let direct = (self.stack_of(output) == "alsa" && self.alsa_plugin() == "hw") || (mac && self.exclusive());
                let mode = self.dsd_mode();
                for (value, label) in fixed(&row.key) {
                    if mac && value == "native" {
                        continue;
                    }
                    row.values.push(value.to_string());
                    row.options.push(label);
                }
                let shown = if mac && mode.key() == "native" { "pcm" } else { mode.key() };
                row.value = if direct { shown } else { "pcm" }.to_string();
                if !direct {
                    row.control = "fixed".into();
                    row.description = if mac {
                        chilpa_i18n::t("This output takes DSD converted to PCM. DoP needs exclusive mode, and a DAC that decodes it.")
                    } else {
                        chilpa_i18n::t("This output takes DSD converted to PCM. Only a card on hw takes it untouched; what is chosen for the card is kept.")
                    };
                }
            }
            "audio.wizard" => {
                if let Some(report) = notes.report.filter(|r|r.device == output) {
                    row.description = report.summary.clone();
                }
            }
            "integrations.listenbrainz.account" => {
                let status = self.listenbrainz();
                account(row, &status);
            }
            "navigation.extraControls" => row.value = self.pref_on("playback.extra", true),
            crate::play_indicator::ANIMATED => row.value = self.play_indicator_animated().to_string(),
            "navigation.autoHide" => row.value = self.pref_on("playback.autoHide", false),
            "playback.gapless" => row.value = self.gapless().to_string(),
            "playback.normalize" => row.value = self.normalize().to_string(),
            "playback.keepQueue" => row.value = self.keep_queue().to_string(),
            "navigation.newtab" => row.value = self.pref_on("navigation.newtab", true),
            "navigation.playerLayout" => {
                row.value = self.player_layout().to_string();
                for (value, label) in fixed(&row.key) { row.values.push(value.into()); row.options.push(label); }
            },
            crate::credits::SHOW_COMPOSERS => row.value = self.track_composers_shown().to_string(),
            "navigation.restoreTabs" => row.value = self.pref_on("navigation.restoreTabs", true),
            crate::source_integration::GROUP_PLAYLISTS => row.value = self.pref_on(&row.key, false),
            crate::search::SEARCH_PREVIEW => row.value = self.pref_on(&row.key, true),
            crate::device_sync::SHOW_IN_SIDEBAR => row.value = self.pref_on(&row.key, true),
            "navigation.folderColors" => row.value = self.pref_on("navigation.folderColors", false),
            "integrations.listenbrainz.enabled" => row.value = self.listenbrainz().enabled.to_string(),
            "integrations.lastfm.account" => {
                let status = self.lastfm();
                if status.checking && !status.link.is_empty() {
                    // Waiting for the person to allow it on the site: the page
                    // opens, and can be opened again from here. "external"
                    // is no control of the schema's: only this state has it.
                    row.description = chilpa_i18n::t("Allow Chilpa on Last.fm; this updates by itself.");
                    row.control = "external".into();
                    row.action = chilpa_i18n::t("Open Last.fm");
                    row.value = status.link.clone();
                } else {
                    account(row, &status);
                }
            }
            "integrations.lastfm.enabled" => row.value = self.lastfm().enabled.to_string(),
            "integrations.musicbrainz.enabled" => row.value = self.musicbrainz_enabled().to_string(),
            "integrations.offline" | crate::discord::ENABLED => row.value = self.pref_on(&row.key, false),
            "integrations.proxy.enabled" | "integrations.lyrics.enabled" | "integrations.lyrics.service" => row.value = self.pref_on(&row.key, true),
            "integrations.lyrics.translate" => {
                row.value = self.pref_on(&row.key, true);
                if self.preference("integrations.lyrics.service").as_deref() == Some("false") {
                    row.control = "fixed".into();
                }
            }
            key if key.starts_with("integrations.lyrics.provider.") => {
                row.value = self.pref_on(&row.key, true);
                if self.preference("integrations.lyrics.service").as_deref() == Some("false") {
                    row.control = "fixed".into();
                }
            }
            "library.folders" => {
                let n = self.library_folders().len();
                let shown = n.to_string();
                row.description = chilpa_i18n::tf("{} folder", "{} folders", n as i64, &[&shown]);
            }
            "library.analysis" => {
                let status = self.analysis_status().clone();
                row.action = if status.running || status.waiting.is_some() { chilpa_i18n::t("Cancel") } else { chilpa_i18n::t("Analyze") };
                if status.waiting.is_some() {
                    row.description = chilpa_i18n::t("Starts when the scan is done");
                } else if status.running {
                    if status.total == 0 {
                        row.description = chilpa_i18n::t("Counting tracks…");
                    } else {
                        let (done, total) = (status.done.to_string(), status.total.to_string());
                        let progress = chilpa_i18n::t_args("Analyzing {} of {} tracks", &[&done, &total]);
                        row.description = if status.last.is_empty() { progress } else { format!("{progress} · {}", status.last) };
                        row.value = (status.done as f64 / status.total as f64).to_string();
                    }
                } else if status.playlists_updated {
                    row.description = status.last;
                } else if status.analyzed + status.without_pulse + status.failed > 0 {
                    let (analyzed, quiet, failed) = (status.analyzed.to_string(), status.without_pulse.to_string(), status.failed.to_string());
                    row.description = chilpa_i18n::t_args("{} analyzed, {} without pulse, {} unreadable", &[&analyzed, &quiet, &failed]);
                }
            }
            "library.mood.generate" => {
                let status = self.analysis_status().clone();
                row.action = if status.running && status.mood_requested { chilpa_i18n::t("Cancel") } else { chilpa_i18n::t("Generate") };
                if status.mood_requested && !status.last.is_empty() {
                    row.description = status.last;
                } else if status.running {
                    row.description = chilpa_i18n::t("Finish or cancel the current analysis first.");
                }
                if status.running && status.total > 0 {
                    row.value = (status.done as f64 / status.total as f64).to_string();
                }
            }
            key => {
                for (value, label) in fixed(key) {
                    row.values.push(value.to_string());
                    row.options.push(label);
                }
                if !row.values.is_empty() {
                    let default = DEFAULTS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).unwrap_or("");
                    row.value = self.preference(key).unwrap_or_else(|| default.to_string());
                }
            }
        }
        if let Some((key, said)) = notes.said {
            if key == &row.key {
                row.description = said.clone();
            }
        }
    }
    /// One status row, plus the token field when that is how the plugin
    /// connects. The available actions are derived from the state so the
    /// menu cannot offer an operation that does not make sense now.
    fn source_rows(&self, plugin: crate::PluginInfo, notes: &RowNotes) -> Vec<Row> {
        let key = format!("sources.plugin/{}", plugin.id);
        let mut details = Vec::new();
        if !plugin.description.is_empty() {
            details.push(plugin.description.clone());
        }
        if !plugin.problem.is_empty() {
            details.push(plugin.problem.clone());
        }
        if !plugin.pending.is_empty() {
            let asks = plugin.pending.join(", ");
            details.push(chilpa_i18n::t_args("Requests: {}", &[&asks]));
            if plugin.wants_more {
                details.push(chilpa_i18n::t("This update asks for additional access."));
            }
        } else if plugin.connected {
            details.push(chilpa_i18n::t("Account connected."));
        } else if plugin.account_expired {
            details.push(chilpa_i18n::t("The account expired; connect it again."));
        } else if !plugin.account_kind.is_empty() {
            details.push(chilpa_i18n::t("Account not connected."));
        }
        // How it is doing, in a word and a color: the word is the pill's and
        // the color its light's.
        let (word, light) = match plugin.state.as_str() {
            "fine" => (chilpa_i18n::t("Active"), "positive"),
            "awaiting approval" => (chilpa_i18n::t("Awaiting approval"), "warning"),
            "stumbling" => (chilpa_i18n::t("Stumbling"), "warning"),
            _ => (chilpa_i18n::t("Quarantined"), "negative"),
        };
        let mut row = Row { kind: 1, key, title: plugin.name.clone(), description: details.join(" "), control: "status".into(), action: word, marks: vec![light.into()], ..Row::default() };
        if let Some((said_key, said)) = notes.said {
            if said_key == &row.key {
                row.description = said.clone();
            }
        }
        if !plugin.pending.is_empty() {
            row.values.push("approve".into());
            row.options.push(if plugin.wants_more { chilpa_i18n::t("Approve new access") } else { chilpa_i18n::t("Approve") });
        } else {
            if plugin.state == "quarantined" {
                row.values.push("reactivate".into());
                row.options.push(chilpa_i18n::t("Reactivate"));
            }
            if plugin.connected {
                row.values.push("disconnect".into());
                row.options.push(chilpa_i18n::t("Disconnect account"));
            } else if plugin.account_kind == "oauth" {
                row.values.push("connect".into());
                let host = if plugin.account_host.is_empty() { chilpa_i18n::t("the source") } else { plugin.account_host.clone() };
                row.options.push(chilpa_i18n::t_args("Connect on {}", &[&host]));
            }
            row.values.push("revoke".into());
            row.options.push(chilpa_i18n::t("Withdraw approval"));
        }
        if let Some((id, url)) = notes.plugin_link {
            if id == &plugin.id {
                row.value = url.clone();
            }
        }
        let mut rows = vec![row];
        if plugin.pending.is_empty() && plugin.account_kind == "token" && !plugin.connected {
            rows.push(Row { kind: 1, key: format!("sources.token/{}", plugin.id), title: chilpa_i18n::t_args("Connect {}", &[&plugin.name]), description: chilpa_i18n::t("Paste the token supplied by the source. Chilpa keeps it encrypted and the plugin never sees it."), control: "secret".into(), action: chilpa_i18n::t("Connect"), ..Row::default() });
        }
        rows
    }
}

/// An account's row: checking it, or who it is with a way out and how many
/// listens wait, or what went wrong.
fn account(row: &mut Row, status: &crate::ScrobbleStatus) {
    if status.checking {
        row.description = chilpa_i18n::t("Checking the account…");
    } else if !status.user.is_empty() {
        row.title = chilpa_i18n::t_args("Connected as {}", &[&status.user]);
        row.control = "button".into();
        row.action = chilpa_i18n::t("Disconnect");
        row.description = if status.pending > 0 {
            let n = status.pending.to_string();
            chilpa_i18n::tf("{} listen waiting to be sent", "{} listens waiting to be sent", status.pending as i64, &[&n])
        } else {
            String::new()
        };
    }
    if !status.error.is_empty() {
        row.description = status.error.clone();
    }
}

#[cfg(test)]
mod dsd_tests {
    use super::*;

    #[test]
    fn navigation_layout_select_has_the_players_three_options_and_current_value() {
        let mut api = Chilpa::new();
        let mut row = Row { key: "navigation.playerLayout".into(), control: "select".into(), ..Row::default() };
        api.fill(&mut row, "", &RowNotes::default());
        assert_eq!(row.values, crate::player_layout::LAYOUTS);
        assert_eq!(row.options.len(), row.values.len());
        assert_eq!(row.value, api.player_layout());
    }

    #[test]
    fn pipewire_exposes_native_dsd_only_when_exclusive_and_keeps_dop_out() {
        for mode in ["native", "dop", "pcm"] {
            for exclusive in [false, true] {
                let mut row = Row::default();
                pipewire_dsd_row(&mut row, exclusive, mode);
                assert_eq!(row.values, ["native", "pcm"]);
                assert_eq!(row.options.len(), 2);
                assert_eq!(row.control, if exclusive { "select" } else { "fixed" });
                assert_eq!(row.value, if exclusive && mode == "native" { "native" } else { "pcm" });
            }
        }
    }
}

