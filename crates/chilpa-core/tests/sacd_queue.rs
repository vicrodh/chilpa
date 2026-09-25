//! The tracks of a SACD image in a queue, each on its own.
//!
//! An image used to play only as the first thing in a queue, whole, from its
//! first track: after a PCM track it was refused, and one of its tracks could not
//! be asked for by itself -- which is what a library, a playlist or a queue
//! holds. Each track is `sacd:<image>#<number>` now, the form QBZ used.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::DeviceCaps;
use chilpa_audio::sink::{AudioSink, SinkError};
use chilpa_core::player::{Player, PlayerEvent};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const SECTOR: usize = 2048;
const AREA: u32 = 540;
const AUDIO_START: u32 = 550;
const AUDIO_END: u32 = 569;

fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

fn sector(img: &mut [u8], lsn: u32) -> &mut [u8] {
    let at = lsn as usize * SECTOR;
    &mut img[at..at + SECTOR]
}

/// A two-track SACD image with the text a pressed disc carries, its track
/// titles in the second sector of the SACDTTxt block as on real discs.
fn image(name: &str) -> PathBuf {
    let mut img = vec![0u8; 600 * SECTOR];
    {
        let m = sector(&mut img, 510);
        m[..8].copy_from_slice(b"SACDMTOC");
        m[8] = 1;
        m[9] = 20;
        m[0x40..0x44].copy_from_slice(&be32(AREA));
        m[0x54..0x56].copy_from_slice(&5u16.to_be_bytes());
        m[0x80] = 1;
        m[0x8a] = 2;
    }
    {
        let t = sector(&mut img, 511);
        t[..8].copy_from_slice(b"SACDText");
        t[0x10..0x12].copy_from_slice(&0x40u16.to_be_bytes());
        t[0x12..0x14].copy_from_slice(&0x80u16.to_be_bytes());
        t[0x40..0x40 + 18].copy_from_slice(b"Agents Of Fortune\0");
        t[0x80..0x80 + 17].copy_from_slice(b"Blue \xD6yster Cult\0");
    }
    {
        let a = sector(&mut img, AREA);
        a[..8].copy_from_slice(b"TWOCHTOC");
        a[0x14] = 4;
        a[0x15] = 2;
        a[0x20] = 2;
        a[0x41] = 2;
        a[0x45] = 2;
        a[0x48..0x4c].copy_from_slice(&be32(AUDIO_START));
        a[0x4c..0x50].copy_from_slice(&be32(AUDIO_END));
        a[0x5a] = 2;
        a[0x80..0x82].copy_from_slice(&3u16.to_be_bytes());
    }
    const SECOND: usize = 8 + 255 * 4;
    {
        let t1 = sector(&mut img, AREA + 1);
        t1[..8].copy_from_slice(b"SACDTRL1");
        t1[8..12].copy_from_slice(&be32(AUDIO_START));
        t1[12..16].copy_from_slice(&be32(AUDIO_START + 10));
        t1[SECOND..SECOND + 4].copy_from_slice(&be32(10));
        t1[SECOND + 4..SECOND + 8].copy_from_slice(&be32(10));
    }
    {
        let t2 = sector(&mut img, AREA + 2);
        t2[..8].copy_from_slice(b"SACDTRL2");
        t2[12..16].copy_from_slice(&[0, 1, 0, 0]);
        t2[SECOND..SECOND + 4].copy_from_slice(&[0, 1, 0, 0]);
        t2[SECOND + 4..SECOND + 8].copy_from_slice(&[0, 1, 0, 0]);
    }
    {
        let b = sector(&mut img, AREA + 3);
        b[..8].copy_from_slice(b"SACDTTxt");
        b[8..10].copy_from_slice(&(SECTOR as u16 + 0x20).to_be_bytes());
        b[10..12].copy_from_slice(&(SECTOR as u16 + 0x80).to_be_bytes());
    }
    {
        let b = sector(&mut img, AREA + 4);
        b[0x20] = 1;
        b[0x24] = 1;
        b[0x26..0x26 + 30].copy_from_slice(b"This Ain't The Summer Of Love\0");
        b[0x80] = 1;
        b[0x84] = 1;
        b[0x86..0x86 + 17].copy_from_slice(b"True Confessions\0");
    }
    for (i, lsn) in (AUDIO_START..=AUDIO_END).enumerate() {
        let s = sector(&mut img, lsn);
        s[0] = (1 << 5) | (1 << 2);
        let header = 1 + 2 + 3;
        let len = SECTOR - header;
        let w: u16 = 0x8000 | (2 << 11) | len as u16;
        s[1..3].copy_from_slice(&w.to_be_bytes());
        s[4] = (i / 75) as u8;
        s[5] = (i % 75) as u8;
        for b in &mut s[header..] {
            *b = 0x69;
        }
    }
    let path = std::env::temp_dir().join(format!("chilpa-sacd-queue-{name}.iso"));
    std::fs::write(&path, img).unwrap();
    path
}

