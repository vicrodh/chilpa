//! ALSA hardware-volume discovery, identity, selection and control.
//!
//! This module is intentionally separate from `alsa_direct`: mixer probing is
//! control-plane work and must not change PCM open/configure/write/drain or any
//! sample-rate decision in the protected direct-output path.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

/// The complete ALSA simple-mixer identity. Names alone are not unique.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlsaMixerControlId {
    pub name: String,
    pub index: u32,
}

impl fmt::Display for AlsaMixerControlId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{},{}", self.name, self.index)
    }
}

/// Opaque, serializable key for one physical card and one PCM route.
///
/// The string form is deliberate: JSON maps only support string keys. Values
/// are versioned and contain a stable card identity, the PCM kind and `DEV=`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StableAlsaRouteKey(String);

impl StableAlsaRouteKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn from_parts(identity: &StableAlsaCardIdentity, pcm: &str, device: u32) -> Self {
        let (kind, value) = match identity {
            StableAlsaCardIdentity::ById(value) => ("by-id", value.as_str()),
            StableAlsaCardIdentity::ByPath(value) => ("by-path", value.as_str()),
            StableAlsaCardIdentity::Card(value) => ("card", value.as_str()),
        };
        Self(format!(
            "alsa-route-v1|card={kind}:{}|pcm={}|DEV={device}",
            escape_key_component(value),
            escape_key_component(pcm)
        ))
    }
}

impl fmt::Display for StableAlsaRouteKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StableAlsaCardIdentity {
    ById(String),
    ByPath(String),
    Card(String),
}

