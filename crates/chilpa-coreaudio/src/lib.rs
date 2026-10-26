//! CoreAudio: the outputs a Mac has, and what each of them can promise.
//!
//! A Mac hands the samples to one output unit and the device converts what
//! it needs to. The device is set to the track's rate either way. Whether
//! every sample is kept depends on one thing, the same one it depends on
//! through a sound server on Linux: whether the output was asked for alone.
//! Alone, the device is Chilpa's, and a float made from a sample of up to 24
//! bits goes back into the wire's integer unchanged. Shared, the Mac mixes
//! whatever else plays into it. So this stack says `Depends`.
//!
//! **Two things are read that look alike and are not.** The nominal rate is
//! what the device is *set to*, and the physical format is what the wire
//! *carries*. A DAC set to 48 will happily accept 44.1 and convert it; what
//! makes a track arrive as it left is the device at the track's rate with
//! an integer wire at least as wide as the track. The verdict reads both,
//! after the device was opened, and not the setting that asked for them.
//!
//! Nothing in the listing opens a device. Taking one to yourself is hog
//! mode, and hog mode is a decision, not a survey.

// Nothing here exists anywhere else. The crate is a workspace member, so
// `cargo check --workspace` builds it on Linux too, where `kind =
// "framework"` is not a thing -- and a platform crate that only compiles on
// its platform is the shape ADR-010 asks for. Off macOS this is an empty
// crate, which is honest: there is no CoreAudio to talk to.
#![cfg(target_os = "macos")]

mod ffi;
mod play;
mod read;
mod evidence;

use chilpa_audio::backend::{BackendInfo, BackendKind, BackendOutput, Fidelity, Knob, OutputBackend};
use chilpa_audio::format::StreamFormat;
use chilpa_audio::sink::{AudioSink, SinkError};

/// The Mac's own audio.
pub struct CoreAudio;

/// A rate as kilohertz, with a decimal only when it needs one.
fn khz(rate: f64) -> String {
    if (rate / 1000.0).fract() == 0.0 {
        format!("{} kHz", (rate / 1000.0) as u32)
    } else {
        format!("{:.1} kHz", rate / 1000.0)
    }
}

/// What a device says it can do, without being opened.
fn declared(device: ffi::AudioObjectID) -> String {
    let widest = integer_wire(device).into_iter().max().unwrap_or(0);
    let highest = read::available_rates(device)
        .iter()
        .map(|r| r.mMaximum)
        .fold(0.0f64, f64::max);

    let mut parts = Vec::new();
    if widest > 0 {
        parts.push(chilpa_i18n::t_args("up to {} bits", &[&widest.to_string()]));
    }
    if highest > 0.0 {
        parts.push(khz(highest));
    }
    let mut line = parts.join(" / ");

    // And what it is set to right now, which is the difference this whole
    // crate is about: a device that can do 192 and is sitting at 44.1 will
    // take a 96 kHz track and convert it. Saying the ceiling alone invites
    // exactly that misunderstanding.
    let now = read::current_rate(device);
    if now > 0.0 {
        if !line.is_empty() {
            line.push_str(" · ");
        }
        line.push_str(&chilpa_i18n::t_args("now at {}", &[&khz(now)]));
    }
    line
}

impl OutputBackend for CoreAudio {
    fn canonical_id(&self, id:&str) -> Option<String> {
        if id.starts_with("uid:") && id.len() > 4 { Some(id.into()) }
        else { find(id).map(device_key) }
    }
    fn inspect(&self, id: &str) -> chilpa_audio::evidence::Inspection { evidence::inspect(id) }
    fn observation(&self, id: &str) -> chilpa_audio::evidence::Observation { evidence::observation(id) }
    fn check(&self, id: &str, format: StreamFormat, exclusive: bool)
        -> Result<(StreamFormat, chilpa_audio::evidence::Observation), SinkError> {
        play::check(id, format, exclusive)
    }
    fn kind(&self) -> BackendKind {
        BackendKind::CoreAudio
    }

