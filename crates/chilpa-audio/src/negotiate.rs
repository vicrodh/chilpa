//! Qué se le pide al dispositivo y qué se abrió de verdad.
//!
//! La auditoría encontró tres formas distintas del mismo error en QBZ: con la
//! salida en modo directo, si el dispositivo no aceptaba el formato, se caía
//! en silencio a `plughw` (que resamplea) o al mixer compartido, y la UI
//! seguía diciendo "bit-perfect". Aquí no hay caída silenciosa: o se abre lo
//! que se pidió, o se dice exactamente qué se cambió, o se rechaza.

use crate::format::{SampleFormat, StreamFormat};

/// Lo que el dispositivo declara aceptar.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct DeviceCaps {
    pub sample_rates: Vec<u32>,
    pub formats: Vec<SampleFormat>,
    pub channels: Vec<u16>,
    /// Cierto si estas capacidades se midieron abriendo el dispositivo. Las
    /// adivinadas no sirven para decidir bit-perfect: el asistente de QBZ
    /// escribía una lista de frecuencias supuestas y apagaba el resampleo,
    /// que es peor que no saber.
    pub probed: bool,
}

impl DeviceCaps {
    pub fn accepts(&self, format: &StreamFormat) -> bool {
        self.sample_rates.contains(&format.sample_rate_hz)
            && self.formats.contains(&format.sample_format)
            && self.channels.contains(&format.channels)
    }
}

/// Qué tanto se permite tocar la señal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Policy {
    /// No se toca ni un bit. Si el dispositivo no puede, se falla.
    BitPerfect,
    /// Se permite acomodar la muestra en un formato más grande, que es
    /// exacto, pero nunca resamplear ni convertir a flotante.
    AllowWidening,
    /// Se permite lo que sea con tal de que suene.
    AllowAnything,
}

/// Cada cosa que le pasó a la señal entre el decodificador y el DAC.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transform {
    /// Se acomodó en más bits con un corrimiento. No pierde información.
    Widened { from_bits: u8, to_bits: u8 },
    /// Se recortaron bits. Sí pierde información, y por eso se dice.
    Narrowed { from_bits: u8, to_bits: u8 },
    Resampled { from_hz: u32, to_hz: u32 },
    /// Pasó por flotantes: el dispositivo convierte.
    Floated,
    /// Se mezcló con otras aplicaciones.
    Mixed,
    /// Se le aplicó volumen por software.
    VolumeApplied,
}

impl Transform {
    /// Cierto si la muestra que llega al DAC es la misma que salió del
    /// decodificador. Ensanchar conserva el valor; lo demás no.
    pub fn preserves_samples(self) -> bool {
        matches!(self, Transform::Widened { .. })
    }
}

/// Lo que de verdad se abrió. Es el único lugar que puede decir si la salida
/// es bit-perfect, y lo dice a partir de los hechos, no de un ajuste.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Delivery {
    pub requested: StreamFormat,
    pub opened: StreamFormat,
    pub transforms: Vec<Transform>,
}

impl Delivery {
    /// Bit-perfect es que ninguna transformación haya cambiado una muestra.
    pub fn is_bit_perfect(&self) -> bool {
        self.transforms.iter().all(|t| t.preserves_samples())
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NegotiationError {
    /// El dispositivo no acepta el formato y la política no deja convertir.
    Unsupported { requested: StreamFormat, policy: Policy },
    /// Las capacidades no se midieron; no se puede prometer bit-perfect.
    CapsNotProbed,
}

/// Decide con qué formato abrir. Regresa lo que se abre y lo que costó.
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

    // Ensanchar es exacto: misma frecuencia, mismos canales, más bits.
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

    // Último recurso: lo que el dispositivo acepte, diciendo qué se perdió.
    //
    // De los formatos que ofrece se elige el que menos daño hace: primero el
    // entero más angosto que todavía quepa lo pedido, y si ninguno cabe, el
    // más ancho que haya. Tomar el primero de la lista tiraba 24 bits a 16
    // en un dispositivo que también sabe 32, sin decirlo.
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
    /// Un dispositivo que ofrece 16 y 32 bits, nada más a 48 kHz: el caso
    /// del códec de una laptop.
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
        // 24/88.2 no cabe: hay que remuestrear. Pero el dispositivo sabe 32
        // bits, así que recortar a 16 sería una pérdida de más.
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
        // Un dispositivo que solo sabe 16 bits: a 24 no le queda de otra.
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
