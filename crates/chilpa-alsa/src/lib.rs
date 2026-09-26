//! ALSA output, straight to the hardware.
//!
//! This backend opens the `hw:` device and nothing else. It does not fall
//! back to `plughw`, which resamples, nor to the shared mixer: if the device
//! cannot take the format, that is said and that is all. In QBZ that fall
//! back was silent and the UI kept saying "bit-perfect".

pub mod backends;
pub mod devices;
pub mod volume;

pub use backends::{Alsa, AlsaPlug, System};
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

/// Chilpa's formats as ALSA names them.
///
/// **`S24In32` is `S32_LE` and not `S24_LE`, and the difference is where the
/// bits sit.** Chilpa packs 24 bit samples left justified -- `v << 8`, the
/// valid bits in the high bytes and the low byte zero -- which on the wire is
/// a 32 bit word whose bottom eight bits are silence. ALSA's `S24_LE` is the
/// other convention: the value in the low 24 bits with the top byte as
/// padding.
///
/// Naming the wrong one did two things. A DAC that publishes only `S32_LE`
/// -- which is most USB DACs, and the one this was found on -- refused every
/// 24 bit track outright, saying it did not accept the format. And on a
/// device that does take `S24_LE` it would have been worse than a refusal:
/// left justified samples handed over as low justified are every value
/// multiplied by 256, which is not a conversion, it is noise.
///
/// A device that offers `S24_LE` and no `S32_LE` is a gap and is left as one.
/// It would need the packing to right justify, and inventing that here
/// without a device to try it on is how the other half of this got wrong.
const FORMATS: &[(SampleFormat, Format)] = &[
    (SampleFormat::S16, Format::s16()),
    (SampleFormat::S24Packed, Format::S243LE),
    (SampleFormat::S24In32, Format::s32()),
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
///
/// The PCM is let go during a long pause (`suspend`) and opened again the
/// same way on resume (`wake`), so the device, its name and how it was
/// opened are kept.
pub struct AlsaSink {
    pcm: Option<PCM>,
    device: String,
    named: bool,
    stream: StreamFormat,
}

impl AlsaSink {
    /// Opens the device at that exact format. If ALSA hands back a different
    /// rate or format, it is closed and the call fails: a "close enough" is
    /// never accepted.
    /// The same, for a PCM the caller named on purpose.
    ///
    /// See [`open_named`]: this is the door for a stack that has declared
    /// what its path costs, not a way around the direct-device rule.
    pub fn open_pcm(device: &str, stream: StreamFormat) -> Result<Self, SinkError> {
        Self::opening(device, stream, true)
    }

    pub fn open(device: &str, stream: StreamFormat) -> Result<Self, SinkError> {
        Self::opening(device, stream, false)
    }

    fn opening(device: &str, stream: StreamFormat, named: bool) -> Result<Self, SinkError> {
        let pcm = Self::pcm_at(device, stream, named)?;
        Ok(Self { pcm: Some(pcm), device: device.to_string(), named, stream })
    }

    fn pcm_at(device: &str, stream: StreamFormat, named: bool) -> Result<PCM, SinkError> {
        let pcm = if named { open_named(device)? } else { open(device)? };
        {
            let params = HwParams::any(&pcm).map_err(backend)?;
            params.set_access(Access::RWInterleaved).map_err(backend)?;
            params.set_format(alsa_format(stream.sample_format)).map_err(backend)?;
            // Exact, not nearest: this is where resampling used to sneak in.
            params.set_rate(stream.sample_rate_hz, ValueOr::Nearest).map_err(backend)?;
            params.set_channels(u32::from(stream.channels)).map_err(backend)?;
            // Half a second of buffer, as mpd keeps. Left to itself ALSA gave
            // a USB DAC two seconds, and everything that waits on what the
            // buffer holds waited that long: a skip, a seek, the clock, the
            // title of the next track. A device that cannot keep this little
            // gets the nearest it can; this is not worth refusing one over.
            let _ = params.set_buffer_time_near(500_000, ValueOr::Nearest);
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
        Ok(pcm)
    }

    /// The open PCM, or the error for a device let go.
    fn pcm(&self) -> Result<&PCM, SinkError> {
        self.pcm.as_ref().ok_or(SinkError::Closed)
    }
}

impl AudioSink for AlsaSink {
    fn format(&self) -> StreamFormat {
        self.stream
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        let pcm = self.pcm()?;
        let io = pcm.io_bytes();
        match io.writei(frames) {
            Ok(written) => Ok(written),
            Err(e) => match e.errno() {
                // The device ran dry: prepare it and carry on.
                EPIPE => {
                    pcm.prepare().map_err(backend)?;
                    Err(SinkError::Xrun)
                }
                ENODEV | ENXIO => Err(SinkError::DeviceLost),
                _ if pcm.state() == State::Disconnected => Err(SinkError::DeviceLost),
                _ => Err(SinkError::Backend(e.to_string())),
            },
        }
    }

    fn set_paused(&mut self, paused: bool) -> Result<(), SinkError> {
        // On hardware that can pause, the stream stays ready and there is no
        // xrun on resume. Hardware that cannot runs dry and recovers through
        // the ordinary xrun path, which is handled above.
        let Some(pcm) = &self.pcm else { return Ok(()) };
        if pcm.hw_params_current().map(|p| p.can_pause()).unwrap_or(false) {
            pcm.pause(paused).map_err(backend)?;
        }
        Ok(())
    }

    fn close(&mut self) -> Result<(), SinkError> {
        // No `drain`: releasing the device does not wait for it to empty,
        // because an endpoint that stopped answering would hang the stop
        // (the stop() deadlock in QBZ). Waiting for the last of a track is
        // `drain` below, which the player asks for at a natural end.
        if let Some(pcm) = &self.pcm {
            pcm.drop().map_err(backend)?;
        }
        Ok(())
    }

    /// Closes the PCM for real -- `drop` only stops it; the device is let go
    /// when the PCM itself goes -- and keeps how to open it again.
    fn suspend(&mut self) -> Result<(), SinkError> {
        if let Some(pcm) = self.pcm.take() {
            let _ = pcm.drop();
        }
        Ok(())
    }

    /// Opens the same device at the same format, with the same exact check
    /// as the first time: resuming never settles for less.
    fn wake(&mut self) -> Result<(), SinkError> {
        if self.pcm.is_none() {
            self.pcm = Some(Self::pcm_at(&self.device, self.stream, self.named)?);
        }
        Ok(())
    }

    fn held(&self) -> u64 {
        let Some(pcm) = &self.pcm else { return 0 };
        pcm.delay().map(|frames| frames.max(0) as u64).unwrap_or(0)
    }

    /// Drops what the device holds and makes it ready for more, without
    /// closing it.
    fn flush(&mut self) -> Result<(), SinkError> {
        let Some(pcm) = &self.pcm else { return Ok(()) };
        pcm.drop().map_err(backend)?;
        pcm.prepare().map_err(backend)
    }

    /// Watches the device play out what it holds, instead of asking ALSA to
    /// drain: that blocks until it is done, and a device that stopped
    /// answering would hold the player there. Never longer than the buffer
    /// plus a second.
    fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        let Some(pcm) = &self.pcm else { return Ok(()) };
        // A PCM below its start threshold holds what it was handed and does
        // not play it: a track shorter than the buffer never started.
        if pcm.state() == State::Prepared {
            let _ = pcm.start();
        }
        let rate = u64::from(self.stream.sample_rate_hz.max(1));
        let held = pcm.hw_params_current().and_then(|p| p.get_buffer_size()).unwrap_or(0).max(0) as u64;
        let limit = std::time::Instant::now()
            + std::time::Duration::from_millis(held * 1000 / rate)
            + std::time::Duration::from_secs(1);
        // It ran out, which is where it stops once all of it has played.
        while pcm.state() == State::Running && std::time::Instant::now() < limit {
            if give_up() {
                return Ok(());
            }
            match pcm.delay() {
                Ok(left) if left > 0 => {
                    let left_ms = left as u64 * 1000 / rate;
                    std::thread::sleep(std::time::Duration::from_millis(left_ms.clamp(1, 10)));
                }
                _ => break,
            }
        }
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
    open_named(device)
}

/// Opens whatever PCM it is given, direct or not.
///
/// The guard above is what keeps this crate from ever *choosing* something
/// that converts: in QBZ a `hw:` that would not open fell back to `plughw:`
/// on its own and the window went on saying bit-perfect. It is not what
/// stops a caller from naming one deliberately, and two do -- the system
/// stack opens `default` and the PipeWire stack opens the PCM PipeWire
/// installs. Both of them declare what that costs before anything is opened.
///
/// So the rule is not "only hw: exists"; it is "nobody ends up somewhere
/// they did not ask for". Whoever calls this asked, and owns the answer.
pub fn open_named(device: &str) -> Result<PCM, SinkError> {
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
