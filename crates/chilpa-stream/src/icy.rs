//! La metadata que mete Shoutcast/Icecast en medio del audio.
//!
//! El servidor avisa cada cuántos bytes de audio viene un bloque de metadata
//! (`icy-metaint`). El bloque empieza con un byte que dice su tamaño en
//! múltiplos de 16; si es cero, no hay metadata esta vez, que es el caso
//! común.
//!
//! Lo importante: **esos bytes NO son audio**. Si se le pasan al
//! decodificador, suenan como un chasquido cada pocos segundos. Este lector
//! los saca del flujo y deja el audio limpio.

use crate::NowPlaying;
use std::io::{self, Read};

/// Lee un stream con metadata ICY intercalada y entrega solo el audio.
pub struct IcyReader<R: Read> {
    inner: R,
    /// Cada cuántos bytes de audio viene un bloque de metadata.
    metaint: usize,
    /// Cuántos bytes de audio faltan para el siguiente bloque.
    restante: usize,
    now_playing: NowPlaying,
}

impl<R: Read> IcyReader<R> {
    pub fn new(inner: R, metaint: usize, now_playing: NowPlaying) -> Self {
        Self { inner, metaint, restante: metaint, now_playing }
    }

    /// Lee y aplica un bloque de metadata.
    fn leer_metadata(&mut self) -> io::Result<()> {
        let mut largo = [0u8; 1];
        if self.inner.read(&mut largo)? == 0 {
            return Ok(());
        }
        let bytes = largo[0] as usize * 16;
        if bytes == 0 {
            // Lo normal: el servidor no tiene nada nuevo que decir.
            return Ok(());
        }
        let mut block = vec![0u8; bytes];
        self.inner.read_exact(&mut block)?;
        if let Some(title) = title_of(&block) {
            self.now_playing.set(Some(title));
        }
        Ok(())
    }
}

impl<R: Read> Read for IcyReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.metaint == 0 {
            return self.inner.read(buf);
        }
        if self.restante == 0 {
            self.leer_metadata()?;
            self.restante = self.metaint;
        }
        // Nunca se lee más allá del siguiente bloque de metadata: pasarse
        // metería esos bytes en el audio.
        let hasta = buf.len().min(self.restante);
        let n = self.inner.read(&mut buf[..hasta])?;
        self.restante -= n;
        Ok(n)
    }
}

/// Saca `StreamTitle` de un bloque de metadata.
///
/// El bloque viene como `StreamTitle='Artista - Canción';StreamUrl='...';`
/// rellenado con ceros hasta un múltiplo de 16.
pub fn title_of(block: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(block);
    let text = text.trim_end_matches('\0');
    let start = text.find("StreamTitle=")? + "StreamTitle=".len();
    let rest = &text[start..];
    let rest = rest.strip_prefix('\'').unwrap_or(rest);
    let fin = rest.find("';").unwrap_or(rest.len());
    let title = rest[..fin].trim();
    (!title.is_empty()).then(|| title.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(text: &str) -> Vec<u8> {
        let mut data = text.as_bytes().to_vec();
        while data.len() % 16 != 0 {
            data.push(0);
        }
        let mut output = vec![(data.len() / 16) as u8];
        output.extend_from_slice(&data);
        output
    }

    #[test]
    fn se_saca_el_titulo_del_bloque() {
        let b = "StreamTitle='Creedence - Born On The Bayou';StreamUrl='';";
        assert_eq!(
            title_of(b.as_bytes()).as_deref(),
            Some("Creedence - Born On The Bayou")
        );
    }

    #[test]
    fn un_bloque_sin_titulo_no_dice_nada() {
        assert_eq!(title_of(b"StreamUrl='http://x';"), None);
        assert_eq!(title_of(b"StreamTitle='';"), None);
    }

    #[test]
    fn la_metadata_sale_del_audio_y_el_titulo_se_publica() {
        // 16 bytes de audio, un bloque con título, y otros 16 de audio.
        let mut data = Vec::new();
        data.extend_from_slice(&[1u8; 16]);
        data.extend_from_slice(&block("StreamTitle='Una canción';"));
        data.extend_from_slice(&[2u8; 16]);
        data.push(0); // el bloque vacío que toca después

        let np = NowPlaying::new();
        let mut lector = IcyReader::new(std::io::Cursor::new(data), 16, np.clone());

        let mut audio = Vec::new();
        lector.read_to_end(&mut audio).unwrap();

        assert_eq!(audio, [[1u8; 16], [2u8; 16]].concat(), "el audio salió limpio");
        assert_eq!(np.get().as_deref(), Some("Una canción"));
    }

    #[test]
    fn sin_metaint_el_flujo_pasa_tal_cual() {
        let data: Vec<u8> = (0..50u8).collect();
        let mut lector =
            IcyReader::new(std::io::Cursor::new(data.clone()), 0, NowPlaying::new());
        let mut output = Vec::new();
        lector.read_to_end(&mut output).unwrap();
        assert_eq!(output, data);
    }
}
