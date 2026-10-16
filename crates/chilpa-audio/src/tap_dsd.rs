//! Bounded copies of accepted DSD carrier frames, without DSP on the writer.

use crate::format::{SampleFormat, StreamFormat};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Encoding {
    pub format: StreamFormat,
    pub dop: bool,
}

impl Encoding {
    pub fn rate(self) -> Option<u32> {
        let bits = match (self.format.sample_format, self.dop) {
            (SampleFormat::DsdU32Be | SampleFormat::DsdU32Le, false) => 32,
            (SampleFormat::S24In32 | SampleFormat::S32 | SampleFormat::S24Packed, true) => 16,
            _ => return None,
        };
        self.format.sample_rate_hz.checked_mul(bits)
    }
}

#[derive(Default)]
pub(crate) struct RawTap {
    state: Mutex<State>,
    /// Advances even if a busy reader prevents a copy, exposing every hole.
    accepted: AtomicU64,
}

#[derive(Default)]
struct State {
    encoding: Option<Encoding>,
    bytes: Vec<u8>,
    cursor: usize,
    length: usize,
    end: u64,
    generation: u64,
}

pub(crate) struct Batch {
    pub encoding: Option<Encoding>,
    pub generation: u64,
    pub start: u64,
    pub end: u64,
    pub bytes: Vec<u8>,
}

impl RawTap {
    /// I allocate only when a stream is opened or its content is announced.
    pub fn prepare(&self, encoding: Encoding) {
        let Ok(mut state) = self.state.lock() else { return };
        state.encoding = encoding.rate().map(|_| encoding);
        let frame_bytes = encoding.format.frame_bytes();
        let capacity = if state.encoding.is_some() && frame_bytes > 0 {
            let wanted = u64::from(encoding.format.sample_rate_hz) * frame_bytes as u64 / 4;
            (wanted.min(MAX_BYTES as u64) as usize / frame_bytes) * frame_bytes
        } else { 0 };
        state.bytes.resize(capacity, 0);
        state.cursor = 0;
        state.length = 0;
        state.end = self.accepted.load(Ordering::Acquire);
        state.generation = state.generation.wrapping_add(1);
    }

    pub fn reset(&self) {
        let Ok(mut state) = self.state.lock() else { return };
        state.cursor = 0;
        state.length = 0;
        state.end = self.accepted.load(Ordering::Acquire);
        state.generation = state.generation.wrapping_add(1);
    }

    pub fn copy(&self, bytes: &[u8]) {
        if bytes.is_empty() { return; }
        let start = self.accepted.fetch_add(bytes.len() as u64, Ordering::AcqRel);
        let Ok(mut state) = self.state.try_lock() else { return };
        let capacity = state.bytes.len();
        if capacity == 0 { return; }
        if state.end != start {
            state.length = 0;
            state.cursor = 0;
        }
        state.end = start + bytes.len() as u64;
        let bytes = &bytes[bytes.len().saturating_sub(capacity)..];
        let cursor = state.cursor;
        let first = bytes.len().min(capacity - cursor);
        state.bytes[cursor..cursor + first].copy_from_slice(&bytes[..first]);
        state.bytes[..bytes.len() - first].copy_from_slice(&bytes[first..]);
        state.cursor = (cursor + bytes.len()) % capacity;
        state.length = (state.length + bytes.len()).min(capacity);
    }

    /// I consume each copied byte once. A missed final write invalidates its tail.
    pub fn drain(&self) -> Batch {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let accepted = self.accepted.load(Ordering::Acquire);
        if state.end != accepted {
            state.length = 0;
            state.end = accepted;
        }
        let capacity = state.bytes.len();
        let mut bytes = Vec::with_capacity(state.length);
        if state.length > 0 {
            let start = (state.cursor + capacity - state.length) % capacity;
            let first = state.length.min(capacity - start);
            bytes.extend_from_slice(&state.bytes[start..start + first]);
            bytes.extend_from_slice(&state.bytes[..state.length - first]);
        }
        let batch = Batch {
            encoding: state.encoding,
            generation: state.generation,
            start: state.end - bytes.len() as u64,
            end: state.end,
            bytes,
        };
        state.length = 0;
        batch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> RawTap {
        let raw = RawTap::default();
        raw.prepare(Encoding { format: StreamFormat::new(SampleFormat::DsdU32Be, 88_200, 2), dop: false });
        raw
    }

    #[test]
    fn contention_is_a_position_gap_and_new_data_is_consumed_once() {
        let raw = raw();
        raw.copy(&[1; 8]);
        let first = raw.drain();
        let guard = raw.state.lock().unwrap();
        raw.copy(&[2; 16]);
        drop(guard);
        raw.copy(&[3; 8]);
        let next = raw.drain();
        assert_eq!(next.start, first.end + 16);
        assert_eq!(next.bytes, [3; 8]);
        assert!(raw.drain().bytes.is_empty());
    }

    #[test]
    fn overrun_retains_only_aligned_latest_frames_and_reset_changes_generation() {
        let raw = raw();
        let capacity = raw.state.lock().unwrap().bytes.len();
        assert_eq!(capacity, 88_200 / 4 * 8);
        let bytes: Vec<u8> = (0..capacity + 80).map(|i| i as u8).collect();
        raw.copy(&bytes);
        let batch = raw.drain();
        assert_eq!(batch.start, 80);
        assert_eq!(batch.bytes, bytes[80..]);
        raw.reset();
        let reset = raw.drain();
        assert_ne!(batch.generation, reset.generation);
        assert!(reset.bytes.is_empty());
    }

    #[test]
    fn a_missed_tail_is_not_replayed_after_the_gap() {
        let raw = raw();
        raw.copy(&[1; 8]);
        let guard = raw.state.lock().unwrap();
        raw.copy(&[2; 8]);
        drop(guard);
        let batch = raw.drain();
        assert!(batch.bytes.is_empty());
        assert_eq!(batch.end, 16);
    }
}
