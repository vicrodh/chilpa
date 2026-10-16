use super::*;
use alsa::ctl::Ctl;
use alsa_sys as ffi;
use std::ffi::{CStr, CString};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const DB_GAIN_MUTE: i64 = -9_999_999;
const CHANNELS: &[(ffi::snd_mixer_selem_channel_id_t, &str)] = &[
    (ffi::SND_MIXER_SCHN_FRONT_LEFT, "Front Left"),
    (ffi::SND_MIXER_SCHN_FRONT_RIGHT, "Front Right"),
    (ffi::SND_MIXER_SCHN_REAR_LEFT, "Rear Left"),
    (ffi::SND_MIXER_SCHN_REAR_RIGHT, "Rear Right"),
    (ffi::SND_MIXER_SCHN_FRONT_CENTER, "Front Center"),
    (ffi::SND_MIXER_SCHN_WOOFER, "Woofer"),
    (ffi::SND_MIXER_SCHN_SIDE_LEFT, "Side Left"),
    (ffi::SND_MIXER_SCHN_SIDE_RIGHT, "Side Right"),
    (ffi::SND_MIXER_SCHN_REAR_CENTER, "Rear Center"),
];

#[cfg(target_pointer_width = "64")]
fn c_long_to_i64(value: libc::c_long) -> i64 {
    value
}

#[cfg(not(target_pointer_width = "64"))]
fn c_long_to_i64(value: libc::c_long) -> i64 {
    i64::from(value)
}

#[repr(C)]
struct SndUseCaseMgr {
    _private: [u8; 0],
}

#[link(name = "asound")]
unsafe extern "C" {
    fn snd_use_case_mgr_open(
        manager: *mut *mut SndUseCaseMgr,
        card_name: *const libc::c_char,
    ) -> libc::c_int;
    fn snd_use_case_mgr_close(manager: *mut SndUseCaseMgr) -> libc::c_int;
    fn snd_use_case_get(
        manager: *mut SndUseCaseMgr,
        identifier: *const libc::c_char,
        value: *mut *const libc::c_char,
    ) -> libc::c_int;
    fn snd_use_case_get_list(
        manager: *mut SndUseCaseMgr,
        identifier: *const libc::c_char,
        list: *mut *mut *const libc::c_char,
    ) -> libc::c_int;
    fn snd_use_case_free_list(
        list: *const *const libc::c_char,
        items: libc::c_int,
    ) -> libc::c_int;
    fn snd_use_case_parse_selem_id(
        destination: *mut ffi::snd_mixer_selem_id_t,
        ucm_id: *const libc::c_char,
        value: *const libc::c_char,
    ) -> libc::c_int;
    fn snd_use_case_parse_ctl_elem_id(
        destination: *mut ffi::snd_ctl_elem_id_t,
        ucm_id: *const libc::c_char,
        value: *const libc::c_char,
    ) -> libc::c_int;
}

struct UcmManager(*mut SndUseCaseMgr);

impl UcmManager {
    fn open(card: &str) -> Option<Self> {
        let card = CString::new(format!("hw:{card}")).ok()?;
        let mut manager = ptr::null_mut();
        let result = unsafe { snd_use_case_mgr_open(&mut manager, card.as_ptr()) };
        // Do not express this with bool::then_some(Self(manager)):
        // then_some eagerly constructs and drops the owner on failure,
        // calling snd_use_case_mgr_close on an invalid pointer.
        if result < 0 || manager.is_null() {
            return None;
        }
        Some(Self(manager))
    }

    fn get(&self, identifier: &str) -> Option<String> {
        let identifier = CString::new(identifier).ok()?;
        let mut value = ptr::null();
        if unsafe { snd_use_case_get(self.0, identifier.as_ptr(), &mut value) } < 0
            || value.is_null()
        {
            return None;
        }
        let result = unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned();
        unsafe {
            libc::free(value.cast_mut().cast());
        }
        (!result.is_empty()).then_some(result)
    }

    fn list(&self, identifier: &str) -> Vec<String> {
        let Ok(identifier) = CString::new(identifier) else {
            return Vec::new();
        };
        let mut list = ptr::null_mut();
        let count = unsafe { snd_use_case_get_list(self.0, identifier.as_ptr(), &mut list) };
        if count <= 0 || list.is_null() {
            return Vec::new();
        }
        let values = (0..count)
            .filter_map(|index| {
                let value = unsafe { *list.add(index as usize) };
                (!value.is_null()).then(|| {
                    unsafe { CStr::from_ptr(value) }
                        .to_string_lossy()
                        .into_owned()
                })
            })
            .collect();
        unsafe {
            snd_use_case_free_list(list.cast_const(), count);
        }
        values
    }
}

impl Drop for UcmManager {
    fn drop(&mut self) {
        unsafe {
            snd_use_case_mgr_close(self.0);
        }
    }
}

struct RawMixer(*mut ffi::snd_mixer_t);