/// Half a second of 16/44.1 silence.
fn wav(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("chilpa-sacd-queue-{name}.wav"));
    let frames = 22_050u32;
    let data = frames * 4;
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&44_100u32.to_le_bytes()).unwrap();
    f.write_all(&176_400u32.to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data.to_le_bytes()).unwrap();
    f.write_all(&vec![0u8; data as usize]).unwrap();
    path
}

struct Swallow(StreamFormat);

impl AudioSink for Swallow {
    fn format(&self) -> StreamFormat {
        self.0
    }
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        Ok(frames.len() / self.0.frame_bytes())
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

/// Plays `paths` and gives back, per track started, its index, its title
/// and its DSD multiple; or what failed.
fn played(paths: &[PathBuf]) -> Result<Vec<(usize, Option<String>, Option<u32>)>, String> {
    let player = Player::spawn(
        Box::new(|_| {
            Ok(DeviceCaps {
                sample_rates: vec![44_100, 176_400],
                formats: vec![SampleFormat::S16, SampleFormat::S24In32, SampleFormat::S32],
                channels: vec![2],
                probed: true,
            })
        }),
        Box::new(|_, format, _| Ok(Box::new(Swallow(format)) as Box<dyn AudioSink + Send>)),
    );
    player.play_all(paths, "dac", true);
    let mut started = Vec::new();
    let limit = Instant::now() + Duration::from_secs(10);
    while Instant::now() < limit {
        for event in player.poll() {
            match event {
                PlayerEvent::Started { index, tags, dsd_multiple, .. } => {
                    started.push((index, tags.and_then(|t| t.title), dsd_multiple))
                }
                PlayerEvent::Finished { .. } => return Ok(started),
                PlayerEvent::Failed { message } => return Err(message),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err("it never finished".into())
}

#[test]
fn a_sacd_image_after_a_wav_plays_track_by_track() {
    let (pcm, iso) = (wav("first"), image("after"));
    let started = played(&[pcm.clone(), iso.clone()]).expect("the queue plays");
    assert_eq!(
        started,
        vec![
            (0, None, None),
            (1, Some("This Ain't The Summer Of Love".into()), Some(64)),
            (2, Some("True Confessions".into()), Some(64)),
        ],
        "the image's two tracks follow the WAV, named by the disc"
    );
    let _ = std::fs::remove_file(pcm);
    let _ = std::fs::remove_file(iso);
}

#[test]
fn one_track_of_a_sacd_image_plays_on_its_own() {
    let iso = image("alone");
    let second = PathBuf::from(format!("sacd:{}#2", iso.display()));
    let started = played(&[second]).expect("a track of an image can be asked for by itself");
    assert_eq!(started, vec![(0, Some("True Confessions".into()), Some(64))]);
    let _ = std::fs::remove_file(iso);
}
