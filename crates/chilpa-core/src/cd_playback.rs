//! Reproducir un CD de audio.
//!
//! Un CD no se decodifica: sus sectores ya son PCM de 16 bits a 44 100 Hz.
//! Por eso la ruta es la más corta de todas: leer sectores y entregarlos.
//!
//! La unidad se abre una sola vez por pista. Si un sector no se puede leer,
//! la pista se corta ahí y se dice; nunca se rellena con silencio.

use chilpa_audio::format::StreamFormat;
use chilpa_audio::negotiate::{negotiate, DeviceCaps, Delivery, Policy};
use chilpa_audio::render::{render_paused, Outcome};
use chilpa_audio::sink::{AudioSink, PauseSignal, SinkError, StopSignal};
use chilpa_cd::{CdDevice, CdError, Toc, Track, TrackReader, CDDA_FORMAT};

#[derive(Debug)]
pub enum CdPlayError {
    Drive(CdError),
    /// El dispositivo de salida no acepta el formato de un CD.
    Unsupported,
    Sink(SinkError),
}

impl std::fmt::Display for CdPlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CdPlayError::Drive(e) => write!(f, "{e}"),
            CdPlayError::Unsupported => {
                write!(f, "la salida no acepta 44 100 Hz de 16 bits, que es lo que trae un CD")
            }
            CdPlayError::Sink(e) => write!(f, "dispositivo: {e:?}"),
        }
    }
}

/// Lee el disco que haya en la unidad.
pub fn read_toc(drive: &mut dyn CdDevice) -> Result<Toc, CdPlayError> {
    chilpa_cd::read_toc(drive).map_err(CdPlayError::Drive)
}

/// Lo que pasó al reproducir una pista del disco.
#[derive(Debug)]
pub struct CdPlayed {
    pub delivery: Delivery,
    pub outcome: Outcome,
    /// Si un sector no se pudo leer, aquí queda dicho.
    pub read_error: Option<String>,
}

/// Reproduce una pista del disco.
pub fn play_track<F, K>(
    drive: &mut dyn CdDevice,
    track: &Track,
    caps: &DeviceCaps,
    stop: &StopSignal,
    pause: &PauseSignal,
    open_sink: F,
    on_progress: &mut dyn FnMut(u64),
) -> Result<CdPlayed, CdPlayError>
where
    F: FnOnce(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    let delivery =
        negotiate(CDDA_FORMAT, caps, Policy::BitPerfect).map_err(|_| CdPlayError::Unsupported)?;
    let mut sink = open_sink(delivery.opened).map_err(CdPlayError::Sink)?;
    let mut lector = TrackReader::new(drive, track);
    let outcome = render_paused(&mut lector, &mut sink, stop, pause, on_progress);
    let read_error = lector.error.as_ref().map(|e| e.to_string());
    sink.close().map_err(CdPlayError::Sink)?;
    Ok(CdPlayed { delivery, outcome, read_error })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_audio::format::SampleFormat;
    use chilpa_cd::{TocEntry, SECTOR_BYTES};

    struct Disco {
        fallar_en: Option<u32>,
    }

    impl CdDevice for Disco {
        fn read_toc(&mut self) -> Result<(Vec<TocEntry>, u32), CdError> {
            Ok((
                vec![
                    TocEntry { number: 1, start_lsn: 0, is_audio: true, session: 1 },
                    TocEntry { number: 2, start_lsn: 100, is_audio: true, session: 1 },
                ],
                200,
            ))
        }
        fn read_audio(&mut self, lsn: u32, sectors: u32, out: &mut Vec<u8>) -> Result<(), CdError> {
            if self.fallar_en == Some(lsn) {
                return Err(CdError::Read { lsn, why: "error de C2".into() });
            }
            out.clear();
            out.resize(sectors as usize * SECTOR_BYTES, 0);
            Ok(())
        }
    }

    struct Nulo(StreamFormat, u64);

    impl AudioSink for &mut Nulo {
        fn format(&self) -> StreamFormat {
            self.0
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            let n = frames.len() / self.0.frame_bytes();
            self.1 += n as u64;
            Ok(n)
        }
        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }
    }

    fn caps() -> DeviceCaps {
        DeviceCaps {
            sample_rates: vec![44_100],
            formats: vec![SampleFormat::S16],
            channels: vec![2],
            probed: true,
        }
    }

    #[test]
    fn una_pista_del_disco_sale_completa_y_bit_perfect() {
        let mut disco = Disco { fallar_en: None };
        let toc = read_toc(&mut disco).unwrap();
        let pista = *toc.track(1).unwrap();
        let mut sink = Nulo(CDDA_FORMAT, 0);
        let played = play_track(
            &mut disco,
            &pista,
            &caps(),
            &StopSignal::new(),
            &PauseSignal::new(),
            |_| Ok(&mut sink),
            &mut |_| {},
        )
        .unwrap();

        assert!(played.delivery.is_bit_perfect());
        assert!(played.outcome.may_advance_queue());
        assert!(played.read_error.is_none());
        assert_eq!(played.outcome.frames(), 100 * 588, "los 100 sectores de la pista");
    }

    #[test]
    fn un_sector_ilegible_corta_la_pista_y_queda_dicho() {
        let mut disco = Disco { fallar_en: Some(75) };
        let toc = read_toc(&mut disco).unwrap();
        let pista = *toc.track(1).unwrap();
        let mut sink = Nulo(CDDA_FORMAT, 0);
        let played = play_track(
            &mut disco,
            &pista,
            &caps(),
            &StopSignal::new(),
            &PauseSignal::new(),
            |_| Ok(&mut sink),
            &mut |_| {},
        )
        .unwrap();
        assert!(played.read_error.is_some(), "el error de lectura se reporta");
        assert!(played.outcome.frames() < 100 * 588, "no salió la pista entera");
    }
}
