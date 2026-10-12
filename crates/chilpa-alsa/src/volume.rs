//! The playback control selected by the ALSA capability probe.
//!
//! A name alone is not a mixer identity. The probe rejects capture and
//! shared input controls, checks range and writability, and only selects a
//! single valid control or an unambiguous UCM hint. Ambiguous cards keep the
//! player slider locked until a control is explicitly chosen.

use crate::hardware_volume::{self as probe, AlsaMixerControlId, HardwareVolumeDecision, StableAlsaRouteKey};
use alsa::mixer::{Mixer, SelemId};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

type Selections = HashMap<StableAlsaRouteKey, AlsaMixerControlId>;

struct ControlState {
    selections: Selections,
    decisions: HashMap<String, (Instant, HardwareVolumeDecision)>,
}

fn selection_path() -> std::path::PathBuf {
    chilpa_dirs::config().join("alsa-hardware-volume.json")
}

fn state() -> &'static Mutex<ControlState> {
    static STATE: OnceLock<Mutex<ControlState>> = OnceLock::new();
    STATE.get_or_init(|| {
        let selections = std::fs::read(selection_path()).ok()
            .filter(|bytes| bytes.len() <= 64 * 1024)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        Mutex::new(ControlState { selections, decisions: HashMap::new() })
    })
}

fn decision(device: &str) -> HardwareVolumeDecision {
    let mut state = state().lock().unwrap_or_else(|error| error.into_inner());
    if let Some((time, decision)) = state.decisions.get(device) {
        if time.elapsed() < Duration::from_secs(2) { return decision.clone(); }
    }
    let decision = probe::resolve_hardware_volume(device, &state.selections);
    state.decisions.insert(device.into(), (Instant::now(), decision.clone()));
    decision
}

#[derive(Clone, Debug)]
pub struct ControlChoice {
    pub id: String,
    pub label: String,
    pub selected: bool,
}

pub fn choices(device: &str) -> Vec<ControlChoice> {
    let selected = match decision(device) {
        HardwareVolumeDecision::Selected { selection } => Some(selection.control),
        _ => None,
    };
    probe::enumerate_hardware_volume_controls(device).valid_candidates()
        .map(|candidate| ControlChoice {
            id: serde_json::to_string(&candidate.id).unwrap_or_default(),
            label: format!("{},{}", candidate.id.name, candidate.id.index),
            selected: selected.as_ref() == Some(&candidate.id),
        }).collect()
}

pub fn select(device: &str, id: &str) -> Result<(), String> {
    let chosen: AlsaMixerControlId = serde_json::from_str(id).map_err(|_| "Choose a listed ALSA playback control.")?;
    let snapshot = probe::enumerate_hardware_volume_controls(device);
    if !snapshot.valid_candidates().any(|candidate| candidate.id == chosen) {
        return Err("This ALSA playback control is no longer available.".into());
    }
    let route = snapshot.route_key.ok_or("This ALSA route has no stable identity.")?;
    // Perform a readback write before enabling a control whose ctl
    // writability could not be established without touching hardware.
    probe::activate_hardware_volume_control(device, &chosen)?;
    let mut state = state().lock().unwrap_or_else(|error| error.into_inner());
    let mut updated = state.selections.clone();
    updated.insert(route, chosen);
    let bytes = serde_json::to_vec_pretty(&updated).map_err(|_| "The ALSA control could not be saved.")?;
    let path = selection_path();
    let parent = path.parent().ok_or("The ALSA control path is invalid.")?;
    std::fs::create_dir_all(parent).map_err(|_| "The ALSA control directory could not be created.")?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = std::fs::File::create(&temporary).map_err(|_| "The ALSA control could not be saved.")?;
    use std::io::Write;
    file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|_| "The ALSA control could not be saved.")?;
    std::fs::rename(&temporary, &path).map_err(|_| "The ALSA control could not be saved.")?;
    state.selections = updated;
    state.decisions.remove(device);
    Ok(())
}

/// Finish the probe only when the user enables this control. Check
/// uncertain ctl writability by writing each channel's current value back.
pub fn activate(device: &str) -> Result<(), String> {
    let HardwareVolumeDecision::Selected { selection } = decision(device) else {
        return Err("Choose a valid ALSA hardware volume control first.".into());
    };
    probe::activate_hardware_volume_control(device, &selection.control).map(|_| ())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Level {
    pub percent: u8,
    pub muted: bool,
}

pub struct Volume {
    device: String,
    control: AlsaMixerControlId,
    ctl_name: String,
    can_mute: bool,
}

impl Volume {
    pub fn of(device: &str) -> Option<Self> {
        // No implicit fallback to another mixer element when a route is
        // ambiguous or a persisted selection goes stale.
        let HardwareVolumeDecision::Selected { selection } = decision(device) else { return None };
        Some(Self {
            device: device.into(),
            ctl_name: probe::mixer_ctl_name(device),
            can_mute: selection.candidate.has_playback_switch,
            control: selection.control,
        })
    }

    pub fn element(&self) -> &str { &self.control.name }

    pub fn read(&self) -> Result<Level, String> {
        let snapshot = probe::read_hardware_volume(&self.device, &self.control)?;
        Ok(Level {
            percent: (snapshot.volume.clamp(0.0, 1.0) * 100.0).round() as u8,
            muted: snapshot.muted,
        })
    }

    pub fn set(&self, percent: u8) -> Result<(), String> {
        probe::set_hardware_volume(&self.device, &self.control, f32::from(percent.min(100)) / 100.0).map(|_| ())
    }

    pub fn mute(&self, on: bool) -> Result<(), String> {
        if !self.can_mute { return Err(chilpa_i18n::t("This output has no hardware mute switch.")); }
        let mixer = Mixer::new(&self.ctl_name, false).map_err(|error| error.to_string())?;
        let element = mixer.find_selem(&SelemId::new(&self.control.name, self.control.index))
            .ok_or_else(|| chilpa_i18n::t("The selected hardware volume control is gone."))?;
        element.set_playback_switch_all(i32::from(!on)).map_err(|error| error.to_string())
    }

    pub fn can_mute(&self) -> bool { self.can_mute }
}