    fn info(&self) -> BackendInfo {
        // What this stack needs is a device that can send sound out. A Mac
        // with none is a Mac with the audio stack turned off, which is worth
        // saying rather than showing an empty list.
        let any = read::every_device().into_iter().any(|d| read::output_channels(d) > 0);
        if !any {
            return BackendInfo::missing(
                BackendKind::CoreAudio,
                chilpa_i18n::t("CoreAudio reports no device that can play"),
            );
        }
        BackendInfo {
            kind: BackendKind::CoreAudio,
            present: true,
            missing: String::new(),
            provider: String::new(),
            // Exact only with the output alone: the Mac mixes whatever else
            // plays into a shared device and converts it to the rate
            // somebody else chose. So exclusive mode is a real switch here,
            // and turning it off is how the Mac is shared.
            fidelity: Fidelity::Depends,
            caveat: chilpa_i18n::t(
                "Chilpa sets the device to each track's rate. With exclusive mode on, the device \
                 is also Chilpa's alone while a track plays, so nothing else on the Mac can use \
                 it meanwhile. Turn exclusive mode off to share it; the Mac mixes in whatever \
                 else plays.",
            ),
        }
    }

    fn outputs(&self) -> Vec<BackendOutput> {
        let default = read::default_device();
        let mut outputs: Vec<(bool, BackendOutput)> = Vec::new();
        for device in read::every_device() {
            if read::output_channels(device) == 0 {
                continue;
            }
            let transport = read::transport(device);
            if !read::is_hardware(transport) {
                // Another program wearing the shape of a device. Sending
                // music into one is not choosing an output.
                continue;
            }
            let mut detail = read::transport_name(transport);
            let says = declared(device);
            if !says.is_empty() {
                if !detail.is_empty() {
                    detail.push_str(" · ");
                }
                detail.push_str(&says);
            }
            outputs.push((
                Some(device) == default,
                BackendOutput {
                    // I persist the UID; equal display names remain distinct.
                    id: device_key(device),
                    label: read::name(device),
                    detail,
                    // Only a wire that carries integers can carry a track's
                    // own. A Mac's speakers carry floats, through the
                    // speaker processing, and are never bit-perfect.
                    bit_perfect: !integer_wire(device).is_empty(),
                },
            ));
        }
        // What the machine is already using first, then the rest as they
        // came: somebody opening the list is most likely looking for the
        // one they are hearing.
        outputs.sort_by_key(|(is_default, _)| !is_default);
        outputs.into_iter().map(|(_, o)| o).collect()
    }

    /// What the output unit takes, which is more than the device does: it
    /// converts the rate and the format on the way. Whether that conversion
    /// kept the samples is `carries_untouched`, read once the device opened.
    fn caps(&self, id: &str) -> Result<chilpa_audio::negotiate::DeviceCaps, SinkError> {
        capabilities(id).ok_or_else(|| {
            SinkError::Backend(chilpa_i18n::t("this output is not there any more"))
        })
    }

    /// The rates the device runs at, and 24 bits in 32 when its wire
    /// publishes an integer format of 24 bits or more. That is what DoP
    /// needs: the device at the carrier's rate, Chilpa's alone, and a float
    /// made from a 24-bit word going back into a 24-bit word unchanged. A
    /// wire of floats -- a Mac's speakers -- gets nothing.
    fn untouched_caps(&self, id: &str) -> Option<chilpa_audio::negotiate::DeviceCaps> {
        let device = find(id)?;
        if integer_wire(device).into_iter().all(|bits| bits < 24) {
            return None;
        }
        let mut sample_rates: Vec<u32> = STANDARD_RATES
            .iter()
            .copied()
            .filter(|rate| play::runs_at(device, f64::from(*rate)))
            .collect();
        sample_rates.dedup();
        let channels = read::output_channels(device).min(u32::from(u16::MAX)) as u16;
        Some(chilpa_audio::negotiate::DeviceCaps {
            sample_rates,
            formats: vec![chilpa_audio::format::SampleFormat::S24In32],
            channels: if channels >= 2 { (2..=channels).collect() } else { vec![channels.max(1)] },
            probed: true,
        })
    }

