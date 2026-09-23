//! De muestras decodificadas a los bytes del cable.
//!
//! La regla: **un entero nunca pasa por flotante.** En QBZ el packer recibía
//! `f32` y lo escalaba de vuelta (`s * 32767.0`, con redondeo), así que una
//! muestra de 24 bits podía salir 1 LSB abajo mientras la UI decía
//! "bit-perfect". Aquí los enteros se acomodan con corrimientos, que no
//! pierden nada, y el camino de flotantes existe solo para las fuentes que
//! de verdad son flotantes.

use crate::format::SampleFormat;

/// Muestras ya decodificadas, en el tipo en el que venían.
pub enum Samples<'a> {
    /// Enteros con signo, justificados a la derecha, con `bits` útiles.
    Int { data: &'a [i32], bits: u8 },
    /// Flotantes en el rango [-1.0, 1.0].
    Float(&'a [f32]),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PackError {
    /// Los bits de la fuente no caben en el formato del cable sin tirar
    /// información. Nunca se trunca en silencio.
    WouldTruncate { from_bits: u8, to_bits: u8 },
}

/// Escribe las muestras en el formato del cable.
///
/// Subir de bits (16 a 24, por ejemplo) es exacto: se corre a la izquierda.
/// Bajar de bits no: se rechaza, y quien llame decide si aplica dither o
/// cambia de dispositivo. Un `f32` a un formato entero sí escala, y eso ya
/// no es bit-perfect; la ruta bit-perfect nunca llega aquí con flotantes.
pub fn pack(samples: &Samples<'_>, to: SampleFormat, out: &mut Vec<u8>) -> Result<(), PackError> {
    match samples {
        Samples::Int { data, bits } => pack_int(data, *bits, to, out),
        Samples::Float(data) => {
            pack_float(data, to, out);
            Ok(())
        }
    }
}

fn pack_int(data: &[i32], bits: u8, to: SampleFormat, out: &mut Vec<u8>) -> Result<(), PackError> {
    if to == SampleFormat::F32 {
        // El dispositivo solo toma flotantes: se convierte, y quien llama ya
        // sabe por `Delivery` que esto no es bit-perfect.
        out.reserve(data.len() * 4);
        let full = (1i64 << (bits - 1)) as f32;
        for &v in data {
            out.extend_from_slice(&(v as f32 / full).to_le_bytes());
        }
        return Ok(());
    }
    let target = to.valid_bits();
    if bits > target {
        return Err(PackError::WouldTruncate { from_bits: bits, to_bits: target });
    }
    let shift = target - bits;
    out.reserve(data.len() * to.bytes());
    for &v in data {
        let v = v << shift;
        match to {
            SampleFormat::S16 => out.extend_from_slice(&(v as i16).to_le_bytes()),
            SampleFormat::S24Packed => out.extend_from_slice(&v.to_le_bytes()[..3]),
            // Justificado a la izquierda: los 24 bits válidos ocupan los
            // bytes altos y el bajo queda en cero.
            SampleFormat::S24In32 => out.extend_from_slice(&(v << 8).to_le_bytes()),
            SampleFormat::S32 => out.extend_from_slice(&v.to_le_bytes()),
            SampleFormat::F32 => unreachable!("se resolvió arriba"),
        }
    }
    Ok(())
}

fn pack_float(data: &[f32], to: SampleFormat, out: &mut Vec<u8>) {
    out.reserve(data.len() * to.bytes());
    for &s in data {
        let s = s.clamp(-1.0, 1.0);
        match to {
            SampleFormat::F32 => out.extend_from_slice(&s.to_le_bytes()),
            SampleFormat::S16 => out.extend_from_slice(&((s * 32767.0).round() as i16).to_le_bytes()),
            SampleFormat::S24Packed => {
                out.extend_from_slice(&((s * 8_388_607.0).round() as i32).to_le_bytes()[..3])
            }
            SampleFormat::S24In32 => {
                let v = (s * 8_388_607.0).round() as i32;
                out.extend_from_slice(&(v << 8).to_le_bytes());
            }
            SampleFormat::S32 => {
                let v = (f64::from(s) * f64::from(i32::MAX)).round() as i32;
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packed(samples: &Samples<'_>, to: SampleFormat) -> Vec<u8> {
        let mut out = Vec::new();
        pack(samples, to, &mut out).unwrap();
        out
    }

    #[test]
    fn un_entero_de_16_sale_identico_en_16() {
        let data = [0i32, 1, -1, 32767, -32768];
        let bytes = packed(&Samples::Int { data: &data, bits: 16 }, SampleFormat::S16);
        let wrap: Vec<i32> = bytes
            .chunks(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as i32)
            .collect();
        assert_eq!(wrap, data, "ida y vuelta sin perder un solo LSB");
    }

    #[test]
    fn subir_de_16_a_24_es_un_corrimiento_exacto() {
        let data = [1i32, -1, 32767];
        let bytes = packed(&Samples::Int { data: &data, bits: 16 }, SampleFormat::S24In32);
        let wrap: Vec<i32> = bytes
            .chunks(4)
            .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) >> 8)
            .collect();
        assert_eq!(wrap, data.iter().map(|v| v << 8).collect::<Vec<_>>());
    }

    #[test]
    fn el_valor_maximo_de_24_bits_no_pierde_su_ultimo_lsb() {
        // Este es el hallazgo del packer de QBZ: escalar por 8_388_607.0 y
        // redondear movía las muestras fuertes un LSB.
        let data = [8_388_607i32, -8_388_608, 4_194_303];
        let bytes = packed(&Samples::Int { data: &data, bits: 24 }, SampleFormat::S24Packed);
        let wrap: Vec<i32> = bytes
            .chunks(3)
            .map(|c| (i32::from_le_bytes([0, c[0], c[1], c[2]])) >> 8)
            .collect();
        assert_eq!(wrap, data);
    }

    #[test]
    fn el_byte_bajo_de_24_en_32_queda_en_cero() {
        let bytes = packed(&Samples::Int { data: &[0x7FFFFF], bits: 24 }, SampleFormat::S24In32);
        assert_eq!(bytes[0], 0, "justificado a la izquierda");
    }

    #[test]
    fn bajar_de_bits_se_rechaza_en_vez_de_truncar_en_silencio() {
        let mut out = Vec::new();
        let r = pack(&Samples::Int { data: &[1], bits: 24 }, SampleFormat::S16, &mut out);
        assert_eq!(r, Err(PackError::WouldTruncate { from_bits: 24, to_bits: 16 }));
        assert!(out.is_empty());
    }
}
