//! The metadata Shoutcast and Icecast put in the middle of the audio.
//!
//! The server says how many audio bytes go between metadata blocks
//! (`icy-metaint`). The block starts with a byte giving its size in
//! multiples of 16; zero means there is none this time, which is the common
//! case.
//!
//! What matters: **those bytes are NOT audio**. Handed to the decoder they
//! sound like a click every few seconds. This reader takes them out of the
//! stream and leaves the audio clean.

use crate::NowPlaying;
use std::io::{self, Read};

/// Reads a stream with ICY metadata woven in and hands over only the audio.
pub struct IcyReader<R: Read> {
    inner: R,
    /// How many audio bytes go between metadata blocks.
    metaint: usize,
    /// How many audio bytes are left before the next block.
    restante: usize,
    now_playing: NowPlaying,
}

impl<R: Read> IcyReader<R> {
    pub fn new(inner: R, metaint: usize, now_playing: NowPlaying) -> Self {
        Self { inner, metaint, restante: metaint, now_playing }
    }

    /// Reads a metadata block and applies it.
    fn leer_metadata(&mut self) -> io::Result<()> {
        let mut largo = [0u8; 1];
        if self.inner.read(&mut largo)? == 0 {
            return Ok(());
        }
        let bytes = largo[0] as usize * 16;
        if bytes == 0 {
            // The usual case: the server has nothing new to say.
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
        // Never read past the next metadata block: overshooting would put
        // those bytes into the audio.
        let hasta = buf.len().min(self.restante);
        let n = self.inner.read(&mut buf[..hasta])?;
        self.restante -= n;
        Ok(n)
    }
}

/// Pulls `StreamTitle` out of a metadata block.
///
/// The block arrives as `StreamTitle='...';StreamUrl='...';` padded with
/// zeros to a multiple of 16.
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
    fn the_title_is_pulled_out_of_the_block() {
        let b = "StreamTitle='Creedence - Born On The Bayou';StreamUrl='';";
        assert_eq!(
            title_of(b.as_bytes()).as_deref(),
            Some("Creedence - Born On The Bayou")
        );
    }

    #[test]
    fn a_block_without_a_title_says_nothing() {
        assert_eq!(title_of(b"StreamUrl='http://x';"), None);
        assert_eq!(title_of(b"StreamTitle='';"), None);
    }

    #[test]
    fn the_metadata_is_taken_out_of_the_audio_and_the_title_is_published() {
        // 16 bytes of audio, a block with a title, and 16 more of audio.
        let mut data = Vec::new();
        data.extend_from_slice(&[1u8; 16]);
        data.extend_from_slice(&block("StreamTitle='A song';"));
        data.extend_from_slice(&[2u8; 16]);
        data.push(0); // the empty block that comes next

        let np = NowPlaying::new();
        let mut lector = IcyReader::new(std::io::Cursor::new(data), 16, np.clone());

        let mut audio = Vec::new();
        lector.read_to_end(&mut audio).unwrap();

        assert_eq!(audio, [[1u8; 16], [2u8; 16]].concat(), "the audio came out clean");
        assert_eq!(np.get().as_deref(), Some("A song"));
    }

    #[test]
    fn without_metaint_the_stream_passes_through_unchanged() {
        let data: Vec<u8> = (0..50u8).collect();
        let mut lector =
            IcyReader::new(std::io::Cursor::new(data.clone()), 0, NowPlaying::new());
        let mut output = Vec::new();
        lector.read_to_end(&mut output).unwrap();
        assert_eq!(output, data);
    }
}