    /// Read from the device as it stands, after it opened: whether it runs
    /// at the track's rate, carries integers at least as wide as the track,
    /// and is Chilpa's alone.
    fn carries_untouched(&self, id: &str, rate_hz: u32, bits: Option<u8>) -> bool {
        untouched(id, rate_hz, bits)
    }

    /// The device's own knob, when it has one.
    ///
    /// Most DACs worth owning do not: the volume lives in the hardware and
    /// CoreAudio reports nothing. That is the right answer and it is given
    /// as such, rather than offering a slider that would work by scaling the
    /// samples -- which is not a volume, it is a conversion, and it is the
    /// end of bit-perfect.
    ///
    /// It is looked for on the master element and then on the stereo pair,
    /// because plenty of devices carry no master and a knob per channel.
    /// Asking only the master said the Mac's own speakers had no volume.
    fn volume(&self, id: &str) -> Option<Knob> {
        let device = find(id)?;
        // A knob the program cannot move is not one the player bar can use:
        // QBZ asks whether the property can be set before taking the volume
        // to the device (`coreaudio_direct.rs`, `set_hardware_volume`).
        if read::movable_volume(device).is_empty() {
            return None;
        }
        let level = read::volume(device)?;
        Some(Knob {
            percent: (level.clamp(0.0, 1.0) * 100.0).round() as u8,
            muted: read::muted(device).unwrap_or(false),
            // Muting is its own capability, and having one is not being
            // allowed to move it.
            can_mute: !read::movable_mute(device).is_empty(),
            // CoreAudio has no name for it: there is one control and it
            // belongs to the device.
            name: String::new(),
        })
    }

    fn set_volume(&self, id: &str, percent: u8) -> Result<(), String> {
        let Some(device) = find(id) else {
            return Err(chilpa_i18n::t("this output is not there any more"));
        };
        let elements = read::movable_volume(device);
        if elements.is_empty() {
            return Err(chilpa_i18n::t("this output has no volume control of its own"));
        }
        let level = f32::from(percent.min(100)) / 100.0;
        for element in elements {
            play::write_element(device, ffi::kAudioDevicePropertyVolumeScalar, element, &level)
                .map_err(|s| {
                    chilpa_i18n::t_args("the volume would not move ({})", &[&s.to_string()])
                })?;
        }
        Ok(())
    }

    fn set_muted(&self, id: &str, muted: bool) -> Result<(), String> {
        let Some(device) = find(id) else {
            return Err(chilpa_i18n::t("this output is not there any more"));
        };
        let elements = read::movable_mute(device);
        if elements.is_empty() {
            return Err(chilpa_i18n::t("this output cannot be muted"));
        }
        let on = u32::from(muted);
        for element in elements {
            play::write_element(device, ffi::kAudioDevicePropertyMute, element, &on).map_err(
                |s| chilpa_i18n::t_args("the mute would not move ({})", &[&s.to_string()]),
            )?;
        }
        Ok(())
    }

    /// Every device the Mac has, by id.
    ///
    /// An in-process property read, which is as cheap as this gets: no
    /// device is opened and nothing is asked over a socket. The ids
    /// themselves are the fingerprint -- CoreAudio hands out a new one for
    /// a device that was plugged in, and drops the one that went away.
    fn generation(&self) -> String {
        read::every_device().iter().map(|d| d.to_string()).collect::<Vec<_>>().join(",")
    }

    fn open(
        &self,
        id: &str,
        format: StreamFormat,
        exclusive: bool,
    ) -> Result<Box<dyn AudioSink + Send>, SinkError> {
        play::CoreAudioSink::open(id, format, exclusive)
            .map(|s| Box::new(s) as Box<dyn AudioSink + Send>)
    }
}

