//! ALSA output, straight to the hardware.
//!
//! This backend opens the `hw:` device and nothing else. It does not fall
//! back to `plughw`, which resamples, nor to the shared mixer: if the device
//! cannot take the format, that is said and that is all. In QBZ that fall
//! back was silent and the UI kept saying "bit-perfect".

pub mod devices;
pub mod volume;

pub use devices::{list, Device};
pub use volume::{Level, Volume};

use alsa::pcm::{Access, Format, HwParams, State, PCM};
use alsa::{Direction, ValueOr};
use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::DeviceCaps;
use chilpa_audio::sink::{AudioSink, SinkError};

/// The POSIX error numbers that matter here. The ALSA crate hands them over
/// as raw `i32`.
const EPIPE: i32 = 32;
const ENODEV: i32 = 19;
const ENXIO: i32 = 6;
const ENOENT: i32 = 2;

/// The rates tried when measuring a device.
const RATES: &[u32] = &[
    44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000, 705_600, 768_000,
];

const FORMATS: &[(SampleFormat, Format)] = &[
    (SampleFormat::S16, Format::s16()),
    (SampleFormat::S24Packed, Format::S243LE),
    (SampleFormat::S24In32, Format::S24LE),
    (SampleFormat::S32, Format::s32()),
    (SampleFormat::F32, Format::float()),
];

fn alsa_format(f: SampleFormat) -> Format {
    FORMATS.iter().find(|(s, _)| *s == f).map(|(_, a)| *a).expect("format covered")
}

/// Measures what a device really accepts, by opening it.
///
/// What comes back carries `probed: true`, which is the only thing that can
/// promise bit-perfect. Guessing rates and turning resampling off, the way
/// the QBZ wizard did, is worse than not knowing.
pub fn probe(device: &str) -> Result<DeviceCaps, SinkError> {
    let pcm = open(device)?;
    let params = HwParams::any(&pcm).map_err(backend)?;
    let mut caps = DeviceCaps { probed: true, ..DeviceCaps::default() };
    for &rate in RATES {
        if params.test_rate(rate).is_ok() {
            caps.sample_rates.push(rate);
        }
    }
    for (ours, own) in FORMATS {
        if params.test_format(*own).is_ok() {
            caps.formats.push(*ours);
        }
    }
    for ch in [1u32, 2, 4, 6, 8] {
        if params.test_channels(ch).is_ok() {
            caps.channels.push(ch as u16);
        }
    }
    Ok(caps)
}

/// An ALSA device opened at one exact format.
pub struct AlsaSink {
    pcm: PCM,
    stream: StreamFormat,
}

impl AlsaSink {
    /// Opens the device at that exact format. If ALSA hands back a different
    /// rate or format, it is closed and the call fails: a "close enough" is
    /// never accepted.
    pub fn open(device: &str, stream: StreamFormat) -> Result<Self, SinkError> {
        let pcm = open(device)?;
        {
            let params = HwParams::any(&pcm).map_err(backend)?;
            params.set_access(Access::RWInterleaved).map_err(backend)?;
            params.set_format(alsa_format(stream.sample_format)).map_err(backend)?;
            // Exact, not nearest: this is where resampling used to sneak in.
            params.set_rate(stream.sample_rate_hz, ValueOr::Nearest).map_err(backend)?;
            params.set_channels(u32::from(stream.channels)).map_err(backend)?;
            pcm.hw_params(&params).map_err(backend)?;
            let rate = params.get_rate().map_err(backend)?;
            let channels = params.get_channels().map_err(backend)? as u16;
            if rate != stream.sample_rate_hz || channels != stream.channels {
                return Err(SinkError::Backend(chilpa_i18n::t_args(
                    "the device opened {} Hz and {} channels, not {} Hz and {}",
                    &[
                        &rate.to_string(),
                        &channels.to_string(),
                        &stream.sample_rate_hz.to_string(),
                        &stream.channels.to_string(),
                    ],
                )));
            }
        }
        Ok(Self { pcm, stream })
    }
}

impl AudioSink for AlsaSink {
    fn format(&self) -> StreamFormat {
        self.stream
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let io = self.pcm.io_bytes();
        match io.writei(frames) {
            Ok(written) => Ok(written),
            Err(e) => match e.errno() {
                // The device ran dry: prepare it and carry on.
                EPIPE => {
                    self.pcm.prepare().map_err(backend)?;
                    Err(SinkError::Xrun)
                }
                ENODEV | ENXIO => Err(SinkError::DeviceLost),
                _ if self.pcm.state() == State::Disconnected => Err(SinkError::DeviceLost),
                _ => Err(SinkError::Backend(e.to_string())),
            },
        }
    }

    fn set_paused(&mut self, paused: bool) -> Result<(), SinkError> {
        // On hardware that can pause, the stream stays ready and there is no
        // xrun on resume. Hardware that cannot runs dry and recovers through
        // the ordinary xrun path, which is handled above.
        if self.pcm.hw_params_current().map(|p| p.can_pause()).unwrap_or(false) {
            self.pcm.pause(paused).map_err(backend)?;
        }
        Ok(())
    }

    fn close(&mut self) -> Result<(), SinkError> {
        // No `drain`: releasing the device does not wait for it to empty,
        // because an endpoint that stopped answering would hang the stop
        // (the stop() deadlock in QBZ).
        self.pcm.drop().map_err(backend)?;
        Ok(())
    }
}

fn open(device: &str) -> Result<PCM, SinkError> {
    if !device.starts_with("hw:") {
        return Err(SinkError::Backend(chilpa_i18n::t_args(
            "{} is not a direct device; only hw: is opened (no plughw and no default)",
            &[device],
        )));
    }
    PCM::new(device, Direction::Playback, false).map_err(|e| match e.errno() {
        ENODEV | ENOENT => SinkError::DeviceLost,
        _ => SinkError::Backend(e.to_string()),
    })
}

fn backend(e: alsa::Error) -> SinkError {
    SinkError::Backend(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_direct_devices_are_opened() {
        match AlsaSink::open("plughw:0,0", StreamFormat::new(SampleFormat::S16, 44_100, 2)) {
            Err(SinkError::Backend(msg)) => assert!(msg.contains("plughw")),
            Err(other) => panic!("it should have been refused for not being hw:, it came out {other:?}"),
            Ok(_) => panic!("plughw must never be opened"),
        }
        assert!(matches!(probe("default"), Err(SinkError::Backend(_))));
    }

    #[test]
    fn every_format_has_an_alsa_equivalent() {
        for f in [
            SampleFormat::S16,
            SampleFormat::S24Packed,
            SampleFormat::S24In32,
            SampleFormat::S32,
            SampleFormat::F32,
        ] {
            let _ = alsa_format(f);
        }
    }
}