impl RawMixer {
    fn open(device_id: &str) -> Result<Self, HardwareVolumeProbeError> {
        let ctl_name = mixer_ctl_name(device_id);
        let ctl = CString::new(ctl_name.as_str()).map_err(|_| HardwareVolumeProbeError {
            kind: HardwareVolumeProbeErrorKind::InvalidDevice,
            message: format!("Invalid ALSA ctl name for {device_id}"),
        })?;
        let mut mixer = ptr::null_mut();
        alsa_call(
            unsafe { ffi::snd_mixer_open(&mut mixer, 0) },
            "open ALSA mixer",
        )?;
        let owner = Self(mixer);
        alsa_call(
            unsafe { ffi::snd_mixer_attach(owner.0, ctl.as_ptr()) },
            &format!("attach ALSA mixer to {ctl_name}"),
        )?;
        alsa_call(
            unsafe { ffi::snd_mixer_selem_register(owner.0, ptr::null_mut(), ptr::null_mut()) },
            "register ALSA simple mixer elements",
        )?;
        alsa_call(unsafe { ffi::snd_mixer_load(owner.0) }, "load ALSA mixer")?;
        Ok(owner)
    }

    fn find(&self, id: &AlsaMixerControlId) -> Option<*mut ffi::snd_mixer_elem_t> {
        let mut element = unsafe { ffi::snd_mixer_first_elem(self.0) };
        while !element.is_null() {
            if unsafe { ffi::snd_mixer_elem_get_type(element) } == ffi::SND_MIXER_ELEM_SIMPLE {
                let candidate = selem_id(element);
                if candidate.as_ref() == Some(id) {
                    return Some(element);
                }
            }
            element = unsafe { ffi::snd_mixer_elem_next(element) };
        }
        None
    }
}

impl Drop for RawMixer {
    fn drop(&mut self) {
        unsafe {
            ffi::snd_mixer_close(self.0);
        }
    }
}

struct RawCtl(*mut ffi::snd_ctl_t);

impl RawCtl {
    fn open(ctl_name: &str, nonblock: bool) -> Result<Self, HardwareVolumeProbeError> {
        let name = CString::new(ctl_name).map_err(|_| HardwareVolumeProbeError {
            kind: HardwareVolumeProbeErrorKind::InvalidDevice,
            message: format!("Invalid ALSA ctl name: {ctl_name}"),
        })?;
        let mut ctl = ptr::null_mut();
        alsa_call(
            unsafe { ffi::snd_ctl_open(&mut ctl, name.as_ptr(), i32::from(nonblock)) },
            &format!("open ALSA ctl {ctl_name}"),
        )?;
        Ok(Self(ctl))
    }

    /// `None` means the Selem -> ctl translation was not reliable enough;
    /// the explicit enable path will do a per-channel current-value writeback.
    fn writable(&self, id: &AlsaMixerControlId) -> Option<bool> {
        let raw_names = if id.name.ends_with(" Playback Volume") {
            vec![id.name.clone()]
        } else {
            vec![format!("{} Playback Volume", id.name), id.name.clone()]
        };
        for raw_name in raw_names {
            let Ok(raw_name) = CString::new(raw_name) else {
                continue;
            };
            let mut info = ptr::null_mut();
            if unsafe { ffi::snd_ctl_elem_info_malloc(&mut info) } < 0 {
                return None;
            }
            unsafe {
                ffi::snd_ctl_elem_info_set_interface(info, ffi::SND_CTL_ELEM_IFACE_MIXER);
                ffi::snd_ctl_elem_info_set_name(info, raw_name.as_ptr());
                ffi::snd_ctl_elem_info_set_index(info, id.index);
            }
            let result = unsafe { ffi::snd_ctl_elem_info(self.0, info) };
            let writable =
                (result >= 0).then(|| unsafe { ffi::snd_ctl_elem_info_is_writable(info) > 0 });
            unsafe {
                ffi::snd_ctl_elem_info_free(info);
            }
            if writable.is_some() {
                return writable;
            }
        }
        None
    }
}

impl Drop for RawCtl {
    fn drop(&mut self) {
        unsafe {
            ffi::snd_ctl_close(self.0);
        }
    }
}

fn alsa_call(code: i32, operation: &str) -> Result<(), HardwareVolumeProbeError> {
    if code >= 0 {
        return Ok(());
    }
    let errno = -code;
    let detail = unsafe {
        let message = ffi::snd_strerror(code);
        if message.is_null() {
            format!("ALSA error {code}")
        } else {
            CStr::from_ptr(message).to_string_lossy().into_owned()
        }
    };
    let kind = match errno {
        libc::EACCES | libc::EPERM => HardwareVolumeProbeErrorKind::PermissionDenied,
        libc::EBUSY => HardwareVolumeProbeErrorKind::DeviceBusy,
        libc::ENODEV | libc::ENXIO => HardwareVolumeProbeErrorKind::DeviceUnavailable,
        libc::ENOENT => HardwareVolumeProbeErrorKind::NoMixer,
        _ => HardwareVolumeProbeErrorKind::Other,
    };
    Err(HardwareVolumeProbeError {
        kind,
        message: format!("Failed to {operation}: {detail}"),
    })
}

fn selem_id(element: *mut ffi::snd_mixer_elem_t) -> Option<AlsaMixerControlId> {
    let name = unsafe { ffi::snd_mixer_selem_get_name(element) };
    if name.is_null() {
        return None;
    }
    Some(AlsaMixerControlId {
        name: unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned(),
        index: unsafe { ffi::snd_mixer_selem_get_index(element) },
    })
}

