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
                    setting("audio.alsaPlugin", Chilpa::mark("ALSA plugin"), Chilpa::mark("hw plays the samples untouched or refuses them. plughw converts what the card refuses. pcm is the card as ALSA sets it up, shared with other programs. Only hw can be bit-perfect."), Control::Select),
                    setting("audio.dsdMode", Chilpa::mark("DSD playback"), Chilpa::mark("Native uses ALSA's DSD_U32 format. DoP carries the same one-bit stream inside marked PCM words. PCM converts DSD so every output can play it."), Control::Select),
                    setting("audio.reserve", Chilpa::mark("Take the card from the sound server"), Chilpa::mark("Asks PipeWire or PulseAudio to let go of the card while Chilpa plays on it, and gives it back when Chilpa stops or pauses for long."), Control::Toggle),
                    setting("audio.forceRate", Chilpa::mark("DAC passthrough"), Chilpa::mark("Sets PipeWire's graph to each track's rate while Chilpa has the output alone, so the DAC receives the rate the file holds. The rate goes back when Chilpa stops."), Control::Toggle),
                    setting("audio.exclusive", Chilpa::mark("Exclusive mode"), Chilpa::mark("The output plays only Chilpa. Other programs wait until it lets go."), Control::Toggle),
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
                    setting("playback.normalize", Chilpa::mark("Normalization"), Chilpa::mark("Every track at one loudness, by its ReplayGain. A track whose gain changes is reported as processed."), Control::Toggle),
                    setting("playback.keepQueue", Chilpa::mark("Keep the queue between sessions"), Chilpa::mark("Reopens the upcoming queue paused, at the place where you left it."), Control::Toggle),
                ],
            },
            Group {
                title: Chilpa::mark("Player bar"),
                settings: &[
                    setting("playback.extra", Chilpa::mark("Extra controls"), Chilpa::mark("Buttons to go back or forward ten seconds."), Control::Toggle),
                    setting("playback.autoHide", Chilpa::mark("Hide when idle"), Chilpa::mark("The player bar steps away after ten seconds without the mouse moving, and any movement brings it back."), Control::Toggle),
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
                setting("appearance.theme", Chilpa::mark("Theme"), "", Control::Select),
                setting("appearance.accent", Chilpa::mark("Accent"), Chilpa::mark("The first follows the theme: crimson in light, red in dark."), Control::Swatches),
                setting("appearance.density", Chilpa::mark("Track lists"), Chilpa::mark("Compact rows like a spreadsheet, or rich rows with the cover."), Control::Segmented),
                setting("appearance.font", Chilpa::mark("Font"), Chilpa::mark("Inter ships with Chilpa and has every weight the design uses. Takes effect the next time Chilpa starts."), Control::Segmented),
                setting("appearance.albumHeader", Chilpa::mark("Album header"), Chilpa::mark("Full atmosphere, artwork colors only, or no derived background."), Control::Segmented),
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
            Group { title: Chilpa::mark("Maintenance"), settings: &[action("library.repair", Chilpa::mark("Repair the library"), Chilpa::mark("Rebuilds the counts and the search index from the tracks and the play log. Nothing is lost."), Control::Button, Chilpa::mark("Repair"))] },
            Group { title: Chilpa::mark("Analysis"), settings: &[action("library.analysis", Chilpa::mark("Analyze the library"), Chilpa::mark("Reads tempo, musical key and loudness in the background. AutoMix appears when enough tracks have tempo."), Control::Progress, Chilpa::mark("Analyze"))] },
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
        }, Group {
            title: Chilpa::mark("Installed plugins"),
            // This sentinel expands to one row per installed plugin in the
            // frontend model.  The schema still owns the section and its
            // control; only the number of rows is necessarily dynamic.
            settings: &[setting("sources.plugins", Chilpa::mark("No source plugins installed"), Chilpa::mark("Put a plugin folder in Chilpa's plugins directory and reopen the app."), Control::Status)],
        }],
    },
    Section {
        id: "integrations",
        label: Chilpa::mark("Integrations"),
        icon: "broadcast",
        groups: &[
            Group {
                title: "ListenBrainz",
                settings: &[
                    action("integrations.listenbrainz.account", Chilpa::mark("Account"), Chilpa::mark("Paste the user token from listenbrainz.org/settings."), Control::Secret, Chilpa::mark("Connect")),
                    setting("integrations.listenbrainz.enabled", Chilpa::mark("Send listens"), Chilpa::mark("A track counts once half of it has played, or four minutes. Listens made offline are sent later."), Control::Toggle),
                ],
            },
            Group {
                title: "Last.fm",
                settings: &[
                    action("integrations.lastfm.account", Chilpa::mark("Account"), Chilpa::mark("Last.fm opens in the browser to allow Chilpa."), Control::Button, Chilpa::mark("Connect")),
                    setting("integrations.lastfm.enabled", Chilpa::mark("Scrobble"), Chilpa::mark("The same rule as ListenBrainz: half of a track, or four minutes."), Control::Toggle),
                ],
            },
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
    Section {
        id: "advanced",
        label: Chilpa::mark("Advanced"),
        icon: "settings",
        groups: &[Group { title: Chilpa::mark("Playback"), settings: &[setting("advanced.hwVolume", Chilpa::mark("Hardware volume"), Chilpa::mark("The player bar moves the output's own volume instead of scaling the samples. It needs an output with a volume of its own."), Control::Toggle)] }],
    },
];

pub fn section(id: &str) -> &'static Section {
    SECTIONS.iter().find(|s| s.id == id).unwrap_or(&SECTIONS[0])
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
            // A secret is a field and the button that sends it.
            let pressable = matches!(set.control, Control::Button | Control::Link | Control::Secret);
            assert_eq!(pressable, !set.action.is_empty(), "{}", set.key);
        }
    }

    #[test]
    fn an_unknown_section_opens_the_first() {
        assert_eq!(section("nowhere").id, "audio");
    }
}
