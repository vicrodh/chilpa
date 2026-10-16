//! The queue and the playback state.

use crate::track::TrackRef;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// One item of the queue: which track, and where it was enqueued from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct QueueItem {
    pub track: TrackRef,
    /// The container it came from (album, playlist, artist), for the "you are
    /// listening from". Stamped at enqueue time, per item, so a stale global
    /// value never lingers.
    pub context: Option<String>,
}

impl QueueItem {
    pub fn new(track: TrackRef) -> Self {
        Self { track, context: None }
    }
}

/// The queue, with its playback order made explicit.
///
/// `order` is indices into `items`. Chilpa never generates an order of its
/// own when someone else is in charge: if a remote service governs the
/// queue, the order that service returns is applied with
/// [`Queue::set_order`]. Generating a local order in parallel is exactly the
/// bug that kept coming back in QBZ and that four models made again.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Queue {
    items: Vec<QueueItem>,
    order: Vec<usize>,
    position: usize,
    pub repeat: RepeatMode,
}

impl Queue {
    pub fn new(items: Vec<QueueItem>) -> Self {
        let order = (0..items.len()).collect();
        Self { items, order, position: 0, repeat: RepeatMode::Off }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// What is playing now, if anything.
    pub fn current(&self) -> Option<&QueueItem> {
        self.order.get(self.position).and_then(|i| self.items.get(*i))
    }

    /// Applies an order that comes from outside. One that is not an exact
    /// permutation of the queue is refused, so the queue is never left partial.
    pub fn set_order(&mut self, order: Vec<usize>) -> Result<(), OrderError> {
        if order.len() != self.items.len() {
            return Err(OrderError::WrongLength { expected: self.items.len(), got: order.len() });
        }
        let mut seen = vec![false; self.items.len()];
        for &i in &order {
            match seen.get_mut(i) {
                Some(seen) if !*seen => *seen = true,
                Some(_) => return Err(OrderError::Repeated(i)),
                None => return Err(OrderError::OutOfRange(i)),
            }
        }
        let actual = self.current().cloned();
        self.order = order;
        // Keep playing the same thing: the position is recomputed over the new
        // order instead of jumping to another track.
        if let Some(actual) = actual {
            if let Some(p) = self.order.iter().position(|i| self.items[*i] == actual) {
                self.position = p;
            }
        }
        Ok(())
    }

    /// Advances. Returns `None` when the queue is done and repeat is off.
    pub fn advance(&mut self) -> Option<&QueueItem> {
        if self.items.is_empty() {
            return None;
        }
        match self.repeat {
            RepeatMode::One => {}
            RepeatMode::All => self.position = (self.position + 1) % self.order.len(),
            RepeatMode::Off => {
                if self.position + 1 >= self.order.len() {
                    return None;
                }
                self.position += 1;
            }
        }
        self.current()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OrderError {
    WrongLength { expected: usize, got: usize },
    Repeated(usize),
    OutOfRange(usize),
}

/// How far into the current track, counted in frames played.
///
/// QBZ measured position with the system clock in whole seconds, and that
/// is where the jumps and the double track changes came from. Frames are
/// the only thing that knows the truth of the output.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Progress {
    pub frames_played: u64,
    pub sample_rate_hz: u32,
}

impl Progress {
    pub fn position(&self) -> Duration {
        if self.sample_rate_hz == 0 {
            return Duration::ZERO;
        }
        Duration::from_nanos(self.frames_played * 1_000_000_000 / u64::from(self.sample_rate_hz))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    fn queue(n: usize) -> Queue {
        Queue::new(
            (0..n)
                .map(|i| QueueItem::new(TrackRef::new(SourceId::local(), i.to_string())))
                .collect(),
        )
    }

    #[test]
    fn the_queue_starts_on_the_first_track() {
        assert_eq!(queue(3).current().unwrap().track.external_id, "0");
    }

    #[test]
    fn an_order_from_outside_does_not_change_what_plays() {
        let mut q = queue(3);
        q.set_order(vec![2, 0, 1]).unwrap();
        assert_eq!(q.current().unwrap().track.external_id, "0", "the same track is still playing");
        assert_eq!(q.advance().unwrap().track.external_id, "1");
    }

    #[test]
    fn an_order_that_is_not_a_permutation_is_refused() {
        let mut q = queue(3);
        assert_eq!(q.set_order(vec![0, 1]), Err(OrderError::WrongLength { expected: 3, got: 2 }));
        assert_eq!(q.set_order(vec![0, 1, 1]), Err(OrderError::Repeated(1)));
        assert_eq!(q.set_order(vec![0, 1, 9]), Err(OrderError::OutOfRange(9)));
        assert_eq!(q.current().unwrap().track.external_id, "0", "the queue was not left half done");
    }

    #[test]
    fn without_repeat_the_queue_ends() {
        let mut q = queue(2);
        assert!(q.advance().is_some());
        assert!(q.advance().is_none());
    }

    #[test]
    fn repeat_all_wraps_around_and_repeat_one_stays_put() {
        let mut q = queue(2);
        q.repeat = RepeatMode::All;
        q.advance();
        assert_eq!(q.advance().unwrap().track.external_id, "0");
        let mut q = queue(2);
        q.repeat = RepeatMode::One;
        assert_eq!(q.advance().unwrap().track.external_id, "0");
    }

    #[test]
    fn position_is_counted_in_frames() {
        let p = Progress { frames_played: 44_100, sample_rate_hz: 44_100 };
        assert_eq!(p.position(), Duration::from_secs(1));
        let p = Progress { frames_played: 96_000 * 3 / 2, sample_rate_hz: 96_000 };
        assert_eq!(p.position(), Duration::from_millis(1500), "without truncating to whole seconds");
    }
}