fn playback_channels(element: *mut ffi::snd_mixer_elem_t) -> Vec<(i32, String)> {
    CHANNELS
        .iter()
        .filter(|(channel, _)| unsafe {
            ffi::snd_mixer_selem_has_playback_channel(element, *channel) > 0
        })
        .map(|(channel, name)| (*channel, (*name).to_string()))
        .collect()
}

fn finite_db_range(
    element: *mut ffi::snd_mixer_elem_t,
    raw_range: HardwareVolumeRange,
) -> Option<HardwareVolumeRange> {
    let mut min = 0;
    let mut max = 0;
    if unsafe { ffi::snd_mixer_selem_get_playback_dB_range(element, &mut min, &mut max) } < 0
        || max <= min
    {
        return None;
    }
    let mut min = c_long_to_i64(min);
    let max = c_long_to_i64(max);
    if min <= DB_GAIN_MUTE {
        if raw_range.max <= raw_range.min {
            return None;
        }
        let mut finite = 0;
        if unsafe {
            ffi::snd_mixer_selem_ask_playback_vol_dB(
                element,
                (raw_range.min + 1) as _,
                &mut finite,
            )
        } < 0
            || c_long_to_i64(finite) <= DB_GAIN_MUTE
        {
            return None;
        }
        min = c_long_to_i64(finite);
    }
    (max > min).then_some(HardwareVolumeRange { min, max })
}

fn read_channels(
    element: *mut ffi::snd_mixer_elem_t,
    channels: &[(i32, String)],
    db_range: Option<HardwareVolumeRange>,
    has_switch: bool,
) -> Result<Vec<HardwareVolumeChannelValue>, String> {
    let mut values = Vec::with_capacity(channels.len());
    for (channel, name) in channels {
        let mut raw = 0;
        let code =
            unsafe { ffi::snd_mixer_selem_get_playback_volume(element, *channel, &mut raw) };
        if code < 0 {
            return Err(format!("{}: {}", name, alsa_error_text(code)));
        }
        let db_millibels = db_range.and_then(|_| {
            let mut value = 0;
            (unsafe { ffi::snd_mixer_selem_get_playback_dB(element, *channel, &mut value) }
                >= 0)
                .then_some(c_long_to_i64(value))
        });
        let playback_switch = has_switch.then(|| {
            let mut enabled = 0;
            let code = unsafe {
                ffi::snd_mixer_selem_get_playback_switch(element, *channel, &mut enabled)
            };
            (code >= 0).then_some(enabled != 0)
        });
        if has_switch && playback_switch.flatten().is_none() {
            return Err(format!("{}: playback switch read failed", name));
        }
        values.push(HardwareVolumeChannelValue {
            channel: name.clone(),
            raw: c_long_to_i64(raw),
            db_millibels,
            playback_switch: playback_switch.flatten(),
        });
    }
    Ok(values)
}

fn alsa_error_text(code: i32) -> String {
    unsafe {
        let message = ffi::snd_strerror(code);
        if message.is_null() {
            format!("ALSA error {code}")
        } else {
            CStr::from_ptr(message).to_string_lossy().into_owned()
        }
    }
}

fn probe_candidate(
    element: *mut ffi::snd_mixer_elem_t,
    ctl: Option<&RawCtl>,
) -> Option<HardwareVolumeCandidate> {
    let id = selem_id(element)?;
    let mut candidate = HardwareVolumeCandidate {
        label: if id.index == 0 {
            id.name.clone()
        } else {
            format!("{} ({})", id.name, id.index)
        },
        id,
        channels: Vec::new(),
        raw_range: None,
        db_range: None,
        current_values: Vec::new(),
        current_volume: None,
        has_playback_switch: unsafe { ffi::snd_mixer_selem_has_playback_switch(element) > 0 },
        recommended: false,
        writability: HardwareVolumeWritability::NeedsEnableCheck,
        rejection_reason: None,
    };

    let active = unsafe { ffi::snd_mixer_selem_is_active(element) } > 0;
    let has_playback = unsafe { ffi::snd_mixer_selem_has_playback_volume(element) } > 0;
    let has_capture = unsafe { ffi::snd_mixer_selem_has_capture_volume(element) } > 0;
    let has_common_volume = unsafe { ffi::snd_mixer_selem_has_common_volume(element) } > 0;

    let mut min = 0;
    let mut max = 0;
    let raw_range = has_playback.then(|| {
        let _ = unsafe {
            ffi::snd_mixer_selem_get_playback_volume_range(element, &mut min, &mut max)
        };
        HardwareVolumeRange {
            min: c_long_to_i64(min),
            max: c_long_to_i64(max),
        }
    });
    candidate.raw_range = raw_range;
    let channels = if has_playback {
        playback_channels(element)
    } else {
        Vec::new()
    };
    candidate.channels = channels.iter().map(|(_, name)| name.clone()).collect();
    if let Some(reason) = structural_rejection(CandidateFacts {
        name: &candidate.id.name,
        active,
        has_playback,
        has_capture,
        has_common_volume,
        raw_range,
        playback_channels: channels.len(),
    }) {
        candidate.rejection_reason = Some(reason);
        return Some(candidate);
    }
    let raw_range = raw_range.expect("structural validation requires a valid range");
    candidate.db_range = finite_db_range(element, raw_range);
    match read_channels(
        element,
        &channels,
        candidate.db_range,
        candidate.has_playback_switch,
    ) {
        Ok(values) => candidate.current_values = values,
        Err(error) => {
            candidate.rejection_reason = Some(HardwareVolumeRejectionReason::ReadFailed(error));
            return Some(candidate);
        }
    }

    let writable = ctl.and_then(|ctl| ctl.writable(&candidate.id));
    if let Some(reason) = writability_rejection(writable) {
        candidate.rejection_reason = Some(reason);
        return Some(candidate);
    }
    if writable == Some(true) {
        candidate.writability = HardwareVolumeWritability::Verified;
    }
    candidate.current_volume = Some(normalize_level(
        raw_range,
        candidate.db_range,
        &candidate.current_values,
    ));
    Some(candidate)
}