/// I resolve a persistent UID, or an unambiguous legacy name.
pub fn find(name: &str) -> Option<ffi::AudioObjectID> {
    let matches = read::every_device().into_iter().filter(|d| read::output_channels(*d) > 0)
        .filter(|d| matches_identifier(name,*d,&read::uid(*d),&read::name(*d)));
    unique(matches)
}
fn matches_identifier(query:&str, object:u32, uid:&str, label:&str) -> bool {
    if let Some(wanted) = query.strip_prefix("uid:") { !wanted.is_empty() && uid == wanted }
    else if let Some(wanted) = query.strip_prefix("session:") {
        wanted.split_once(':').is_some_and(|(session,id)|session == session_id() && id.parse::<u32>().ok() == Some(object))
    }
    else { label == query }
}
fn unique(mut matches:impl Iterator<Item=ffi::AudioObjectID>) -> Option<ffi::AudioObjectID> {
    let first = matches.next()?;
    // I migrate old names only when they identify exactly one output.
    matches.next().is_none().then_some(first)
}

fn device_key(device: ffi::AudioObjectID) -> String {
    let uid = read::uid(device);
    if uid.is_empty() { format!("session:{}:{device}",session_id()) } else { format!("uid:{uid}") }
}
fn session_id() -> &'static str {
    static ID:std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(||format!("{}-{:x}",std::process::id(),std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()))
}

/// The rates a track can come in, and the ones a device may run at.
const STANDARD_RATES: [u32; 16] = [
    8_000, 11_025, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
    352_800, 384_000, 705_600, 768_000,
];

/// What an output can be handed, without opening it.
///
/// Every rate, because the output unit converts whatever the device does
/// not run at; the integer formats Chilpa writes and floats for a source
/// that decodes to them; and the device's own channel counts. Not one, even
/// on a device that has it: the unit sends a single channel to the left
/// only, so a mono track goes out in both, the way it does on a card.
pub fn capabilities(name: &str) -> Option<chilpa_audio::negotiate::DeviceCaps> {
    use chilpa_audio::format::SampleFormat;
    let device = find(name)?;
    let channels = read::output_channels(device).min(u32::from(u16::MAX)) as u16;
    Some(chilpa_audio::negotiate::DeviceCaps {
        sample_rates: STANDARD_RATES.to_vec(),
        formats: vec![
            SampleFormat::S16,
            SampleFormat::S24Packed,
            SampleFormat::S24In32,
            SampleFormat::S32,
            SampleFormat::F32,
        ],
        channels: if channels >= 2 { (2..=channels).collect() } else { vec![channels.max(1)] },
        probed: true,
    })
}

/// The integer widths the device's wire can carry, from its published
/// physical formats. Empty for a device that carries only floats.
fn integer_wire(device: ffi::AudioObjectID) -> Vec<u32> {
    read::physical_formats(device)
        .iter()
        .filter(|f| {
            f.mFormatID == ffi::kAudioFormatLinearPCM
                && f.mFormatFlags & ffi::kAudioFormatFlagIsFloat == 0
                && f.mFormatFlags & ffi::kAudioFormatFlagIsSignedInteger != 0
        })
        .map(|f| f.mBitsPerChannel)
        .collect()
}

/// Whether the device, as it stands right now, carries a stream at that
/// rate and depth to the wire untouched.
///
/// Three facts, read and not assumed. The device runs at the track's rate,
/// or the output unit resampled it. Every output stream carries integers at
/// least as wide as what the track carries, or the device rounded the floats
/// into fewer bits -- and a wire of floats is never untouched, because the
/// speakers that carry them process them. And it is this program's alone,
/// or something else could be mixed in. A track with no integer depth of its
/// own -- a lossy one -- has nothing to keep and is never untouched.
pub fn untouched(name: &str, rate_hz: u32, bits: Option<u8>) -> bool {
    let (Some(device), Some(bits)) = (find(name), bits) else { return false };
    if (read::current_rate(device) - f64::from(rate_hz)).abs() > 0.5 {
        return false;
    }
    if read::hogged_by(device) != std::process::id() as i32 {
        return false;
    }
    let streams = read::output_streams(device);
    !streams.is_empty()
        && streams.into_iter().all(|stream| {
            read::format(stream, ffi::kAudioStreamPropertyPhysicalFormat).is_some_and(|f| {
                f.mFormatID == ffi::kAudioFormatLinearPCM
                    && f.mFormatFlags & ffi::kAudioFormatFlagIsFloat == 0
                    && f.mFormatFlags & ffi::kAudioFormatFlagIsSignedInteger != 0
                    && f.mBitsPerChannel >= u32::from(bits)
            })
        })
}

