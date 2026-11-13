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

use chilpa_core::player::PlayerEvent;

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
        let listening = self.in_progress.as_ref().is_some_and(|listen| !listen.resuming);
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
                self.replaced_play_ended(listening, !matches!(event, PlayerEvent::Finished { .. }));
            }
            PlayerEvent::Failed { .. } => self.replaced_play_ended(listening, true),
            _ => {}
        }
        chilpa_core::trace_playback("facade", "replaced_play_event", format_args!("generation={generation} listening={listening}"));
        true
    }

    /// The replaced play is over, however it ended: the relaunch that
    /// replaced it is settled here and waits for no `Stopped`, which a play
    /// that failed or ran out by itself never says.
    fn replaced_play_ended(&mut self, listening: bool, cut_short: bool) {
        let why = self.relaunching.take();
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
}