fn ucm_identifier(value: &str, device: Option<&str>) -> String {
    match device {
        Some(device) => format!("={value}/{device}/"),
        None => format!("={value}//"),
    }
}

fn ucm_route_matches(
    manager: &UcmManager,
    route: &ParsedAlsaRoute,
    device: Option<&str>,
) -> bool {
    let identifier = ucm_identifier("PlaybackPCM", device);
    let Some(pcm) = manager.get(&identifier) else {
        return false;
    };
    parse_alsa_route(&pcm)
        .is_some_and(|ucm_route| ucm_route.device == route.device && ucm_route.pcm == route.pcm)
}

fn parse_ucm_control(
    value_name: &str,
    value: &str,
    candidates: &[HardwareVolumeCandidate],
) -> Option<AlsaMixerControlId> {
    // The parser API takes a canonical UCM value kind, not the
    // `snd_use_case_get()` path used to retrieve it. ALSA names the
    // simple-element parser kind `PlaybackMixerId` even when the value
    // originated in PlaybackMasterElem or PlaybackMixerElem.
    let parser_id = CString::new(ucm_parser_identifier(value_name)?).ok()?;
    let value_c = CString::new(value).ok()?;
    let parsed = if value_name == "PlaybackVolume" {
        let mut raw_id = ptr::null_mut();
        if unsafe { ffi::snd_ctl_elem_id_malloc(&mut raw_id) } < 0 || raw_id.is_null() {
            return None;
        }
        let code = unsafe {
            snd_use_case_parse_ctl_elem_id(raw_id, parser_id.as_ptr(), value_c.as_ptr())
        };
        let result = (code >= 0).then(|| {
            let name = unsafe { ffi::snd_ctl_elem_id_get_name(raw_id) };
            AlsaMixerControlId {
                name: if name.is_null() {
                    String::new()
                } else {
                    unsafe { CStr::from_ptr(name) }
                        .to_string_lossy()
                        .into_owned()
                },
                index: unsafe { ffi::snd_ctl_elem_id_get_index(raw_id) },
            }
        });
        unsafe {
            ffi::snd_ctl_elem_id_free(raw_id);
        }
        result
    } else {
        let mut selem_id = ptr::null_mut();
        if unsafe { ffi::snd_mixer_selem_id_malloc(&mut selem_id) } < 0 || selem_id.is_null() {
            return None;
        }
        let code = unsafe {
            snd_use_case_parse_selem_id(selem_id, parser_id.as_ptr(), value_c.as_ptr())
        };
        let result = (code >= 0).then(|| {
            let name = unsafe { ffi::snd_mixer_selem_id_get_name(selem_id) };
            AlsaMixerControlId {
                name: if name.is_null() {
                    String::new()
                } else {
                    unsafe { CStr::from_ptr(name) }
                        .to_string_lossy()
                        .into_owned()
                },
                index: unsafe { ffi::snd_mixer_selem_id_get_index(selem_id) },
            }
        });
        unsafe {
            ffi::snd_mixer_selem_id_free(selem_id);
        }
        result
    }
    .or_else(|| parse_ucm_control_text(value))?;

    candidates
        .iter()
        .find(|candidate| candidate.id == parsed)
        .map(|candidate| candidate.id.clone())
        .or_else(|| {
            let selem_name = parsed
                .name
                .strip_suffix(" Playback Volume")
                .or_else(|| parsed.name.strip_suffix(" Volume"))?;
            candidates
                .iter()
                .find(|candidate| {
                    candidate.id.index == parsed.index && candidate.id.name == selem_name
                })
                .map(|candidate| candidate.id.clone())
        })
}

fn parse_ucm_control_text(value: &str) -> Option<AlsaMixerControlId> {
    let trimmed = value.trim();
    let name = if let Some(position) = trimmed.find("name=") {
        let rest = trimmed[position + 5..].trim_start();
        quoted_or_token(rest)?
    } else {
        quoted_or_token(trimmed)?
    };
    let index = trimmed
        .find("index=")
        .and_then(|position| {
            trimmed[position + 6..]
                .trim_start()
                .split(|character: char| !character.is_ascii_digit())
                .next()
        })
        .and_then(|value| value.parse::<u32>().ok())
        .or_else(|| {
            trimmed
                .rsplit_once(',')
                .and_then(|(_, value)| value.trim().parse::<u32>().ok())
        })
        .unwrap_or(0);
    Some(AlsaMixerControlId { name, index })
}

