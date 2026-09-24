//! The output's own volume, the one in the hardware.
//!
//! **Chilpa never scales samples.** That is what bit-perfect means: what
//! goes out the cable is what was in the file. A volume applied by
//! multiplying the samples is not a volume, it is a conversion, and it
//! throws away the bits at the bottom of every one.
//!
//! So what is offered here is the device's own control and nothing else. If
//! the device has none, there is nothing to offer and that is said, instead
//! of quietly multiplying and going on calling it bit-perfect. On a DAC that
//! is usually the right answer anyway: its knob, or its remote.
//!
//! A caveat worth stating rather than hiding: many cards implement their
//! "hardware" volume as a digital attenuation inside the card. That is still
//! not Chilpa touching the samples -- what leaves this process is the file's
//! bits -- but it is not an analogue volume either, and on those cards
//! turning it down does cost resolution. Which kind a device has is the
//! device's business and Chilpa cannot tell, so it does not claim to.

use alsa::mixer::{Mixer, Selem, SelemChannelId, SelemId};

/// The elements worth looking at, best first.
///
/// A card can carry several playback controls and they are not equal:
/// `Master` is the one a person means, and `PCM` is the one that moves when
/// there is no master. The rest are there so a card that names its output
/// something else still gets a control instead of none.
const PREFERRED: &[&str] = &["Master", "PCM", "Speaker", "Headphone", "Digital", "Front"];

/// What the device says its volume is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Level {
    /// Where the control sits, from 0 to 100.
    pub percent: u8,
    /// True if the device is muted. A device with no mute switch is never
    /// muted, which is the truth and not a guess.
    pub muted: bool,
}

/// A device's volume control.
pub struct Volume {
    /// The card, as ALSA wants it for a mixer: `hw:0`.
    card: String,
    /// The element inside it.
    element: String,
}

/// The card a device belongs to: `hw:0,0` is device 0 of card `hw:0`.
fn card_of(device: &str) -> Option<String> {
    let rest = device.strip_prefix("hw:")?;
    let card = rest.split(',').next()?;
    (!card.is_empty()).then(|| format!("hw:{card}"))
}

/// Picks the element to offer, if the card has one at all.
fn choose(mixer: &Mixer) -> Option<String> {
    let usable: Vec<String> = mixer
        .iter()
        .filter_map(Selem::new)
        .filter(Selem::has_playback_volume)
        .filter_map(|s| s.get_id().get_name().ok().map(str::to_string))
        .collect();
    PREFERRED
        .iter()
        .find_map(|want| usable.iter().find(|have| have.as_str() == *want).cloned())
        .or_else(|| usable.into_iter().next())
}

/// Where a raw value from the card sits, as a percentage.
///
/// Rounds to nearest so the number under the slider is the number the card
/// would come back with, and not one off it. A card whose range is a single
/// value has no room to move: that reads as zero rather than as a division
/// by nothing.
fn to_percent(raw: i64, min: i64, max: i64) -> u8 {
    let span = max - min;
    if span <= 0 {
        return 0;
    }
    (((raw - min) * 100 + span / 2) / span).clamp(0, 100) as u8
}

/// The raw value for a percentage on that card's range.
fn to_raw(percent: u8, min: i64, max: i64) -> i64 {
    min + (max - min) * i64::from(percent.min(100)) / 100
}

impl Volume {
    /// The control for that device, or `None` if the card has none.
    pub fn of(device: &str) -> Option<Self> {
        let card = card_of(device)?;
        let mixer = Mixer::new(&card, false).ok()?;
        let element = choose(&mixer)?;
        Some(Self { card, element })
    }

    /// Which element ended up being offered, so the window can say so. A
    /// card whose control is called `PCM` and not `Master` is worth knowing
    /// about when the knob does not do what someone expected.
    pub fn element(&self) -> &str {
        &self.element
    }

