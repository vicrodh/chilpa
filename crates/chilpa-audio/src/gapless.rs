//! Joining two tracks with no gap and no extra samples.
//!
//! Two things have to be solved for gapless to be real:
//!
//! 1. **The encoder's padding.** AAC and MP3 carry leading and trailing
//!    frames the encoder added that are not music. Left in, you hear a
//!    click between tracks; trimmed too far, it eats music. The trim has to
//!    be exact even when it falls in the middle of a block.
//! 2. **The format.** You can only carry on uncut if the next track plays at
//!    the SAME stream format. If the rate or the bits change, the device
//!    has to be renegotiated, and that always leaves a gap: it is stated,
//!    not covered up.

use crate::format::StreamFormat;

/// What can be done on reaching the next track.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transition {
    /// Same format: keep writing to the same device, without cutting or
    /// draining the buffer.
    Seamless,
    /// The format changes: the device has to be reopened. The gap is
    /// unavoidable, and the user deserves to know why.
    Renegotiate { from: StreamFormat, to: StreamFormat },
}

impl Transition {
    pub fn is_seamless(&self) -> bool {
        matches!(self, Transition::Seamless)
    }
}

/// Decides how to move from one track to the next.
pub fn transition(current: StreamFormat, next: StreamFormat) -> Transition {
    if current.same_stream(&next) {
        Transition::Seamless
    } else {
        Transition::Renegotiate { from: current, to: next }
    }
}

/// Trims the encoder's padding by counting frames, not blocks.
///
/// Trimming the end needs to know how many frames the track has; if the
/// container does not say, the last `padding` frames are held back until it
/// is known whether they were the last (`finish`).
#[derive(Debug)]
pub struct Trimmer {
    delay: u64,
    padding: usize,
    channels: usize,
    consumed: u64,
    /// Frames held back until it is known whether they are the end.
    queue: Vec<i32>,
}

impl Trimmer {
    /// `delay` and `padding` in frames, as the container declares them.
    pub fn new(delay: u64, padding: u32, channels: u16) -> Self {
        Self {
            delay,
            padding: padding as usize,
            channels: channels.max(1) as usize,
            consumed: 0,
            queue: Vec::new(),
        }
    }

    /// Frames that have gone out, not counting the trimmed ones.
    pub fn emitted(&self) -> u64 {
        self.consumed
    }

    /// Runs a block through the trim. What comes back is music.
    pub fn push(&mut self, block: &[i32]) -> Vec<i32> {
        let mut data = std::mem::take(&mut self.queue);
        data.extend_from_slice(block);

        // The leading trim: it can span several blocks.
        let pending = self.delay.saturating_sub(self.consumed);
        let frames = data.len() / self.channels;
        let skip = pending.min(frames as u64) as usize;
        if skip > 0 {
            data.drain(..skip * self.channels);
            self.consumed += skip as u64;
        }

        // The last `padding` frames are held back in case they are the tail.
        let frames = data.len() / self.channels;
        let hold = self.padding.min(frames);
        let cut = (frames - hold) * self.channels;
        self.queue = data.split_off(cut);
        self.consumed += (data.len() / self.channels) as u64;
        data
    }

    /// The track is over: what was held back was padding and is dropped.
    pub fn finish(&mut self) -> Vec<i32> {
        self.queue.clear();
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::SampleFormat;

    fn fmt(rate: u32) -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, rate, 2)
    }

    fn frames(from: i32, n: i32) -> Vec<i32> {
        (from..from + n).flat_map(|v| [v, -v]).collect()
    }

    #[test]
    fn the_same_format_chains_without_a_gap() {
        assert!(transition(fmt(44_100), fmt(44_100)).is_seamless());
    }

    #[test]
    fn changing_rate_forces_a_renegotiation_and_says_so() {
        let t = transition(fmt(44_100), fmt(96_000));
        assert!(!t.is_seamless());
        assert!(matches!(t, Transition::Renegotiate { .. }));
    }

    #[test]
    fn without_padding_nothing_is_trimmed() {
        let mut t = Trimmer::new(0, 0, 2);
        let output = t.push(&frames(0, 10));
        assert_eq!(output, frames(0, 10));
        assert_eq!(t.emitted(), 10);
    }

    #[test]
    fn the_start_trim_spans_blocks() {
        // 5 frames of delay spread across two blocks of 3.
        let mut t = Trimmer::new(5, 0, 2);
        assert!(t.push(&frames(0, 3)).is_empty(), "el primer bloque es todo relleno");
        let second = t.push(&frames(3, 3));
        assert_eq!(second, frames(5, 1), "solo sale el frame 5 en adelante");
    }

    #[test]
    fn the_tail_trim_keeps_the_final_padding_in() {
        let mut t = Trimmer::new(0, 4, 2);
        let primero = t.push(&frames(0, 10));
        assert_eq!(primero, frames(0, 6), "se retienen los 4 últimos");
        assert!(t.finish().is_empty(), "el relleno retenido se tira");
        assert_eq!(t.emitted(), 6, "la pista duró 6 frames de música");
    }

    #[test]
    fn what_was_held_comes_out_when_more_music_arrives() {
        let mut t = Trimmer::new(0, 4, 2);
        t.push(&frames(0, 10));
        let second = t.push(&frames(10, 2));
        assert_eq!(second, frames(6, 2), "lo retenido resulta ser música");
    }

    #[test]
    fn delay_and_padding_together_leave_the_track_exact() {
        let mut t = Trimmer::new(3, 2, 2);
        let mut output = t.push(&frames(0, 12));
        output.extend(t.finish());
        assert_eq!(output, frames(3, 7), "12 frames menos 3 de arranque y 2 de cola");
    }
}
