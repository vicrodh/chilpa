//! What is asked of the device and what actually opened.
//!
//! The audit found three different shapes of the same bug in QBZ: with the
//! output in direct mode, if the device would not take the format, it fell
//! silently back to `plughw` (which resamples) or to the shared mixer, and
//! the UI kept saying "bit-perfect". Here there is no silent fallback:
//! either what was asked for opens, or exactly what changed is stated, or
//! it is refused.

use crate::format::{SampleFormat, StreamFormat};

/// What the device declares it accepts.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct DeviceCaps {
    pub sample_rates: Vec<u32>,
    pub formats: Vec<SampleFormat>,
    pub channels: Vec<u16>,
    /// True when these caps were measured by opening the device. Guessed ones
    /// are no good for deciding bit-perfect: the QBZ wizard wrote a list of
    /// assumed rates and turned resampling off, which is worse than not
    /// knowing.
    pub probed: bool,
}

impl DeviceCaps {
    pub fn accepts(&self, format: &StreamFormat) -> bool {
        self.sample_rates.contains(&format.sample_rate_hz)
            && self.formats.contains(&format.sample_format)
            && self.channels.contains(&format.channels)
    }
}

/// How much the signal may be touched.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Policy {
    /// Every sample reaches the device with the value it left the decoder
    /// with. If the device cannot take that, it fails.
    ///
    /// Laying a sample out in another integer word that holds its bits is
    /// allowed -- 24 bits in three bytes instead of four, or 16 in 32 with
    /// zeros below -- because that keeps every bit: widening is the one
    /// transform [`Transform::preserves_samples`] admits, and the gate and
    /// the verdict have to agree on what the word means. They did not. This refused to
    /// widen, so a DAC that only takes 32 bit words -- most USB DACs -- could
    /// not play a CD with the switch on, while the same DAC with the switch
    /// off was handed the same widened bytes and the verdict called them
    /// bit-perfect.
    ///
    /// A source that decodes to floats is the exception, because it has no
    /// integer values to keep: it plays, rounded into the widest word the
    /// device takes at its own rate, and it is never called bit-perfect.
    BitPerfect,
    /// Anything goes, as long as it plays.
    AllowAnything,
}

/// Each thing that happened to the signal between decoder and DAC.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transform {
    /// Laid out in more bits with a shift. Loses nothing.
    Widened { from_bits: u8, to_bits: u8 },
    /// Bits were dropped. That does lose information, which is why it is said.
    Narrowed { from_bits: u8, to_bits: u8 },
    /// It went through floats: the device converts.
    Floated,
    /// A source that decodes to floats -- MP3, AAC, Vorbis -- was rounded
    /// into integers of this width. It never had integer samples, so there
    /// was nothing exact to keep; it is said because it is not bit-perfect.
    Quantized { to_bits: u8 },
    /// A mono track sent to both channels of a device that takes two. Each
    /// sample goes out twice and unchanged, which is why it keeps the verdict.
    Duplicated,
    /// It was mixed with other applications.
    Mixed,
    /// Software volume was applied to it.
    VolumeApplied,
}

impl Transform {
    /// True when the sample reaching the DAC is the same one that left the
    /// decoder. Widening keeps the value, and so does sending a mono sample
    /// to both channels; nothing else does.
    pub fn preserves_samples(self) -> bool {
        matches!(self, Transform::Widened { .. } | Transform::Duplicated)
    }
}

/// What actually opened. It is the only place that can say whether the
/// output is bit-perfect, and it says so from the facts, not from a setting.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Delivery {
    pub requested: StreamFormat,
    pub opened: StreamFormat,
    pub transforms: Vec<Transform>,
}

