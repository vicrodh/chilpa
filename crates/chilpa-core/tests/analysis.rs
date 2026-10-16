//! Analysing a real file: decoding it and pulling out its pulse.
//!
//! The test of the analyser alone, with samples in memory, lives in
//! `chilpa-analysis`. This one checks the whole path: open the file, decode
//! it, mix it down to mono and count.

use chilpa_core::analysis_job::analyze_file;
use std::io::Write;
use std::path::PathBuf;

/// A 16 bit stereo WAV with beats at that tempo.
fn pulse(name: &str, bpm: f32, seconds: u32) -> PathBuf {
    let rate = 44_100u32;
    let frames = rate * seconds;
    let mut p = std::env::temp_dir();
    p.push(format!("chilpa-analysis-{name}.wav"));

    let data = frames * 4;
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&rate.to_le_bytes()).unwrap();
    f.write_all(&(rate * 4).to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data.to_le_bytes()).unwrap();

    let period = (rate as f32 * 60.0 / bpm) as usize;
    let tail = (rate / 33) as usize;
    for i in 0..frames as usize {
        let from_beat = i % period;
        let v = if from_beat < tail {
            let decay = 1.0 - from_beat as f32 / tail as f32;
            // A beat with some tone, so it is not a pure click.
            let tone = (2.0 * std::f32::consts::PI * 80.0 * i as f32 / rate as f32).sin();
            (decay * decay * tone * 20_000.0) as i16
        } else {
            0
        };
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    p
}

#[test]
fn a_file_beat_comes_out_through_the_whole_path() {
    let path = pulse("128", 128.0, 25);
    let features = analyze_file(&path).expect("it could be read and analysed");
    let bpm = features.bpm.expect("a pulse was found");
    assert!((bpm - 128.0).abs() < 3.0, "it came out at {bpm}");
    assert!(
        features.tempo_confidence.is_some_and(|c| c > 0.1),
        "confidence: {:?}",
        features.tempo_confidence
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_file_without_a_beat_is_analyzed_and_says_nothing() {
    // Silence: it reads fine, but there is nothing to count.
    let path = pulse("silence", 128.0, 25);
    // The audio is overwritten with zeros, leaving the header.
    let mut data = std::fs::read(&path).unwrap();
    for b in data.iter_mut().skip(44) {
        *b = 0;
    }
    std::fs::write(&path, &data).unwrap();

    let features = analyze_file(&path).expect("the file was read");
    assert_eq!(features.bpm, None, "no tempo was invented");
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_file_that_is_not_audio_brings_nothing_down() {
    let mut p = std::env::temp_dir();
    p.push("chilpa-analysis-junk.wav");
    std::fs::write(&p, b"this is not a WAV").unwrap();
    assert!(analyze_file(&p).is_none());
    let _ = std::fs::remove_file(p);
}
