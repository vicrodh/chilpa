//! Lock output device: every play goes to the output that is locked,
//! whatever its source, and a moved system default does not move it.
//!
//! Without it an output that nobody chose is picked again at every play:
//! the first one listed, and macOS and Windows list the system default
//! first. A Mac that hands its default to the speakers while a DAC is held
//! in exclusive mode then sends the next track to the speakers. With the
//! lock on, the output a play opens on is remembered, so the next play
//! finds it there.
//!
//! A locked output that is gone is not played to: the play falls back to the
//! first output listed and says so, and the lock stays on the missing one.
//! Gone is only judged from a listing made after the play asked, so a stale
//! snapshot never moves playback away from a DAC that is there.
//!
//! Nothing here is platform code. It reads what every stack already answers
//! through `OutputBackend`: its outputs, the default first, and a cheap
//! `generation` that moves when a device comes or goes. The setting lives in
//! the facade, so the bridge learns no key (frontend ADR-0015).

use super::listing::Found;
use super::{offered, own_stack, FIRST_ANSWER};
use crate::Chilpa;

pub(crate) const LOCK: &str = "audio.lockDevice";

/// Whether an output is there, as one listing says.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Presence {
    Here,
    Gone,
    /// The listing cannot tell: the stacks have not answered, or the
    /// output's stack was cut at the cap before it.
    Unknown,
}

pub(crate) fn presence(found: &Found, output: &str) -> Presence {
    if !found.listed {
        return Presence::Unknown;
    }
    if found.outputs.iter().any(|o| o.id == output) {
        return Presence::Here;
    }
    if found.clipped.iter().any(|kind| kind.key() == own_stack(output)) {
        return Presence::Unknown;
    }
    Presence::Gone
}

/// Where a play goes when its locked output is gone: the first output
/// offered, which is the system default on every platform.
fn fallback<'a>(found: &'a Found, gone: &str) -> Option<&'a chilpa_core::Output> {
    found.outputs.iter().find(|o| offered(o) && o.id != gone)
}

impl Chilpa {
    /// Whether the output is locked.
    pub fn device_locked(&self) -> bool {
        self.preference(LOCK).as_deref() == Some("true")
    }

    pub(crate) fn device_lock_setting(&mut self, key: &str, value: &str) -> bool {
        if key != LOCK {
            return false;
        }
        let on = value == "true";
        self.remember_preference(LOCK, &on.to_string());
        // Locking with nothing remembered locks what is in use: the output
        // that is open, or the one the next play would take.
        if on && chilpa_core::settings::load().output.is_empty() {
            let open = self.audio_output.route.as_ref().filter(|_| self.now.playing).map(|r| r.device.clone());
            let in_use = open.unwrap_or_else(|| self.device_or_first());
            if !in_use.is_empty() {
                self.remember(&in_use, false);
            }
        }
        true
    }

    pub(crate) fn device_lock_row(&self, row: &mut crate::SettingsRow) -> bool {
        if row.key != LOCK {
            return false;
        }
        row.value = self.device_locked().to_string();
        true
    }

    /// The output a play opens on, and what to say about it: empty, or why
    /// it is not the locked one. `None` when no output is known yet.
    pub(crate) fn launch_output(&mut self, asked: &str) -> Option<(String, String)> {
        self.launch_output_seeing(asked, chilpa_core::backends::generation())
    }

    /// The same, with the stacks' generation read by the caller.
    pub(crate) fn launch_output_seeing(&mut self, asked: &str, generation: String) -> Option<(String, String)> {
        let asked = self.output_for_play(asked)?;
        if !self.device_locked() {
            return Some((asked, String::new()));
        }
        // A device came or went since the last locked play: the snapshot that
        // lists the locked output may be from before it went. The first look
        // only takes note.
        let moved = !self.outputs_listing.generation.is_empty() && self.outputs_listing.generation != generation;
        self.outputs_listing.generation = generation;
        let remembered = chilpa_core::settings::load().output;
        if remembered.is_empty() {
            chilpa_core::trace_playback("facade", "lock_pinned", format_args!("device={asked:?}"));
            self.remember(&asked, false);
            return Some((asked, String::new()));
        }
        // Somebody named another output for this play; it goes there.
        if asked != remembered {
            return Some((asked, String::new()));
        }
        if !moved && presence(&self.outputs_listing.found(), &asked) == Presence::Here {
            return Some((asked, String::new()));
        }
        let Some(found) = self.outputs_listing.fresh(FIRST_ANSWER) else {
            return Some((asked, String::new()));
        };
        if presence(&found, &asked) != Presence::Gone {
            return Some((asked, String::new()));
        }
        let Some(to) = fallback(&found, &asked) else {
            return Some((asked, String::new()));
        };
        chilpa_core::trace_playback("facade", "lock_fell_back", format_args!("locked={asked:?} to={:?}", to.id));
        let said = chilpa_i18n::t_args("The locked output is not connected; playing on {}.", &[&to.label]);
        Some((to.id.clone(), said))
    }
}

