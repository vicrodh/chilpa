//! One pending conversion choice and one queue-scoped consent; no background worker.
use crate::{Chilpa, Relaunch};
use std::{path::Path, time::Duration};
struct Consent {
    output_key: String,
    epoch: u64,
}
struct Offer {
    token: u64,
    output: String,
    output_key: String,
    selection: String,
    path: String,
    index: usize,
    epoch: u64,
    position: f64,
    requested: u16,
}
#[derive(Default)]
pub(crate) struct State {
    revision: u64,
    pending: Option<Offer>,
    consent: Option<Consent>,
    output_key: String,
}
impl State {
    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }
    fn clear(&mut self) {
        if self.pending.take().is_some() {
            self.changed();
        }
    }
}
impl Chilpa {
    fn channel_snapshot_valid(&self, offer: &Offer) -> bool {
        !self.casting.active()
            && !self.casting.disconnecting
            && offer.epoch == self.rows_epoch
            && offer.output_key == self.audio_output.channels.output_key
            && self.queue.get(offer.index) == Some(&offer.path)
            && self
                .removed
                .get(offer.index)
                .is_some_and(|removed| !*removed)
            && !self.marked_at(offer.index)
    }
    pub(crate) fn reset_channel_consent(&mut self) {
        self.audio_output.channels.clear();
        self.audio_output.channels.consent = None;
    }
    pub(crate) fn prepare_channel_output(&mut self, output: &str) {
        let state = &mut self.audio_output.channels;
        state.clear();
        let key = super::channel_mode::identity(output);
        state.output_key = key.clone();
        if state
            .consent
            .as_ref()
            .is_some_and(|held| held.epoch != self.rows_epoch || held.output_key != key)
        {
            state.consent = None;
        }
    }
    pub(crate) fn stereo_fallback_enabled(&self, output: &str) -> bool {
        self.remembered_stereo_fallback(output)
            || self
                .audio_output
                .channels
                .consent
                .as_ref()
                .is_some_and(|held| {
                    held.epoch == self.rows_epoch
                        && held.output_key == super::channel_mode::identity(output)
                })
    }
    pub(crate) fn offer_channel_conversion(
        &mut self,
        output: String,
        path: &Path,
        index: usize,
        requested: u16,
        offered: &[u16],
    ) -> bool {
        let index = self.base + index;
        let Some(entry) = self
            .queue
            .get(index)
            .filter(|entry| Path::new(entry.as_str()) == path)
        else {
            return false;
        };
        if !(3..=8).contains(&requested)
            || !offered.contains(&2)
            || output.len() > 2048
            || entry.len() > 16384
            || super::channel_mode::identity(&output) != self.audio_output.channels.output_key
        {
            return false;
        }
        let path = entry.clone();
        let position =
            if self.now.path == path && self.now.index == index && self.now.seconds.is_finite() {
                self.now.seconds.max(0.0)
            } else {
                0.0
            };
        let selection = self.chosen_output().0;
        if selection.len() > 2048 {
            return false;
        }
        let output_key = super::channel_mode::identity(&output);
        let state = &mut self.audio_output.channels;
        state.changed();
        state.pending = Some(Offer {
            token: state.revision,
            output,
            output_key,
            selection,
            path,
            index,
            epoch: self.rows_epoch,
            position,
            requested,
        });
        self.now.playing = false;
        self.now.error.clear();
        true
    }
    /// A scalar read by the existing transport heartbeat; JSON is only requested on change.
    pub fn channel_offer_revision(&mut self) -> u64 {
        if self
            .audio_output
            .channels
            .pending
            .as_ref()
            .is_some_and(|offer| !self.channel_snapshot_valid(offer))
        {
            self.audio_output.channels.clear();
        }
        self.audio_output.channels.revision
    }
    pub fn channel_offer_doc(&self) -> String {
        let Some(offer) = self
            .audio_output
            .channels
            .pending
            .as_ref()
            .filter(|offer| self.channel_snapshot_valid(offer))
        else {
            return "{}".into();
        };
        let found = self.outputs_listing.found();
        let output = found
            .outputs
            .iter()
            .find(|item| item.id == offer.output)
            .map(|item| item.label.as_str())
            .unwrap_or(&offer.output);
        let output: String = output.chars().take(128).collect();
        serde_json::json!({"token":offer.token.to_string(),"message":chilpa_i18n::t_args("This track has {} channels. Play it in stereo on {}?",&[&offer.requested.to_string(),&output])}).to_string()
    }
    pub(crate) fn accept_pending_channel_offer(&mut self) -> bool {
        let Some(token) = self
            .audio_output
            .channels
            .pending
            .as_ref()
            .map(|offer| offer.token.to_string())
        else {
            return false;
        };
        self.answer_channel_offer(&token, true, false)
    }
    pub fn channel_offer_current(&self, token: &str) -> bool {
        self.audio_output
            .channels
            .pending
            .as_ref()
            .is_some_and(|offer| {
                token == offer.token.to_string()
                    && self.channel_snapshot_valid(offer)
                    && self.chosen_output().0 == offer.selection
                    && self.channel_target_available(offer.index)
            })
    }
    pub fn answer_channel_offer(&mut self, token: &str, accept: bool, remember: bool) -> bool {
        if !self
            .audio_output
            .channels
            .pending
            .as_ref()
            .is_some_and(|offer| token == offer.token.to_string())
        {
            return false;
        }
        if !self.channel_offer_current(token) {
            self.audio_output.channels.clear();
            return false;
        }
        let offer = self
            .audio_output
            .channels
            .pending
            .take()
            .expect("validated pending offer");
        self.audio_output.channels.changed();
        if !accept {
            self.stop();
            return true;
        }
        let error = if remember {
            self.keep_stereo_fallback(&offer.output, true).err()
        } else {
            None
        };
        self.audio_output.channels.consent = Some(Consent {
            output_key: offer.output_key.clone(),
            epoch: self.rows_epoch,
        });
        let why = if self.now.path == offer.path && self.now.index == offer.index {
            Relaunch::Output
        } else {
            Relaunch::Track
        };
        self.now.error.clear();
        self.spawn_from(
            offer.index,
            &offer.output,
            false,
            Some(Duration::from_secs_f64(offer.position)),
            why,
        );
        if let Some(error) = error {
            self.now.error = error;
        }
        true
    }
}
#[cfg(test)]
mod tests;
