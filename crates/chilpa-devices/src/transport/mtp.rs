//! I serialize a raw MTP session on Linux/macOS and the library's WPD backend on Windows.
//! Session handles never enter the profile or journal.
use super::{Entry, Transport};
use crate::{conflict, invalid, Cancellation, RelativePath, Result, BUFFER_BYTES, MAX_ASSETS};
use futures::executor::block_on;
use mtp_rs::mtp::{ByteRange, ListingItem, MtpDevice, NewObjectInfo, ObjectHandle, ObjectInfo, Storage, StorageId};
use std::io::{Read, Write};
use std::time::Duration;
use std::{cell::RefCell, collections::BTreeMap};

const FOLDER_CACHE_LIMIT: usize = 256;
#[derive(Clone, Copy)]
struct FolderRef { handle:ObjectHandle, parent:ObjectHandle, reported_parent:ObjectHandle, storage:StorageId }

#[derive(Clone, Debug)]
pub struct Device { pub location: u64, pub name: String, pub serial: Option<String> }
#[derive(Clone, Debug)]
pub struct Volume { pub id: u64, pub name: String, pub capacity: u64, pub available: u64, pub writable: bool }
pub struct Description { pub name:String,pub volumes:Vec<Volume> }

fn protocol(error: mtp_rs::Error) -> crate::Error {
    use chilpa_sync::TransportProblem as Problem;
    let problem = match &error {
        mtp_rs::Error::Disconnected | mtp_rs::Error::NoDevice | mtp_rs::Error::DeviceReset => Problem::Disconnected,
        mtp_rs::Error::Busy | mtp_rs::Error::ExclusiveAccess => Problem::Busy,
        mtp_rs::Error::PermissionDenied | mtp_rs::Error::AccessDenied => Problem::Permission,
        mtp_rs::Error::Unsupported => Problem::Unsupported,
        mtp_rs::Error::StaleHandle => Problem::StaleReference,
        mtp_rs::Error::Timeout => Problem::Timeout,
        mtp_rs::Error::InvalidData { .. } => Problem::InvalidData,
        _ => Problem::Other,
    };
    crate::Error::Transport { problem, detail:format!("MTP: {error}") }
}
fn during(operation: &str, error: crate::Error) -> crate::Error {
    match error {
        crate::Error::Transport { problem, detail } => crate::Error::Transport { problem, detail:format!("{operation}: {detail}") },
        other => other,
    }
}

pub fn devices() -> Result<Vec<Device>> {
    Ok(MtpDevice::list_devices().map_err(protocol)?.into_iter().map(|d| Device {
        location:d.location_id, name:d.display(), serial:d.serial_number,
    }).collect())
}
fn open(location: u64) -> Result<MtpDevice> {
    let mut last = None;
    // An interrupted Android session can reject the first reopen while its server settles.
    // I retry only session establishment, with quiet gaps; I never replay a device mutation.
    for pause in [0, 2, 5] {
        if pause != 0 { std::thread::sleep(Duration::from_secs(pause)); }
        match block_on(MtpDevice::builder().timeout(Duration::from_secs(30)).open_by_location(location)) {
            Ok(device) => return Ok(device),
            Err(error @ (mtp_rs::Error::PermissionDenied | mtp_rs::Error::ExclusiveAccess
                | mtp_rs::Error::Unsupported | mtp_rs::Error::NoDevice | mtp_rs::Error::AccessDenied)) => return Err(protocol(error)),
            Err(error) => last = Some(error),
        }
    }
    Err(during("opening device", protocol(last.unwrap())))
}
pub fn volumes(location: u64) -> Result<Vec<Volume>> {
    Ok(describe(location)?.volumes)
}
pub fn describe(location: u64) -> Result<Description> {
    let device = open(location)?;
    let name=device.device_info().model.clone();
    let result = block_on(device.storages()).map_err(protocol).map(|storages| storages.into_iter().map(|s| {
        let info = s.info();
        Volume { id:s.id().0, name:info.description.clone(), capacity:info.total_capacity, available:info.free_space, writable:info.is_writable }
    }).collect());
    block_on(device.close()).map_err(protocol)?;
    result.map(|volumes|Description{name,volumes})
}