fn quoted_or_token(value: &str) -> Option<String> {
    let value = value.trim_start();
    let first = value.chars().next()?;
    if first == '\'' || first == '"' {
        let rest = &value[first.len_utf8()..];
        let end = rest.find(first)?;
        return Some(rest[..end].to_string());
    }
    let end = value
        .find(|character: char| character == ',' || character.is_whitespace())
        .unwrap_or(value.len());
    (end > 0).then(|| value[..end].to_string())
}

fn ucm_controls_for_route(
    route: &ParsedAlsaRoute,
    candidates: &[HardwareVolumeCandidate],
) -> Vec<AlsaMixerControlId> {
    let Some(manager) = UcmManager::open(&route.card) else {
        return Vec::new();
    };
    let Some(verb) = manager.get("_verb") else {
        return Vec::new();
    };
    if verb.eq_ignore_ascii_case("Inactive") {
        return Vec::new();
    }

    let enabled_devices = manager.list("_enadevs");
    let mut contexts = Vec::new();
    if ucm_route_matches(&manager, route, None) {
        contexts.push(None);
    }
    contexts.extend(
        enabled_devices
            .iter()
            .filter(|device| ucm_route_matches(&manager, route, Some(device)))
            .map(|device| Some(device.as_str())),
    );
    if contexts.is_empty() {
        return Vec::new();
    }

    for value_name in ["PlaybackMasterElem", "PlaybackMixerElem", "PlaybackVolume"] {
        let mut controls = HashSet::new();
        for device in &contexts {
            if value_name == "PlaybackMasterElem" {
                let master_type = manager
                    .get(&ucm_identifier("PlaybackMasterType", *device))
                    .or_else(|| {
                        device
                            .is_some()
                            .then(|| manager.get(&ucm_identifier("PlaybackMasterType", None)))
                            .flatten()
                    });
                if master_type.is_some_and(|value| value.eq_ignore_ascii_case("soft")) {
                    continue;
                }
            }
            let identifier = ucm_identifier(value_name, *device);
            let Some(value) = manager.get(&identifier) else {
                continue;
            };
            if let Some(control) = parse_ucm_control(value_name, &value, candidates) {
                controls.insert(control);
            }
        }
        if !controls.is_empty() {
            let mut controls = controls.into_iter().collect::<Vec<_>>();
            controls.sort_by(|left, right| {
                left.name
                    .cmp(&right.name)
                    .then_with(|| left.index.cmp(&right.index))
            });
            return controls;
        }
    }
    Vec::new()
}

pub(super) fn stable_route_key(
    device_id: &str,
) -> Result<StableAlsaRouteKey, HardwareVolumeProbeError> {
    let route = parse_alsa_route(device_id).ok_or_else(|| HardwareVolumeProbeError {
        kind: HardwareVolumeProbeErrorKind::InvalidDevice,
        message: format!("Cannot parse ALSA PCM route: {device_id}"),
    })?;
    let (card_number, short_id) =
        resolve_card(&route.card).ok_or_else(|| HardwareVolumeProbeError {
            kind: HardwareVolumeProbeErrorKind::DeviceUnavailable,
            message: format!("ALSA card '{}' is not present", route.card),
        })?;
    let control = PathBuf::from(format!("/dev/snd/controlC{card_number}"));
    let identity = select_stable_card_identity(
        has_real_card_serial(card_number),
        find_stable_link("/dev/snd/by-id", &control),
        find_stable_link("/dev/snd/by-path", &control),
        short_id,
    );
    Ok(StableAlsaRouteKey::from_parts(
        &identity,
        &route.pcm,
        route.device,
    ))
}

/// A udev `by-id` link may exist even when USB's ID_SERIAL was synthesized
/// from vendor + model alone (for example `Fosi_Fosi_Audio_ZH3`). Only a
/// usable serial attribute on the owning USB device is strong enough to
/// distinguish two otherwise-identical DACs; without it, port identity
/// (`by-path`) is the safe key. Some devices publish placeholders such as
/// `0000`, which udev still includes in `ID_SERIAL`; those are not stable
/// identities. Stop at the first USB device so a root hub's own serial
/// cannot be mistaken for the DAC's.
fn has_real_card_serial(card_number: u32) -> bool {
    let Ok(device) = fs::canonicalize(format!("/sys/class/sound/card{card_number}/device"))
    else {
        return false;
    };
    has_real_usb_serial(device, Path::new("/sys/devices"))
}

fn has_real_usb_serial(mut device: PathBuf, devices_root: &Path) -> bool {
    while device.starts_with(devices_root) {
        if device.join("idVendor").is_file() && device.join("idProduct").is_file() {
            return fs::read_to_string(device.join("serial"))
                .ok()
                .is_some_and(|serial| is_usable_usb_serial(&serial));
        }
        if !device.pop() {
            break;
        }
    }
    false
}

fn is_usable_usb_serial(serial: &str) -> bool {
    let serial = serial.trim();
    if serial.is_empty()
        || matches!(
            serial.to_ascii_lowercase().as_str(),
            "none" | "null" | "unknown" | "default" | "n/a" | "na"
        )
    {
        return false;
    }

    // A run of zeroes is a common firmware placeholder (the DacMagic Plus
    // reports `0000`). Keep zero-padded real identifiers such as `0001`.
    serial
        .chars()
        .any(|character| character.is_ascii_alphanumeric() && character != '0')
}

