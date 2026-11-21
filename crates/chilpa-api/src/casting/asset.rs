use chilpa_cast::{media_server::{ExternalAsset, RangeSource}, Error, Metadata};
use std::{io::{self, Read, Seek, SeekFrom}, path::{Path, PathBuf}, sync::Arc, time::Duration};

pub(super) struct Presentation {
    pub playback: Option<Arc<dyn chilpa_core::playback_events::Observer>>,
    pub artwork: Option<PathBuf>, pub bits: Option<u8>, pub rate: Option<u32>,
    /// The DSD bits travel in another container than the track's own.
    pub repackaged: bool,
}
struct FileSource(PathBuf);
impl RangeSource for FileSource {
    fn open(&self, start: u64, length: u64) -> io::Result<Box<dyn Read + Send>> {
        let mut file = std::fs::File::open(&self.0)?; file.seek(SeekFrom::Start(start))?;
        Ok(Box::new(file.take(length)))
    }
}
// Each reader owns its position. The shared open handle pins the verified
// bytes, and only a bounded read holds its lock across concurrent ranges.
struct OpenFile {file:Arc<std::sync::Mutex<std::fs::File>>,lease:Arc<dyn Send+Sync>}
struct FileReader {file:Arc<std::sync::Mutex<std::fs::File>>,_lease:Arc<dyn Send+Sync>,position:u64,left:u64}
impl Read for FileReader {
    fn read(&mut self, output:&mut [u8]) -> io::Result<usize> {
        let amount=output.len().min(self.left.min(64*1024) as usize);
        if amount==0 {return Ok(0);}
        let mut file=self.file.lock().unwrap_or_else(|error| error.into_inner());
        file.seek(SeekFrom::Start(self.position))?;
        let count=file.read(&mut output[..amount])?;
        self.position+=count as u64;self.left-=count as u64;
        Ok(count)
    }
}
impl RangeSource for OpenFile {
    fn open(&self,start:u64,length:u64) -> io::Result<Box<dyn Read+Send>> {
        Ok(Box::new(FileReader {file:self.file.clone(),_lease:self.lease.clone(),position:start,left:length}))
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
/// A DSD track laid out as DSF on the way out.
struct Repackaged(chilpa_core::dsf_stream::DsfStream);
impl RangeSource for Repackaged {
    fn open(&self, start: u64, length: u64) -> io::Result<Box<dyn Read + Send>> { self.0.read(start, length) }
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
        let metadata = Metadata { title: described.tags.title.clone().unwrap_or_default(), artist: described.tags.artist_line(), album: described.tags.album.unwrap_or_default(), duration: described.duration.as_secs_f64() };
        if let chilpa_core::remote::Media::File {file,lease}=located.media {
            let length=file.metadata().map_err(|_| Error::Asset)?.len();
            return Ok((ExternalAsset {length,mime:mime.into(),source:Arc::new(OpenFile {file:Arc::new(std::sync::Mutex::new(file)),lease})},metadata,
                Presentation {playback:located.playback,artwork:described.artwork,bits:None,rate:None,repackaged:false}));
        }
        let chilpa_core::remote::Media::Remote(request)=located.media else {return Err(Error::Asset)};
        let mut headers = request.headers.clone();
        headers.push(("Range".into(), "bytes=0-0".into())); headers.push(("Accept-Encoding".into(), "identity".into()));
        let reply = chilpa_core::call_server(&chilpa_core::Call { method: "GET", url: &request.url, headers: &headers, body: None, timeout: Duration::from_secs(20), max_bytes: 1 }).map_err(|_| Error::Asset)?;
        let length = reply.headers.iter().find(|(k,_)| k.eq_ignore_ascii_case("content-range")).and_then(|(_,v)| v.strip_prefix("bytes 0-0/")).and_then(|v| v.parse::<u64>().ok()).filter(|n| *n > 0).ok_or(Error::Asset)?;
        if reply.status != 206 || reply.body.len() != 1 { return Err(Error::Asset); }
        return Ok((ExternalAsset { length, mime: mime.into(), source: Arc::new(ServerSource { request, length }) }, metadata, Presentation { playback: located.playback, artwork: described.artwork, bits: None, rate: None, repackaged: false }));
    }
    let path = Path::new(path);
    // A track inside a disc image is no file a renderer can fetch: it goes
    // out as DSF, the same DSD bits repackaged.
    if let Some((image, _)) = chilpa_core::sacd_tracks::parse(path) {
        let stream = chilpa_core::dsf_stream::DsfStream::open(path).map_err(|_| Error::Asset)?;
        let tags = stream.tags().cloned().unwrap_or_default();
        let metadata = Metadata { title: tags.title.clone().unwrap_or_default(), artist: tags.artist_line(), album: tags.album.clone().unwrap_or_default(), duration: stream.seconds() };
        let presentation = Presentation { playback: None, artwork: chilpa_tags::artwork_for(&image, &chilpa_core::artwork_cache()), bits: Some(1), rate: Some(stream.rate_hz()), repackaged: true };
        return Ok((ExternalAsset { length: stream.length(), mime: "audio/x-dsf".into(), source: Arc::new(Repackaged(stream)) }, metadata, presentation));
    }
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
        repackaged: false,
    };
    Ok((ExternalAsset { length: info.len(), mime: mime.into(), source: Arc::new(FileSource(path.into())) }, metadata, presentation))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-track SACD image, laid out as `chilpa-core`'s own SACD tests
    /// lay it out.
    fn sacd_image(name: &str) -> PathBuf {
        const SECTOR: usize = 2048;
        const AREA: u32 = 540;
        const AUDIO_START: u32 = 550;
        const AUDIO_END: u32 = 569;
        const SECOND: usize = 8 + 255 * 4;
        fn sector(img: &mut [u8], lsn: u32) -> &mut [u8] { let at = lsn as usize * SECTOR; &mut img[at..at + SECTOR] }
        let mut img = vec![0u8; 600 * SECTOR];
        let m = sector(&mut img, 510);
        m[..8].copy_from_slice(b"SACDMTOC"); m[8] = 1; m[9] = 20;
        m[0x40..0x44].copy_from_slice(&AREA.to_be_bytes()); m[0x54..0x56].copy_from_slice(&5u16.to_be_bytes()); m[0x80] = 1; m[0x8a] = 2;
        let t = sector(&mut img, 511);
        t[..8].copy_from_slice(b"SACDText");
        t[0x10..0x12].copy_from_slice(&0x40u16.to_be_bytes()); t[0x12..0x14].copy_from_slice(&0x80u16.to_be_bytes());
        t[0x40..0x40 + 18].copy_from_slice(b"Agents Of Fortune\0");
        let a = sector(&mut img, AREA);
        a[..8].copy_from_slice(b"TWOCHTOC"); a[0x14] = 4; a[0x15] = 2; a[0x20] = 2; a[0x41] = 2; a[0x45] = 2;
        a[0x48..0x4c].copy_from_slice(&AUDIO_START.to_be_bytes()); a[0x4c..0x50].copy_from_slice(&AUDIO_END.to_be_bytes());
        a[0x5a] = 2; a[0x80..0x82].copy_from_slice(&3u16.to_be_bytes());
        let t1 = sector(&mut img, AREA + 1);
        t1[..8].copy_from_slice(b"SACDTRL1");
        t1[8..12].copy_from_slice(&AUDIO_START.to_be_bytes()); t1[12..16].copy_from_slice(&(AUDIO_START + 10).to_be_bytes());
        t1[SECOND..SECOND + 4].copy_from_slice(&10u32.to_be_bytes()); t1[SECOND + 4..SECOND + 8].copy_from_slice(&10u32.to_be_bytes());
        let t2 = sector(&mut img, AREA + 2);
        t2[..8].copy_from_slice(b"SACDTRL2"); t2[12..16].copy_from_slice(&[0, 1, 0, 0]);
        t2[SECOND..SECOND + 4].copy_from_slice(&[0, 0, 2, 0]); t2[SECOND + 4..SECOND + 8].copy_from_slice(&[0, 0, 2, 0]);
        let b = sector(&mut img, AREA + 3);
        b[..8].copy_from_slice(b"SACDTTxt");
        b[8..10].copy_from_slice(&(SECTOR as u16 + 0x20).to_be_bytes()); b[10..12].copy_from_slice(&(SECTOR as u16 + 0x80).to_be_bytes());
        let b = sector(&mut img, AREA + 4);
        b[0x20] = 1; b[0x24] = 1; b[0x26..0x26 + 30].copy_from_slice(b"This Ain't The Summer Of Love\0");
        b[0x80] = 1; b[0x84] = 1; b[0x86..0x86 + 17].copy_from_slice(b"True Confessions\0");
        for (i, lsn) in (AUDIO_START..=AUDIO_END).enumerate() {
            let s = sector(&mut img, lsn);
            s[0] = (1 << 5) | (1 << 2);
            let w: u16 = 0x8000 | (2 << 11) | (SECTOR - 6) as u16;
            s[1..3].copy_from_slice(&w.to_be_bytes());
            s[4] = (i / 75) as u8; s[5] = (i % 75) as u8;
            s[6..].fill(0x69);
        }
        let path = std::env::temp_dir().join(format!("chilpa-cast-asset-{name}.iso"));
        std::fs::write(&path, img).unwrap();
        path
    }

