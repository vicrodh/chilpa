//! One play, from the order that asks for it to the single event that ends it.
//!
//! A launch owns what used to be shared between a play and the one replacing
//! it: its queue, its stop, its seek and Next, its stop mark and where it
//! starts. Its number is unique in the process, so a player made again
//! does not count over the numbers of the one before it. And it ends exactly
//! once: played, cancelled while it waited, left in the channel when the
//! player closed, or cut by a panic, it says one terminal event and nothing
//! after it.

use std::num::NonZeroU64;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::player::PlayerEvent;
use crate::queue_playback::{Controls, SharedQueue};

/// The number of a launch: never zero, never given twice in a process.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LaunchId(NonZeroU64);

static NEXT: AtomicU64 = AtomicU64::new(1);

impl LaunchId {
    /// The next number. Running out of them is not survived: giving one
    /// twice would hand a play the events of another.
    pub(crate) fn next() -> Self {
        Self::from_counter(&NEXT).expect("launch numbers ran out")
    }

    fn from_counter(counter: &AtomicU64) -> Option<Self> {
        let taken = counter.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1)).ok()?;
        NonZeroU64::new(taken).map(Self)
    }

    pub fn get(self) -> u64 {
        self.0.get()
    }
}

/// What a play is asked with.
#[derive(Clone, Debug, Default)]
pub struct LaunchSpec {
    /// The queue, whole: its earlier rows stay in it.
    pub paths: Vec<PathBuf>,
    pub device: String,
    /// The position it starts from; the first of its order when there is
    /// none.
    pub start_index: Option<usize>,
    /// Where the first track starts, when it is not at its beginning.
    pub start_at: Option<std::time::Duration>,
    pub paused: bool,
    /// The positions it passes without opening.
    pub removed: Vec<bool>,
    /// The position it stops after.
    pub stop_after: Option<usize>,
    /// The order it walks the queue in; empty for the queue's own. One that
    /// is not a permutation of the queue's positions is ignored.
    pub order: Vec<usize>,
}

/// A launch, as whoever asked for it holds it: what is asked through it
/// reaches that play and no other, and nothing once the play is over.
#[derive(Clone)]
pub struct LaunchHandle(Arc<Launch>);

impl LaunchHandle {
    pub(crate) fn of(launch: Arc<Launch>) -> Self {
        Self(launch)
    }

    pub fn id(&self) -> LaunchId {
        self.0.id
    }

    /// Whether it has said how it ended.
    pub fn ended(&self) -> bool {
        self.0.settled()
    }

    /// Asks to seek to that point of its playing track.
    pub fn seek(&self, position: std::time::Duration) {
        chilpa_audio::diagnostics::trace("player", "seek_requested", format_args!("launch={} position={position:?}", self.0.id.get()));
        self.0.controls.seek.request(position);
    }

    /// Skips to its next track.
    pub fn next(&self) {
        chilpa_audio::diagnostics::trace("player", "skip_requested", format_args!("launch={}", self.0.id.get()));
        self.0.controls.skip.skip();
    }

    /// Stops after this position of its queue, or clears the mark.
    pub fn set_stop_after(&self, index: Option<usize>) {
        self.0.controls.stop_after.set(index);
    }

    /// Adds tracks to the end of its queue, without cutting it off. Returns
    /// how many it holds.
    pub fn enqueue(&self, paths: &[PathBuf]) -> usize {
        self.0.queue.extend(paths)
    }

    /// Takes a position out of its walk. The one playing is not removed.
    pub fn remove(&self, index: usize) -> bool {
        self.0.queue.remove(index)
    }

    pub fn queue_len(&self) -> usize {
        self.0.queue.len()
    }

    /// Sets the order it walks its queue in, from the end of the playing
    /// track on. One that is not a permutation of `0..len` is refused.
    pub fn set_order(&self, order: Vec<usize>, len: usize) -> bool {
        self.0.controls.order.set(order, len)
    }

    /// The order it walks its queue in; empty when it is the queue's own.
    pub fn order(&self) -> Vec<usize> {
        self.0.controls.order.get()
    }

    /// Goes back to the order things were queued in.
    pub fn clear_order(&self) {
        self.0.controls.order.clear();
    }
}

/// What a launch owns, shared by whoever asked for it and the thread that
/// plays it.
pub(crate) struct Launch {
    pub id: LaunchId,
    pub queue: SharedQueue,
    /// The player's settings, with this launch's own stop, seek, Next, stop
    /// mark, start and hand-off.
    pub controls: Controls,
    /// Why the output refused it, said before it stopped itself: how it ends.
    pub verdict: Mutex<Option<String>>,
    /// A later request was sent before this one was taken.
    superseded: AtomicBool,
    settled: AtomicBool,
}