fn select_stable_card_identity(
    has_real_serial: bool,
    by_id: Option<String>,
    by_path: Option<String>,
    short_id: String,
) -> StableAlsaCardIdentity {
    if has_real_serial {
        if let Some(by_id) = by_id {
            return StableAlsaCardIdentity::ById(by_id);
        }
    }
    by_path
        .map(StableAlsaCardIdentity::ByPath)
        .unwrap_or(StableAlsaCardIdentity::Card(short_id))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareVolumeRange {
    pub min: i64,
    pub max: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareVolumeChannelValue {
    pub channel: String,
    pub raw: i64,
    pub db_millibels: Option<i64>,
    pub playback_switch: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareVolumeWritability {
    Verified,
    NeedsEnableCheck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum HardwareVolumeRejectionReason {
    Inactive,
    CaptureOnly,
    UnsafeInputPath,
    CommonPlaybackCaptureVolume,
    NoPlaybackVolume,
    InvalidRange { min: i64, max: i64 },
    NoPlaybackChannels,
    ReadFailed(String),
    ReadOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareVolumeCandidate {
    pub id: AlsaMixerControlId,
    pub label: String,
    pub channels: Vec<String>,
    pub raw_range: Option<HardwareVolumeRange>,
    /// ALSA millibels (1/100 dB). `None` means raw-linear fallback.
    pub db_range: Option<HardwareVolumeRange>,
    pub current_values: Vec<HardwareVolumeChannelValue>,
    pub current_volume: Option<f32>,
    pub has_playback_switch: bool,
    pub recommended: bool,
    pub writability: HardwareVolumeWritability,
    pub rejection_reason: Option<HardwareVolumeRejectionReason>,
}

impl HardwareVolumeCandidate {
    pub fn is_valid(&self) -> bool {
        self.rejection_reason.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareVolumeProbeErrorKind {
    InvalidDevice,
    PermissionDenied,
    DeviceBusy,
    DeviceUnavailable,
    NoMixer,
    UnsupportedPlatform,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareVolumeProbeError {
    pub kind: HardwareVolumeProbeErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareVolumeProbe {
    pub device_id: String,
    pub ctl_name: String,
    pub route_key: Option<StableAlsaRouteKey>,
    /// Every simple element, including rejected controls for diagnostics.
    pub candidates: Vec<HardwareVolumeCandidate>,
    /// Read-only UCM hints that resolve to simple-mixer identities.
    pub ucm_controls: Vec<AlsaMixerControlId>,
    pub error: Option<HardwareVolumeProbeError>,
}

impl HardwareVolumeProbe {
    pub fn valid_candidates(&self) -> impl Iterator<Item = &HardwareVolumeCandidate> {
        self.candidates
            .iter()
            .filter(|candidate| candidate.is_valid())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareVolumeSelectionSource {
    Persisted,
    Ucm,
    OnlyCandidate,
    UserChoice,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareVolumeSelection {
    pub route_key: StableAlsaRouteKey,
    pub control: AlsaMixerControlId,
    pub candidate: HardwareVolumeCandidate,
    pub source: HardwareVolumeSelectionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareVolumeChoiceReason {
    Ambiguous,
    UcmAmbiguous,
    PersistedSelectionStale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum HardwareVolumeUnsupportedReason {
    Probe(HardwareVolumeProbeError),
    NoValidControls,
    PersistedSelectionStale(AlsaMixerControlId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HardwareVolumeDecision {
    Selected {
        selection: HardwareVolumeSelection,
    },
    NeedsChoice {
        route_key: StableAlsaRouteKey,
        candidates: Vec<HardwareVolumeCandidate>,
        reason: HardwareVolumeChoiceReason,
    },
    Unsupported {
        route_key: Option<StableAlsaRouteKey>,
        reason: HardwareVolumeUnsupportedReason,
    },
}

/// A fresh physical-mixer snapshot after an exact read or write.
#[derive(Debug, Clone, PartialEq)]
pub struct HardwareVolumeSnapshot {
    pub control: AlsaMixerControlId,
    pub channels: Vec<HardwareVolumeChannelValue>,
    pub volume: f32,
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HardwareVolumeEvent {
    Changed(HardwareVolumeSnapshot),
    Unavailable(String),
}

pub type HardwareVolumeEventCallback = Arc<dyn Fn(HardwareVolumeEvent) + Send + Sync + 'static>;

/// Owns the ctl event thread. Dropping it stops and joins the subscription.
pub struct HardwareVolumeEventSubscription {
    #[cfg(target_os = "linux")]
    stop: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(target_os = "linux")]
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(target_os = "linux")]
impl Drop for HardwareVolumeEventSubscription {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Decide without touching hardware. A stale persisted identity is a hard
/// stop: it must never silently fall through to a different control.
pub fn decide_hardware_volume(
    probe: &HardwareVolumeProbe,
    persisted: &HashMap<StableAlsaRouteKey, AlsaMixerControlId>,
) -> HardwareVolumeDecision {
    if let Some(error) = &probe.error {
        return HardwareVolumeDecision::Unsupported {
            route_key: probe.route_key.clone(),
            reason: HardwareVolumeUnsupportedReason::Probe(error.clone()),
        };
    }

    let Some(route_key) = probe.route_key.clone() else {
        return HardwareVolumeDecision::Unsupported {
            route_key: None,
            reason: HardwareVolumeUnsupportedReason::Probe(HardwareVolumeProbeError {
                kind: HardwareVolumeProbeErrorKind::InvalidDevice,
                message: "ALSA route has no stable identity".to_string(),
            }),
        };
    };

    let valid = probe
        .valid_candidates()
        .cloned()
        .collect::<Vec<HardwareVolumeCandidate>>();

    if let Some(saved) = persisted.get(&route_key) {
        if let Some(candidate) = valid.iter().find(|candidate| candidate.id == *saved) {
            return HardwareVolumeDecision::Selected {
                selection: HardwareVolumeSelection {
                    route_key,
                    control: saved.clone(),
                    candidate: candidate.clone(),
                    source: HardwareVolumeSelectionSource::Persisted,
                },
            };
        }
        if valid.is_empty() {
            return HardwareVolumeDecision::Unsupported {
                route_key: Some(route_key),
                reason: HardwareVolumeUnsupportedReason::PersistedSelectionStale(saved.clone()),
            };
        }
        return HardwareVolumeDecision::NeedsChoice {
            route_key,
            candidates: valid,
            reason: HardwareVolumeChoiceReason::PersistedSelectionStale,
        };
    }

    let ucm_ids = probe.ucm_controls.iter().cloned().collect::<HashSet<_>>();
    let ucm_matches = valid
        .iter()
        .filter(|candidate| ucm_ids.contains(&candidate.id))
        .cloned()
        .collect::<Vec<_>>();
    if ucm_matches.len() == 1 {
        let candidate = ucm_matches.into_iter().next().expect("length checked");
        return HardwareVolumeDecision::Selected {
            selection: HardwareVolumeSelection {
                route_key,
                control: candidate.id.clone(),
                candidate,
                source: HardwareVolumeSelectionSource::Ucm,
            },
        };
    }
    if ucm_matches.len() > 1 {
        return HardwareVolumeDecision::NeedsChoice {
            route_key,
            candidates: valid,
            reason: HardwareVolumeChoiceReason::UcmAmbiguous,
        };
    }

    match valid.len() {
        0 => HardwareVolumeDecision::Unsupported {
            route_key: Some(route_key),
            reason: HardwareVolumeUnsupportedReason::NoValidControls,
        },
        1 => {
            let candidate = valid.into_iter().next().expect("length checked");
            HardwareVolumeDecision::Selected {
                selection: HardwareVolumeSelection {
                    route_key,
                    control: candidate.id.clone(),
                    candidate,
                    source: HardwareVolumeSelectionSource::OnlyCandidate,
                },
            }
        }
        _ => HardwareVolumeDecision::NeedsChoice {
            route_key,
            candidates: valid,
            reason: HardwareVolumeChoiceReason::Ambiguous,
        },
    }
}

pub fn resolve_hardware_volume(
    device_id: &str,
    persisted: &HashMap<StableAlsaRouteKey, AlsaMixerControlId>,
) -> HardwareVolumeDecision {
    decide_hardware_volume(&enumerate_hardware_volume_controls(device_id), persisted)
}

fn escape_key_component(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            escaped.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(escaped, "%{byte:02X}");
        }
    }
    escaped
}

fn hardware_volume_rank(name: &str) -> u8 {
    let lower = name.to_ascii_lowercase();
    [
        ("master", 100),
        ("pcm", 90),
        ("speaker", 80),
        ("headphone", 70),
        ("digital", 60),
        ("dac", 50),
        ("line out", 40),
        ("playback", 30),
    ]
    .into_iter()
    .find_map(|(token, score)| lower.contains(token).then_some(score))
    .unwrap_or(1)
}

fn unsafe_input_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["capture", "mic", "boost", "sidetone", "loopback"]
        .iter()
        .any(|token| lower.contains(token))
}

#[derive(Debug, Clone, Copy)]
struct CandidateFacts<'a> {
    name: &'a str,
    active: bool,
    has_playback: bool,
    has_capture: bool,
    has_common_volume: bool,
    raw_range: Option<HardwareVolumeRange>,
    playback_channels: usize,
}

/// Pure structural validation, shared by the live ALSA probe and unit tests.
/// Read failures and ctl writability are checked immediately after this gate.
fn structural_rejection(facts: CandidateFacts<'_>) -> Option<HardwareVolumeRejectionReason> {
    if !facts.active {
        return Some(HardwareVolumeRejectionReason::Inactive);
    }
    if facts.has_common_volume {
        return Some(HardwareVolumeRejectionReason::CommonPlaybackCaptureVolume);
    }
    if facts.has_capture && !facts.has_playback {
        return Some(HardwareVolumeRejectionReason::CaptureOnly);
    }
    if unsafe_input_name(facts.name) {
        return Some(HardwareVolumeRejectionReason::UnsafeInputPath);
    }
    if !facts.has_playback {
        return Some(HardwareVolumeRejectionReason::NoPlaybackVolume);
    }
    let range = facts
        .raw_range
        .unwrap_or(HardwareVolumeRange { min: 0, max: 0 });
    if range.max <= range.min {
        return Some(HardwareVolumeRejectionReason::InvalidRange {
            min: range.min,
            max: range.max,
        });
    }
    if facts.playback_channels == 0 {
        return Some(HardwareVolumeRejectionReason::NoPlaybackChannels);
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HardwareVolumeWritePlan {
    playback_switch: Option<bool>,
    write_level: bool,
}

fn hardware_volume_write_plan(scalar: f32, has_playback_switch: bool) -> HardwareVolumeWritePlan {
    if scalar <= f32::EPSILON && has_playback_switch {
        HardwareVolumeWritePlan {
            playback_switch: Some(false),
            write_level: false,
        }
    } else {
        HardwareVolumeWritePlan {
            playback_switch: has_playback_switch.then_some(true),
            write_level: true,
        }
    }
}

fn suppress_local_write_echo(written: f32, age_millis: u128, observed: f32) -> bool {
    age_millis <= 300 && (written - observed).abs() < 0.001
}

fn writability_rejection(writable: Option<bool>) -> Option<HardwareVolumeRejectionReason> {
    (writable == Some(false)).then_some(HardwareVolumeRejectionReason::ReadOnly)
}

fn mark_recommended(candidates: &mut [HardwareVolumeCandidate]) {
    let best = candidates
        .iter()
        .filter(|candidate| candidate.is_valid())
        .map(|candidate| hardware_volume_rank(&candidate.id.name))
        .max();
    for candidate in candidates {
        candidate.recommended =
            candidate.is_valid() && best == Some(hardware_volume_rank(&candidate.id.name));
    }
}

fn normalize_level(
    raw_range: HardwareVolumeRange,
    db_range: Option<HardwareVolumeRange>,
    values: &[HardwareVolumeChannelValue],
) -> f32 {
    let switches = values
        .iter()
        .filter_map(|value| value.playback_switch)
        .collect::<Vec<_>>();
    if !switches.is_empty() && switches.iter().all(|enabled| !enabled) {
        return 0.0;
    }

    if let Some(db_range) = db_range {
        let levels = values
            .iter()
            .map(|value| value.db_millibels)
            .collect::<Option<Vec<_>>>();
        if let Some(levels) = levels.filter(|levels| !levels.is_empty()) {
            return levels
                .into_iter()
                .map(|value| normalized(value, db_range))
                .fold(0.0_f32, f32::max);
        }
    }

    values
        .iter()
        .map(|value| normalized(value.raw, raw_range))
        .fold(0.0_f32, f32::max)
}

fn normalized(value: i64, range: HardwareVolumeRange) -> f32 {
    if range.max <= range.min {
        return 0.0;
    }
    ((value - range.min) as f32 / (range.max - range.min) as f32).clamp(0.0, 1.0)
}

fn channel_targets(values: &[i64], range: HardwareVolumeRange, scalar: f32) -> Vec<i64> {
    if values.is_empty() || range.max <= range.min {
        return Vec::new();
    }
    let current_peak = values.iter().copied().max().unwrap_or(range.min);
    let target_peak =
        range.min + (((range.max - range.min) as f32 * scalar.clamp(0.0, 1.0)).round() as i64);
    values
        .iter()
        .map(|value| (target_peak + (*value - current_peak)).clamp(range.min, range.max))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedAlsaRoute {
    card: String,
    pcm: String,
    device: u32,
}

fn parse_alsa_route(device_id: &str) -> Option<ParsedAlsaRoute> {
    let (prefix, arguments) = device_id.split_once(':')?;
    let pcm = match prefix {
        "hw" | "plughw" => "hw",
        "front" => "front",
        "iec958" => "iec958",
        "hdmi" => "hdmi",
        "sysdefault" => "sysdefault",
        other => other,
    }
    .to_string();

    let mut card = None;
    let mut device = None;
    for (position, argument) in arguments.split(',').enumerate() {
        let argument = argument.trim();
        if let Some(value) = argument.strip_prefix("CARD=") {
            card = Some(value.to_string());
        } else if let Some(value) = argument.strip_prefix("DEV=") {
            device = value.parse::<u32>().ok();
        } else if position == 0 && !argument.is_empty() {
            card = Some(argument.to_string());
        } else if position == 1 {
            device = argument.parse::<u32>().ok();
        }
    }

    Some(ParsedAlsaRoute {
        card: card?,
        pcm,
        device: device.unwrap_or(0),
    })
}

#[cfg(target_os = "linux")]
fn ucm_parser_identifier(value_name: &str) -> Option<&'static str> {
    match value_name {
        "PlaybackVolume" => Some("PlaybackVolume"),
        "PlaybackMasterElem" | "PlaybackMixerElem" => Some("PlaybackMixerId"),
        _ => None,
    }
}

/// Resolve a PCM id to the card ctl name accepted by ALSA mixer/control APIs.
pub fn mixer_ctl_name(device_id: &str) -> String {
    let Some(route) = parse_alsa_route(device_id) else {
        return device_id.to_string();
    };
    if route
        .card
        .chars()
        .all(|character| character.is_ascii_digit())
    {
        format!("hw:{}", route.card)
    } else {
        format!("hw:CARD={}", route.card)
    }
}

#[cfg(target_os = "linux")]
mod linux;

/// Enumerate every simple-mixer element and retain rejected candidates for
/// diagnostics. This function never writes to ALSA.
pub fn enumerate_hardware_volume_controls(device_id: &str) -> HardwareVolumeProbe {
    #[cfg(target_os = "linux")]
    {
        linux::enumerate(device_id)
    }
    #[cfg(not(target_os = "linux"))]
    {
        HardwareVolumeProbe {
            device_id: device_id.to_string(),
            ctl_name: device_id.to_string(),
            route_key: None,
            candidates: Vec::new(),
            ucm_controls: Vec::new(),
            error: Some(HardwareVolumeProbeError {
                kind: HardwareVolumeProbeErrorKind::UnsupportedPlatform,
                message: "ALSA hardware volume is only available on Linux".to_string(),
            }),
        }
    }
}

pub fn stable_alsa_route_key(device_id: &str) -> Result<StableAlsaRouteKey, String> {
    #[cfg(target_os = "linux")]
    {
        linux::stable_route_key(device_id).map_err(|error| error.message)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = device_id;
        Err("ALSA route identities are only available on Linux".to_string())
    }
}

/// Explicit enable-time validation. If ctl identity cannot prove writability,
/// this performs a per-channel writeback of the values just read.
pub fn activate_hardware_volume_control(
    device_id: &str,
    id: &AlsaMixerControlId,
) -> Result<HardwareVolumeSnapshot, String> {
    #[cfg(target_os = "linux")]
    {
        linux::activate(device_id, id)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (device_id, id);
        Err("ALSA hardware volume is only available on Linux".to_string())
    }
}

pub fn read_hardware_volume(
    device_id: &str,
    id: &AlsaMixerControlId,
) -> Result<HardwareVolumeSnapshot, String> {
    #[cfg(target_os = "linux")]
    {
        linux::read(device_id, id)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (device_id, id);
        Err("ALSA hardware volume is only available on Linux".to_string())
    }
}

pub fn set_hardware_volume(
    device_id: &str,
    id: &AlsaMixerControlId,
    volume: f32,
) -> Result<HardwareVolumeSnapshot, String> {
    #[cfg(target_os = "linux")]
    {
        linux::set(device_id, id, volume)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (device_id, id, volume);
        Err("ALSA hardware volume is only available on Linux".to_string())
    }
}

pub fn subscribe_hardware_volume_events(
    device_id: &str,
    id: &AlsaMixerControlId,
    callback: HardwareVolumeEventCallback,
) -> Result<HardwareVolumeEventSubscription, String> {
    #[cfg(target_os = "linux")]
    {
        linux::subscribe(device_id, id, callback)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (device_id, id, callback);
        Err("ALSA hardware volume is only available on Linux".to_string())
    }
}

#[cfg(test)]
mod tests;