#[cfg(test)]
mod tests {
    use super::super::listing::tests::{answered, output, wait_until};
    use super::super::listing::Listing;
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    // Ids that name a stack, as a remembered output must to be read back,
    // and no device: no Mac has these uids and other systems have no
    // CoreAudio, so nothing a test does can reach a device.
    const DAC: &str = "coreaudio:uid:fixture-dac";
    const SPEAKERS: &str = "coreaudio:uid:fixture-speakers";

    /// The outputs as listed, each named by the last word of its id.
    fn listed(outputs: &[&str]) -> Vec<chilpa_core::Output> {
        outputs.iter().map(|id| output(id, id.rsplit('-').next().unwrap_or_default())).collect()
    }

    /// A Chilpa whose listing answers whatever `outputs` holds when asked.
    fn machine(outputs: &[&str]) -> (Chilpa, Arc<Mutex<Vec<chilpa_core::Output>>>) {
        let held = Arc::new(Mutex::new(listed(outputs)));
        let survey = Arc::clone(&held);
        let api = Chilpa::with_listing(Listing::with(Arc::new(move || answered(survey.lock().unwrap().clone()))));
        assert!(wait_until(|| api.outputs_listed(), Duration::from_secs(5)), "the stand-in listing did not answer");
        (api, held)
    }

    /// The machine's outputs change, and the listing is made again.
    fn outputs_become(api: &Chilpa, held: &Mutex<Vec<chilpa_core::Output>>, outputs: &[&str]) {
        *held.lock().unwrap() = listed(outputs);
        assert!(api.outputs_listing.fresh(Duration::from_secs(5)).is_some(), "the listing was not made again");
    }

    /// Nothing remembered and the lock as asked: the profile is shared by
    /// every test in the process.
    fn start(api: &mut Chilpa, locked: bool) {
        api.remember("", false);
        api.remember_preference(LOCK, &locked.to_string());
    }

    fn finish(api: &mut Chilpa) {
        start(api, false);
    }

    /// What every play passes in: the local library's `play_from`, a
    /// plugin's stream (`sources.rs`, `sound`) and its queue entries all
    /// take `chosen_output` and go through `spawn_from`.
    fn play(api: &mut Chilpa, generation: &str) -> (String, String) {
        let asked = api.chosen_output().0;
        api.launch_output_seeing(&asked, generation.into()).expect("an output is known")
    }

    #[test]
    fn a_locked_output_stays_when_the_default_moves_between_a_local_track_and_a_plugin_stream() {
        // The owner's Mac on 2026-11-10: nothing remembered, the DAC is the
        // default and plays a local track; the default moves to the
        // speakers; the next play, a plugin's, came out of the speakers.
        let (mut api, held) = machine(&[DAC, SPEAKERS]);
        start(&mut api, true);
        assert_eq!(play(&mut api, "g1"), (DAC.to_string(), String::new()), "the local track");
        assert_eq!(chilpa_core::settings::load().output, DAC, "the output that played is the locked one");
        outputs_become(&api, &held, &[SPEAKERS, DAC]);
        assert_eq!(play(&mut api, "g1").0, DAC, "the plugin stream went to the new default");
        finish(&mut api);
    }

    #[test]
    fn unlocked_an_output_nobody_chose_follows_the_default_as_before() {
        let (mut api, held) = machine(&[DAC, SPEAKERS]);
        start(&mut api, false);
        assert_eq!(play(&mut api, "g1").0, DAC);
        assert_eq!(chilpa_core::settings::load().output, "", "nothing is remembered without the lock");
        outputs_become(&api, &held, &[SPEAKERS, DAC]);
        assert_eq!(play(&mut api, "g1").0, SPEAKERS, "the behavior the lock is for");
        finish(&mut api);
    }