fn resolve_card(card: &str) -> Option<(u32, String)> {
    if let Ok(number) = card.parse::<u32>() {
        let short = fs::read_to_string(format!("/proc/asound/card{number}/id"))
            .ok()?
            .trim()
            .to_string();
        return (!short.is_empty()).then_some((number, short));
    }
    let entries = fs::read_dir("/proc/asound").ok()?;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(suffix) = file_name
            .to_string_lossy()
            .strip_prefix("card")
            .map(str::to_string)
        else {
            continue;
        };
        let Ok(number) = suffix.parse::<u32>() else {
            continue;
        };
        let Ok(short) = fs::read_to_string(entry.path().join("id")) else {
            continue;
        };
        if short.trim() == card {
            return Some((number, card.to_string()));
        }
    }
    None
}

fn find_stable_link(directory: &str, control: &Path) -> Option<String> {
    let target = fs::canonicalize(control).ok()?;
    let target_metadata = fs::metadata(&target).ok();
    let mut matches = fs::read_dir(directory)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let resolved = fs::canonicalize(entry.path()).ok()?;
            let same_path = resolved == target;
            let same_device = target_metadata.as_ref().is_some_and(|target| {
                fs::metadata(&resolved)
                    .ok()
                    .is_some_and(|candidate| candidate.rdev() == target.rdev())
            });
            (same_path || same_device).then(|| entry.file_name().to_string_lossy().into_owned())
        })
        .collect::<Vec<_>>();
    matches.sort();
    matches.into_iter().next()
}

pub(super) fn enumerate(device_id: &str) -> HardwareVolumeProbe {
    let ctl_name = mixer_ctl_name(device_id);
    let route_key = stable_route_key(device_id);
    let mixer = match RawMixer::open(device_id) {
        Ok(mixer) => mixer,
        Err(error) => {
            return HardwareVolumeProbe {
                device_id: device_id.to_string(),
                ctl_name,
                route_key: route_key.ok(),
                candidates: Vec::new(),
                ucm_controls: Vec::new(),
                error: Some(error),
            };
        }
    };
    let ctl = RawCtl::open(&ctl_name, false).ok();
    let mut candidates = Vec::new();
    let mut element = unsafe { ffi::snd_mixer_first_elem(mixer.0) };
    while !element.is_null() {
        if unsafe { ffi::snd_mixer_elem_get_type(element) } == ffi::SND_MIXER_ELEM_SIMPLE {
            if let Some(candidate) = probe_candidate(element, ctl.as_ref()) {
                candidates.push(candidate);
            }
        }
        element = unsafe { ffi::snd_mixer_elem_next(element) };
    }
    mark_recommended(&mut candidates);
    candidates.sort_by(|left, right| {
        right
            .is_valid()
            .cmp(&left.is_valid())
            .then_with(|| right.recommended.cmp(&left.recommended))
            .then_with(|| left.id.name.cmp(&right.id.name))
            .then_with(|| left.id.index.cmp(&right.id.index))
    });

    let (route_key, error) = match route_key {
        Ok(route_key) => (Some(route_key), None),
        Err(error) => (None, Some(error)),
    };
    let ucm_controls = parse_alsa_route(device_id)
        .map(|route| ucm_controls_for_route(&route, &candidates))
        .unwrap_or_default();
    HardwareVolumeProbe {
        device_id: device_id.to_string(),
        ctl_name,
        route_key,
        candidates,
        ucm_controls,
        error,
    }
}

fn exact_candidate(
    device_id: &str,
    id: &AlsaMixerControlId,
) -> Result<HardwareVolumeCandidate, String> {
    let probe = enumerate(device_id);
    if let Some(error) = probe.error {
        return Err(error.message);
    }
    probe
        .candidates
        .into_iter()
        .find(|candidate| candidate.id == *id && candidate.is_valid())
        .ok_or_else(|| format!("ALSA mixer control {id} is missing or no longer valid"))
}

pub(super) fn activate(
    device_id: &str,
    id: &AlsaMixerControlId,
) -> Result<HardwareVolumeSnapshot, String> {
    let candidate = exact_candidate(device_id, id)?;
    if candidate.writability == HardwareVolumeWritability::NeedsEnableCheck {
        let mixer = RawMixer::open(device_id).map_err(|error| error.message)?;
        let element = mixer
            .find(id)
            .ok_or_else(|| format!("ALSA mixer control {id} disappeared during enable"))?;
        let channels = playback_channels(element);
        let values = read_channels(
            element,
            &channels,
            candidate.db_range,
            candidate.has_playback_switch,
        )?;
        // Explicit user action only: write each channel's current value
        // back independently so the capability check cannot alter balance.
        for ((channel, name), value) in channels.iter().zip(&values) {
            let code = unsafe {
                ffi::snd_mixer_selem_set_playback_volume(element, *channel, value.raw as _)
            };
            if code < 0 {
                return Err(format!(
                    "ALSA mixer control {id} is not writable on {name}: {}",
                    alsa_error_text(code)
                ));
            }
        }
    }
    read(device_id, id)
}

