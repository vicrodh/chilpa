//! Streams por HTTP: radio y cualquier otra cosa que llegue por la red.
//!
//! Un stream no es un archivo: no tiene final conocido, no se puede buscar y
//! puede cortarse a media canción. Este crate lo envuelve para que el resto
//! del motor no tenga que saberlo, y guarda dos cosas que sí cambian la
//! experiencia:
//!
//! - **La metadata ICY** (`StreamTitle`), que es lo único que dice qué suena
//!   en una radio.
//! - **El formato que anuncia el servidor**, para elegir el decodificador sin
//!   adivinar por la extensión de una URL que muchas veces no tiene.

pub mod icy;
pub mod request;

pub use icy::IcyReader;
pub use request::send as send_request;

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub enum StreamError {
    /// La URL no se pudo abrir.
    Open(String),
    /// El servidor contestó algo que no es audio.
    NotAudio(String),
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamError::Open(s) => write!(f, "no se pudo abrir el stream: {s}"),
            StreamError::NotAudio(s) => write!(f, "el servidor no mandó audio: {s}"),
        }
    }
}

/// Lo que se sabe de un stream cuando se abre.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct StreamInfo {
    /// El nombre de la estación, cuando lo manda.
    pub name: Option<String>,
    /// El tipo de contenido: `audio/mpeg`, `audio/flac`, …
    pub content_type: Option<String>,
    /// La extensión que le corresponde, para ayudar al decodificador.
    pub hint: Option<String>,
    /// Cada cuántos bytes viene un bloque de metadata, si el servidor lo
    /// ofrece.
    pub icy_metaint: Option<usize>,
}

impl StreamInfo {
    /// La extensión que le pasamos al probador de formatos.
    pub fn extension_hint(&self) -> Option<String> {
        self.hint.clone().or_else(|| {
            let kind = self.content_type.as_deref()?;
            Some(
                match kind.split(';').next()?.trim() {
                    "audio/mpeg" | "audio/mp3" => "mp3",
                    "audio/aac" | "audio/aacp" => "aac",
                    "audio/flac" | "audio/x-flac" => "flac",
                    "audio/ogg" | "application/ogg" => "ogg",
                    "audio/wav" | "audio/x-wav" => "wav",
                    _ => return None,
                }
                .to_string(),
            )
        })
    }
}

/// Lo que va cambiando mientras suena: hoy, el título que manda la estación.
#[derive(Clone, Debug, Default)]
pub struct NowPlaying(Arc<Mutex<Option<String>>>);

impl NowPlaying {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, title: Option<String>) {
        if let Ok(mut g) = self.0.lock() {
            *g = title;
        }
    }

    pub fn get(&self) -> Option<String> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
}

/// Un stream abierto, listo para que lo lea el decodificador.
///
/// Implementa `Seek` porque el decodificador lo exige, pero solo contesta la
/// posición: un stream no se puede rebobinar, y fingir que sí produce audio
/// corrupto en vez de un error claro.
pub struct HttpStream {
    inner: Box<dyn Read + Send + Sync>,
    info: StreamInfo,
    now_playing: NowPlaying,
    position: u64,
}

impl HttpStream {
    /// Construye el stream a partir de un reader ya abierto.
    pub fn new(
        inner: Box<dyn Read + Send + Sync>,
        info: StreamInfo,
        now_playing: NowPlaying,
    ) -> Self {
        Self { inner, info, now_playing, position: 0 }
    }

    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    /// El título que manda la estación, si manda alguno.
    pub fn now_playing(&self) -> NowPlaying {
        self.now_playing.clone()
    }
}

impl Read for HttpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.position += n as u64;
        Ok(n)
    }
}

impl Seek for HttpStream {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match pos {
            // Quedarse donde se está es la única búsqueda posible.
            SeekFrom::Current(0) => Ok(self.position),
            SeekFrom::Start(n) if n == self.position => Ok(self.position),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "un stream no se puede rebobinar",
            )),
        }
    }
}

impl symphonia::core::io::MediaSource for HttpStream {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

/// Abre una URL y deja el audio listo para el decodificador.
///
/// Pide la metadata ICY: si el servidor la ofrece, el reader la saca del
/// flujo y publica el título; si no, el audio pasa tal cual.
pub fn open(url: &str) -> Result<HttpStream, StreamError> {
    let reply = ureq::get(url)
        .header("Icy-MetaData", "1")
        .header("User-Agent", "Chilpa")
        .call()
        .map_err(|e| StreamError::Open(e.to_string()))?;

    let header = |name: &str| reply.headers().get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let info = StreamInfo {
        name: header("icy-name"),
        content_type: header("content-type"),
        hint: None,
        icy_metaint: header("icy-metaint").and_then(|v| v.parse().ok()),
    };
    if let Some(kind) = &info.content_type {
        if kind.starts_with("text/") || kind.contains("html") {
            return Err(StreamError::NotAudio(kind.clone()));
        }
    }

    let now_playing = NowPlaying::new();
    let body = reply.into_body().into_reader();
    let reader: Box<dyn Read + Send + Sync> = match info.icy_metaint {
        Some(metaint) if metaint > 0 => {
            Box::new(IcyReader::new(body, metaint, now_playing.clone()))
        }
        _ => Box::new(body),
    };
    Ok(HttpStream::new(reader, info, now_playing))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_content_type_suggests_the_extension() {
        let mut info = StreamInfo { content_type: Some("audio/mpeg".into()), ..Default::default() };
        assert_eq!(info.extension_hint().as_deref(), Some("mp3"));
        info.content_type = Some("audio/flac; charset=utf-8".into());
        assert_eq!(info.extension_hint().as_deref(), Some("flac"));
        info.content_type = Some("text/html".into());
        assert_eq!(info.extension_hint(), None);
    }

    #[test]
    fn a_stream_does_not_rewind() {
        let data: Vec<u8> = (0..100u8).collect();
        let mut s = HttpStream::new(
            Box::new(std::io::Cursor::new(data)),
            StreamInfo::default(),
            NowPlaying::new(),
        );
        let mut buf = [0u8; 10];
        assert_eq!(s.read(&mut buf).unwrap(), 10);
        assert_eq!(s.seek(SeekFrom::Current(0)).unwrap(), 10, "sí dice dónde va");
        assert!(s.seek(SeekFrom::Start(0)).is_err(), "pero no vuelve atrás");
    }

    #[test]
    fn the_station_title_is_shared_across_threads() {
        let np = NowPlaying::new();
        let copy = np.clone();
        std::thread::spawn(move || copy.set(Some("Una canción".into()))).join().unwrap();
        assert_eq!(np.get().as_deref(), Some("Una canción"));
    }
}