    #[test]
    fn a_track_inside_a_disc_image_is_cast_as_dsf() {
        let iso = sacd_image("disc-track");
        let track = format!("sacd:{}#2", iso.display());
        // Before, this was Error::Asset: a disc track is no file to serve.
        let (asset, metadata, presentation) = resolve(&track).unwrap_or_else(|_| panic!("a disc track resolves"));
        assert_eq!(asset.mime, "audio/x-dsf");
        assert_eq!(asset.length, chilpa_core::dsf_stream::DsfStream::open(Path::new(&track)).unwrap().length());
        assert_eq!((metadata.title.as_str(), metadata.album.as_str()), ("True Confessions", "Agents Of Fortune"));
        assert!(metadata.duration > 0.0);
        assert!(presentation.repackaged);
        assert_eq!((presentation.bits, presentation.rate), (Some(1), Some(2_822_400)));
        let mut head = Vec::new();
        asset.source.open(0, 4).unwrap().read_to_end(&mut head).unwrap();
        assert_eq!(head, b"DSD ");
        let mut tail = Vec::new();
        asset.source.open(asset.length - 16, 16).unwrap().read_to_end(&mut tail).unwrap();
        assert_eq!(tail.len(), 16, "the last bytes of the file read on their own");
        let _ = std::fs::remove_file(iso);
    }
}