impl Launch {
    pub fn new(queue: SharedQueue, controls: Controls) -> Arc<Self> {
        Arc::new(Self {
            id: LaunchId::next(),
            queue,
            controls,
            verdict: Mutex::new(None),
            superseded: AtomicBool::new(false),
            settled: AtomicBool::new(false),
        })
    }

    /// Asks it to stop; `handoff` when another play is about to take the
    /// device. Never undone.
    pub fn cancel(&self, handoff: bool) {
        self.controls.handoff.set(handoff);
        self.controls.stop.stop();
    }

    /// A later request went out: one that has not started will not.
    pub fn supersede(&self) {
        self.superseded.store(true, Ordering::Release);
    }

    /// Whether it must not start: it was stopped or replaced while it waited.
    pub fn called_off(&self) -> bool {
        self.controls.stop.is_stopped() || self.superseded.load(Ordering::Acquire)
    }

    pub fn settled(&self) -> bool {
        self.settled.load(Ordering::Acquire)
    }

    pub fn take_verdict(&self) -> Option<String> {
        self.verdict.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take()
    }

    pub fn refuse(&self, why: String) {
        *self.verdict.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(why);
        self.controls.stop.stop();
    }
}

/// The player thread's half of a launch: what it needs to play it, and the
/// one place its terminal event is said.
pub(crate) struct LaunchRun {
    pub launch: Arc<Launch>,
    pub device: String,
    /// Where the first track starts, when it is not at its beginning.
    pub start_at: Option<std::time::Duration>,
    events: Sender<(u64, PlayerEvent)>,
}

impl LaunchRun {
    pub fn new(launch: Arc<Launch>, device: String, start_at: Option<std::time::Duration>, events: Sender<(u64, PlayerEvent)>) -> Self {
        Self { launch, device, start_at, events }
    }

    /// Where this launch says what happens while it plays.
    pub fn events(&self) -> Sender<(u64, PlayerEvent)> {
        self.events.clone()
    }

    /// Says how the launch ended, once. A second call says nothing.
    pub fn settle(&self, event: PlayerEvent) {
        if !self.launch.settled.swap(true, Ordering::AcqRel) {
            let _ = self.events.send((self.launch.id.get(), event));
        }
    }
}

impl Drop for LaunchRun {
    /// A launch let go without having played ends as stopped: called off
    /// while it waited, or still in the channel when the player closed.
    fn drop(&mut self) {
        self.settle(PlayerEvent::Stopped { frames: 0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn numbers_grow_and_are_never_zero() {
        let first = LaunchId::next();
        let second = LaunchId::next();
        assert!(second > first, "{first:?} then {second:?}");
        assert!(first.get() > 0);
    }

    #[test]
    fn numbers_running_out_are_refused_instead_of_starting_over() {
        let counter = AtomicU64::new(u64::MAX);
        assert_eq!(LaunchId::from_counter(&counter), None, "the last number is never handed out twice");
        assert_eq!(counter.load(Ordering::Acquire), u64::MAX);
        let zero = AtomicU64::new(0);
        assert_eq!(LaunchId::from_counter(&zero), None, "zero is no launch");
    }

    #[test]
    fn a_launch_ends_once_however_often_it_is_told() {
        let (to, heard) = channel();
        let launch = Launch::new(SharedQueue::default(), Controls::new());
        let id = launch.id.get();
        let run = LaunchRun::new(Arc::clone(&launch), "fake".into(), None, to);
        run.settle(PlayerEvent::Finished { frames: 3 });
        run.settle(PlayerEvent::Failed { message: "late".into() });
        drop(run);
        let said: Vec<_> = heard.try_iter().collect();
        assert_eq!(said, [(id, PlayerEvent::Finished { frames: 3 })]);
        assert!(launch.settled());
    }

    #[test]
    fn a_launch_dropped_unplayed_ends_as_stopped() {
        let (to, heard) = channel();
        let launch = Launch::new(SharedQueue::default(), Controls::new());
        let id = launch.id.get();
        drop(LaunchRun::new(launch, "fake".into(), None, to));
        assert_eq!(heard.try_iter().collect::<Vec<_>>(), [(id, PlayerEvent::Stopped { frames: 0 })]);
    }

    #[test]
    fn a_refusal_stops_the_launch_and_is_kept_to_be_said_once() {
        let launch = Launch::new(SharedQueue::default(), Controls::new());
        launch.refuse("the output would not take it".into());
        assert!(launch.controls.stop.is_stopped());
        assert_eq!(launch.take_verdict().as_deref(), Some("the output would not take it"));
        assert_eq!(launch.take_verdict(), None);
    }
}
