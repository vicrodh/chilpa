//! I borrow queue payloads when a view or session save only needs to read them.
use super::*;
use std::borrow::Cow;

#[derive(Serialize)]
pub(super) struct BufferView<'a> {
    pub id: u64,
    pub name: &'a str,
    pub renamed: bool,
    pub epoch: u64,
    pub paths: &'a [String],
    pub origins: &'a [Option<crate::queue_origin::QueueOrigin>],
    pub removed: &'a [bool],
    pub placement: &'a [QueuePlacement],
    pub groups: &'a [QueueGroup],
    pub order: Cow<'a, [usize]>,
    pub cursor: usize,
    pub seconds: f64,
    pub shuffle: bool,
    pub repeat: u8,
    pub stop_after: Option<usize>,
}
impl<'a> From<&'a Buffer> for BufferView<'a> {
    fn from(b: &'a Buffer) -> Self {
        Self { id: b.id, name: &b.name, renamed: b.renamed, epoch: b.epoch,
            paths: &b.paths, origins: &b.origins, removed: &b.removed,
            placement: &b.placement, groups: &b.groups, order: Cow::Borrowed(&b.order),
            cursor: b.cursor, seconds: b.seconds, shuffle: b.shuffle,
            repeat: b.repeat, stop_after: b.stop_after }
    }
}
impl BufferView<'_> {
    pub fn live(&self, index: usize) -> bool {
        index < self.paths.len() && !self.removed.get(index).copied().unwrap_or(false)
    }
}

#[derive(Serialize)]
pub(super) struct WorkspaceView<'a> {
    version: u8,
    buffers: Vec<BufferView<'a>>,
    active: u64,
    selected: u64,
    next: u64,
}
impl Chilpa {
    pub(super) fn buffer_view(&self, id: u64) -> Option<BufferView<'_>> {
        let b = self.listen.buffers.iter().find(|b| b.id == id)?;
        let mut view = BufferView::from(b);
        if id == self.listen.active {
            view.paths = &self.queue; view.origins = &self.queue_origins;
            view.removed = &self.removed; view.placement = &self.queue_placement;
            view.groups = &self.queue_groups; view.order = Cow::Owned(self.full_queue_order());
            view.cursor = self.now.index; view.seconds = self.now.seconds.max(0.0);
            view.shuffle = self.shuffle; view.repeat = self.repeat as u8;
            view.stop_after = self.stop_after;
        }
        Some(view)
    }
    pub(super) fn workspace_view<'a>(&'a self, fallback: &'a Buffer) -> WorkspaceView<'a> {
        let media = self.media.as_ref().map(|m| m.buffer);
        let mut buffers: Vec<_> = self.listen.buffers.iter().filter(|b| Some(b.id) != media)
            .filter_map(|b| self.buffer_view(b.id)).collect();
        let mut next = self.listen.next;
        if media.is_some() && buffers.is_empty() { buffers.push(fallback.into()); next += 1; }
        WorkspaceView { version: self.listen.version,
            active: if Some(self.listen.active) == media { 0 } else { self.listen.active },
            selected: if Some(self.listen.selected) == media { buffers[0].id } else { self.listen.selected },
            buffers, next }
    }
}

/// I feed the same JSON bytes and string terminator to the revision hash,
/// without allocating a serialized copy of every path and origin.
struct HashWriter<'a, H>(pub &'a mut H);
impl<H: std::hash::Hasher> std::io::Write for HashWriter<'_, H> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.write(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

pub(super) fn hash_buffer(b: &BufferView<'_>, hash: &mut impl std::hash::Hasher) {
    use std::io::Write;
    let mut writer = std::io::BufWriter::new(HashWriter(hash));
    let _ = serde_json::to_writer(&mut writer, b);
    let _ = writer.flush();
    writer.get_mut().0.write_u8(0xff);
}