    #[test]
    fn a_locked_output_that_is_gone_falls_back_says_so_and_stays_locked() {
        let (mut api, held) = machine(&[DAC, SPEAKERS]);
        start(&mut api, true);
        play(&mut api, "g1");
        // Unplugged while the snapshot still lists it: the generation moved.
        *held.lock().unwrap() = listed(&[SPEAKERS]);
        let (device, said) = play(&mut api, "g2");
        assert_eq!(device, SPEAKERS);
        assert!(said.contains("speakers"), "the fallback was not said: {said:?}");
        assert_eq!(chilpa_core::settings::load().output, DAC, "the fallback took the lock");
        outputs_become(&api, &held, &[SPEAKERS, DAC]);
        assert_eq!(play(&mut api, "g3"), (DAC.to_string(), String::new()), "the DAC came back and was not used");
        finish(&mut api);
    }

    #[test]
    fn unlocked_a_remembered_output_that_is_gone_is_handed_on_as_before() {
        let (mut api, _held) = machine(&[SPEAKERS]);
        start(&mut api, false);
        api.remember(DAC, false);
        assert_eq!(play(&mut api, "g1"), (DAC.to_string(), String::new()));
        finish(&mut api);
    }

    #[test]
    fn a_stale_snapshot_without_the_locked_output_is_checked_before_falling_back() {
        // Plugged in after the last listing: the snapshot lacks it, the
        // listing made for this play has it.
        let (mut api, held) = machine(&[SPEAKERS]);
        start(&mut api, true);
        api.remember(DAC, false);
        *held.lock().unwrap() = listed(&[SPEAKERS, DAC]);
        assert_eq!(play(&mut api, "g1"), (DAC.to_string(), String::new()));
        finish(&mut api);
    }

    #[test]
    fn turning_the_lock_on_locks_what_is_in_use() {
        let (mut api, _held) = machine(&[DAC, SPEAKERS]);
        start(&mut api, false);
        assert!(api.adjust(LOCK, "true"), "the setting is the feature's");
        assert!(api.device_locked());
        assert_eq!(chilpa_core::settings::load().output, DAC);
        // A choice already made is kept.
        api.adjust(LOCK, "false");
        api.remember(SPEAKERS, false);
        api.adjust(LOCK, "true");
        assert_eq!(chilpa_core::settings::load().output, SPEAKERS);
        finish(&mut api);
    }

    #[test]
    fn the_fallback_reaches_the_window_after_the_play_resets_what_it_shows() {
        let (mut api, held) = machine(&[DAC, SPEAKERS]);
        start(&mut api, true);
        api.remember(DAC, false);
        *held.lock().unwrap() = listed(&[SPEAKERS]);
        api.outputs_listing.generation = "before".into();
        api.replace_queue(vec!["/nonexistent/device-lock.flac".into()]);
        api.spawn(0, DAC, false);
        assert!(api.now.error.contains("speakers"), "the window was not told: {:?}", api.now.error);
        api.stop();
        finish(&mut api);
    }

    #[test]
    fn the_row_sits_under_the_output_device() {
        let audio = crate::settings_schema::section("audio");
        let keys: Vec<&str> = audio.groups[0].settings.iter().map(|s| s.key).collect();
        let device = keys.iter().position(|k| *k == "audio.device").unwrap();
        assert_eq!(keys.get(device + 1), Some(&LOCK));
        let (mut api, _held) = machine(&[DAC]);
        start(&mut api, true);
        let mut row = crate::SettingsRow { key: LOCK.into(), ..Default::default() };
        assert!(api.device_lock_row(&mut row));
        assert_eq!(row.value, "true");
        finish(&mut api);
    }

    #[test]
    fn presence_is_never_judged_without_an_answer_or_past_the_cap() {
        let mut found = answered(vec![output(SPEAKERS, "speakers")]);
        assert_eq!(presence(&found, SPEAKERS), Presence::Here);
        assert_eq!(presence(&found, DAC), Presence::Gone);
        found.listed = false;
        assert_eq!(presence(&found, DAC), Presence::Unknown);
        let mut cut = answered(vec![output("pipewire:one", "one")]);
        cut.clipped = vec![chilpa_core::backends::StackKind::PipeWire];
        assert_eq!(presence(&cut, "pipewire:two"), Presence::Unknown);
        assert_eq!(presence(&cut, "alsa:hw:9,0"), Presence::Gone);
    }
}
