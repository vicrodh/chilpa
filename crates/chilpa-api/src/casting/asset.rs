use chilpa_cast::{media_server::{ExternalAsset, RangeSource}, Error, Metadata};
use std::{io::{self, Read, Seek, SeekFrom}, path::{Path, PathBuf}, sync::Arc, time::Duration};

pub(super) struct Presentation {
    pub playback: Option<Arc<dyn chilpa_core::playback_events::Observer>>,
    pub artwork: Option<PathBuf>, pub bits: Option<u8>, pub rate: Option<u32>,
}
struct FileSource(PathBuf);
impl RangeSource for FileSource {
    fn open(&self, start: u64, length: u64) -> io::Result<Box<dyn Read + Send>> {
        let mut file = std::fs::File::open(&self.0)?; file.seek(SeekFrom::Start(start))?;
        Ok(Box::new(file.take(length)))
    }
}
#[derive(Clone)]
struct ServerSource { request: chilpa_core::RangedRequest, length: u64 }
impl ServerSource {
    fn fetch(&self, start: u64, length: u64) -> io::Result<Vec<u8>> {
        let mut headers = self.request.headers.clone();
        headers.push(("Range".into(), format!("bytes={start}-{}", start + length - 1)));
        headers.push(("Accept-Encoding".into(), "identity".into()));
        let reply = chilpa_core::call_server(&chilpa_core::Call { method: "GET", url: &self.request.url, headers: &headers, body: None, timeout: Duration::from_secs(20), max_bytes: length as usize }).map_err(|_| io::Error::other(Error::Asset))?;
        let expected = format!("bytes {start}-{}/{}", start + length - 1, self.length);
        if reply.status != 206 || reply.body.len() as u64 != length || !reply.headers.iter().any(|(k,v)| k.eq_ignore_ascii_case("content-range") && v == &expected) { return Err(io::Error::other(Error::Asset)); }
        Ok(reply.body)
    }
}
struct ServerReader { source: ServerSource, next: u64, left: u64, buffer: io::Cursor<Vec<u8>> }
impl Read for ServerReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() { return Ok(0); }
        if self.buffer.position() as usize == self.buffer.get_ref().len() && self.left > 0 {
            let amount = self.left.min(256 * 1024);
            self.buffer = io::Cursor::new(self.source.fetch(self.next, amount)?);
            self.next += amount; self.left -= amount;
        }
        self.buffer.read(output)
    }
}
impl RangeSource for ServerSource {
    fn open(&self, start: u64, length: u64) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(ServerReader { source: self.clone(), next: start, left: length, buffer: io::Cursor::new(Vec::new()) }))
    }
}
fn mime(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "flac" => "audio/flac", "mp3" => "audio/mpeg", "m4a" | "alac" | "mp4" => "audio/mp4",
        "aac" => "audio/aac", "wav" => "audio/wav", "ogg" | "oga" => "audio/ogg", "opus" => "audio/opus",
        "aif" | "aiff" => "audio/aiff", "dsf" => "audio/x-dsf", "dff" => "audio/x-dff", "ape" => "audio/x-ape",
        _ => "application/octet-stream",
    }
}
pub(super) fn resolve(path: &str) -> Result<(ExternalAsset, Metadata, Presentation), Error> {
    if let Some(entry) = chilpa_models::RemoteEntry::parse(path) {
        let (located, described) = chilpa_core::remote::external(&entry).map_err(|_| Error::Asset)?;
        let mime = mime(located.extension.as_deref().unwrap_or_default());
        let mut headers = located.request.headers.clone();
        headers.push(("Range".into(), "bytes=0-0".into())); headers.push(("Accept-Encoding".into(), "identity".into()));
        let reply = chilpa_core::call_server(&chilpa_core::Call { method: "GET", url: &located.request.url, headers: &headers, body: None, timeout: Duration::from_secs(20), max_bytes: 1 }).map_err(|_| Error::Asset)?;
        let length = reply.headers.iter().find(|(k,_)| k.eq_ignore_ascii_case("content-range")).and_then(|(_,v)| v.strip_prefix("bytes 0-0/")).and_then(|v| v.parse::<u64>().ok()).filter(|n| *n > 0).ok_or(Error::Asset)?;
        if reply.status != 206 || reply.body.len() != 1 { return Err(Error::Asset); }
        let metadata = Metadata { title: described.tags.title.clone().unwrap_or_default(), artist: described.tags.artist_line(), album: described.tags.album.unwrap_or_default(), duration: described.duration.as_secs_f64() };
        return Ok((ExternalAsset { length, mime: mime.into(), source: Arc::new(ServerSource { request: located.request, length }) }, metadata, Presentation { playback: located.playback, artwork: described.artwork, bits: None, rate: None }));
    }
    let path = Path::new(path);
    let mime = mime(path.extension().and_then(|s| s.to_str()).unwrap_or_default());
    let info = std::fs::metadata(path).map_err(|_| Error::Asset)?;
    if !info.is_file() || info.len() == 0 { return Err(Error::Asset); }
    let tags = chilpa_tags::read(path).ok();
    let metadata = Metadata {
        title: tags.as_ref().and_then(|t| t.tags.title.clone()).unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into()),
        artist: tags.as_ref().map(|t| t.tags.artist_line()).unwrap_or_default(),
        album: tags.as_ref().and_then(|t| t.tags.album.clone()).unwrap_or_default(), duration: tags.as_ref().map(|t| t.duration.as_secs_f64()).unwrap_or_default(),
    };
    let presentation = Presentation {
        playback: None,
        artwork: chilpa_tags::artwork_for(path, &chilpa_core::artwork_cache()),
        bits: tags.as_ref().and_then(|t| t.bit_depth), rate: tags.as_ref().and_then(|t| t.sample_rate_hz),
    };
    Ok((ExternalAsset { length: info.len(), mime: mime.into(), source: Arc::new(FileSource(path.into())) }, metadata, presentation))
}