pub(super) fn read(
    device_id: &str,
    id: &AlsaMixerControlId,
) -> Result<HardwareVolumeSnapshot, String> {
    let mixer = RawMixer::open(device_id).map_err(|error| error.message)?;
    let element = mixer
        .find(id)
        .ok_or_else(|| format!("ALSA mixer control {id} is missing"))?;
    let channels = playback_channels(element);
    if channels.is_empty() {
        return Err(format!("ALSA mixer control {id} has no playback channels"));
    }
    let mut min = 0;
    let mut max = 0;
    if unsafe { ffi::snd_mixer_selem_get_playback_volume_range(element, &mut min, &mut max) }
        < 0
        || max <= min
    {
        return Err(format!(
            "ALSA mixer control {id} has no usable volume range"
        ));
    }
    let raw_range = HardwareVolumeRange {
        min: c_long_to_i64(min),
        max: c_long_to_i64(max),
    };
    let db_range = finite_db_range(element, raw_range);
    let has_switch = unsafe { ffi::snd_mixer_selem_has_playback_switch(element) } > 0;
    let values = read_channels(element, &channels, db_range, has_switch)?;
    let muted = has_switch
        && values
            .iter()
            .filter_map(|value| value.playback_switch)
            .all(|enabled| !enabled);
    Ok(HardwareVolumeSnapshot {
        control: id.clone(),
        volume: normalize_level(raw_range, db_range, &values),
        channels: values,
        muted,
    })
}

pub(super) fn set(
    device_id: &str,
    id: &AlsaMixerControlId,
    scalar: f32,
) -> Result<HardwareVolumeSnapshot, String> {
    let candidate = exact_candidate(device_id, id)?;
    let raw_range = candidate
        .raw_range
        .ok_or_else(|| format!("ALSA mixer control {id} has no usable raw range"))?;
    let mixer = RawMixer::open(device_id).map_err(|error| error.message)?;
    let element = mixer
        .find(id)
        .ok_or_else(|| format!("ALSA mixer control {id} disappeared before write"))?;
    let channels = playback_channels(element);
    let scalar = scalar.clamp(0.0, 1.0);
    let write_plan = hardware_volume_write_plan(scalar, candidate.has_playback_switch);

    if write_plan.playback_switch == Some(false) {
        for (channel, name) in &channels {
            let code =
                unsafe { ffi::snd_mixer_selem_set_playback_switch(element, *channel, 0) };
            if code < 0 {
                return Err(format!(
                    "Failed to mute {id} on {name}: {}",
                    alsa_error_text(code)
                ));
            }
        }
        let snapshot = read(device_id, id)?;
        remember_local_write(device_id, id, snapshot.volume);
        return Ok(snapshot);
    }

    if write_plan.playback_switch == Some(true) {
        // Restore the hardware route before moving its volume away from zero.
        for (channel, name) in &channels {
            let code =
                unsafe { ffi::snd_mixer_selem_set_playback_switch(element, *channel, 1) };
            if code < 0 {
                return Err(format!(
                    "Failed to unmute {id} on {name}: {}",
                    alsa_error_text(code)
                ));
            }
        }
    }
    debug_assert!(write_plan.write_level);

    let values = read_channels(
        element,
        &channels,
        candidate.db_range,
        candidate.has_playback_switch,
    )?;
    let mut used_db = false;
    // With no playback switch, UI zero means the raw minimum even when a
    // driver's finite dB range deliberately excludes its mute sentinel.
    if scalar > f32::EPSILON {
        if let Some(db_range) = candidate.db_range {
            let current_db = values
                .iter()
                .map(|value| value.db_millibels)
                .collect::<Option<Vec<_>>>();
            if let Some(current_db) = current_db {
                let targets = channel_targets(&current_db, db_range, scalar);
                if targets.len() == channels.len() {
                    let mut failed = None;
                    for ((channel, name), target) in channels.iter().zip(targets) {
                        let code = unsafe {
                            ffi::snd_mixer_selem_set_playback_dB(
                                element,
                                *channel,
                                target as _,
                                0,
                            )
                        };
                        if code < 0 {
                            failed = Some(format!("{name}: {}", alsa_error_text(code)));
                            break;
                        }
                    }
                    if let Some(error) = failed {
                        log::warn!(
                            "[ALSA HW Volume] dB write failed for {id}; using raw fallback: {error}"
                        );
                    } else {
                        used_db = true;
                    }
                }
            }
        }
    }

    if !used_db {
        let current_raw = values.iter().map(|value| value.raw).collect::<Vec<_>>();
        let targets = channel_targets(&current_raw, raw_range, scalar);
        for ((channel, name), target) in channels.iter().zip(targets) {
            let code = unsafe {
                ffi::snd_mixer_selem_set_playback_volume(element, *channel, target as _)
            };
            if code < 0 {
                return Err(format!(
                    "Failed to set raw ALSA volume for {id} on {name}: {}",
                    alsa_error_text(code)
                ));
            }
        }
    }

    let snapshot = read(device_id, id)?;
    remember_local_write(device_id, id, snapshot.volume);
    Ok(snapshot)
}

type LocalWriteMap = HashMap<String, (f32, Instant)>;
static LOCAL_WRITES: std::sync::OnceLock<std::sync::Mutex<LocalWriteMap>> =
    std::sync::OnceLock::new();

