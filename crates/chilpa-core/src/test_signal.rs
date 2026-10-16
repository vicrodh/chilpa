//! The HiFi Wizard's test track for a format the library does not have.
//!
//! QBZ played four tracks of its streaming service; Chilpa depends on none,
//! and no site gives today what 2L's test bench gave. So a format the
//! library lacks is made here: an arpeggio with harmonics over a soft bed of
//! pink noise, 30 seconds, written once into the cache as WAV. It is
//! deterministic, so the same file comes out on every machine.

use std::io::Write;
use std::path::PathBuf;

pub const SECONDS: u32 = 30;

/// Where the file for `bits` and `rate` goes.
pub fn path_for(bits: u16, rate: u32) -> PathBuf {
    chilpa_dirs::cache().join("wizard").join(format!("test-{bits}-{rate}.wav"))
}

/// The file, written unless it is already there. Blocking: a 24/192 file
/// is 34 MB.
pub fn ensure(bits: u16, rate: u32) -> std::io::Result<PathBuf> {
    let path = path_for(bits, rate);
    let bytes_per_sample = u32::from(bits / 8);
    let expected = 44 + u64::from(rate) * u64::from(SECONDS) * 2 * u64::from(bytes_per_sample);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() == expected) {
        return Ok(path);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("wav.part");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&partial)?);
    write_wav(&mut out, bits, rate)?;
    out.flush()?;
    drop(out);
    std::fs::rename(&partial, &path)?;
    Ok(path)
}

/// A stereo PCM WAV of the signal at `bits` (16 or 24) and `rate`.
fn write_wav(out: &mut impl Write, bits: u16, rate: u32) -> std::io::Result<()> {
    let channels: u16 = 2;
    let block = channels * (bits / 8);
    let frames = rate * SECONDS;
    let data = frames * u32::from(block);
    out.write_all(b"RIFF")?;
    out.write_all(&(36 + data).to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16u32.to_le_bytes())?;
    out.write_all(&1u16.to_le_bytes())?;
    out.write_all(&channels.to_le_bytes())?;
    out.write_all(&rate.to_le_bytes())?;
    out.write_all(&(rate * u32::from(block)).to_le_bytes())?;
    out.write_all(&block.to_le_bytes())?;
    out.write_all(&bits.to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&data.to_le_bytes())?;
    let mut signal = Signal::new(rate);
    let top = f64::from((1i32 << (bits - 1)) - 1);
    let mut chunk = Vec::with_capacity(rate as usize * usize::from(block));
    for frame in 0..frames {
        let (left, right) = signal.next(frame);
        for value in [left, right] {
            let sample = (value * top).round() as i32;
            chunk.extend_from_slice(&sample.to_le_bytes()[..usize::from(bits / 8)]);
        }
        if chunk.len() >= chunk.capacity() {
            out.write_all(&chunk)?;
            chunk.clear();
        }
    }
    out.write_all(&chunk)
}

/// A minor arpeggio, half a second a note, each note with its second and
/// third harmonics and a quick attack and a long fall, panned a little to
/// alternate sides; under it a soft bed of pink noise (Paul Kellet's
/// filter over a fixed sequence of white noise).
struct Signal {
    rate: f64,
    seed: u32,
    pink: [f64; 7],
}

const NOTES: [f64; 4] = [220.0, 261.63, 329.63, 440.0];
const NOTE_SECONDS: f64 = 0.5;

impl Signal {
    fn new(rate: u32) -> Self {
        Self { rate: f64::from(rate), seed: 0x2545_f491, pink: [0.0; 7] }
    }

    fn white(&mut self) -> f64 {
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(self.seed) / f64::from(u32::MAX) * 2.0 - 1.0
    }

    fn pink(&mut self) -> f64 {
        let white = self.white();
        let b = &mut self.pink;
        b[0] = 0.99886 * b[0] + white * 0.0555179;
        b[1] = 0.99332 * b[1] + white * 0.0750759;
        b[2] = 0.96900 * b[2] + white * 0.1538520;
        b[3] = 0.86650 * b[3] + white * 0.3104856;
        b[4] = 0.55000 * b[4] + white * 0.5329522;
        b[5] = -0.7616 * b[5] - white * 0.0168980;
        let out = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + white * 0.5362;
        b[6] = white * 0.115926;
        out * 0.11
    }

    fn next(&mut self, frame: u32) -> (f64, f64) {
        let t = f64::from(frame) / self.rate;
        let index = (t / NOTE_SECONDS) as usize;
        let into = t - index as f64 * NOTE_SECONDS;
        let frequency = NOTES[index % NOTES.len()];
        let envelope = (into / 0.01).min(1.0) * (-into * 4.0).exp();
        let phase = std::f64::consts::TAU * frequency * into;
        let tone = (phase.sin() + 0.5 * (2.0 * phase).sin() + 0.25 * (3.0 * phase).sin()) / 1.75 * envelope * 0.5;
        let bed = self.pink() * 0.1;
        let pan = if index % 2 == 0 { 0.4 } else { -0.4 };
        (tone * (1.0 - pan) * 0.8 + bed, tone * (1.0 + pan) * 0.8 + bed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_is_a_wav_of_the_format_asked_for() {
        let mut bytes = Vec::new();
        write_wav(&mut bytes, 24, 8_000).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2, "stereo");
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 8_000);
        assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 24);
        assert_eq!(bytes.len(), 44 + 8_000 * SECONDS as usize * 2 * 3);
    }

    #[test]
    fn it_plays_music_under_full_scale_and_never_silence() {
        let mut signal = Signal::new(8_000);
        let (mut peak, mut energy) = (0.0f64, 0.0f64);
        for frame in 0..8_000 * 4 {
            let (l, r) = signal.next(frame);
            peak = peak.max(l.abs()).max(r.abs());
            energy += l * l;
        }
        assert!(peak < 0.95, "{peak}");
        assert!(energy / 32_000.0 > 1e-4, "it is heard");
        let mut again = Signal::new(8_000);
        assert_eq!(again.next(0), Signal::new(8_000).next(0), "the same file on every machine");
    }
}
