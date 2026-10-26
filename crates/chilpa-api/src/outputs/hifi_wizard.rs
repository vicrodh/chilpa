//! The HiFi Wizard, ported from QBZ (`dac_wizard_qt.rs`): one document with
//! everything the modal shows, and the actions that change it.
//!
//! It writes no system file and runs no command: it probes, formats text,
//! and on the test step plays music through the normal queue. The probes
//! and the test tracks are read on threads of their own; what they find
//! lands in the document and its revision moves, and the window reads it
//! again. A play the test asks for is done by the window's thread on that
//! read, since only it holds the facade.
//!
//! The steps, the welcome's checkbox, the review's three checkboxes and the
//! manual entry's disclosure are the modal's own, as in QBZ.
//!
//! The test tracks are what changes from QBZ, which played four tracks of
//! its streaming service: here each format's is the most played file of
//! the library at it, and a format the library lacks gets a generated one
//! (`chilpa_core::test_signal`).

use crate::Chilpa;
use chilpa_core::dac_setup as core;
use serde::Serialize;
use std::sync::Mutex;

/// The formats tested, in QBZ's order: bits and rate.
const FORMATS: [(u8, u32); 4] = [(16, 44_100), (24, 44_100), (24, 96_000), (24, 192_000)];

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    id: String,
    description: String,
    bus: String,
    is_default: bool,
    looks_like_dac: bool,
    checked: bool,
    rates_label: String,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigRow {
    name: String,
    node_name: String,
    pipewire_conf: String,
    pulse_conf: String,
    wireplumber_conf: String,
    expanded: bool,
}

#[derive(Clone, Default, Serialize)]
struct Remediation {
    caption: String,
    command: String,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Doc {
    open: bool,
    /// Moved on every open: the modal forgets its own state when it does.
    open_seq: u32,
    distro_options: Vec<String>,
    distro_index: i32,
    init_options: Vec<String>,
    init_index: i32,
    health_ok: bool,
    health_summary: String,
    remediations: Vec<Remediation>,
    sandboxed: bool,
    sandbox_name: String,
    candidates: Vec<Candidate>,
    detecting: bool,
    has_enumeration: bool,
    any_dac_selected: bool,
    manual_node_name: String,
    manual_valid: bool,
    manual_dac_type: String,
    test_requested_label: String,
    test_output: String,
    test_negotiated_label: String,
    test_rate_matched: bool,
    test_playing: bool,
    test_tracks: Vec<String>,
    dac_configs: Vec<ConfigRow>,
    backup_cmd: String,
    restart_cmd: String,
    created_paths: Vec<String>,
}

/// One format's test track: the library's file, or none yet.
#[derive(Clone, Default)]
struct TestTrack {
    bits: u8,
    rate: u32,
    path: Option<String>,
}

#[derive(Default)]
struct State {
    doc: Doc,
    revision: u64,
    health: Option<core::Health>,
    tracks: Vec<TestTrack>,
    /// The test's files, once worked out, so a row can jump to one.
    paths: Vec<String>,
    /// A play for the window's thread: the files and where to start.
    play: Option<(Vec<String>, usize)>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// Changes the state and moves the revision, so the window reads it again.
fn change<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let state = guard.get_or_insert_with(State::default);
    let out = f(state);
    state.revision += 1;
    out
}

fn read<R>(f: impl FnOnce(&State) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(State::default))
}

/// Work off the window's thread.
fn background(name: &str, work: impl FnOnce() + Send + 'static) {
    if let Err(error) = std::thread::Builder::new().name(format!("chilpa-wizard-{name}")).spawn(work) {
        eprintln!("wizard: {name} could not start: {error}");
    }
}

