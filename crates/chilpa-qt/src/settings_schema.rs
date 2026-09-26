//! What Settings shows, as data (frontend ADR-0010).
//!
//! Sections, their groups and their settings, each with the control that
//! draws it. No setting has QML of its own: `SettingsRow` picks the control
//! from this table, and the settings page model fills in the values. A
//! section with no groups is one not built yet, and the page says so.

use chilpa_api::Chilpa;

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

const fn action(
    key: &'static str,
    title: &'static str,
    description: &'static str,
    control: Control,
    action: &'static str,
) -> Setting {
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
                    setting(
                        "audio.backend",
                        Chilpa::mark("Audio stack"),
                        Chilpa::mark("Where the samples go: straight to the device, or through a sound server."),
                        Control::Select,
                    ),
                    setting(
                        "audio.device",
                        Chilpa::mark("Output device"),
                        Chilpa::mark("BP marks an output that takes the samples untouched."),
                        Control::Device,
                    ),
                    setting(
                        "audio.bitperfect",
                        Chilpa::mark("Bit-perfect"),
                        Chilpa::mark("Only what reaches the device untouched plays. A format it cannot take stops with the reason instead of being converted."),
                        Control::Toggle,
                    ),
                ],
            },
            Group {
                title: Chilpa::mark("Diagnosis"),
                settings: &[action(
                    "audio.wizard",
                    Chilpa::mark("Diagnose the output"),
                    Chilpa::mark("Opens the device and says which formats reach it untouched."),
                    Control::Button,
                    Chilpa::mark("Diagnose"),
                )],
            },
        ],
    },
    Section { id: "playback", label: Chilpa::mark("Playback"), icon: "music", groups: &[] },
    Section {
        id: "appearance",
        label: Chilpa::mark("Appearance"),
        icon: "adjustmentsHorizontal",
        groups: &[Group {
            title: Chilpa::mark("Look"),
            settings: &[
                setting("appearance.theme", Chilpa::mark("Theme"), "", Control::Segmented),
                setting(
                    "appearance.density",
                    Chilpa::mark("Track lists"),
                    Chilpa::mark("Compact rows like a spreadsheet, or rich rows with the cover."),
                    Control::Segmented,
                ),
                setting(
                    "appearance.font",
                    Chilpa::mark("Font"),
                    Chilpa::mark("Inter ships with Chilpa and has every weight the design uses. Takes effect the next time Chilpa starts."),
                    Control::Segmented,
                ),
            ],
        }],
    },
    Section { id: "navigation", label: Chilpa::mark("Navigation"), icon: "list", groups: &[] },
    Section {
        id: "library",
        label: Chilpa::mark("Library"),
        icon: "columns3",
        groups: &[
            Group {
                title: Chilpa::mark("Folders"),
                settings: &[
                    action(
                        "library.folders",
                        Chilpa::mark("Library folders"),
                        "",
                        Control::Link,
                        Chilpa::mark("Manage"),
                    ),
                    action(
                        "library.rescan",
                        Chilpa::mark("Rescan"),
                        Chilpa::mark("Reads every folder again. What did not change is not read twice."),
                        Control::Button,
                        Chilpa::mark("Rescan"),
                    ),
                ],
            },
            Group {
                title: Chilpa::mark("Maintenance"),
                settings: &[action(
                    "library.repair",
                    Chilpa::mark("Repair the library"),
                    Chilpa::mark("Rebuilds the counts and the search index from the tracks and the play log. Nothing is lost."),
                    Control::Button,
                    Chilpa::mark("Repair"),
                )],
            },
        ],
    },
    Section { id: "sources", label: Chilpa::mark("Sources"), icon: "server", groups: &[] },
    Section {
        id: "integrations",
        label: Chilpa::mark("Integrations"),
        icon: "broadcast",
        groups: &[Group {
            title: "ListenBrainz",
            settings: &[
                action(
                    "listenbrainz.account",
                    Chilpa::mark("Account"),
                    Chilpa::mark("Paste the user token from listenbrainz.org/settings."),
                    Control::Secret,
                    Chilpa::mark("Connect"),
                ),
                setting(
                    "listenbrainz.enabled",
                    Chilpa::mark("Send listens"),
                    Chilpa::mark("A track counts once half of it has played, or four minutes. Listens made offline are sent later."),
                    Control::Toggle,
                ),
            ],
        }],
    },
    Section { id: "devices", label: Chilpa::mark("Devices & cast"), icon: "cast", groups: &[] },
    Section { id: "updates", label: Chilpa::mark("Updates"), icon: "infoCircle", groups: &[] },
    Section { id: "advanced", label: Chilpa::mark("Advanced"), icon: "settings", groups: &[] },
];

pub fn section(id: &str) -> &'static Section {
    SECTIONS.iter().find(|s| s.id == id).unwrap_or(&SECTIONS[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_is_named_once() {
        let mut keys: Vec<&str> =
            SECTIONS.iter().flat_map(|s| s.groups).flat_map(|g| g.settings).map(|s| s.key).collect();
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
            let pressable = matches!(set.control, Control::Button | Control::Link);
            assert_eq!(pressable, !set.action.is_empty(), "{}", set.key);
        }
    }

    #[test]
    fn an_unknown_section_opens_the_first() {
        assert_eq!(section("nowhere").id, "audio");
    }
}
