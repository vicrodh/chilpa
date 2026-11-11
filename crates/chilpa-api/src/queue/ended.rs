//! What the window shows once a queue has played to its end.
//!
//! The player reports `Finished` when its order is done, which is also how a
//! "stop after this track" mark ends. Only the first is the queue running
//! out: the last track of the queue finished, with nothing left to play and
//! no mark asking to stop sooner; what is left but cannot be played
//! (`unavailable.rs`) does not count. Then nothing is playing, and the player
//! says so instead of keeping the last track on its face with a position.
//! The queue itself stays, and Play starts its last track again.

use crate::{Chilpa, NowPlaying};

impl Chilpa {
    /// The player's order finished. Clears what was playing when the queue
    /// ran out, and the stop marks either way.
    pub(crate) fn finish_queue(&mut self) {
        let ran_out = self.stop_after.is_none() && !self.tracks_left_after(self.now.index);
        self.stop_after = None;
        self.sleep_until = None;
        self.log_play(false);
        if ran_out {
            self.show_nothing_playing();
        }
    }

    /// Whether a track the queue would still play stands after `index` in
    /// the order it is heard in. One found unavailable this run will not.
    fn tracks_left_after(&self, index: usize) -> bool {
        let order = self.full_queue_order();
        let Some(at) = order.iter().position(|&i| i == index) else { return false };
        order[at + 1..].iter().any(|&i| !self.removed.get(i).copied().unwrap_or(false) && !self.marked_at(i))
    }

    /// The window as before anything was played, but for the queue: where
    /// it stands and how long it is. What was skipped on the way is still
    /// said.
    fn show_nothing_playing(&mut self) {
        let NowPlaying { index, queue_len, lyrics_revision, .. } = self.now;
        let error = if self.skipped_any() { std::mem::take(&mut self.now.error) } else { String::new() };
        self.now = NowPlaying { index, queue_len, lyrics_revision: lyrics_revision + 1, error, ..NowPlaying::default() };
    }
}
