//! A passive stereo history aligned with frames still queued by the device.
//!
//! I retain half a second plus the 2,048-frame analysis window, at most
//! 1,048,576 floats (4 MiB) per tap. The control thread allocates on visibility
//! or format changes and frees on hide; writes and snapshots never allocate.

const WINDOW_FRAMES: usize = 2048;
const MAX_SAMPLES: usize = 1_048_576;

#[derive(Default)]
pub(super) struct History {
    samples: Vec<f32>,
    cursor: usize,
    captured: u64,
    delay: u64,
}

impl History {
    pub fn prepare(&mut self, rate: u32, active: bool) {
        let frames = if active && rate != 0 {
            (rate as usize / 2 + WINDOW_FRAMES).min(MAX_SAMPLES / 2)
        } else { 0 };
        if frames == 0 {
            self.samples = Vec::new();
        } else if self.samples.len() != frames * 2 {
            // I release the old rate's allocation before reserving its replacement;
            // format changes never hold two history rings at once.
            self.samples = Vec::new();
            self.samples = vec![0.0; frames * 2];
        }
        self.reset();
    }

    pub fn reset(&mut self) {
        // The capture count excludes old storage after a seek or reopen.
        self.cursor = 0;
        self.captured = 0;
        self.delay = 0;
    }

    pub fn push(&mut self, stereo: [f32; 2]) {
        if self.samples.is_empty() { return; }
        self.samples[self.cursor..self.cursor + 2].copy_from_slice(&stereo);
        self.cursor = (self.cursor + 2) % self.samples.len();
        self.captured = self.captured.saturating_add(1);
    }

    pub fn set_delay(&mut self, frames: u64) { self.delay = frames; }

    pub fn snapshot(&self, out: &mut [f32]) -> bool {
        out.fill(0.0);
        if self.samples.is_empty() || self.captured == 0 || out.len() % 2 != 0 { return false; }
        let capacity = self.samples.len() / 2;
        let window = (out.len() / 2).min(capacity);
        let lag = self.delay.min(capacity.saturating_sub(window) as u64).min(self.captured) as usize;
        let available = (self.captured - lag as u64).min(window as u64) as usize;
        let end = (self.cursor / 2 + capacity - lag) % capacity;
        let start = (end + capacity - available) % capacity;
        let destination = out.len() - available * 2;
        let first = available.min(capacity - start) * 2;
        out[destination..destination + first].copy_from_slice(&self.samples[start * 2..start * 2 + first]);
        out[destination + first..].copy_from_slice(&self.samples[..available * 2 - first]);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(frames: usize) -> History {
        History { samples: vec![0.0; frames * 2], ..History::default() }
    }

    #[test]
    fn queued_pcm_does_not_advance_the_audible_window() {
        let mut ring = history(16);
        for frame in 0..10 { ring.push([frame as f32, -(frame as f32)]); }
        ring.set_delay(3);
        let mut out = [0.0; 8];
        assert!(ring.snapshot(&mut out));
        assert_eq!(out, [3.0, -3.0, 4.0, -4.0, 5.0, -5.0, 6.0, -6.0]);
        // The device accepted another chunk, but has not played anything yet.
        for frame in 10..14 { ring.push([frame as f32, -(frame as f32)]); }
        ring.set_delay(7);
        let mut again = [0.0; 8];
        assert!(ring.snapshot(&mut again));
        assert_eq!(again, out);
        ring.set_delay(3);
        assert!(ring.snapshot(&mut again));
        assert_eq!(again, [7.0, -7.0, 8.0, -8.0, 9.0, -9.0, 10.0, -10.0]);
    }

    #[test]
    fn missing_history_is_silent_and_seek_never_reuses_old_audio() {
        let mut ring = history(8);
        ring.push([1.0, 2.0]);
        ring.push([3.0, 4.0]);
        ring.set_delay(1);
        let mut out = [9.0; 8];
        assert!(ring.snapshot(&mut out));
        assert_eq!(out, [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0]);
        ring.reset();
        assert!(!ring.snapshot(&mut out));
        assert_eq!(out, [0.0; 8]);
        ring.push([7.0, 8.0]);
        assert!(ring.snapshot(&mut out));
        assert_eq!(out, [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 7.0, 8.0]);
    }

    #[test]
    fn wrapped_and_excessive_delays_stay_inside_retained_history() {
        let mut ring = history(8);
        for frame in 0..12 { ring.push([frame as f32; 2]); }
        ring.set_delay(u64::MAX);
        let mut out = [0.0; 8];
        assert!(ring.snapshot(&mut out));
        assert_eq!(out, [4.0, 4.0, 5.0, 5.0, 6.0, 6.0, 7.0, 7.0]);
        ring.set_delay(0);
        assert!(ring.snapshot(&mut out));
        assert_eq!(out, [8.0, 8.0, 9.0, 9.0, 10.0, 10.0, 11.0, 11.0]);
    }

    #[test]
    fn memory_is_rate_bounded_reused_per_frame_and_released_when_hidden() {
        let mut ring = History::default();
        ring.prepare(48_000, true);
        assert_eq!(ring.samples.len(), (24_000 + WINDOW_FRAMES) * 2);
        let address = ring.samples.as_ptr();
        let mut out = [0.0; 4096];
        for frame in 0..100 {
            ring.push([frame as f32; 2]);
            ring.snapshot(&mut out);
            assert_eq!(ring.samples.as_ptr(), address);
        }
        ring.prepare(u32::MAX, true);
        assert_eq!(ring.samples.len(), MAX_SAMPLES);
        ring.prepare(768_000, false);
        assert_eq!(ring.samples.capacity(), 0);
    }
}