/// Whether another program has this device to itself right now.
pub fn taken_by_someone_else(name: &str) -> bool {
    let Some(device) = find(name) else { return false };
    let holder = read::hogged_by(device);
    holder != -1 && holder != std::process::id() as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equal_labels_require_their_uid_and_legacy_names_resolve_only_when_unique() {
        let devices = [(1,"serial-a","USB DAC"),(2,"serial-b","USB DAC"),(3,"serial-c","Speakers")];
        let resolve = |query:&str| unique(devices.iter().filter(|(id,uid,label)|matches_identifier(query,*id,uid,label)).map(|(id,_,_)|*id));
        assert_eq!(resolve("USB DAC"),None);
        assert_eq!(resolve("uid:serial-a"),Some(1)); assert_eq!(resolve("uid:serial-b"),Some(2));
        assert_eq!(resolve("Speakers"),Some(3)); assert_eq!(resolve("uid:"),None);
        assert_eq!(resolve(&format!("session:{}:2",session_id())),Some(2));
        assert_eq!(resolve("session:previous-process:2"),None);
    }

    #[test]
    fn reading_the_machine_never_panics() {
        // Whatever this Mac has or does not, every read returns.
        let info = CoreAudio.info();
        let _ = CoreAudio.outputs();
        if info.present {
            assert_eq!(info.fidelity, Fidelity::Depends);
        } else {
            assert!(!info.missing.is_empty());
        }
    }

    #[test]
    fn every_output_carries_a_name_and_says_how_it_is_attached() {
        for output in CoreAudio.outputs() {
            assert!(!output.id.is_empty(), "an output with no name cannot be remembered");
            assert!(output.id.starts_with("uid:") || output.id.starts_with("session:"));
            assert!(!output.detail.is_empty(), "{} says nothing about itself", output.label);
        }
    }

    #[test]
    fn what_is_offered_can_be_found_again() {
        // The round trip a settings file takes: a name is written, and after
        // a restart it has to name the same device.
        for output in CoreAudio.outputs() {
            assert!(find(&output.id).is_some(), "{} cannot be found again", output.id);
            let caps = capabilities(&output.id).expect("it has capabilities");
            assert!(caps.probed, "guessed caps cannot promise anything");
            assert!(!caps.sample_rates.is_empty(), "{} offers no rate", output.id);
        }
    }

    #[test]
    fn a_name_nobody_has_is_not_found_and_is_not_taken() {
        assert!(find("no such device anywhere").is_none());
        assert!(!taken_by_someone_else("no such device anywhere"));
        assert!(!untouched("no such device anywhere", 44_100, Some(16)));
    }

    #[test]
    fn a_device_nobody_opened_carries_nothing_untouched() {
        // Not hogged by this test, so not alone: the verdict has to say no
        // whatever the device's rate and wire happen to be.
        for output in CoreAudio.outputs() {
            assert!(!untouched(&output.id, 44_100, Some(16)), "{}", output.id);
        }
    }

    #[test]
    fn every_output_takes_a_cd_and_a_mono_track_goes_out_in_two() {
        for output in CoreAudio.outputs() {
            let caps = capabilities(&output.id).expect("it has capabilities");
            let cd = chilpa_audio::format::StreamFormat::new(
                chilpa_audio::format::SampleFormat::S16,
                44_100,
                2,
            );
            assert!(caps.accepts(&cd), "{} refuses a CD", output.id);
            if caps.channels.contains(&2) {
                assert!(!caps.channels.contains(&1), "{} would play mono on the left only", output.id);
            }
        }
    }

    #[test]
    fn a_rate_reads_the_way_people_say_it() {
        assert_eq!(khz(44_100.0), "44.1 kHz");
        assert_eq!(khz(48_000.0), "48 kHz");
        assert_eq!(khz(768_000.0), "768 kHz");
    }
}
