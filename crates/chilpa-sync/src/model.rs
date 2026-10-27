use crate::{invalid, Result, BUFFER_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) { self.0.store(true, Ordering::Relaxed); }
    pub fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Relaxed) { Err(crate::Error::Cancelled) } else { Ok(()) }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub version: u32,
    pub id: String,
    pub name: String,
}
impl Target {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.version != 1 || self.id.len() != 32 || !self.id.bytes().all(|c| c.is_ascii_hexdigit())
            || self.name.len() > 256 { return Err(invalid("unsupported or damaged device identity")); }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint { pub size: u64, pub sha256: String }

// I reserve the largest compact Fingerprint replacing a deferred JSON null.
// A validated digest has exactly 64 hexadecimal bytes, none requiring escaping.
pub(crate) const MAX_DIGEST_JSON_GROWTH: usize =
    br#"{"size":18446744073709551615,"sha256":""}"#.len() + 64 - b"null".len();

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetKind { Media, Playlist }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub id: String,
    pub source: PathBuf,
    pub artist: String,
    pub album: String,
    pub title: String,
    pub track_number: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    /// I preserve both order and duplicate occurrences.
    pub tracks: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Progress {
    pub completed_files: usize,
    pub total_files: usize,
    pub current_path: String,
    pub transferred_bytes: u64,
    pub verifying: bool,
}

/// I report completed units within the named review stage, never simulated transfer bytes.
#[derive(Clone, Debug)]
pub struct ReviewProgress {
    pub stage: &'static str,
    pub completed: usize,
    pub total: usize,
    pub current_path: String,
    pub selected_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) enum Source { File(PathBuf), CheckedFile { path: PathBuf, stamp: crate::source::Stamp }, Bytes(Vec<u8>) }
impl Source {
    pub fn check(&self) -> Result<()> {
        if let Self::CheckedFile { path, stamp } = self { stamp.check(path)?; }
        Ok(())
    }
    pub fn reader(&self) -> Result<SourceReader<'_>> {
        let (input, guard): (Box<dyn Read + Send + '_>, _) = match self {
            Self::File(path) => (Box::new(std::fs::File::open(path)?), None),
            Self::CheckedFile { path, stamp } => {
                let file = std::fs::File::open(path)?;
                stamp.check_file(&file, path)?;
                (Box::new(file.try_clone()?), Some((file, path.as_path(), stamp)))
            },
            Self::Bytes(bytes) => (Box::new(bytes.as_slice()), None),
        };
        Ok(SourceReader { input, guard })
    }
}
pub(crate) struct SourceReader<'a> {
    input: Box<dyn Read + Send + 'a>,
    guard: Option<(std::fs::File, &'a std::path::Path, &'a crate::source::Stamp)>,
}
impl SourceReader<'_> {
    pub fn check(&self) -> Result<()> {
        if let Some((file, path, stamp)) = &self.guard { stamp.check_file(file, path)?; }
        Ok(())
    }
}
impl Read for SourceReader<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> { self.input.read(bytes) }
}

pub(crate) struct HashWriter { hash: Sha256, size: u64, cancel: Cancellation }
impl HashWriter {
    pub fn new(cancel: &Cancellation) -> Self { Self { hash: Sha256::new(), size: 0, cancel: cancel.clone() } }
    pub fn finish(self) -> Fingerprint { Fingerprint { size: self.size, sha256: format!("{:x}", self.hash.finalize()) } }
}
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.cancel.check().map_err(std::io::Error::other)?;
        self.hash.update(bytes);
        self.size += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}
pub(crate) fn fingerprint(reader: &mut dyn Read, cancel: &Cancellation) -> Result<Fingerprint> {
    let mut writer = HashWriter::new(cancel);
    let mut buffer = vec![0; BUFFER_BYTES];
    loop {
        cancel.check()?;
        let n = reader.read(&mut buffer)?;
        if n == 0 { break; }
        writer.write_all(&buffer[..n])?;
    }
    Ok(writer.finish())
}
pub(crate) fn hash_text(text: &str) -> String { format!("{:x}", Sha256::digest(text.as_bytes())) }