impl Delivery {
    /// Whether anything Chilpa did to the samples changed one.
    ///
    /// **This is not the whole of bit-perfect and its name says so.** It
    /// knows what happened up to the moment the samples were handed over,
    /// and nothing about what happens after. Handing them to a server that
    /// mixes and resamples leaves every transform here empty, so a verdict
    /// built on this alone reads "bit-perfect" over a resampler -- which is
    /// the exact claim this project exists to stop making.
    ///
    /// What happens afterwards is the stack's `Fidelity`, and whoever
    /// reports a verdict has to put the two together.
    pub fn nothing_was_converted(&self) -> bool {
        self.transforms.iter().all(|t| t.preserves_samples())
    }

    /// True when a mono track opened in stereo, and every block has to be
    /// laid out in both channels before it is packed.
    pub fn duplicates_mono(&self) -> bool {
        self.transforms.contains(&Transform::Duplicated)
    }

    /// How many bits carry the music on the way out, or `None` when the
    /// device takes floats. See [`bits_carried`].
    pub fn bits_carried(&self) -> Option<u8> {
        bits_carried(self.requested, self.opened)
    }
}

/// How many bits carry the music when `source` goes out as `opened`, or
/// `None` when either is a float.
///
/// Not the width of the word the output opened at. A 16 bit track laid out
/// in a 32 bit word, for a DAC that takes nothing narrower, still carries 16,
/// and calling it 24 or 32 shows a CD rip as something it is not, in the
/// window and in the play history. When bits were dropped, what is left is
/// the word. A lossy track decodes to floats and has no depth of its own:
/// the word it was rounded into is not one.
pub fn bits_carried(source: StreamFormat, opened: StreamFormat) -> Option<u8> {
    let (from, word) = (source.sample_format, opened.sample_format);
    (from.is_integer() && word.is_integer()).then(|| word.valid_bits().min(from.valid_bits()))
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NegotiationError {
    /// The device will not take the format and the policy forbids converting.
    Unsupported { requested: StreamFormat, policy: Policy },
    /// The caps were not measured; bit-perfect cannot be promised.
    CapsNotProbed,
    /// The device takes none of the channel counts this track could go out
    /// in: six into two is a downmix, and there is none.
    Channels { requested: u16, offered: Vec<u16> },
    /// The device does not run at the track's rate, and nothing here
    /// converts rates.
    Rate { requested_hz: u32, offered: Vec<u32> },
}

/// Decides what format to open at. Returns what opens and what it cost.
pub fn negotiate(
    requested: StreamFormat,
    caps: &DeviceCaps,
    policy: Policy,
) -> Result<Delivery, NegotiationError> {
    if policy == Policy::BitPerfect && !caps.probed {
        return Err(NegotiationError::CapsNotProbed);
    }

    // The channels first, so every other question is asked at the count
    // that will actually open. A mono track on a device that takes two goes
    // out in both, each sample unchanged. Anything else the device does not
    // list would need a downmix, and there is none: the fallback used to
    // hand back the track's own count anyway, and the open failed with a
    // message about something else.
    let (channels, spread) = if caps.channels.contains(&requested.channels) {
        (requested.channels, false)
    } else if requested.channels == 1 && caps.channels.contains(&2) {
        (2, true)
    } else {
        return Err(NegotiationError::Channels {
            requested: requested.channels,
            offered: caps.channels.clone(),
        });
    };
    let wanted = StreamFormat { channels, ..requested };
    let done = |opened: StreamFormat, mut transforms: Vec<Transform>| {
        if spread {
            transforms.insert(0, Transform::Duplicated);
        }
        Ok(Delivery { requested, opened, transforms })
    };
    if caps.accepts(&wanted) {
        // A float handed over as a float is still not bit-perfect: the device
        // rounds it into whatever its DAC takes, and that is a rounding
        // nobody here can see.
        let transforms =
            if requested.sample_format.is_integer() { Vec::new() } else { vec![Transform::Floated] };
        return done(wanted, transforms);
    }

    // Another integer word that holds at least the same bits keeps every
    // value: the same 24 bits in three bytes instead of four, or a sample
    // laid out wider with zeros below. Same rate, same channels, the nearest
    // word first -- and that is why this is tried before bit-perfect gives
    // up and not after. Only more bits used to count, so a 24 bit track was
    // refused by a DAC that takes 24 bits in three bytes.
    //
    // A float is never one of these. F32 is as wide as S32, and laying it
    // into an integer word scales it.
    let bits = requested.sample_format.valid_bits();
    let exact = caps
        .formats
        .iter()
        .filter(|f| requested.sample_format.is_integer() && f.is_integer() && f.valid_bits() >= bits)
        .min_by_key(|f| f.valid_bits())
        .map(|f| StreamFormat { sample_format: *f, ..wanted });
    if let Some(opened) = exact {
        if caps.accepts(&opened) {
            let to_bits = opened.sample_format.valid_bits();
            let transforms = if to_bits > bits {
                vec![Transform::Widened { from_bits: bits, to_bits }]
            } else {
                Vec::new()
            };
            return done(opened, transforms);
        }
    }

    // A source that decodes to floats -- MP3, AAC, Vorbis -- never had
    // integer samples, so no word keeps them. Refusing it would silence every
    // lossy track and every radio with the switch on; letting it through
    // with nothing to report is what this used to do, and the verdict called
    // an MP3 bit-perfect. It is rounded once, into the widest integer word
    // the device takes at the track's own rate, and it is said.
    if !requested.sample_format.is_integer() {
        let widest = caps
            .formats
            .iter()
            .filter(|f| f.is_integer())
            .max_by_key(|f| f.valid_bits())
            .map(|f| StreamFormat { sample_format: *f, ..wanted });
        if let Some(opened) = widest {
            if caps.accepts(&opened) {
                let to_bits = opened.sample_format.valid_bits();
                return done(opened, vec![Transform::Quantized { to_bits }]);
            }
        }
    }

    if policy == Policy::BitPerfect {
        return Err(NegotiationError::Unsupported { requested, policy });
    }

    // Last resort: whatever the device takes, saying what was lost.
    //
    // Of the formats offered, the one that does least harm is picked: first
    // the narrowest integer that still fits what was asked, and if none
    // fits, the widest there is. Taking the first of the list threw 24 bits
    // down to 16 on a device that also does 32, without saying so.
    let requests = requested.sample_format.valid_bits();
    let format = caps
        .formats
        .iter()
        .filter(|f| f.is_integer() && f.valid_bits() >= requests)
        .min_by_key(|f| f.valid_bits())
        .or_else(|| caps.formats.iter().filter(|f| f.is_integer()).max_by_key(|f| f.valid_bits()))
        .or_else(|| caps.formats.first())
        .copied()
        .ok_or(NegotiationError::Unsupported { requested, policy })?;
    // Only the track's own rate. There is no resampler: this used to open
    // the nearest rate the device lists, record "resampled", and hand over
    // the samples as they were, so a 44.1 kHz track on a 48 kHz device
    // played 8.8 per cent fast and a semitone and a half high.
    if !caps.sample_rates.contains(&requested.sample_rate_hz) {
        return Err(NegotiationError::Rate {
            requested_hz: requested.sample_rate_hz,
            offered: caps.sample_rates.clone(),
        });
    }
    let opened = StreamFormat::new(format, requested.sample_rate_hz, channels);
    let mut transforms = Vec::new();
    if !format.is_integer() {
        transforms.push(Transform::Floated);
    } else if !requested.sample_format.is_integer() {
        transforms.push(Transform::Quantized { to_bits: format.valid_bits() });
    } else if format.valid_bits() > requests {
        transforms.push(Transform::Widened { from_bits: requests, to_bits: format.valid_bits() });
    } else if format.valid_bits() < requests {
        transforms.push(Transform::Narrowed { from_bits: requests, to_bits: format.valid_bits() });
    }
    done(opened, transforms)
}

#[cfg(test)]
mod tests {
    /// A device offering 16 and 32 bits, only at 48 kHz: the case of a
    /// laptop codec.
    fn laptop_codec() -> DeviceCaps {
        DeviceCaps {
            sample_rates: vec![48_000],
            formats: vec![SampleFormat::S16, SampleFormat::S32],
            channels: vec![2],
            probed: true,
        }
    }

    #[test]
    fn a_rate_the_device_lacks_is_refused_even_with_the_switch_off() {
        // This used to open at 48 kHz, record "resampled", and hand the 88.2
        // kHz samples over as they were: nothing resamples. Refusing, and
        // naming both rates, is the honest answer until something does.
        let request = StreamFormat::new(SampleFormat::S24In32, 88_200, 2);
        let r = negotiate(request, &laptop_codec(), Policy::AllowAnything);
        assert_eq!(r, Err(NegotiationError::Rate { requested_hz: 88_200, offered: vec![48_000] }));
    }

    #[test]
    fn no_bits_are_thrown_in_for_free() {
        // The device does 16 and 32. A 24 bit track goes in 32, not 16.
        let request = StreamFormat::new(SampleFormat::S24In32, 48_000, 2);
        let d = negotiate(request, &laptop_codec(), Policy::AllowAnything).unwrap();
        assert_eq!(d.opened.sample_format, SampleFormat::S32, "it fits in 32, nothing is truncated");
    }

    #[test]
    fn dropping_bits_is_reported() {
        // A device that only knows 16 bits: 24 has no other option.
        let caps = DeviceCaps {
            sample_rates: vec![48_000],
            formats: vec![SampleFormat::S16],
            channels: vec![2],
            probed: true,
        };
        let request = StreamFormat::new(SampleFormat::S24In32, 48_000, 2);
        let d = negotiate(request, &caps, Policy::AllowAnything).unwrap();
        assert_eq!(d.opened.sample_format, SampleFormat::S16);
        assert!(
            d.transforms.contains(&Transform::Narrowed { from_bits: 24, to_bits: 16 }),
            "the truncation has to show up: {:?}",
            d.transforms
        );
        assert!(!d.nothing_was_converted(), "truncating does not keep the samples");
    }

    use super::*;

    fn caps(rates: &[u32], formats: &[SampleFormat]) -> DeviceCaps {
        DeviceCaps {
            sample_rates: rates.to_vec(),
            formats: formats.to_vec(),
            channels: vec![2],
            probed: true,
        }
    }

    fn cd() -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, 44_100, 2)
    }

    #[test]
    fn what_the_device_accepts_is_opened_as_is() {
        let d = negotiate(cd(), &caps(&[44_100], &[SampleFormat::S16]), Policy::BitPerfect).unwrap();
        assert_eq!(d.opened, cd());
        assert!(d.nothing_was_converted());
    }

    #[test]
    fn without_support_bit_perfect_fails_instead_of_falling_back_silently() {
        let caps = caps(&[48_000], &[SampleFormat::F32]);
        let r = negotiate(cd(), &caps, Policy::BitPerfect);
        assert_eq!(r, Err(NegotiationError::Unsupported { requested: cd(), policy: Policy::BitPerfect }));
    }

    #[test]
    fn widening_keeps_the_samples_and_stays_bit_perfect() {
        let caps = caps(&[44_100], &[SampleFormat::S32]);
        let d = negotiate(cd(), &caps, Policy::BitPerfect).unwrap();
        assert_eq!(d.opened.sample_format, SampleFormat::S32);
        assert_eq!(d.transforms, vec![Transform::Widened { from_bits: 16, to_bits: 32 }]);
        assert!(d.nothing_was_converted(), "a shift does not change the value");
    }

    /// What a USB DAC that publishes only `S32_LE` looks like once measured.
    /// ALSA's one format is both of Chilpa's 32 bit words, so both are listed.
    /// This is most USB DACs sold today, and the one this was found on.
    fn only_32_bit_words() -> DeviceCaps {
        caps(&[44_100, 48_000, 96_000], &[SampleFormat::S24In32, SampleFormat::S32])
    }

    #[test]
    fn a_cd_reaches_a_dac_that_only_takes_32_bit_words_bit_perfect() {
        // The switch refused this while the switch turned off played the
        // same bytes and called them bit-perfect: the gate and the verdict
        // disagreed about what the word means.
        let d = negotiate(cd(), &only_32_bit_words(), Policy::BitPerfect)
            .expect("16 bits fit in a wider word without losing one");
        assert_eq!(d.opened.sample_rate_hz, 44_100, "widening never resamples");
        assert_eq!(d.opened.channels, 2);
        assert_eq!(d.transforms, vec![Transform::Widened { from_bits: 16, to_bits: 24 }]);
        assert!(d.nothing_was_converted());
    }

    #[test]
    fn the_bits_carried_are_the_music_s_and_not_the_word_s() {
        let at = |f| StreamFormat::new(f, 44_100, 2);
        let widened = negotiate(cd(), &only_32_bit_words(), Policy::BitPerfect).unwrap();
        assert_eq!(widened.bits_carried(), Some(16), "a CD in a 32 bit word is still 16 bits");
        assert_eq!(bits_carried(at(SampleFormat::S16), at(SampleFormat::S32)), Some(16));
        assert_eq!(
            bits_carried(at(SampleFormat::S24In32), at(SampleFormat::S16)),
            Some(16),
            "when bits were dropped, what is left is the word"
        );
        assert_eq!(bits_carried(at(SampleFormat::S24In32), at(SampleFormat::S24In32)), Some(24));
        assert_eq!(bits_carried(at(SampleFormat::S16), at(SampleFormat::F32)), None);
    }

    #[test]
    fn a_24_bit_track_reaches_a_dac_that_only_takes_three_byte_words_bit_perfect() {
        // USB DACs with three byte subslots publish only S24_3LE. The same
        // 24 bits in three bytes instead of four are the same samples: with
        // the switch off this opened and the verdict said bit-perfect, and
        // with it on it was refused.
        let request = StreamFormat::new(SampleFormat::S24In32, 96_000, 2);
        let only_packed = caps(&[96_000], &[SampleFormat::S16, SampleFormat::S24Packed]);
        let d = negotiate(request, &only_packed, Policy::BitPerfect)
            .expect("24 bits fit in three bytes without losing one");
        assert_eq!(d.opened.sample_format, SampleFormat::S24Packed);
        assert!(d.transforms.is_empty(), "no value changed: {:?}", d.transforms);
        assert_eq!(d.bits_carried(), Some(24));
    }

    fn lossy() -> StreamFormat {
        StreamFormat::new(SampleFormat::F32, 44_100, 2)
    }

    #[test]
    fn a_float_is_never_passed_off_as_a_32_bit_integer() {
        // Both are 32 bits wide, and only one of them is an integer: laying a
        // float into it scales it. It plays, and it is said.
        let d = negotiate(lossy(), &caps(&[44_100], &[SampleFormat::S32]), Policy::BitPerfect)
            .expect("a lossy track still plays with the switch on");
        assert_eq!(d.transforms, vec![Transform::Quantized { to_bits: 32 }]);
        assert!(!d.nothing_was_converted());
    }

    #[test]
    fn a_lossy_track_goes_out_in_the_widest_word_at_its_own_rate_and_is_never_bit_perfect() {
        let only_integers = caps(&[44_100, 48_000], &[SampleFormat::S16, SampleFormat::S24In32, SampleFormat::S32]);
        let d = negotiate(lossy(), &only_integers, Policy::BitPerfect).expect("it plays");
        assert_eq!(d.opened, StreamFormat::new(SampleFormat::S32, 44_100, 2));
        assert!(!d.nothing_was_converted(), "an MP3 is not bit-perfect: {:?}", d.transforms);
        assert_eq!(d.bits_carried(), None, "and it has no bit depth of its own to report");
    }

    #[test]
    fn a_float_handed_to_a_dac_that_takes_floats_is_still_not_bit_perfect() {
        // The DAC does the rounding then, not Chilpa, and it is still a
        // rounding.
        let d = negotiate(lossy(), &caps(&[44_100], &[SampleFormat::F32]), Policy::BitPerfect).unwrap();
        assert!(!d.nothing_was_converted(), "{:?}", d.transforms);
    }

    #[test]
    fn a_lossy_track_is_not_resampled_with_the_switch_on() {
        let r = negotiate(lossy(), &caps(&[48_000], &[SampleFormat::S32]), Policy::BitPerfect);
        assert!(r.is_err(), "{r:?}");
    }

    fn stereo_only() -> DeviceCaps {
        caps(&[44_100], &[SampleFormat::S16, SampleFormat::S32])
    }

    #[test]
    fn a_mono_track_reaches_a_stereo_only_dac_in_both_channels() {
        // The Fosi publishes two channels and nothing else. A mono track was
        // refused with the switch on, and with it off the fallback handed
        // back one channel the device does not take, so the open failed.
        let mono = StreamFormat::new(SampleFormat::S16, 44_100, 1);
        let d = negotiate(mono, &stereo_only(), Policy::BitPerfect)
            .expect("the same samples in both channels keep every value");
        assert_eq!(d.opened, StreamFormat::new(SampleFormat::S16, 44_100, 2));
        assert_eq!(d.transforms, vec![Transform::Duplicated]);
        assert!(d.nothing_was_converted());
    }

    #[test]
    fn no_policy_opens_a_channel_count_the_device_does_not_list() {
        // Six channels into two is a downmix, and there is none. Handing back
        // a delivery the device refuses at open time only moves the failure
        // somewhere it says less.
        let surround = StreamFormat::new(SampleFormat::S16, 44_100, 6);
        for policy in [Policy::BitPerfect, Policy::AllowAnything] {
            let r = negotiate(surround, &stereo_only(), policy);
            assert!(
                matches!(r, Err(NegotiationError::Channels { requested: 6, .. })),
                "{policy:?}: {r:?}"
            );
        }
    }

    #[test]
    fn bit_perfect_still_refuses_everything_that_changes_a_sample() {
        // Widening is the one thing let through. Each of these would be
        // accepted by a gate that let through too much.
        let narrowing = negotiate(
            StreamFormat::new(SampleFormat::S24In32, 44_100, 2),
            &caps(&[44_100], &[SampleFormat::S16]),
            Policy::BitPerfect,
        );
        assert!(narrowing.is_err(), "24 bits into 16 drops eight of them: {narrowing:?}");

        let resampling = negotiate(cd(), &caps(&[48_000], &[SampleFormat::S32]), Policy::BitPerfect);
        assert!(resampling.is_err(), "a wider word at another rate is a resample: {resampling:?}");

        let floating = negotiate(cd(), &caps(&[44_100], &[SampleFormat::F32]), Policy::BitPerfect);
        assert!(floating.is_err(), "a float is 32 bits and still not the same sample: {floating:?}");
    }

    #[test]
    fn floating_stops_being_bit_perfect_and_says_so() {
        let caps = caps(&[44_100], &[SampleFormat::F32]);
        let d = negotiate(cd(), &caps, Policy::AllowAnything).unwrap();
        assert!(d.transforms.contains(&Transform::Floated));
        assert!(!d.nothing_was_converted());
    }

    #[test]
    fn bit_perfect_is_not_promised_from_guessed_caps() {
        let mut caps = caps(&[44_100], &[SampleFormat::S16]);
        caps.probed = false;
        assert_eq!(negotiate(cd(), &caps, Policy::BitPerfect), Err(NegotiationError::CapsNotProbed));
    }
}