fn label(track: &TestTrack, found: Option<(&str, &str)>) -> String {
    let format = chilpa_i18n::t_args("{}-bit / {}", &[&track.bits.to_string(), &core::format_rates(&[track.rate])]);
    match found {
        Some((artist, title)) if !artist.is_empty() => format!("{format} — {artist} · {title}"),
        Some((_, title)) => format!("{format} — {title}"),
        None => format!("{format} — {}", chilpa_i18n::t("Generated test signal")),
    }
}

/// Each format's track in the library, and its line.
fn find_tracks() -> (Vec<TestTrack>, Vec<String>) {
    let library = chilpa_library::Library::open_read_only(&chilpa_library::Library::default_path()).ok();
    FORMATS
        .iter()
        .map(|&(bits, rate)| {
            let stored = library.as_ref().and_then(|lib| lib.most_played_at(bits, rate).ok().flatten());
            let track = TestTrack { bits, rate, path: stored.as_ref().map(|t| t.external_id.clone()) };
            let line = label(&track, stored.as_ref().map(|t| (t.artist.as_str(), t.title.as_str())));
            (track, line)
        })
        .unzip()
}

fn recompute_check(state: &mut State) {
    let Some(health) = state.health else { return };
    let view = core::check_view(health, state.doc.distro_index, state.doc.init_index, state.doc.sandboxed);
    state.doc.health_ok = view.health_ok;
    state.doc.health_summary = view.summary;
    state.doc.remediations = view.remediations.into_iter().map(|r| Remediation { caption: r.caption, command: r.command }).collect();
}

fn begin_test(state: &mut State) {
    state.doc.test_playing = true;
    state.doc.test_rate_matched = false;
    state.doc.test_requested_label = chilpa_i18n::t("Starting…");
    state.doc.test_negotiated_label.clear();
}

/// (node, name) of every checked candidate, else the valid manual one.
fn checked_dacs(doc: &Doc) -> Vec<(String, String)> {
    let checked: Vec<(String, String)> = doc.candidates.iter().filter(|c| c.checked).map(|c| (c.id.clone(), c.description.clone())).collect();
    if !checked.is_empty() {
        return checked;
    }
    let manual = doc.manual_node_name.trim();
    if doc.manual_valid && !manual.is_empty() {
        return vec![(manual.to_string(), manual.to_string())];
    }
    Vec::new()
}

fn index(argument: &str) -> usize {
    argument.parse::<i64>().unwrap_or(-1).max(0) as usize
}

impl Chilpa {
    /// The document, when it changed since `seen`, with its revision. A play
    /// the test asked for is done here, on the window's thread.
    pub fn wizard_doc(&mut self, seen: u64) -> Option<(u64, String)> {
        let (play, revision, json) = {
            let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
            let state = guard.get_or_insert_with(State::default);
            let play = state.play.take();
            let json = (state.revision != seen).then(|| serde_json::to_string(&state.doc).unwrap_or_else(|_| "{}".into()));
            (play, state.revision, json)
        };
        if let Some((paths, start)) = play {
            self.play_from(&paths, start);
        }
        json.map(|json| (revision, json))
    }

