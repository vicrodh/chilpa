//! Salida por ALSA directo a hardware.
//!
//! Este backend abre el dispositivo `hw:` y nada más. No cae a `plughw`, que
//! resamplea, ni al mixer compartido: si el dispositivo no puede con el
//! formato, se dice y punto. En QBZ esa caída era silenciosa y la UI seguía
//! diciendo "bit-perfect".

pub mod devices;

pub use devices::{list, Device};

use alsa::pcm::{Access, Format, HwParams, State, PCM};
use alsa::{Direction, ValueOr};
use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::DeviceCaps;
use chilpa_audio::sink::{AudioSink, SinkError};

/// Números de error de POSIX que nos importan. El crate de ALSA los entrega
/// como `i32` crudos.
const EPIPE: i32 = 32;
const ENODEV: i32 = 19;
const ENXIO: i32 = 6;
const ENOENT: i32 = 2;

/// Frecuencias que se prueban al medir un dispositivo.
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
    FORMATS.iter().find(|(s, _)| *s == f).map(|(_, a)| *a).expect("formato cubierto")
}

/// Mide qué acepta de verdad un dispositivo, abriéndolo.
///
/// Lo que regresa lleva `probed: true`, que es lo único con lo que se puede
/// prometer bit-perfect. Adivinar frecuencias y apagar el resampleo, como
/// hacía el asistente de QBZ, es peor que no saber.
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

/// Un dispositivo ALSA abierto con un formato exacto.
pub struct AlsaSink {
    pcm: PCM,
    stream: StreamFormat,
}

impl AlsaSink {
    /// Abre el dispositivo con ese formato exacto. Si ALSA entrega otra
    /// frecuencia o formato, se cierra y se falla: nunca se acepta un
    /// "parecido".
    pub fn open(device: &str, stream: StreamFormat) -> Result<Self, SinkError> {
        let pcm = open(device)?;
        {
            let params = HwParams::any(&pcm).map_err(backend)?;
            params.set_access(Access::RWInterleaved).map_err(backend)?;
            params.set_format(alsa_format(stream.sample_format)).map_err(backend)?;
            // Exacta, no la más cercana: aquí es donde se colaba el resampleo.
            params.set_rate(stream.sample_rate_hz, ValueOr::Nearest).map_err(backend)?;
            params.set_channels(u32::from(stream.channels)).map_err(backend)?;
            pcm.hw_params(&params).map_err(backend)?;
            let rate = params.get_rate().map_err(backend)?;
            let channels = params.get_channels().map_err(backend)? as u16;
            if rate != stream.sample_rate_hz || channels != stream.channels {
                return Err(SinkError::Backend(format!(
                    "el dispositivo abrió {rate} Hz y {channels} canales, no {} Hz y {}",
                    stream.sample_rate_hz, stream.channels
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
                // El dispositivo se quedó sin datos: se prepara y se sigue.
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
        // Con hardware que sabe pausar, el stream se queda listo y no hay
        // xrun al reanudar. El que no sabe se queda sin datos y se recupera
        // con el xrun normal, que ya se maneja arriba.
        if self.pcm.hw_params_current().map(|p| p.can_pause()).unwrap_or(false) {
            self.pcm.pause(paused).map_err(backend)?;
        }
        Ok(())
    }

    fn close(&mut self) -> Result<(), SinkError> {
        // Sin `drain`: soltar el dispositivo no espera a que termine de
        // vaciarse, porque un endpoint que dejó de responder colgaría el
        // paro (el deadlock de stop() en QBZ).
        self.pcm.drop().map_err(backend)?;
        Ok(())
    }
}

fn open(device: &str) -> Result<PCM, SinkError> {
    if !device.starts_with("hw:") {
        return Err(SinkError::Backend(format!(
            "{device} no es un dispositivo directo; solo se abre hw: (nada de plughw ni default)"
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
            Err(other) => panic!("debía rechazarse por no ser hw:, salió {other:?}"),
            Ok(_) => panic!("plughw no se debe abrir nunca"),
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
