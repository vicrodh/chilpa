//! Opening a stream that never ends.
//!
//! A station does not stop sending, so whatever opens it must not read
//! until it finds what it expects. Both cases here hung the player for good
//! before: the thread that opened the stream never came back, and every
//! track asked for afterwards waited behind it.

use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The same bytes over and over, counting how many were taken.
struct Endless {
    chunk: Vec<u8>,
    at: usize,
    taken: Arc<AtomicUsize>,
}

impl Read for Endless {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = buf.len().min(self.chunk.len() - self.at);
        buf[..n].copy_from_slice(&self.chunk[self.at..self.at + n]);
        self.at = (self.at + n) % self.chunk.len();
        self.taken.fetch_add(n, Ordering::Relaxed);
        Ok(n)
    }
}

/// Opens the stream on another thread and waits a bounded time for it.
fn open(chunk: Vec<u8>, content_type: &str) -> (Option<Result<chilpa_audio::format::StreamFormat, String>>, usize) {
    let taken = Arc::new(AtomicUsize::new(0));
    let reader = Endless { chunk, at: 0, taken: taken.clone() };
    let info = chilpa_stream::StreamInfo { content_type: Some(content_type.into()), ..Default::default() };
    let stream = chilpa_stream::HttpStream::new(Box::new(reader), info, chilpa_stream::NowPlaying::new());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let opened = chilpa_decode::Decoded::open_stream(stream);
        let _ = tx.send(opened.map(|d| d.stream_format()).map_err(|e| e.to_string()));
    });
    let started = Instant::now();
    let answer = rx.recv_timeout(Duration::from_secs(20)).ok();
    assert!(answer.is_none() || started.elapsed() < Duration::from_secs(20));
    (answer, taken.load(Ordering::Relaxed))
}

/// One ADTS frame as an MPEG-2 station sends it: the header starts `FF F9`,
/// AAC LC at 22,050 Hz in stereo, 270 bytes in all. The payload is empty
/// audio: what is checked is that the stream opens, not how it sounds.
fn mpeg2_adts_frame() -> Vec<u8> {
    let mut frame = vec![0xff, 0xf9, 0x5c, 0x80, 0x21, 0xc1, 0xc8];
    frame.resize(270, 0);
    frame
}

#[test]
fn an_aac_station_with_the_mpeg2_header_opens() {
    let (answer, _) = open(mpeg2_adts_frame(), "audio/aacp");
    let format = answer.expect("opening came back").expect("and it opened");
    assert_eq!(format.sample_rate_hz, 22_050);
    assert_eq!(format.channels, 2);
}

#[test]
fn a_stream_nobody_recognizes_is_refused_instead_of_read_forever() {
    // Not audio of any kind, and no end to it.
    let (answer, taken) = open(vec![0x55; 4096], "application/octet-stream");
    let refused = answer.expect("opening came back");
    assert!(refused.is_err(), "it is not a format: {refused:?}");
    assert!(taken < 4 * 1024 * 1024, "it stopped looking after its budget, having read {taken} bytes");
}
