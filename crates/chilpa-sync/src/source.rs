//! I separate a local metadata snapshot from a committed content digest.
use crate::{conflict, model::HashWriter, Cancellation, Fingerprint, Result};
use serde::{Deserialize, Serialize};
use std::{fs::{File, Metadata}, io::{Read, Write}, path::Path, time::{SystemTime, UNIX_EPOCH}};

// I store the direction and full duration from the epoch: SystemTime's own
// serializer rejects valid dates before 1970, while signed seconds can overflow.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Timestamp {
    before_epoch: bool,
    seconds: u64,
    nanoseconds: u32,
}
impl From<SystemTime> for Timestamp {
    fn from(time: SystemTime) -> Self {
        let (before_epoch, duration) = match time.duration_since(UNIX_EPOCH) {
            Ok(duration) => (false, duration),
            Err(error) => (true, error.duration()),
        };
        Self { before_epoch, seconds:duration.as_secs(), nanoseconds:duration.subsec_nanos() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stamp {
    pub size: u64,
    modified: Timestamp,
    created: Option<Timestamp>,
    identity: Option<[u64; 4]>,
}
impl Stamp {
    fn metadata(metadata: &Metadata) -> Result<Self> {
        if !metadata.is_file() { return Err(conflict("sync source is not a regular file")); }
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            Some([metadata.dev(), metadata.ino(), metadata.ctime() as u64, metadata.ctime_nsec() as u64])
        };
        #[cfg(not(unix))]
        let identity = None;
        Ok(Self { size:metadata.len(), modified:metadata.modified()?.into(), created:metadata.created().ok().map(Into::into), identity })
    }
    fn file(file: &File) -> Result<Self> {
        let stamp = Self::metadata(&file.metadata()?)?;
        #[cfg(windows)]
        let stamp = {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, GetFileInformationByHandleEx, FileBasicInfo, BY_HANDLE_FILE_INFORMATION, FILE_BASIC_INFO};
            let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let mut basic: FILE_BASIC_INFO = unsafe { std::mem::zeroed() };
            if unsafe { GetFileInformationByHandleEx(file.as_raw_handle(), FileBasicInfo,
                (&mut basic as *mut FILE_BASIC_INFO).cast(), std::mem::size_of::<FILE_BASIC_INFO>() as u32) } == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            Self { identity:Some([info.dwVolumeSerialNumber as u64, info.nFileIndexHigh as u64,
                info.nFileIndexLow as u64, basic.ChangeTime as u64]), ..stamp }
        };
        Ok(stamp)
    }
    pub fn read(path: &Path) -> Result<Self> { Self::file(&File::open(path)?) }
    pub fn check(&self, path: &Path) -> Result<()> {
        if Self::read(path)? != *self { return Err(conflict("sync source changed after review")); }
        Ok(())
    }
    pub fn check_file(&self, file: &File, path: &Path) -> Result<()> {
        if Self::file(file)? != *self { return Err(conflict("sync source changed during transfer")); }
        self.check(path)
    }
    pub fn version(&self) -> String {
        // This names a metadata version, never a content fingerprint.
        crate::model::hash_text(&serde_json::to_string(self).expect("serializable source metadata"))
    }
}

/// I hash the bytes consumed by the transport in the same pass, using its existing buffer.
pub(crate) struct DigestReader<R> { input: R, hash: HashWriter }
impl<R: Read> DigestReader<R> {
    pub fn new(input: R, cancel: &Cancellation) -> Self { Self { input, hash:HashWriter::new(cancel) } }
    pub fn finish(self) -> Fingerprint { self.hash.finish() }
}
impl<R: Read> Read for DigestReader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let count = self.input.read(bytes)?;
        self.hash.write_all(&bytes[..count])?;
        Ok(count)
    }
}
