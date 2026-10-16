//! What Settings shows, as data (frontend ADR-0010).
//!
//! Sections, their groups and their settings, each with the control that
//! draws it. No setting has QML of its own: `SettingsRow` picks the control
//! from this table, and the settings page model fills in the values. A
//! section with no groups is one not built yet, and the page says so.
//!
//! It lives in the facade and not in the window's crate: it is what Chilpa
//! can be set to, which is the same whatever draws it.

use crate::Chilpa;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Toggle,
    Select,
    /// A select of outputs, with the bit-perfect mark on the ones that take
    /// samples untouched.
    Device,
    Segmented,
    /// Runs something: its label is the setting's `action`.
    Button,
    /// Opens a page of its own.
    Link,
    /// A field whose text is hidden as it is typed, and a button that sends
    /// it: a token.
    Secret,
    /// Colors to pick one from.
    Swatches,
    /// A background job: its progress and a button to start or cancel it.
    Progress,
    /// A state pill whose menu contains the actions valid in that state.
    Status,
    /// A level from 0 to 100, shown live while it is dragged and kept when
    /// it is let go.
    Slider,
    /// A file chosen from a dialog, and a way to choose none again: its
    /// label is the setting's `action`.
    File,
}

impl Control {
    pub fn name(self) -> &'static str {
        match self {
            Control::Toggle => "toggle",
            Control::Select => "select",
            Control::Device => "device",
            Control::Segmented => "segmented",
            Control::Button => "button",
            Control::Link => "link",
            Control::Secret => "secret",
            Control::Swatches => "swatches",
            Control::Progress => "progress",
            Control::Status => "status",
            Control::Slider => "slider",
            Control::File => "file",
        }
    }
}

pub struct Setting {
    pub key: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub control: Control,
    /// The button's label, for a button or a link.
    pub action: &'static str,
}

pub struct Group {
    pub title: &'static str,
    pub settings: &'static [Setting],
}

pub struct Section {
    pub id: &'static str,
    pub label: &'static str,
    /// The name of its glyph in `Icons`.
    pub icon: &'static str,
    pub groups: &'static [Group],
}

const fn setting(key: &'static str, title: &'static str, description: &'static str, control: Control) -> Setting {
    Setting { key, title, description, control, action: "" }
}

const fn action(key: &'static str, title: &'static str, description: &'static str, control: Control, action: &'static str) -> Setting {
    Setting { key, title, description, control, action }
}