pub struct Mtp {
    device: Option<MtpDevice>, storage: Storage, root: RelativePath, partial_reads: bool,
    folders: RefCell<BTreeMap<String, FolderRef>>,
}
impl Mtp {
    pub fn open(location: u64, volume: u64, root: RelativePath) -> Result<Self> {
        Self::connect(location,volume,root,true)
    }
    pub fn open_existing(location: u64, volume: u64, root: RelativePath) -> Result<Self> {
        Self::connect(location,volume,root,false)
    }
    fn connect(location: u64, volume: u64, root: RelativePath, create: bool) -> Result<Self> {
        let device = open(location)?;
        let capabilities = device.capabilities();
        let partial_reads = capabilities.supports_partial_download;
        if !capabilities.can_upload || !capabilities.can_rename || !capabilities.can_delete || !capabilities.can_create_folder {
            return Err(conflict("MTP device cannot stage and publish a recoverable sync"));
        }
        let storage = block_on(device.storage(StorageId(volume))).map_err(|e| during("reading selected storage", protocol(e)))?;
        if !storage.info().is_writable { return Err(conflict("selected MTP storage is read-only")); }
        let transport = Self { device:Some(device), storage, root, partial_reads, folders:RefCell::default() };
        let root = transport.root.clone();
        if create { transport.ensure(&root).map_err(|e| during("preparing root folder", e))?; }
        else if !transport.locate(&root)?.is_some_and(|info| info.is_folder()) { return Err(conflict("paired MTP folder is missing")); }
        Ok(transport)
    }
    pub fn close(mut self) -> Result<()> {
        if let Some(device) = self.device.take() { block_on(device.close()).map_err(protocol)?; }
        Ok(())
    }
    fn path(&self, path: &RelativePath) -> Result<RelativePath> { RelativePath::new(format!("{}/{path}", self.root)) }
    fn child(&self, parent: ObjectHandle, name: &str) -> Result<Option<ObjectInfo>> {
        // I consume every item, including errors, before deciding that a child is absent.
        let parent = (parent != ObjectHandle::ROOT).then_some(parent);
        let operation = if parent.is_none() { "listing storage root" } else { "listing folder" };
        let mut listing = block_on(self.storage.list_objects_stream(parent)).map_err(|e| during(operation, protocol(e)))?;
        if listing.total() > MAX_ASSETS { return Err(invalid("MTP folder exceeds the sync listing budget")); }
        let mut found = None;
        while let Some(item) = block_on(listing.next()) {
            let info = match item.map_err(|e| during("reading directory entry", protocol(e)))? {
                ListingItem::Object(info) => info,
                ListingItem::Skipped(skipped) => return Err(protocol(skipped.error)),
            };
            if info.filename.eq_ignore_ascii_case(name) {
                if found.is_some() || info.filename != name { return Err(conflict("ambiguous or case-colliding MTP path")); }
                found = Some(info);
            }
        }
        Ok(found)
    }
    fn locate(&self, path: &RelativePath) -> Result<Option<ObjectInfo>> {
        for pause in [0, 50, 200] {
            if pause != 0 { std::thread::sleep(Duration::from_millis(pause)); }
            match self.locate_once(path) {
                Err(crate::Error::Transport { problem:chilpa_sync::TransportProblem::StaleReference, .. }) if pause != 200 => self.folders.borrow_mut().clear(),
                result => return result,
            }
        }
        unreachable!()
    }
    fn locate_once(&self, path: &RelativePath) -> Result<Option<ObjectInfo>> {
        let mut parent = ObjectHandle::ROOT;
        let parts: Vec<_> = path.as_str().split('/').collect();
        let mut last = None;
        let mut prefix = String::new();
        for (index, part) in parts.iter().enumerate() {
            if !prefix.is_empty() { prefix.push('/'); }
            prefix.push_str(part);
            let ancestor = index + 1 < parts.len();
            let cached = ancestor.then(|| self.folders.borrow().get(&prefix).copied()).flatten();
            let info = if let Some(cached) = cached {
                let info = block_on(self.storage.get_object_info(cached.handle)).map_err(protocol)?;
                // Handles are session-local hints. I revalidate each ancestor, including root
                // parent quirks as originally reported, before resolving its children.
                if cached.parent != parent || info.parent != cached.reported_parent || info.storage_id != cached.storage
                    || info.filename != *part || !info.is_folder() {
                    self.folders.borrow_mut().clear();
                    return Err(conflict("a cached MTP folder changed its name, parent or storage"));
                }
                info
            } else {
                let Some(info) = self.child(parent, part)? else { return Ok(None) };
                if ancestor {
                    if !info.is_folder() { return Err(conflict("a file occupies an MTP parent folder")); }
                    let mut folders = self.folders.borrow_mut();
                    if folders.len() == FOLDER_CACHE_LIMIT { folders.clear(); }
                    folders.insert(prefix.clone(), FolderRef { handle:info.handle, parent, reported_parent:info.parent, storage:info.storage_id });
                }
                info
            };
            parent = info.handle;
            last = Some(info);
        }
        Ok(last)
    }
    fn ensure(&self, path: &RelativePath) -> Result<ObjectHandle> {
        let mut prefix = String::new();
        for part in path.as_str().split('/') {
            if !prefix.is_empty() { prefix.push('/'); }
            prefix.push_str(part);
            let current = RelativePath::new(prefix.clone())?;
            if let Some(info) = self.locate(&current)? {
                if !info.is_folder() { return Err(conflict("a file occupies an MTP folder")); }
            } else {
                let parent = if let Some(parent) = current.parent() {
                    self.locate(&parent)?.ok_or_else(|| conflict("MTP parent folder disappeared"))?.handle
                } else { ObjectHandle::ROOT };
                // I never retry a create from its error. A later attempt resolves the path first.
                let parent = (parent != ObjectHandle::ROOT).then_some(parent);
                block_on(self.storage.create_folder(parent, part)).map_err(|e| during("creating folder", protocol(e)))?;
            }
        }
        Ok(self.locate(path)?.ok_or_else(|| conflict("new MTP folder is not visible"))?.handle)
    }
}

