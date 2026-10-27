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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) enum Source { File(PathBuf), Bytes(Vec<u8>) }
impl Source {
    pub fn reader(&self) -> Result<Box<dyn Read + Send + '_>> {
        match self { Self::File(p) => Ok(Box::new(std::fs::File::open(p)?)), Self::Bytes(b) => Ok(Box::new(b.as_slice())) }
    }
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