pub const SECTIONS: &[Section] = &[
    Section {
        id: "audio",
        label: Chilpa::mark("Audio"),
        icon: "volume2",
        groups: &[
            Group {
                title: Chilpa::mark("Output"),
                settings: &[
                    setting("audio.backend", Chilpa::mark("Audio stack"), Chilpa::mark("Where the samples go: straight to the device, or through a sound server."), Control::Select),
                    setting("audio.device", Chilpa::mark("Output device"), Chilpa::mark("BP marks an output that takes the samples untouched."), Control::Device),
                    setting("audio.dsdMode", Chilpa::mark("DSD playback"), Chilpa::mark("Native uses ALSA's DSD_U32 format. DoP carries the same one-bit stream inside marked PCM words. PCM converts DSD so every output can play it."), Control::Select),
                    setting("audio.exclusive", Chilpa::mark("Exclusive mode"), Chilpa::mark("The output plays only Chilpa. Other programs wait until it lets go."), Control::Toggle),
                    setting("audio.forceRate", Chilpa::mark("DAC passthrough"), Chilpa::mark("Sets PipeWire's graph to each track's rate while Chilpa has the output alone, so the DAC receives the rate the file holds. The rate goes back when Chilpa stops."), Control::Toggle),
                    action(crate::outputs::HIFI_WIZARD, Chilpa::mark("HiFi Wizard"), Chilpa::mark("Auto-detect your DACs and set up bit-perfect playback, step by step."), Control::Button, Chilpa::mark("Open Wizard")),
                ],
            },
            Group {
                title: Chilpa::mark("ALSA direct"),
                settings: &[
                    setting("audio.alsaPlugin", Chilpa::mark("ALSA plugin"), Chilpa::mark("hw plays the samples untouched or refuses them. plughw converts what the card refuses. pcm is the card as ALSA sets it up, shared with other programs. Only hw can be bit-perfect."), Control::Select),
                    setting("audio.reserve", Chilpa::mark("Take the card from the sound server"), Chilpa::mark("Asks PipeWire or PulseAudio to let go of the card while Chilpa plays on it, and gives it back when Chilpa stops or pauses for long."), Control::Toggle),
                    setting("audio.hwVolumeControl", Chilpa::mark("Hardware volume control"), Chilpa::mark("Choose the playback mixer element that belongs to this output. Chilpa verifies it before saving the choice."), Control::Select),
                    setting("audio.hwVolume", Chilpa::mark("Hardware volume"), Chilpa::mark("Move the output's own volume from the player bar when the selected ALSA control supports safe playback volume."), Control::Toggle),
                ],
            },
            Group { title: Chilpa::mark("Diagnosis"), settings: &[action("audio.wizard", Chilpa::mark("Diagnose the output"), Chilpa::mark("Opens the device and says which formats reach it untouched."), Control::Button, Chilpa::mark("Diagnose"))] },
        ],
    },
    Section {
        id: "playback",
        label: Chilpa::mark("Playback"),
        icon: "music",
        groups: &[
            Group {
                title: Chilpa::mark("Between tracks"),
                settings: &[
                    setting("playback.gapless", Chilpa::mark("Gapless"), Chilpa::mark("One track straight into the next. Off, the device plays out and opens again."), Control::Toggle),
                    setting("playback.normalize", Chilpa::mark("Volume normalization"), Chilpa::mark("Level loudness across tracks. This breaks strict bit perfect."), Control::Toggle),
                    setting(crate::queue::normalization::TARGET, Chilpa::mark("Target loudness"), Chilpa::mark("The loudness every track is aligned to. -14 LUFS matches streaming services, -18 LUFS is the ReplayGain reference."), Control::Select),
                    setting(crate::queue::normalization::PREVENT_CLIPPING, Chilpa::mark("Prevent clipping"), Chilpa::mark("Cap the boost so the loudest peak stays under full scale. Off lets quiet tracks reach the target even if their peaks clip."), Control::Toggle),
                    setting("playback.keepQueue", Chilpa::mark("Keep the queue between sessions"), Chilpa::mark("Reopens the upcoming queue paused, at the place where you left it."), Control::Toggle),
                ],
            },
        ],
    },
    Section {
        id: "appearance",
        label: Chilpa::mark("Appearance"),
        icon: "adjustmentsHorizontal",
        groups: &[Group {
            title: Chilpa::mark("Look"),
            settings: &[
                setting("appearance.language", Chilpa::mark("Language"), Chilpa::mark("Use the system language or choose a language."), Control::Select),
                setting("appearance.theme", Chilpa::mark("Theme"), "", Control::Select),
                setting("appearance.accent", Chilpa::mark("Accent"), Chilpa::mark("The first follows the theme: crimson in light, red in dark."), Control::Swatches),
                setting("appearance.density", Chilpa::mark("Track lists"), Chilpa::mark("Compact rows like a spreadsheet, or rich rows with the cover."), Control::Segmented),
                setting("appearance.font", Chilpa::mark("Font"), Chilpa::mark("Inter ships with Chilpa and has every weight the design uses. Takes effect the next time Chilpa starts."), Control::Segmented),
                setting("appearance.albumHeader", Chilpa::mark("Album header"), Chilpa::mark("Full atmosphere, artwork colors only, or no derived background."), Control::Segmented),
                setting(crate::play_indicator::ANIMATED, Chilpa::mark("Play indicator animation"), Chilpa::mark("Animate the playing track's mark with equalizer bars. Off by default: a still mark with an accent edge, lighter on the CPU."), Control::Toggle),
            ],
        }, Group {
            title: Chilpa::mark("Background"),
            settings: &[
                setting(crate::ambient::MODE, Chilpa::mark("Dynamic background"), Chilpa::mark("Animated album-art background behind the whole app. High resource use: GPU accelerated."), Control::Select),
                action(crate::ambient::IMAGE, Chilpa::mark("Background image"), Chilpa::mark("The system wallpaper. Choose an image of your own instead."), Control::File, Chilpa::mark("Choose image…")),
                setting(crate::ambient::BLUR, Chilpa::mark("Wallpaper blur"), Chilpa::mark("How much the wallpaper behind the window is blurred: 0% keeps it sharp, 100% is the strongest blur."), Control::Slider),
            ],
        }, Group {
            title: Chilpa::mark("Mini player"),
            settings: &[setting(crate::mini::DEFAULT_VIEW, Chilpa::mark("Mini player default view"), Chilpa::mark("The view the mini player opens with. Remember keeps the one it was last on."), Control::Select)],
        }, Group {
            title: Chilpa::mark("Immersive view"),
            settings: &[
                setting(crate::immersive::DEFAULT_VIEW, Chilpa::mark("Immersive default view"), Chilpa::mark("The view the immersive view opens with. Remember keeps the one it was last on."), Control::Select),
                setting(crate::immersive::SEARCH_ACTION, Chilpa::mark("Immersive search"), Chilpa::mark("What Enter does with a result of the immersive view's search. Disabled turns the search off."), Control::Select),
            ],
        }, Group {
            title: Chilpa::mark("Renderer"),
            settings: &[
                setting(crate::graphics_boot::INTERFACE_SIZE, Chilpa::mark("Interface size"), Chilpa::mark("Scales the whole interface, like browser zoom. Small fits more content on screen, Large and Extra large improve readability (requires restart)"), Control::Select),
                setting(crate::graphics_boot::RENDERER, Chilpa::mark("Rendering backend"), Chilpa::mark("Auto picks the best renderer for your graphics hardware. Only change this if the app feels slow or renders incorrectly (requires restart)"), Control::Select),
                setting(crate::graphics_boot::gpu::GPU, Chilpa::mark("Preferred GPU"), Chilpa::mark("Which GPU renders the app. Only the GPUs actually present are listed. Choosing one other than the default also switches the renderer to Vulkan — the only backend Qt can select a GPU on (requires restart)"), Control::Select),
            ],
        }],
    },
    Section {
        id: "desktop",
        label: Chilpa::mark("Desktop"),
        icon: "deviceDesktop",
        groups: &[Group {
            title: Chilpa::mark("Tray icon"),
            settings: &[
                setting("desktop.tray", Chilpa::mark("Tray icon"), Chilpa::mark("Chilpa's icon on the panel: a click shows or hides the window, the middle button plays or pauses, the wheel moves the volume."), Control::Toggle),
                setting("desktop.closeToTray", Chilpa::mark("Close to tray"), Chilpa::mark("Closing the window keeps Chilpa playing in the tray instead of quitting."), Control::Toggle),
                setting("desktop.confirmQuit", Chilpa::mark("Ask before quitting"), Chilpa::mark("Quitting from the window, its menu, Ctrl+Q or the tray asks first."), Control::Toggle),
                setting("desktop.trayIcon", Chilpa::mark("Tray icon style"), Chilpa::mark("Auto follows the desktop's colours: the light glyph on a dark panel, the dark one on a light panel."), Control::Select),
            ],
        }],
    },
    Section {
        id: "navigation",
        label: Chilpa::mark("Navigation"),
        icon: "list",
        groups: &[
            Group {
                title: Chilpa::mark("Tabs"),
                settings: &[
                    setting("navigation.newtab", Chilpa::mark("Middle click opens a new tab"), Chilpa::mark("On the sidebar's destinations."), Control::Toggle),
                    setting("navigation.restoreTabs", Chilpa::mark("Restore tabs"), Chilpa::mark("Chilpa opens with the tabs it had when it closed."), Control::Toggle),
                ],
            },
            Group { title: Chilpa::mark("Records"), settings: &[setting("navigation.albumClick", Chilpa::mark("A click on a record opens"), Chilpa::mark("Its page, or its quick view under the row."), Control::Segmented)] },
            Group {
                title: Chilpa::mark("Player bar"),
                settings: &[
                    setting("navigation.playerLayout", Chilpa::mark("Player bar position"), Chilpa::mark("Floating over the content, or spanning the window at the top or bottom."), Control::Select),
                    setting("navigation.extraControls", Chilpa::mark("Extra controls"), Chilpa::mark("Buttons to go back or forward ten seconds, and the A-B loop."), Control::Toggle),
                    setting("navigation.autoHide", Chilpa::mark("Hide when idle"), Chilpa::mark("The floating player bar steps away after ten seconds without the mouse moving, and any movement brings it back."), Control::Toggle),
                ],
            },
            Group {
                title: Chilpa::mark("Playlists"),
                settings: &[
                    setting("navigation.playlistArtwork", Chilpa::mark("Playlist artwork in the sidebar"), Chilpa::mark("Use the playlist icon or a cover collage."), Control::Segmented),
                    setting("navigation.folderColors", Chilpa::mark("Folder colors in the sidebar"), Chilpa::mark("Show each playlist folder in its chosen color."), Control::Toggle),
                ],
            },
        ],
    },
    Section {
        id: "library",
        label: Chilpa::mark("Library"),
        icon: "columns3",
        groups: &[
            Group {
                title: Chilpa::mark("Folders"),
                settings: &[
                    action("library.folders", Chilpa::mark("Library folders"), "", Control::Link, Chilpa::mark("Manage")),
                    action("library.rescan", Chilpa::mark("Rescan"), Chilpa::mark("Reads every folder again. What did not change is not read twice."), Control::Button, Chilpa::mark("Rescan")),
                ],
            },
            Group {
                title: Chilpa::mark("Metadata"),
                settings: &[setting(
                    "library.metaSrc",
                    Chilpa::mark("Look records up in"),
                    Chilpa::mark("Where the metadata editor searches first. \"Embedded tags only\" never goes out to the network."),
                    Control::Select,
                )],
            },
            Group {
                title: Chilpa::mark("Maintenance"),
                settings: &[
                    action("library.repair", Chilpa::mark("Repair the library"), Chilpa::mark("Rebuilds the counts and the search index from the tracks and the play log. Nothing is lost."), Control::Button, Chilpa::mark("Repair")),
                    action("library.lyricsCache", Chilpa::mark("Clear lyrics cache"), Chilpa::mark("Forgets the lyrics found so far, and the tracks known to have none. They are looked up again as tracks play."), Control::Button, Chilpa::mark("Clear")),
                ],
            },
            Group { title: Chilpa::mark("Analysis"), settings: &[
                action("library.analysis", Chilpa::mark("Analyze the library"), Chilpa::mark("Reads tempo, musical key and loudness in the background. AutoMix appears when enough tracks have tempo."), Control::Progress, Chilpa::mark("Analyze")),
                action("library.mood.generate", Chilpa::mark("Generate Mood playlists"), Chilpa::mark("Analyzes local files and puts the results in Playlists > Mood."), Control::Progress, Chilpa::mark("Generate")),
                action("library.mood", Chilpa::mark("Mood models"), Chilpa::mark("Enable the optional Mood plugin and manage its model download."), Control::Button, Chilpa::mark("Manage")),
                action("library.analysis.inventory", Chilpa::mark("Analysis inventory"), Chilpa::mark("See which local tracks have analysis results and which are pending."), Control::Button, Chilpa::mark("View")),
            ] },
        ],
    },
    Section {
        id: "storage",
        label: Chilpa::mark("Storage"),
        icon: "folderOpen",
        groups: &[
            Group { title: Chilpa::mark("Usage"), settings: &[
                setting("storage.total", Chilpa::mark("Cache used"), Chilpa::mark("Artwork, generated images, plugin data and other temporary files."), Control::Status),
                setting("storage.remote", Chilpa::mark("Remote artwork"), Chilpa::mark("Images from sources and plugins. Covers saved in your library stay protected."), Control::Status),
                setting("storage.embedded", Chilpa::mark("Local artwork"), Chilpa::mark("Extracted from your music files and kept while the library refers to it."), Control::Status),
                setting("storage.derivatives", Chilpa::mark("Generated images"), Chilpa::mark("Smaller covers and background images; Chilpa can make these again."), Control::Status),
                setting("storage.other", Chilpa::mark("Other cache"), Chilpa::mark("Includes downloaded plugin models, waveform data and server caches."), Control::Status),
                action("storage.refresh", Chilpa::mark("Recalculate usage"), Chilpa::mark("Read the current size of the cache folders in the background."), Control::Button, Chilpa::mark("Refresh")),
                action("storage.open", Chilpa::mark("Cache folder"), Chilpa::mark("Open the folder used for disposable files."), Control::Button, Chilpa::mark("Open")),
            ] },
            Group { title: Chilpa::mark("Limits and cleanup"), settings: &[
                setting("storage.remoteLimitMb", Chilpa::mark("Remote image limit"), Chilpa::mark("Older temporary images are removed in the background. Artwork referenced by your library is kept."), Control::Select),
                action("storage.clear.remote", Chilpa::mark("Clear temporary remote artwork"), Chilpa::mark("Remove unreferenced images from sources and plugins. Needed images download again."), Control::Button, Chilpa::mark("Clear…")),
                action("storage.clear.derivatives", Chilpa::mark("Clear generated images"), Chilpa::mark("Remove smaller covers and backgrounds. Chilpa makes them again as needed."), Control::Button, Chilpa::mark("Clear…")),
            ] },
        ],
    },
    Section {
        id: "sources",
        label: Chilpa::mark("Sources"),
        icon: "server",
        groups: &[Group {
            title: Chilpa::mark("Servers"),
            // Expands to one row per server, and the row that adds one
            // (ADR-017).
            settings: &[setting("sources.servers", Chilpa::mark("Servers"), "", Control::Status)],
        }],
    },
    // A section of its own and not a group under Sources: a plugin is not
    // always a source, and approving one is the first thing done with it.
    Section {
        id: "plugins",
        label: Chilpa::mark("Plugins"),
        icon: "stack2",
        groups: &[Group {
            title: Chilpa::mark("Installed"),
            // This sentinel expands to one row per installed plugin in the
            // frontend model.  The schema still owns the section and its
            // control; only the number of rows is necessarily dynamic.
            settings: &[setting("plugins.installed", Chilpa::mark("No plugins installed"), Chilpa::mark("Put a plugin folder in Chilpa's plugins directory and reopen the app."), Control::Status)],
        }],
    },
    Section {
        id: "integrations",
        label: Chilpa::mark("Integrations"),
        icon: "broadcast",
        groups: &[
            Group {
                title: Chilpa::mark("Scrobbling"),
                settings: &[
                    action("integrations.listenbrainz.account", Chilpa::mark("ListenBrainz account"), Chilpa::mark("Paste the user token from listenbrainz.org/settings."), Control::Secret, Chilpa::mark("Connect")),
                    setting("integrations.listenbrainz.enabled", Chilpa::mark("Send to ListenBrainz"), Chilpa::mark("A track counts once half of it has played, or four minutes. Listens made offline are sent later."), Control::Toggle),
                    action("integrations.lastfm.account", Chilpa::mark("Last.fm account"), Chilpa::mark("Last.fm opens in the browser to allow Chilpa."), Control::Button, Chilpa::mark("Connect")),
                    setting("integrations.lastfm.enabled", Chilpa::mark("Send to Last.fm"), Chilpa::mark("The same rule as ListenBrainz: half of a track, or four minutes."), Control::Toggle),
                ],
            },
            Group { title: "MusicBrainz", settings: &[
                setting("integrations.musicbrainz.enabled", Chilpa::mark("Use MusicBrainz"), Chilpa::mark("Chilpa sends no telemetry. When enabled, artist and release searches make one-way requests to MusicBrainz for results. Turning it off disables MusicBrainz features in Artist Network."), Control::Toggle),
            ] },
            Group { title: Chilpa::mark("Lyrics"), settings: &[
                setting("integrations.lyrics.service", Chilpa::mark("Chilpa lyrics API"), Chilpa::mark("Look up lyrics through Chilpa's lyrics service when the file and its source have none."), Control::Toggle),
                setting("integrations.lyrics.plugin", Chilpa::mark("Plugin lyrics"), Chilpa::mark("Allow a plugin to supply lyrics for its own tracks and prefer them over Chilpa's lyrics API."), Control::Toggle),
                setting("integrations.lyrics.translate", Chilpa::mark("Translate lyrics"), Chilpa::mark("Ask Chilpa's lyrics API to translate lyrics that carry no translation in your language. The lines of the track are sent to it. Off by default."), Control::Toggle),
                setting("integrations.lyrics.provider.lrclib", "LRCLIB", Chilpa::mark("Allow results from LRCLIB through Chilpa's lyrics API."), Control::Toggle),
                setting("integrations.lyrics.provider.musixmatch", "Musixmatch", Chilpa::mark("Allow results from Musixmatch when the lyrics service has access to it."), Control::Toggle),
                setting("integrations.lyrics.provider.amll", "AMLL", Chilpa::mark("Allow word-timed lyrics from AMLL."), Control::Toggle),
                setting("integrations.lyrics.provider.netease", "NetEase", Chilpa::mark("Allow results from NetEase."), Control::Toggle),
                setting("integrations.lyrics.provider.qq", "QQ Music", Chilpa::mark("Allow results from QQ Music."), Control::Toggle),
                setting("integrations.lyrics.provider.kugou", "KuGou", Chilpa::mark("Allow results from KuGou."), Control::Toggle),
            ] },
        ],
    },
    Section { id: "devices", label: Chilpa::mark("Devices & cast"), icon: "cast", groups: &[
        Group { title: Chilpa::mark("Network playback"), settings: &[
            action("devices.cast", Chilpa::mark("DLNA / UPnP and Chromecast"), Chilpa::mark("Choose a renderer on your local network. The queue stays in Chilpa."), Control::Button, Chilpa::mark("Choose device")),
            setting("devices.original", Chilpa::mark("Original files"), Chilpa::mark("Original bytes · no transcoding by Chilpa · receiver processing unknown"), Control::Status),
            setting("devices.discovery", Chilpa::mark("Device discovery"), Chilpa::mark("Search only while the picker is open. Disconnecting stops the receiver and leaves local playback stopped."), Control::Status),
        ] },
    ] },
    Section { id: "updates", label: Chilpa::mark("Updates"), icon: "infoCircle", groups: &[] },
];