    /// Runs something against the element, opening the mixer for it.
    ///
    /// The handle is not kept: a mixer held open across a device change goes
    /// stale, and opening one costs a syscall on a control the user is
    /// turning by hand, not sixty times a second.
    fn with<T>(&self, f: impl FnOnce(&Selem) -> Result<T, alsa::Error>) -> Result<T, String> {
        let mixer = Mixer::new(&self.card, false).map_err(|e| e.to_string())?;
        let selem = mixer
            .find_selem(&SelemId::new(&self.element, 0))
            .ok_or_else(|| chilpa_i18n::t("the device's volume control is gone"))?;
        f(&selem).map_err(|e| e.to_string())
    }

    /// Where the control sits now.
    pub fn read(&self) -> Result<Level, String> {
        self.with(|selem| {
            let (min, max) = selem.get_playback_volume_range();
            let raw = selem.get_playback_volume(SelemChannelId::FrontLeft)?;
            let percent = to_percent(raw, min, max);
            // A card with no mute switch is not muted: saying it might be
            // would put a state in the window that nothing can clear.
            let muted = selem.has_playback_switch()
                && selem.get_playback_switch(SelemChannelId::FrontLeft)? == 0;
            Ok(Level { percent, muted })
        })
    }

    /// Moves the control. Out of range is clamped, not refused: a slider
    /// that stops moving is worse than one that stops at the end.
    pub fn set(&self, percent: u8) -> Result<(), String> {
        self.with(|selem| {
            let (min, max) = selem.get_playback_volume_range();
            selem.set_playback_volume_all(to_raw(percent, min, max))
        })
    }

    /// Mutes or unmutes. A card with no switch says so instead of pretending.
    pub fn mute(&self, on: bool) -> Result<(), String> {
        self.with(|selem| {
            if !selem.has_playback_switch() {
                return Ok(());
            }
            selem.set_playback_switch_all(i32::from(!on))
        })
    }

    /// True if the card can mute at all.
    pub fn can_mute(&self) -> bool {
        self.with(|selem| Ok(selem.has_playback_switch())).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_devices_card_is_the_part_before_the_comma() {
        assert_eq!(card_of("hw:0,0").as_deref(), Some("hw:0"));
        assert_eq!(card_of("hw:2,3").as_deref(), Some("hw:2"));
        assert_eq!(card_of("hw:Headphones,0").as_deref(), Some("hw:Headphones"));
        // A card on its own is already a card.
        assert_eq!(card_of("hw:1").as_deref(), Some("hw:1"));
    }

    #[test]
    fn a_percentage_and_a_raw_value_say_the_same_thing() {
        // A card's range is whatever the card says; these are two real ones,
        // a 0..87 mixer step count and a 0..65536 one.
        for (min, max) in [(0, 87), (0, 65_536), (-10_239, 400)] {
            assert_eq!(to_percent(to_raw(0, min, max), min, max), 0, "{min}..{max}");
            assert_eq!(to_percent(to_raw(100, min, max), min, max), 100, "{min}..{max}");
            for percent in 0..=100u8 {
                let back = to_percent(to_raw(percent, min, max), min, max);
                assert!(
                    back.abs_diff(percent) <= 1,
                    "{percent}% came back as {back}% on {min}..{max}"
                );
            }
        }
    }

    #[test]
    fn a_range_with_no_room_reads_as_zero_and_does_not_divide_by_nothing() {
        assert_eq!(to_percent(5, 5, 5), 0);
        assert_eq!(to_raw(50, 5, 5), 5);
    }

    #[test]
    fn what_is_not_a_direct_device_has_no_card_here() {
        // Chilpa only ever opens `hw:`, so anything else is a caller's
        // mistake and not a card with a volume.
        assert_eq!(card_of("default"), None);
        assert_eq!(card_of("plughw:0,0"), None);
        assert_eq!(card_of("hw:"), None);
        assert_eq!(card_of("capture"), None);
    }
}
