//! An output that writes to a file instead of to a DAC.
//!
//! It is good for two things: exercising the whole path without making a
//! sound, and checking bit-perfect against what actually went out. It
//! neither resamples nor converts: it takes the format it is given and
//! stores the bytes as they are.

use crate::format::StreamFormat;
use crate::negotiate::DeviceCaps;
use crate::sink::{AudioSink, SinkError};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// The identifier this output is asked for by.
pub const CAPTURE_ID: &str = "capture";

/// The capture's caps: it accepts any integer format, so it never forces
/// a conversion.
pub fn capabilities() -> DeviceCaps {
    use crate::format::SampleFormat::*;
    DeviceCaps {
        sample_rates: vec![
            44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000, 705_600, 768_000,
        ],
        formats: vec![S16, S24Packed, S24In32, S32],
        channels: vec![1, 2, 4, 6, 8],
        // Really measured: this output knows exactly what it accepts.
        probed: true,
    }
}

/// Stores what was delivered in a raw file.
pub struct CaptureSink {
    file: BufWriter<File>,
    stream: StreamFormat,
    frames: u64,
}

impl CaptureSink {
    pub fn create(path: &Path, stream: StreamFormat) -> Result<Self, SinkError> {
        let file = File::create(path).map_err(|e| SinkError::Backend(e.to_string()))?;
        Ok(Self { file: BufWriter::new(file), stream, frames: 0 })
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }
}

impl AudioSink for CaptureSink {
    fn format(&self) -> StreamFormat {
        self.stream
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        self.file.write_all(frames).map_err(|e| SinkError::Backend(e.to_string()))?;
        let n = frames.len() / self.stream.frame_bytes();
        self.frames += n as u64;
        Ok(n)
    }

    fn close(&mut self) -> Result<(), SinkError> {
        self.file.flush().map_err(|e| SinkError::Backend(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::SampleFormat;

    #[test]
    fn what_was_delivered_lands_in_the_file_unchanged() {
        let mut path = std::env::temp_dir();
        path.push("chilpa-capture-test.raw");
        let stream = StreamFormat::new(SampleFormat::S16, 44_100, 2);
        let mut sink = CaptureSink::create(&path, stream).unwrap();
        let data: Vec<u8> = (0..40u8).collect();
        assert_eq!(sink.write(&data).unwrap(), 10);
        sink.close().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), data);
        assert_eq!(sink.frames(), 10);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn capture_accepts_whatever_it_is_given_and_declares_itself_measured() {
        let caps = capabilities();
        assert!(caps.probed, "without this bit-perfect could not be promised");
        assert!(caps.accepts(&StreamFormat::new(SampleFormat::S24In32, 192_000, 2)));
    }
}
