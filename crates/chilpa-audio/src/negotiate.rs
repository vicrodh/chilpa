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
    /// Not one bit is touched. If the device cannot, it fails.
    BitPerfect,
    /// The sample may be laid out in a wider format, which is exact, but
    /// never resampled nor converted to float.
    AllowWidening,
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
    Resampled { from_hz: u32, to_hz: u32 },
    /// It went through floats: the device converts.
    Floated,
    /// It was mixed with other applications.
    Mixed,
    /// Software volume was applied to it.
    VolumeApplied,
}

impl Transform {
    /// True when the sample reaching the DAC is the same one that left the
    /// decoder. Widening keeps the value; nothing else does.
    pub fn preserves_samples(self) -> bool {
        matches!(self, Transform::Widened { .. })
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
    /// Bit-perfect means no transformation changed a sample.
    pub fn is_bit_perfect(&self) -> bool {
        self.transforms.iter().all(|t| t.preserves_samples())
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NegotiationError {
    /// The device will not take the format and the policy forbids converting.
    Unsupported { requested: StreamFormat, policy: Policy },
    /// The caps were not measured; bit-perfect cannot be promised.
    CapsNotProbed,
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
    if caps.accepts(&requested) {
        return Ok(Delivery { requested, opened: requested, transforms: Vec::new() });
    }
    if policy == Policy::BitPerfect {
        return Err(NegotiationError::Unsupported { requested, policy });
    }

    // Widening is exact: same rate, same channels, more bits.
    let widened = caps
        .formats
        .iter()
        .filter(|f| f.is_integer() && f.valid_bits() > requested.sample_format.valid_bits())
        .min_by_key(|f| f.valid_bits())
        .map(|f| StreamFormat { sample_format: *f, ..requested });
    if let Some(opened) = widened {
        if caps.accepts(&opened) {
            return Ok(Delivery {
                requested,
                opened,
                transforms: vec![Transform::Widened {
                    from_bits: requested.sample_format.valid_bits(),
                    to_bits: opened.sample_format.valid_bits(),
                }],
            });
        }
    }

    if policy == Policy::AllowWidening {
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
    let rate = *caps
        .sample_rates
        .iter()
        .min_by_key(|r| r.abs_diff(requested.sample_rate_hz))
        .ok_or(NegotiationError::Unsupported { requested, policy })?;
    let opened = StreamFormat::new(format, rate, requested.channels);
    let mut transforms = Vec::new();
    if rate != requested.sample_rate_hz {
        transforms.push(Transform::Resampled { from_hz: requested.sample_rate_hz, to_hz: rate });
    }
    if !format.is_integer() {
        transforms.push(Transform::Floated);
    } else if format.valid_bits() > requests {
        transforms.push(Transform::Widened { from_bits: requests, to_bits: format.valid_bits() });
    } else if format.valid_bits() < requests {
        transforms.push(Transform::Narrowed { from_bits: requests, to_bits: format.valid_bits() });
    }
    Ok(Delivery { requested, opened, transforms })
}

#[cfg(test)]
mod tests {
    /// A device offering 16 and 32 bits, only at 48 kHz: the case of a
    /// laptop codec.
    fn codec_de_laptop() -> DeviceCaps {
        DeviceCaps {
            sample_rates: vec![48_000],
            formats: vec![SampleFormat::S16, SampleFormat::S32],
            channels: vec![2],
            probed: true,
        }
    }

    #[test]
    fn when_resampling_is_needed_no_bits_are_thrown_in_for_free() {
        // 24/88.2 does not fit: it has to resample. But the device does 32
        // bits, so cutting down to 16 would be a loss too many.
        let request = StreamFormat::new(SampleFormat::S24In32, 88_200, 2);
        let d = negotiate(request, &codec_de_laptop(), Policy::AllowAnything).unwrap();
        assert_eq!(d.opened.sample_format, SampleFormat::S32, "cabe en 32, no se recorta");
        assert_eq!(d.opened.sample_rate_hz, 48_000);
        assert!(!d.is_bit_perfect(), "remuestrear nunca es bit-perfect");
        assert!(
            d.transforms.iter().any(|t| matches!(t, Transform::Resampled { .. })),
            "se dijo que se remuestreó: {:?}",
            d.transforms
        );
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
            "el recorte tiene que aparecer: {:?}",
            d.transforms
        );
        assert!(!d.is_bit_perfect(), "recortar no conserva las muestras");
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
        assert!(d.is_bit_perfect());
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
        let d = negotiate(cd(), &caps, Policy::AllowWidening).unwrap();
        assert_eq!(d.opened.sample_format, SampleFormat::S32);
        assert_eq!(d.transforms, vec![Transform::Widened { from_bits: 16, to_bits: 32 }]);
        assert!(d.is_bit_perfect(), "un corrimiento no cambia el valor");
    }

    #[test]
    fn resampling_or_floating_stops_being_bit_perfect_and_says_so() {
        let caps = caps(&[48_000], &[SampleFormat::F32]);
        let d = negotiate(cd(), &caps, Policy::AllowAnything).unwrap();
        assert!(d.transforms.contains(&Transform::Resampled { from_hz: 44_100, to_hz: 48_000 }));
        assert!(d.transforms.contains(&Transform::Floated));
        assert!(!d.is_bit_perfect());
    }

    #[test]
    fn bit_perfect_is_not_promised_from_guessed_caps() {
        let mut caps = caps(&[44_100], &[SampleFormat::S16]);
        caps.probed = false;
        assert_eq!(negotiate(cd(), &caps, Policy::BitPerfect), Err(NegotiationError::CapsNotProbed));
    }
}
