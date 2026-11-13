//! What the player says about a play that was replaced.
//!
//! Every event carries the generation of the play that said it. Starting
//! another play does not wait for the old one: it is told to stop and the new
//! one is queued behind it, so what the old play said last -- a track it
//! passed over, a track that started, how it ended -- is often read after
//! the new launch went out. None of it is about the queue in front of the
//! window: a failure of the old play is not the new one's, its end is not the
//! new queue running out, and a track it started is not what plays now. What
//! it still settles is the listen that was open when it was replaced.
//!
//! Every play ends with one event, the ones that never started too. So the
//! listen that is open and the mark of a relaunch each name the play they
//! belong to: a play that never opened a listen cannot write over the count
//! of one that waits to be picked up on another output, nor take the mark
//! that says how another play's listen is settled.

use chilpa_core::attempts::How;
use chilpa_core::player::PlayerEvent;

use crate::plays::ledger::{self, Verdict};
use crate::plays::{Fate, Replaced};
use crate::{Chilpa, Relaunch};

impl Chilpa {
    /// Whether `event` was said by a play the launch in flight replaced. If
    /// so it has been dealt with here: the listen left open is closed, or
    /// carried to the next output, and nothing of the window or the queue is
    /// touched.
    pub(crate) fn from_replaced_play(&mut self, generation: u64, event: &PlayerEvent) -> bool {
        if self.launch_generation().is_none_or(|current| current == generation) {
            return false;
        }
        // A listen carried to another output waits for the new play's first
        // track; the old play has nothing more to say about it.
        let listening = self.in_progress.as_ref().is_some_and(|listen| !listen.resuming && listen.play == generation);
        match event {
            // The old play went on to another track: the one the listen was
            // open for had ended. The new one was replaced before anybody
            // read that it started, and opens no listen.
            PlayerEvent::Started { .. } if listening => self.log_play(false),
            PlayerEvent::Progress { played, .. } if listening => self.set_played(*played),
            // Heard up to where it broke: a listen cut short, whatever queue
            // plays now.
            PlayerEvent::Skipped { path, played: Some(frames), .. }
                if listening && self.in_progress.as_ref().is_some_and(|listen| &listen.path == path) =>
            {
                self.set_played(*frames);
                self.log_play(true);
            }
            PlayerEvent::Finished { frames } | PlayerEvent::Stopped { frames } => {
                if listening {
                    self.set_played(*frames);
                }
                self.replaced_play_ended(generation, listening, !matches!(event, PlayerEvent::Finished { .. }));
            }
            PlayerEvent::Failed { .. } => self.replaced_play_ended(generation, listening, true),
            _ => {}
        }
        chilpa_core::trace_playback("facade", "replaced_play_event", format_args!("generation={generation} listening={listening}"));
        true
    }

    /// The replaced play is over, however it ended: the relaunch that
    /// replaced it is settled here and waits for no `Stopped`, which a play
    /// that failed or ran out by itself never says.
    fn replaced_play_ended(&mut self, generation: u64, listening: bool, cut_short: bool) {
        let why = self.take_relaunch(generation);
        if !listening {
            return;
        }
        match why {
            // The delivered time is carried into the next output segment.
            Some(Relaunch::Output) => {
                if let Some(current) = self.in_progress.as_mut() {
                    current.resuming = true;
                }
            }
            _ => self.log_play(cut_short),
        }
    }

    /// A track that started is over: `heard` frames of it left the device
    /// and it ended as `how`. If the open listen is that track's, it is
    /// settled here, by how the track ended and what became of its play:
    /// written, or kept to be picked up on another output. A listen that was
    /// already closed the older way -- by the track after it starting, or by
    /// a break said first -- is not this one any more, and nothing happens.
    pub(crate) fn listen_closed(&mut self, generation: u64, attempt: u64, heard: u64, how: How) {
        let Some(listen) = self.in_progress.as_mut().filter(|listen| !listen.resuming && listen.play == generation && listen.attempt == attempt) else { return };
        listen.played_frames = heard;
        self.scrobble_if_heard();
        let fate = self.fate_of(generation);
        chilpa_core::trace_playback("facade", "listen_closed", format_args!("generation={generation} attempt={attempt} heard={heard} how={how:?} fate={fate:?}"));
        match ledger::verdict(how, fate) {
            Verdict::Write { skipped } => self.log_play(skipped),
            Verdict::Park => {
                if let Some(listen) = self.in_progress.as_mut() {
                    listen.resuming = true;
                }
            }
        }
    }

    /// What became of a play, as the mark of a relaunch and the launch in
    /// flight say it.
    fn fate_of(&self, generation: u64) -> Fate {
        match self.relaunching {
            Some((Relaunch::Output, owner)) if owner == generation => Fate::Replaced(Replaced::Output { by: self.launch_generation().unwrap_or(0) }),
            Some((Relaunch::Track, owner)) if owner == generation => Fate::Replaced(Replaced::Track),
            _ if self.launch_generation().is_none_or(|current| current == generation) => Fate::Current,
            _ => Fate::Replaced(Replaced::Track),
        }
    }

    /// Marks why what plays is being replaced, for the play whose ending
    /// settles the listen: the one that owns the listen while it is open,
    /// and otherwise the launch being replaced. Replacing a launch that
    /// never opened a listen does not take the mark away from the play
    /// whose listen still waits for its own ending; the reason is always
    /// the newest request's.
    pub(crate) fn mark_relaunch(&mut self, why: Relaunch) {
        let owner = match &self.in_progress {
            Some(listen) if !listen.resuming => listen.play,
            _ => self.launch_generation().unwrap_or(0),
        };
        self.relaunching = Some((why, owner));
    }

    /// The mark, if it is this play's to take.
    pub(crate) fn take_relaunch(&mut self, generation: u64) -> Option<Relaunch> {
        match self.relaunching {
            Some((why, owner)) if owner == generation => {
                self.relaunching = None;
                Some(why)
            }
            _ => None,
        }
    }

    /// Whether the open listen is this play's.
    pub(crate) fn owns_listen(&self, generation: u64) -> bool {
        self.in_progress.as_ref().is_some_and(|listen| listen.play == generation)
    }
}