#[cfg(test)]
#[path = "mtp_tests.rs"]
mod tests;
impl Drop for Mtp {
    fn drop(&mut self) {
        if let Some(device) = self.device.take() { let _ = block_on(device.close()); }
    }
}

impl Transport for Mtp {
    fn available_bytes(&mut self) -> Result<u64> {
        block_on(self.storage.refresh()).map_err(protocol)?;
        if !self.storage.info().is_writable { return Err(conflict("MTP storage became read-only")); }
        Ok(self.storage.info().free_space)
    }
    fn stat(&mut self, path: &RelativePath) -> Result<Option<Entry>> {
        Ok(self.locate(&self.path(path)?)?.map(|info| Entry { size:info.size, directory:info.is_folder() }))
    }
    fn create_dir_all(&mut self, path: &RelativePath) -> Result<()> { self.ensure(&self.path(path)?)?; Ok(()) }
    fn read(&mut self, path: &RelativePath, output: &mut dyn Write, cancel: &Cancellation) -> Result<()> {
        cancel.check()?;
        let info = self.locate(&self.path(path)?)?.ok_or_else(|| conflict("MTP file disappeared"))?;
        if !self.partial_reads {
            let mut download = block_on(self.storage.download(info.handle, ByteRange::Full)).map_err(protocol)?;
            while let Some(chunk) = block_on(download.next_chunk()) {
                if cancel.check().is_err() {
                    let _ = block_on(download.cancel(Duration::from_secs(3)));
                    return Err(crate::Error::Cancelled);
                }
                output.write_all(&chunk.map_err(protocol)?)?;
            }
            return Ok(());
        }
        // Bounded windows release the protocol transaction before I check cancellation or write.
        let mut download = block_on(self.storage.download_windowed(info.handle, ByteRange::Full, BUFFER_BYTES as u32)).map_err(protocol)?;
        while let Some(window) = block_on(download.next_window()) {
            cancel.check()?;
            output.write_all(&window.map_err(protocol)?)?;
        }
        Ok(())
    }
    fn write_new(&mut self, path: &RelativePath, input: &mut (dyn Read + Send), size: u64,
        cancel: &Cancellation, progress: &mut (dyn FnMut(u64) + Send)) -> Result<()> {
        cancel.check()?;
        let full = self.path(path)?;
        if self.locate(&full)?.is_some() { return Err(conflict("MTP upload destination already exists")); }
        let parent = self.ensure(&full.parent().ok_or_else(|| invalid("MTP upload has no parent"))?)?;
        let mut finished = false;
        let mut sent = 0;
        let data = futures::stream::iter(std::iter::from_fn(move || {
            if finished { return None; }
            if cancel.check().is_err() { finished = true; return Some(Err(std::io::Error::other("sync cancelled"))); }
            let mut buffer = vec![0; BUFFER_BYTES];
            match input.read(&mut buffer) {
                Ok(0) => { finished = true; None },
                Ok(n) => { buffer.truncate(n); sent += n as u64; progress(sent); Some(Ok(bytes::Bytes::from(buffer))) },
                Err(e) => { finished = true; Some(Err(e)) },
            }
        }));
        block_on(self.storage.upload(Some(parent), NewObjectInfo::file(full.filename(), size), data)).map_err(|error| protocol(error.source))?;
        cancel.check()?;
        Ok(())
    }
    fn rename_new(&mut self, from: &RelativePath, to: &RelativePath) -> Result<()> {
        if from.parent() != to.parent() { return Err(invalid("MTP publication needs staging in the same folder")); }
        let (from, to) = (self.path(from)?, self.path(to)?);
        if self.locate(&to)?.is_some() { return Err(conflict("MTP rename destination already exists")); }
        let source = self.locate(&from)?.ok_or_else(|| conflict("MTP rename source disappeared"))?;
        block_on(self.storage.rename(source.handle, to.filename())).map_err(protocol)
    }
    fn remove_file(&mut self, path: &RelativePath) -> Result<()> {
        let info = self.locate(&self.path(path)?)?.ok_or_else(|| conflict("MTP removal target disappeared"))?;
        if info.is_folder() { return Err(conflict("MTP sync never recursively removes folders")); }
        block_on(self.storage.delete(info.handle)).map_err(protocol)
    }
}