    /// One of the modal's actions; `argument` is an index or a text.
    pub fn wizard_command(&mut self, action: &str, argument: &str) {
        match action {
            "open" => self.wizard_open(),
            "close" => change(|s| s.doc.open = false),
            "setDistro" => change(|s| {
                s.doc.distro_index = index(argument) as i32;
                recompute_check(s);
            }),
            "setInit" => change(|s| {
                s.doc.init_index = index(argument) as i32;
                recompute_check(s);
                // The review's restart follows the init too.
                if !s.doc.restart_cmd.is_empty() {
                    s.doc.restart_cmd = core::restart_command_for(s.doc.init_index);
                }
            }),
            "runDetect" => {
                change(|s| s.doc.detecting = true);
                background("detect", || {
                    let found = core::detect_blocking();
                    change(|s| {
                        s.doc.candidates = found
                            .iter()
                            .map(|d| Candidate {
                                id: d.id.clone(),
                                description: d.description.clone(),
                                bus: d.bus.clone(),
                                is_default: d.is_default,
                                looks_like_dac: d.looks_like_dac,
                                checked: d.looks_like_dac,
                                rates_label: d.rates_label.clone(),
                            })
                            .collect();
                        s.doc.any_dac_selected = s.doc.candidates.iter().any(|c| c.checked);
                        s.doc.has_enumeration = !found.is_empty();
                        s.doc.detecting = false;
                    });
                });
            }
            "toggleDac" => change(|s| {
                if let Some(row) = s.doc.candidates.get_mut(index(argument)) {
                    row.checked = !row.checked;
                }
                s.doc.any_dac_selected = s.doc.candidates.iter().any(|c| c.checked);
            }),
            "validateManual" => change(|s| {
                s.doc.manual_node_name = argument.to_string();
                s.doc.manual_valid = core::validate_node_name(argument);
                s.doc.manual_dac_type = core::detect_dac_type(argument).to_string();
            }),
            "genConfigs" => {
                let dacs = read(|s| checked_dacs(&s.doc));
                background("configs", move || {
                    let configs = core::gen_configs_blocking(dacs);
                    change(|s| {
                        let single = configs.len() == 1;
                        s.doc.created_paths = core::created_paths(&configs);
                        s.doc.dac_configs = configs
                            .into_iter()
                            .map(|d| ConfigRow { name: d.name, node_name: d.node_name, pipewire_conf: d.pipewire_conf, pulse_conf: d.pulse_conf, wireplumber_conf: d.wireplumber_conf, expanded: single })
                            .collect();
                        s.doc.backup_cmd = core::BACKUP_CMD.to_string();
                        s.doc.restart_cmd = core::restart_command_for(s.doc.init_index);
                    });
                });
            }
            "toggleConfig" => change(|s| {
                if let Some(row) = s.doc.dac_configs.get_mut(index(argument)) {
                    row.expanded = !row.expanded;
                }
            }),
            "startTest" => {
                let tracks = change(|s| {
                    begin_test(s);
                    s.tracks.clone()
                });
                background("test", move || {
                    let paths: Vec<String> = tracks
                        .iter()
                        .filter_map(|t| {
                            t.path.clone().or_else(|| {
                                chilpa_core::test_signal::ensure(u16::from(t.bits), t.rate)
                                    .map_err(|e| eprintln!("wizard: the {}/{} test signal could not be written: {e}", t.bits, t.rate))
                                    .ok()
                                    .map(|p| p.to_string_lossy().into_owned())
                            })
                        })
                        .collect();
                    change(|s| {
                        if paths.is_empty() {
                            s.doc.test_playing = false;
                            s.doc.test_requested_label = chilpa_i18n::t("Couldn't load the test tracks");
                        } else {
                            s.paths = paths.clone();
                            s.play = Some((paths, 0));
                        }
                    });
                });
            }
            "testPlayIndex" => change(|s| {
                if !s.paths.is_empty() {
                    let start = index(argument).min(s.paths.len() - 1);
                    s.play = Some((s.paths.clone(), start));
                }
            }),
            "stopTest" => {
                // A pause, as in QBZ: the queue stays as it is.
                self.set_paused(true);
                change(|s| s.doc.test_playing = false);
            }
            "verifyOwn" => {
                if self.queue_paths().is_empty() {
                    change(|s| {
                        s.doc.test_playing = false;
                        s.doc.test_rate_matched = false;
                        s.doc.test_negotiated_label.clear();
                        s.doc.test_requested_label = chilpa_i18n::t("Your queue is empty — add some tracks first, or press Play test.");
                    });
                } else {
                    self.set_paused(false);
                    change(begin_test);
                }
            }
            "pollTest" => {
                // What Chilpa asked the device for, against what the card runs at.
                let now = self.now_playing();
                let (rate, bits) = if now.playing { (self.sample_rate, now.quality.bits.map_or(0, u32::from)) } else { (0, 0) };
                let output = self.audio_output.route.as_ref().filter(|_|now.playing).map(|r|r.device.clone()).unwrap_or_default();
                change(|s| s.doc.test_output = output.clone());
                background("poll", move || {
                    let view = core::poll_view(rate, bits, core::negotiated_output(&output));
                    change(|s| {
                        if !s.doc.open || s.doc.test_output != output { return; }
                        s.doc.test_requested_label = view.requested_label;
                        s.doc.test_negotiated_label = view.negotiated_label;
                        s.doc.test_rate_matched = view.rate_matched;
                    });
                });
            }
            _ => eprintln!("wizard: unknown action {action}"),
        }
    }