pub fn section(id: &str) -> &'static Section {
    SECTIONS.iter().find(|s| s.id == id).unwrap_or(&SECTIONS[0])
}

/// A setting found by what it says, for the search in Settings.
#[derive(Debug, serde::Serialize)]
pub struct FoundSetting {
    pub section: &'static str,
    pub key: &'static str,
    pub title: String,
    pub description: String,
    /// Where it lives: the section and the group, as Settings names them.
    pub place: String,
}

/// A text as it is matched: lower case, without the accents of the
/// languages Chilpa speaks, so "naive" finds "Naïve".
fn folded(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).map(|c| match c {
        'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        other => other,
    }).collect()
}

impl Chilpa {
    /// The settings that say every word typed, in the language shown or in
    /// English, in the order Settings shows them: those whose title, group
    /// or section say it first, then those whose description does.
    pub fn settings_find(&self, text: &str, limit: usize) -> Vec<FoundSetting> {
        let words: Vec<String> = folded(text).split_whitespace().map(str::to_string).collect();
        if words.is_empty() {
            return Vec::new();
        }
        let (mut named, mut described) = (Vec::new(), Vec::new());
        for section in SECTIONS {
            for group in section.groups {
                for setting in group.settings {
                    let title = self.t(setting.title);
                    let description = self.t(setting.description);
                    let place = format!("{} › {}", self.t(section.label), self.t(group.title));
                    let head = folded(&[title.as_str(), place.as_str(), setting.title, group.title, section.label].join(" "));
                    let all = format!("{head} {} {}", folded(&description), folded(setting.description));
                    if !words.iter().all(|word| all.contains(word.as_str())) {
                        continue;
                    }
                    let found = FoundSetting { section: section.id, key: setting.key, title, description, place };
                    if words.iter().all(|word| head.contains(word.as_str())) { named.push(found) } else { described.push(found) }
                }
            }
        }
        named.extend(described);
        named.truncate(limit);
        named
    }