fn runtime_control_key(device_id: &str, id: &AlsaMixerControlId) -> String {
    format!("{device_id}\u{1f}{}\u{1f}{}", id.name, id.index)
}

fn remember_local_write(device_id: &str, id: &AlsaMixerControlId, volume: f32) {
    let writes = LOCAL_WRITES.get_or_init(Default::default);
    if let Ok(mut writes) = writes.lock() {
        writes.insert(runtime_control_key(device_id, id), (volume, Instant::now()));
    }
}

fn is_recent_local_write(device_id: &str, id: &AlsaMixerControlId, volume: f32) -> bool {
    let writes = LOCAL_WRITES.get_or_init(Default::default);
    let Ok(mut writes) = writes.lock() else {
        return false;
    };
    let key = runtime_control_key(device_id, id);
    let suppress = writes.get(&key).is_some_and(|(written, at)| {
        suppress_local_write_echo(*written, at.elapsed().as_millis(), volume)
    });
    if suppress {
        writes.remove(&key);
    }
    suppress
}

pub(super) fn subscribe(
    device_id: &str,
    id: &AlsaMixerControlId,
    callback: HardwareVolumeEventCallback,
) -> Result<HardwareVolumeEventSubscription, String> {
    let ctl_name = mixer_ctl_name(device_id);
    let ctl = Ctl::new(&ctl_name, true)
        .map_err(|error| format!("Failed to open ALSA ctl {ctl_name}: {error}"))?;
    ctl.subscribe_events(true)
        .map_err(|error| format!("Failed to subscribe to ALSA ctl {ctl_name}: {error}"))?;
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let device_id = device_id.to_string();
    let id = id.clone();
    let thread = std::thread::Builder::new()
        .name("qbz-alsa-volume-events".to_string())
        .spawn(move || {
            let mut last_published = read(&device_id, &id).ok().map(|snapshot| snapshot.volume);
            'events: while !thread_stop.load(Ordering::SeqCst) {
                match ctl.wait(Some(100)) {
                    Ok(true) => {
                        let mut changed = false;
                        loop {
                            match ctl.read() {
                                Ok(Some(event)) => changed |= event.get_mask().value(),
                                Ok(None) => break,
                                Err(error) => {
                                    callback(HardwareVolumeEvent::Unavailable(format!(
                                        "ALSA ctl event read failed: {error}"
                                    )));
                                    break 'events;
                                }
                            }
                        }
                        if !changed {
                            continue;
                        }
                        std::thread::sleep(Duration::from_millis(35));
                        match read(&device_id, &id) {
                            Ok(snapshot) => {
                                if is_recent_local_write(&device_id, &id, snapshot.volume) {
                                    last_published = Some(snapshot.volume);
                                    continue;
                                }
                                if last_published.is_some_and(|previous| {
                                    (previous - snapshot.volume).abs() < 0.001
                                }) {
                                    continue;
                                }
                                last_published = Some(snapshot.volume);
                                callback(HardwareVolumeEvent::Changed(snapshot));
                            }
                            Err(error) => {
                                callback(HardwareVolumeEvent::Unavailable(error));
                                break;
                            }
                        }
                    }
                    Ok(false) => {}
                    Err(error) => {
                        callback(HardwareVolumeEvent::Unavailable(format!(
                            "ALSA ctl event subscription failed: {error}"
                        )));
                        break;
                    }
                }
            }
            let _ = ctl.subscribe_events(false);
        })
        .map_err(|error| format!("Failed to start ALSA volume event thread: {error}"))?;
    Ok(HardwareVolumeEventSubscription {
        stop,
        thread: Some(thread),
    })
}

#[cfg(test)]
mod sysfs_tests {
    use super::*;

    #[test]
    fn dac_without_serial_does_not_inherit_the_root_hub_serial() {
        let root = std::env::temp_dir().join(format!(
            "qbz-alsa-serial-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        ));
        let hub = root.join("usb3");
        let dac = hub.join("3-5");
        let interface = dac.join("3-5:1.1");
        fs::create_dir_all(&interface).expect("create fake sysfs tree");
        for device in [&hub, &dac] {
            fs::write(device.join("idVendor"), "1234").expect("write vendor");
            fs::write(device.join("idProduct"), "5678").expect("write product");
        }
        fs::write(hub.join("serial"), "0000:00:14.0").expect("write hub serial");

        assert!(!has_real_usb_serial(interface.clone(), &root));
        fs::write(dac.join("serial"), "0000").expect("write placeholder DAC serial");
        assert!(!has_real_usb_serial(interface.clone(), &root));
        fs::write(dac.join("serial"), "REAL-DAC-SERIAL").expect("write DAC serial");
        assert!(has_real_usb_serial(interface, &root));

        fs::remove_dir_all(root).expect("remove fake sysfs tree");
    }

    #[test]
    fn usb_serial_placeholders_are_not_stable_identities() {
        for placeholder in [
            "", "   ", "0000", "none", "NULL", "unknown", "default", "N/A",
        ] {
            assert!(!is_usable_usb_serial(placeholder), "{placeholder:?}");
        }
        assert!(is_usable_usb_serial("0001"));
        assert!(is_usable_usb_serial("REAL-DAC-SERIAL"));
    }
}