    /// The Settings row's button.
    pub(crate) fn wizard_maintenance(&mut self, key: &str) -> Option<String> {
        (key == super::HIFI_WIZARD).then(|| {
            self.wizard_open();
            String::new()
        })
    }

    /// Settings > Audio > "Open Wizard": the whole wizard anew, the dropdowns
    /// from what is detected, and the stack probed off the window's thread.
    fn wizard_open(&mut self) {
        let sandbox = core::sandbox_name();
        change(|s| {
            let seq = s.doc.open_seq.wrapping_add(1);
            *s = State {
                revision: s.revision,
                doc: Doc {
                    open: true,
                    open_seq: seq,
                    distro_options: core::distro_options(),
                    distro_index: core::detected_distro_index(),
                    init_options: core::init_options(),
                    init_index: core::detected_init_index(),
                    sandboxed: !sandbox.is_empty(),
                    sandbox_name: sandbox.to_string(),
                    health_summary: chilpa_i18n::t("Checking your audio stack…"),
                    has_enumeration: true,
                    ..Doc::default()
                },
                ..State::default()
            };
        });
        background("open", || {
            let health = core::probe_health();
            let (tracks, lines) = find_tracks();
            change(|s| {
                s.health = Some(health);
                recompute_check(s);
                s.tracks = tracks;
                s.doc.test_tracks = lines;
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checked_candidates_come_first_then_a_valid_manual_name() {
        let mut doc = Doc { candidates: vec![Candidate { id: "a".into(), description: "A".into(), checked: true, ..Candidate::default() }, Candidate { id: "b".into(), ..Candidate::default() }], ..Doc::default() };
        doc.manual_node_name = "alsa_output.usb-x".into();
        doc.manual_valid = true;
        assert_eq!(checked_dacs(&doc), vec![("a".to_string(), "A".to_string())]);
        doc.candidates[0].checked = false;
        assert_eq!(checked_dacs(&doc), vec![("alsa_output.usb-x".to_string(), "alsa_output.usb-x".to_string())]);
        doc.manual_valid = false;
        assert!(checked_dacs(&doc).is_empty());
    }

    #[test]
    fn a_format_s_line_names_its_track_or_says_it_is_generated() {
        let track = TestTrack { bits: 24, rate: 96_000, path: None };
        assert_eq!(label(&track, Some(("Band", "Song"))), "24-bit / 96 kHz — Band · Song");
        assert_eq!(label(&track, None), "24-bit / 96 kHz — Generated test signal");
        let cd = TestTrack { bits: 16, rate: 44_100, path: None };
        assert!(label(&cd, Some(("", "Song"))).starts_with("16-bit / 44.1 kHz — Song"));
    }

    #[test]
    fn the_document_reads_as_the_modal_expects() {
        let doc = Doc { open: true, open_seq: 2, any_dac_selected: true, ..Doc::default() };
        let json: serde_json::Value = serde_json::to_value(&doc).unwrap();
        for key in ["open", "openSeq", "distroOptions", "healthOk", "candidates", "anyDacSelected", "manualValid", "testPlaying", "testTracks", "dacConfigs", "backupCmd", "restartCmd", "createdPaths"] {
            assert!(json.get(key).is_some(), "{key}");
        }
    }
}
