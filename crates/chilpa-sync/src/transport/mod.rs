//! The device-independent contract used by planning and journal reconciliation.

use crate::{Cancellation, Fingerprint, Result, RelativePath};
use std::io::{Read, Write};

#[derive(Clone, Copy, Debug)]
pub struct Entry { pub size: u64, pub directory: bool }

/// I require an exclusive session for this root until the transport is dropped.
/// Failed lookups and incomplete listings are errors, never an empty collection.
pub trait Transport {
    /// I let metadata enumeration observe the job cancellation between protocol calls.
    fn set_cancellation(&mut self, _cancel: &Cancellation) {}
    /// I allow a bounded metadata snapshot only during a read-only preview.
    fn begin_review(&mut self) {}
    /// I discard every preview hint before execution can inspect or mutate the device.
    fn end_review(&mut self) {}
    fn available_bytes(&mut self) -> Result<u64>;
    fn stat(&mut self, path: &RelativePath) -> Result<Option<Entry>>;
    fn create_dir_all(&mut self, path: &RelativePath) -> Result<()>;
    fn read(&mut self, path: &RelativePath, output: &mut dyn Write, cancel: &Cancellation) -> Result<()>;
    fn write_new(&mut self, path: &RelativePath, input: &mut (dyn Read + Send), size: u64,
        cancel: &Cancellation, progress: &mut (dyn FnMut(u64) + Send)) -> Result<()>;
    fn rename_new(&mut self, from: &RelativePath, to: &RelativePath) -> Result<()>;
    fn remove_file(&mut self, path: &RelativePath) -> Result<()>;
    /// I let a device refresh its own media index after a durable publication or removal.
    fn published(&mut self, _path: &RelativePath, _cancel: &Cancellation) -> Result<()> { Ok(()) }

    fn fingerprint(&mut self, path: &RelativePath, cancel: &Cancellation) -> Result<Option<Fingerprint>> {
        self.fingerprint_with_progress(path, cancel, &mut |_| ())
    }
    fn fingerprint_with_progress(&mut self, path: &RelativePath, cancel: &Cancellation,
        progress: &mut (dyn FnMut(u64) + Send)) -> Result<Option<Fingerprint>> {
        let Some(info) = self.stat(path)? else { return Ok(None) };
        if info.directory { return Err(crate::conflict(format!("a directory occupies {path}"))); }
        let mut writer = crate::model::HashWriter::new(cancel);
        struct Reporting<'a> { writer:&'a mut crate::model::HashWriter, bytes:u64, progress:&'a mut (dyn FnMut(u64) + Send) }
        impl Write for Reporting<'_> {
            fn write(&mut self, bytes:&[u8]) -> std::io::Result<usize> {
                let count = self.writer.write(bytes)?;
                self.bytes += count as u64;
                (self.progress)(self.bytes);
                Ok(count)
            }
            fn flush(&mut self) -> std::io::Result<()> { self.writer.flush() }
        }
        progress(0);
        self.read(path, &mut Reporting { writer:&mut writer, bytes:0, progress }, cancel)?;
        cancel.check()?;
        let digest = writer.finish();
        if digest.size != info.size { return Err(crate::conflict(format!("device file changed while reading {path}"))); }
        Ok(Some(digest))
    }
}
