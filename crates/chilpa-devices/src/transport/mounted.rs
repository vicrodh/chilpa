use super::{native_fs, Entry, Transport};
use crate::{conflict, Cancellation, RelativePath, Result, BUFFER_BYTES};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// I accept an explicitly selected volume/folder; discovery never chooses a destination for me.
pub struct Mounted { root: PathBuf, anchor: File, _lease: File }

impl Mounted {
    pub fn open(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        if !root.is_dir() { return Err(crate::invalid("the selected sync root is not a directory")); }
        let anchor = crate::volumes::directory_handle(&root)?;
        let metadata = root.join(".chilpa-sync");
        match std::fs::symlink_metadata(&metadata) {
            Ok(m) if m.file_type().is_symlink() || !m.is_dir() => return Err(conflict("sync metadata is not a real directory")),
            Ok(_) => (), Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir(&metadata)?,
            Err(e) => return Err(e.into()),
        }
        let lease_path = metadata.join("lock");
        if std::fs::symlink_metadata(&lease_path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(conflict("sync lock is a symbolic link"));
        }
        let lease = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lease_path)?;
        lease.try_lock().map_err(|_| conflict("another sync session owns this storage"))?;
        crate::volumes::unchanged(&root, &anchor)?;
        Ok(Self { root, anchor, _lease: lease })
    }
    fn path(&self, relative: &RelativePath) -> Result<PathBuf> {
        crate::volumes::unchanged(&self.root, &self.anchor)?;
        let mut path = self.root.clone();
        for part in relative.as_str().split('/') {
            path.push(part);
            match std::fs::symlink_metadata(&path) {
                Ok(m) if m.file_type().is_symlink() => return Err(conflict(format!("a symbolic link occupies {relative}"))),
                Ok(_) => (), Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(path)
    }
}

impl Transport for Mounted {
    fn available_bytes(&mut self) -> Result<u64> { crate::volumes::unchanged(&self.root, &self.anchor)?; native_fs::available(&self.root) }
    fn stat(&mut self, path: &RelativePath) -> Result<Option<Entry>> {
        match std::fs::symlink_metadata(self.path(path)?) {
            Ok(m) if m.is_file() || m.is_dir() => Ok(Some(Entry { size: m.len(), directory: m.is_dir() })),
            Ok(_) => Err(conflict(format!("unsupported device entry at {path}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None), Err(e) => Err(e.into()),
        }
    }
    fn create_dir_all(&mut self, path: &RelativePath) -> Result<()> {
        let mut prefix = String::new();
        for part in path.as_str().split('/') {
            if !prefix.is_empty() { prefix.push('/'); }
            prefix.push_str(part);
            let path = self.path(&RelativePath::new(prefix.clone())?)?;
            match std::fs::create_dir(&path) {
                Ok(()) => native_fs::sync_parent(&path)?,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => (),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    fn read(&mut self, path: &RelativePath, output: &mut dyn Write, cancel: &Cancellation) -> Result<()> {
        let mut input = File::open(self.path(path)?)?;
        let mut buffer = vec![0; BUFFER_BYTES];
        loop {
            cancel.check()?;
            let n = input.read(&mut buffer)?;
            if n == 0 { break; }
            output.write_all(&buffer[..n])?;
        }
        Ok(())
    }
    fn write_new(&mut self, path: &RelativePath, input: &mut (dyn Read + Send), size: u64,
        cancel: &Cancellation, progress: &mut (dyn FnMut(u64) + Send)) -> Result<()> {
        let path = self.path(path)?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(&path)?;
        let mut buffer = vec![0; BUFFER_BYTES];
        let mut written = 0;
        loop {
            cancel.check()?;
            let n = input.read(&mut buffer)?;
            if n == 0 { break; }
            written += n as u64;
            if written > size { return Err(conflict("source grew during transfer")); }
            file.write_all(&buffer[..n])?;
            progress(written);
        }
        if written != size { return Err(conflict("source shrank during transfer")); }
        file.sync_all()?;
        native_fs::sync_parent(&path)
    }
    fn rename_new(&mut self, from: &RelativePath, to: &RelativePath) -> Result<()> {
        let (from, to) = (self.path(from)?, self.path(to)?);
        native_fs::rename_new(&from, &to)?;
        native_fs::sync_parent(&to)?;
        native_fs::sync_parent(&from)
    }
    fn remove_file(&mut self, path: &RelativePath) -> Result<()> {
        let path = self.path(path)?;
        std::fs::remove_file(&path)?;
        native_fs::sync_parent(&path)
    }
}
