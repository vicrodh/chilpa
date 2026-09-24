//! CoreAudio: the outputs a Mac has, and what each of them can promise.
//!
//! macOS is the platform where bit-perfect is least in doubt, and that is
//! worth saying plainly because it is the opposite of what the other
//! platforms here needed. There is no server in the way that has to be
//! talked into passing samples through: a device has a rate, that rate can
//! be set, and a program can take the device to itself. So this stack says
//! `Exact` rather than `Depends` -- but only for real hardware, and only
//! when it is told what "exact" costs.
//!
//! **Two things are read that look alike and are not.** The nominal rate is
//! what the device is *set to*, and the physical formats are what the wire
//! *carries*. A DAC set to 48 will happily accept 44.1 and convert it; what
//! makes a track arrive as it left is setting the nominal rate to the
//! track's and finding the track's width among the physical formats. Reading
//! only the first is how a player ends up claiming bit-perfect over
//! CoreAudio's own resampler.
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

use chilpa_audio::backend::{BackendInfo, BackendKind, BackendOutput, Fidelity, OutputBackend};
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
    let formats = read::physical_formats(device);
    let widest = formats.iter().map(|f| f.mBitsPerChannel).max().unwrap_or(0);
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
            // Exact, and it is the platform's doing: there is no mixer that
            // has to be persuaded. What it costs is set out in the caveat --
            // exact is not free here either, it is a device taken.
            fidelity: Fidelity::Exact,
            caveat: chilpa_i18n::t(
                "Exact here means the device is Chilpa's alone while a track plays, and set to \
                 that track's rate. Nothing else on the Mac can use it meanwhile, and the rate \
                 changes between tracks. Turn bit-perfect off to share it and let the Mac \
                 convert.",
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
            let mut detail = read::transport_name(transport).to_string();
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
                    // CoreAudio's own id is a number that changes between
                    // boots, so what travels is the name, which does not.
                    // See `find`.
                    id: read::name(device),
                    label: read::name(device),
                    detail,
                    bit_perfect: true,
                },
            ));
        }
        // What the machine is already using first, then the rest as they
        // came: somebody opening the list is most likely looking for the
        // one they are hearing.
        outputs.sort_by_key(|(is_default, _)| !is_default);
        outputs.into_iter().map(|(_, o)| o).collect()
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

/// The device with that name, if it is still there.
///
/// By name and not by CoreAudio's id, which is assigned at boot and reused:
/// a settings file that remembered a number would, after a restart, point at
/// whatever device happened to get that number. Named, a device that is gone
/// is simply not found, which is the answer somebody can act on.
pub fn find(name: &str) -> Option<ffi::AudioObjectID> {
    read::every_device()
        .into_iter()
        .find(|d| read::output_channels(*d) > 0 && read::name(*d) == name)
}

/// What the machine can tell about an output without opening it.
///
/// Public because the wizard asks the same question the picker does, and
/// because it is the only honest source for "would this track arrive
/// untouched": the nominal rate says what the device is set to and the
/// physical formats say what the wire carries.
pub fn capabilities(name: &str) -> Option<chilpa_audio::negotiate::DeviceCaps> {
    let device = find(name)?;
    let mut sample_rates: Vec<u32> = Vec::new();
    for range in read::available_rates(device) {
        // A range with two different ends means a converter inside, and
        // then every standard rate between them really is available.
        for rate in [44_100u32, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000, 705_600, 768_000] {
            let r = f64::from(rate);
            if r >= range.mMinimum && r <= range.mMaximum && !sample_rates.contains(&rate) {
                sample_rates.push(rate);
            }
        }
    }
    sample_rates.sort_unstable();

    use chilpa_audio::format::SampleFormat;
    let mut formats = Vec::new();
    for physical in read::physical_formats(device) {
        // Every field, not just the width. A description that is not
        // interleaved signed little-endian linear PCM is not something
        // Chilpa's samples can be handed to, and offering it would be
        // promising a format that fails at open time -- after the DAC has
        // already been taken and re-clocked.
        if physical.mFormatID != ffi::kAudioFormatLinearPCM
            || physical.mFormatFlags & ffi::kAudioFormatFlagIsFloat != 0
            || physical.mFormatFlags & ffi::kAudioFormatFlagIsSignedInteger == 0
            || physical.mFormatFlags & ffi::kAudioFormatFlagIsBigEndian != 0
            || physical.mFormatFlags & ffi::kAudioFormatFlagIsNonInterleaved != 0
            || physical.mChannelsPerFrame == 0
        {
            continue;
        }
        // Bytes per channel only means anything once interleaving is known,
        // which is why that is checked above and not here.
        let per_channel = physical.mBytesPerFrame / physical.mChannelsPerFrame;
        let high = physical.mFormatFlags & ffi::kAudioFormatFlagIsAlignedHigh != 0;
        let found = match (physical.mBitsPerChannel, per_channel, high) {
            (16, 2, false) => SampleFormat::S16,
            (24, 3, false) => SampleFormat::S24Packed,
            // Twenty-four carried in four with the valid bits at the top is
            // exactly what `S24In32` is.
            (24, 4, true) => SampleFormat::S24In32,
            (32, 4, false) => SampleFormat::S32,
            // Twenty bits is a real thing on HDMI, and a 24-bit word with
            // the bits somewhere else is a real thing too. Chilpa has no
            // format for either, and the nearest size would be a guess about
            // what the device does with the rest of the word.
            _ => continue,
        };
        if !formats.contains(&found) {
            formats.push(found);
        }
    }
    let channels = read::output_channels(device) as u16;
    Some(chilpa_audio::negotiate::DeviceCaps {
        sample_rates,
        formats,
        channels: (1..=channels).collect(),
        // Read from the hardware, not guessed. That is what lets a verdict
        // be given at all.
        probed: true,
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
    fn reading_the_machine_never_panics() {
        // Whatever this Mac has or does not, every read returns.
        let info = CoreAudio.info();
        let _ = CoreAudio.outputs();
        if info.present {
            assert_eq!(info.fidelity, Fidelity::Exact);
        } else {
            assert!(!info.missing.is_empty());
        }
    }

    #[test]
    fn every_output_carries_a_name_and_says_how_it_is_attached() {
        for output in CoreAudio.outputs() {
            assert!(!output.id.is_empty(), "an output with no name cannot be remembered");
            assert_eq!(output.id, output.label);
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
    }

    #[test]
    fn a_rate_reads_the_way_people_say_it() {
        assert_eq!(khz(44_100.0), "44.1 kHz");
        assert_eq!(khz(48_000.0), "48 kHz");
        assert_eq!(khz(768_000.0), "768 kHz");
    }
}
