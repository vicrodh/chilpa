//! Seeking leaves the reader on the exact frame, not near it.
//!
//! A WAV is written whose sample number N is worth N, so looking at the
//! first sample that comes out after seeking is enough to tell whether it
//! landed where it was asked to.

use chilpa_decode::{Block, Decoded};
use std::io::Write;
use std::path::PathBuf;

const RATE: u32 = 44_100;

fn write_ramp(path: &PathBuf, frames: u32) {
    let data_len = frames * 4; // 2 channels of 16 bits
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&RATE.to_le_bytes()).unwrap();
    f.write_all(&(RATE * 4).to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    for i in 0..frames {
        // The left channel carries the frame number; the right one, its negation.
        let v = (i % 30_000) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&(-v).to_le_bytes()).unwrap();
    }
}

/// Reads blocks, drops the frames the plan calls for, and returns the first
/// music sample that is left.
fn first_sample(decoded: &mut Decoded, mut discard: u64) -> i32 {
    loop {
        let block = decoded.next_block().unwrap().expect("hay música");
        let Block::Int { data, .. } = block else { panic!("el WAV es entero") };
        let frames = (data.len() / 2) as u64;
        if discard >= frames {
            discard -= frames;
            continue;
        }
        return data[(discard as usize) * 2];
    }
}

fn temporal(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-seek-{name}.wav"));
    p
}

#[test]
fn seeking_leaves_the_reader_on_the_requested_frame() {
    let path = temporal("rampa");
    write_ramp(&path, 60_000);
    let mut decoded = Decoded::open(&path).unwrap();

    for target in [0u64, 1, 44_100, 12_345, 59_999] {
        let plan = decoded.seek_to_frame(target).unwrap();
        assert!(plan.request <= target, "el contenedor nunca se pasa del objetivo");
        let sample = first_sample(&mut decoded, plan.discard);
        assert_eq!(
            sample,
            (target % 30_000) as i32,
            "tras buscar a {target} la música empieza en ese frame"
        );
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_file_declares_its_duration_in_frames() {
    let path = temporal("duracion");
    write_ramp(&path, 1_000);
    let decoded = Decoded::open(&path).unwrap();
    assert_eq!(decoded.frame_count(), Some(1_000));
    assert_eq!(decoded.encoder_delay(), 0, "un WAV no trae relleno de codificador");
    assert_eq!(decoded.encoder_padding(), 0);
    let _ = std::fs::remove_file(path);
}