    /// `settings_find` as the window reads it.
    pub fn settings_find_json(&self, text: &str) -> String {
        serde_json::to_string(&self.settings_find(text, 40)).unwrap_or_else(|_| "[]".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_is_named_once() {
        let mut keys: Vec<&str> = SECTIONS.iter().flat_map(|s| s.groups).flat_map(|g| g.settings).map(|s| s.key).collect();
        let n = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), n);
    }

    #[test]
    fn every_key_lives_in_its_own_section() {
        for s in SECTIONS {
            for set in s.groups.iter().flat_map(|g| g.settings) {
                assert!(set.key.starts_with(&format!("{}.", s.id)), "{} is not under {}", set.key, s.id);
            }
        }
    }

    #[test]
    fn a_button_says_what_it_does() {
        for set in SECTIONS.iter().flat_map(|s| s.groups).flat_map(|g| g.settings) {
            // A secret is a field and the button that sends it; a progress
            // and a file carry the button that starts or chooses.
            let pressable = matches!(set.control, Control::Button | Control::Link | Control::Secret | Control::Progress | Control::File);
            assert_eq!(pressable, !set.action.is_empty(), "{}", set.key);
        }
    }

    #[test]
    fn a_setting_is_found_by_its_words_titles_first_and_without_accents() {
        let api = Chilpa::new();
        let found = api.settings_find("exclusive", 10);
        assert_eq!(found.first().map(|f| f.key), Some("audio.exclusive"));
        assert!(found.iter().all(|f| f.section == "audio" || !f.description.is_empty()));
        assert!(api.settings_find("  ", 10).is_empty());
        assert!(api.settings_find("exclusive nowhere-at-all", 10).is_empty(), "every word has to be said");
        assert_eq!(folded("Naïve Ü Ñ Ç"), "naive u n c");
        let says: Vec<bool> = api.settings_find("output", 40).iter().map(|f| folded(&f.title).contains("output") || folded(&f.place).contains("output")).collect();
        let mut ordered = says.clone();
        ordered.sort_by(|a, b| b.cmp(a));
        assert_eq!(says, ordered, "what says it in its name comes first");
    }

    #[test]
    fn an_unknown_section_opens_the_first() {
        assert_eq!(section("nowhere").id, "audio");
    }

    #[test]
    fn appearance_renderer_controls_follow_all_other_settings() {
        let groups = section("appearance").groups;
        assert_eq!(groups.last().unwrap().title, "Renderer");
        let keys: Vec<&str> = groups.last().unwrap().settings.iter().map(|setting| setting.key).collect();
        assert_eq!(keys, [crate::graphics_boot::INTERFACE_SIZE, crate::graphics_boot::RENDERER, crate::graphics_boot::gpu::GPU]);
    }
}
