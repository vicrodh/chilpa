//! Compressed-data packet passed between demuxer → decoder and encoder → muxer.

use std::ops::{Bound, Deref, Range, RangeBounds};
use std::sync::Arc;

use crate::time::TimeBase;

/// Metadata flags on a packet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PacketFlags {
    /// Packet is (or starts) a keyframe / random-access point.
    pub keyframe: bool,
    /// Packet holds codec-level headers rather than media data.
    pub header: bool,
    /// Packet's data may be corrupt but decode should still be attempted.
    pub corrupt: bool,
    /// Packet should be discarded (e.g., decoder delay padding).
    pub discard: bool,
    /// Packet is the last in its source container's natural framing unit
    /// (Ogg page, MP4 chunk, MKV cluster, …). Container muxers may use this
    /// signal to recreate similar boundaries in their output. Decoders
    /// should ignore it.
    pub unit_boundary: bool,
}

/// A chunk of compressed (encoded) data belonging to one stream.
#[derive(Clone, Debug)]
pub struct Packet {
    /// Stream index this packet belongs to.
    pub stream_index: u32,
    /// Time base in which `pts` and `dts` are expressed.
    pub time_base: TimeBase,
    /// Presentation timestamp (display order). `None` if unknown.
    pub pts: Option<i64>,
    /// Decode timestamp (decode order). Often equal to `pts` for intra-only codecs.
    pub dts: Option<i64>,
    /// Packet duration in `time_base` units, or `None` if unknown.
    pub duration: Option<i64>,
    /// Flags describing this packet.
    pub flags: PacketFlags,
    /// Compressed payload.
    pub data: Vec<u8>,
}

impl Packet {
    /// Construct a packet with the given payload and no timing
    /// information (all timestamps `None`, default flags).
    pub fn new(stream_index: u32, time_base: TimeBase, data: Vec<u8>) -> Self {
        Self {
            stream_index,
            time_base,
            pts: None,
            dts: None,
            duration: None,
            flags: PacketFlags::default(),
            data,
        }
    }

    /// Builder: set the presentation timestamp (in `time_base` units).
    pub fn with_pts(mut self, pts: i64) -> Self {
        self.pts = Some(pts);
        self
    }

    /// Builder: set the decode timestamp (in `time_base` units).
    pub fn with_dts(mut self, dts: i64) -> Self {
        self.dts = Some(dts);
        self
    }

    /// Builder: set the packet duration (in `time_base` units).
    pub fn with_duration(mut self, d: i64) -> Self {
        self.duration = Some(d);
        self
    }

    /// Builder: mark (or unmark) the packet as a keyframe /
    /// random-access point.
    pub fn with_keyframe(mut self, kf: bool) -> Self {
        self.flags.keyframe = kf;
        self
    }

    /// Mark this packet as carrying codec-level headers rather than
    /// media data (extradata, parameter sets, codec-private blobs).
    pub fn with_header(mut self, header: bool) -> Self {
        self.flags.header = header;
        self
    }

    /// Mark this packet's payload as possibly corrupt. Decoders should
    /// still attempt to decode it but may produce best-effort output.
    pub fn with_corrupt(mut self, corrupt: bool) -> Self {
        self.flags.corrupt = corrupt;
        self
    }

    /// Mark this packet for downstream discard (e.g. decoder delay
    /// padding, encoder priming samples, ASS dialogue tags shipped only
    /// for muxer round-trip).
    pub fn with_discard(mut self, discard: bool) -> Self {
        self.flags.discard = discard;
        self
    }

    /// Mark this packet as the last entry inside its source container's
    /// natural framing unit (Ogg page, MP4 chunk, MKV cluster). Decoders
    /// ignore the flag; muxers may use it to recreate similar
    /// boundaries in their output.
    pub fn with_unit_boundary(mut self, boundary: bool) -> Self {
        self.flags.unit_boundary = boundary;
        self
    }

    /// Replace this packet's full flag set in one call. Useful for
    /// demuxers that compute flags up front and want a single setter
    /// rather than four chained builder calls.
    pub fn with_flags(mut self, flags: PacketFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Override the packet's stream index. Builder-style chainable
    /// counterpart to the public field, for cases where the demuxer
    /// builds packets with a placeholder stream index and remaps them
    /// to the final index downstream.
    pub fn with_stream_index(mut self, stream_index: u32) -> Self {
        self.stream_index = stream_index;
        self
    }

    /// Override the packet's time base. Builder-style chainable
    /// counterpart to the public field, for cases where the time base
    /// isn't known at construction time (e.g. a remuxer rescaling all
    /// packets onto a unified output base).
    pub fn with_time_base(mut self, time_base: TimeBase) -> Self {
        self.time_base = time_base;
        self
    }

    /// Compute the packet's end PTS (`pts + duration`) when both are
    /// known. Returns `None` if either is missing, or if the sum would
    /// overflow `i64`. Useful for muxers that need to derive a per-
    /// packet end timestamp without recomputing it at every call site.
    pub fn end_pts(&self) -> Option<i64> {
        self.pts
            .zip(self.duration)
            .and_then(|(p, d)| p.checked_add(d))
    }

    /// Convenience accessor: `true` when [`PacketFlags::keyframe`] is
    /// set. Mirrors the builder pair `with_keyframe(true)`.
    pub fn is_keyframe(&self) -> bool {
        self.flags.keyframe
    }

    /// Convenience accessor: `true` when [`PacketFlags::header`] is set
    /// (the packet carries codec-level headers rather than media data).
    pub fn is_header(&self) -> bool {
        self.flags.header
    }

    /// Convenience accessor: `true` when [`PacketFlags::discard`] is
    /// set (downstream consumers should drop the packet).
    pub fn is_discard(&self) -> bool {
        self.flags.discard
    }

    /// Convert into a [`SharedPacket`] without copying: the payload
    /// `Vec` is moved behind the shared buffer.
    pub fn into_shared(self) -> SharedPacket {
        SharedPacket::from(self)
    }
}

/// Backing storage of a [`SharedBytes`].
#[derive(Clone)]
enum SharedOwner {
    Vec(Arc<Vec<u8>>),
    Slice(Arc<[u8]>),
    Other(Arc<dyn AsRef<[u8]> + Send + Sync>),
}

impl SharedOwner {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Vec(v) => v.as_slice(),
            Self::Slice(s) => s,
            Self::Other(o) => (**o).as_ref(),
        }
    }
}

/// An immutable, cheaply clonable view of a range of a shared byte
/// buffer.
///
/// Cloning or [slicing](Self::slice) never copies the bytes — every view
/// holds a reference count on the same buffer, which is freed when the
/// last view goes away. It dereferences to `&[u8]`, so code that reads
/// bytes takes it wherever a slice is accepted.
///
/// The typical producer is a demuxer that holds a whole file (or a large
/// region of one) in memory and hands out many payloads from it: it
/// wraps the buffer once and gives each [`SharedPacket`] a view instead
/// of a copy.
///
/// ```
/// use oxideav_core::packet::SharedBytes;
///
/// let file = SharedBytes::from(vec![0u8, 1, 2, 3, 4, 5, 6, 7]);
/// let item = file.slice(2..6); // no copy
/// assert_eq!(&item[..], &[2, 3, 4, 5]);
/// assert!(item.shares_buffer_with(&file));
/// assert_eq!(item.range(), 2..6);
/// ```
#[derive(Clone)]
pub struct SharedBytes {
    owner: SharedOwner,
    start: usize,
    end: usize,
}

impl SharedBytes {
    /// An empty view.
    pub fn new() -> Self {
        Self::from(Vec::new())
    }

    fn whole(owner: SharedOwner) -> Self {
        let end = owner.bytes().len();
        Self {
            owner,
            start: 0,
            end,
        }
    }

    /// Share any owner of bytes (a memory map, a pooled buffer, …). The
    /// view covers everything `owner.as_ref()` answers at this moment;
    /// the owner must keep answering the same bytes for its lifetime (a
    /// later, shorter answer reads as a truncated or empty view, never
    /// as an out-of-bounds access).
    pub fn from_owner<T>(owner: T) -> Self
    where
        T: AsRef<[u8]> + Send + Sync + 'static,
    {
        Self::whole(SharedOwner::Other(Arc::new(owner)))
    }

    /// Copy `bytes` into a new shared buffer.
    pub fn copy_from_slice(bytes: &[u8]) -> Self {
        Self::whole(SharedOwner::Slice(Arc::from(bytes)))
    }

    /// The viewed bytes.
    pub fn as_slice(&self) -> &[u8] {
        self.owner.bytes().get(self.start..self.end).unwrap_or(&[])
    }

    /// Number of viewed bytes.
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// True when the view is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A sub-view, `range` being relative to this view. No bytes are
    /// copied.
    ///
    /// # Panics
    ///
    /// When `range` is decreasing or reaches past the end of this view
    /// — the rule of slice indexing. [`try_slice`](Self::try_slice) is
    /// the non-panicking form.
    pub fn slice(&self, range: impl RangeBounds<usize>) -> SharedBytes {
        let len = self.len();
        match self.try_slice(range) {
            Some(view) => view,
            None => panic!("SharedBytes::slice: range out of bounds of a {len}-byte view"),
        }
    }

    /// A sub-view, `range` being relative to this view, or `None` when
    /// `range` is decreasing or reaches past the end of this view.
    pub fn try_slice(&self, range: impl RangeBounds<usize>) -> Option<SharedBytes> {
        let len = self.len();
        let start = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n.checked_add(1)?,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&n) => n.checked_add(1)?,
            Bound::Excluded(&n) => n,
            Bound::Unbounded => len,
        };
        if start > end || end > len {
            return None;
        }
        Some(SharedBytes {
            owner: self.owner.clone(),
            start: self.start + start,
            end: self.start + end,
        })
    }

    /// Where this view sits inside its buffer: `buffer()[range()]` are
    /// the viewed bytes.
    pub fn range(&self) -> Range<usize> {
        self.start..self.start + self.len()
    }

    /// A view of the whole underlying buffer, of which this view is the
    /// [`range`](Self::range). Lets a consumer that was handed a slice
    /// reach the surrounding bytes when the format needs them, still
    /// without a copy.
    pub fn buffer(&self) -> SharedBytes {
        Self::whole(self.owner.clone())
    }

    /// True when both views are backed by the same buffer (whatever
    /// their ranges).
    pub fn shares_buffer_with(&self, other: &SharedBytes) -> bool {
        match (&self.owner, &other.owner) {
            (SharedOwner::Vec(a), SharedOwner::Vec(b)) => Arc::ptr_eq(a, b),
            (SharedOwner::Slice(a), SharedOwner::Slice(b)) => Arc::ptr_eq(a, b),
            (SharedOwner::Other(a), SharedOwner::Other(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// The viewed bytes as an owned `Vec`. Free of any copy when the
    /// buffer is a `Vec` nobody else holds and the view covers all of it
    /// (the case of a payload that came from an ordinary [`Packet`]);
    /// a copy of the viewed range otherwise.
    pub fn into_vec(self) -> Vec<u8> {
        match self.owner {
            SharedOwner::Vec(v) if self.start == 0 && self.end == v.len() => {
                Arc::try_unwrap(v).unwrap_or_else(|shared| shared.as_slice().to_vec())
            }
            owner => owner
                .bytes()
                .get(self.start..self.end)
                .unwrap_or(&[])
                .to_vec(),
        }
    }
}

impl Default for SharedBytes {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for SharedBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsRef<[u8]> for SharedBytes {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

/// Moves the `Vec` behind the shared buffer — no copy.
impl From<Vec<u8>> for SharedBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self::whole(SharedOwner::Vec(Arc::new(bytes)))
    }
}

/// Shares the existing allocation — no copy.
impl From<Arc<Vec<u8>>> for SharedBytes {
    fn from(bytes: Arc<Vec<u8>>) -> Self {
        Self::whole(SharedOwner::Vec(bytes))
    }
}

/// Shares the existing allocation — no copy.
impl From<Arc<[u8]>> for SharedBytes {
    fn from(bytes: Arc<[u8]>) -> Self {
        Self::whole(SharedOwner::Slice(bytes))
    }
}

/// Compares the viewed bytes.
impl PartialEq for SharedBytes {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for SharedBytes {}

impl PartialEq<[u8]> for SharedBytes {
    fn eq(&self, other: &[u8]) -> bool {
        self.as_slice() == other
    }
}

impl PartialEq<Vec<u8>> for SharedBytes {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl std::fmt::Debug for SharedBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bytes = self.as_slice();
        let head = &bytes[..bytes.len().min(16)];
        f.debug_struct("SharedBytes")
            .field("len", &bytes.len())
            .field("range", &self.range())
            .field("head", &head)
            .finish()
    }
}

/// A [`Packet`] whose payload is a [`SharedBytes`] view instead of an
/// owned `Vec<u8>`: cloning it, or producing many of them from one
/// buffer, copies no payload bytes.
///
/// It carries the same fields with the same meaning as [`Packet`], and
/// the two convert into each other ([`Packet::into_shared`] /
/// `From<Packet>` never copy; [`into_packet`](Self::into_packet) copies
/// only when the payload is a slice of, or shares, a larger buffer).
///
/// Producers and consumers opt in through the provided trait methods
/// [`Demuxer::next_shared_packet`](crate::Demuxer::next_shared_packet),
/// [`PacketSource::next_shared_packet`](crate::PacketSource::next_shared_packet),
/// [`Decoder::send_shared_packet`](crate::Decoder::send_shared_packet)
/// and [`Muxer::write_shared_packet`](crate::Muxer::write_shared_packet).
/// Each has a default that bridges to the `Packet` method, so a shared
/// packet flows through components that know nothing about it; the copy
/// disappears wherever both ends of a hop override.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct SharedPacket {
    /// Stream index this packet belongs to.
    pub stream_index: u32,
    /// Time base in which `pts` and `dts` are expressed.
    pub time_base: TimeBase,
    /// Presentation timestamp (display order). `None` if unknown.
    pub pts: Option<i64>,
    /// Decode timestamp (decode order).
    pub dts: Option<i64>,
    /// Packet duration in `time_base` units, or `None` if unknown.
    pub duration: Option<i64>,
    /// Flags describing this packet.
    pub flags: PacketFlags,
    /// Compressed payload.
    pub data: SharedBytes,
}

impl SharedPacket {
    /// Construct a packet with the given payload and no timing
    /// information (all timestamps `None`, default flags). `data` is
    /// anything that converts into a [`SharedBytes`]: a view, a
    /// `Vec<u8>`, an `Arc<[u8]>`, an `Arc<Vec<u8>>`.
    pub fn new(stream_index: u32, time_base: TimeBase, data: impl Into<SharedBytes>) -> Self {
        Self {
            stream_index,
            time_base,
            pts: None,
            dts: None,
            duration: None,
            flags: PacketFlags::default(),
            data: data.into(),
        }
    }

    /// Builder: set the presentation timestamp (in `time_base` units).
    pub fn with_pts(mut self, pts: i64) -> Self {
        self.pts = Some(pts);
        self
    }

    /// Builder: set the decode timestamp (in `time_base` units).
    pub fn with_dts(mut self, dts: i64) -> Self {
        self.dts = Some(dts);
        self
    }

    /// Builder: set the packet duration (in `time_base` units).
    pub fn with_duration(mut self, d: i64) -> Self {
        self.duration = Some(d);
        self
    }

    /// Builder: mark (or unmark) the packet as a keyframe /
    /// random-access point.
    pub fn with_keyframe(mut self, kf: bool) -> Self {
        self.flags.keyframe = kf;
        self
    }

    /// Builder: replace the packet's full flag set.
    pub fn with_flags(mut self, flags: PacketFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Builder: override the packet's stream index.
    pub fn with_stream_index(mut self, stream_index: u32) -> Self {
        self.stream_index = stream_index;
        self
    }

    /// Builder: override the packet's time base.
    pub fn with_time_base(mut self, time_base: TimeBase) -> Self {
        self.time_base = time_base;
        self
    }

    /// Builder: replace the payload.
    pub fn with_data(mut self, data: impl Into<SharedBytes>) -> Self {
        self.data = data.into();
        self
    }

    /// `pts + duration` when both are known and the sum does not
    /// overflow — same rule as [`Packet::end_pts`].
    pub fn end_pts(&self) -> Option<i64> {
        self.pts
            .zip(self.duration)
            .and_then(|(p, d)| p.checked_add(d))
    }

    /// `true` when [`PacketFlags::keyframe`] is set.
    pub fn is_keyframe(&self) -> bool {
        self.flags.keyframe
    }

    /// `true` when [`PacketFlags::header`] is set.
    pub fn is_header(&self) -> bool {
        self.flags.header
    }

    /// `true` when [`PacketFlags::discard`] is set.
    pub fn is_discard(&self) -> bool {
        self.flags.discard
    }

    /// Convert into an owned-payload [`Packet`]. The payload is moved
    /// when this packet is the only holder of a whole `Vec` buffer (see
    /// [`SharedBytes::into_vec`]) and copied otherwise.
    pub fn into_packet(self) -> Packet {
        Packet {
            stream_index: self.stream_index,
            time_base: self.time_base,
            pts: self.pts,
            dts: self.dts,
            duration: self.duration,
            flags: self.flags,
            data: self.data.into_vec(),
        }
    }

    /// An owned-payload [`Packet`] holding a copy of the viewed bytes.
    pub fn to_packet(&self) -> Packet {
        Packet {
            stream_index: self.stream_index,
            time_base: self.time_base,
            pts: self.pts,
            dts: self.dts,
            duration: self.duration,
            flags: self.flags,
            data: self.data.as_slice().to_vec(),
        }
    }
}

/// Moves the payload behind a shared buffer — no copy.
impl From<Packet> for SharedPacket {
    fn from(p: Packet) -> Self {
        Self {
            stream_index: p.stream_index,
            time_base: p.time_base,
            pts: p.pts,
            dts: p.dts,
            duration: p.duration,
            flags: p.flags,
            data: SharedBytes::from(p.data),
        }
    }
}

/// Same as [`SharedPacket::into_packet`].
impl From<SharedPacket> for Packet {
    fn from(p: SharedPacket) -> Self {
        p.into_packet()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tb() -> TimeBase {
        TimeBase::new(1, 1000)
    }

    #[test]
    fn new_packet_has_default_flags_and_no_timing() {
        let p = Packet::new(3, tb(), vec![1, 2, 3]);
        assert_eq!(p.stream_index, 3);
        assert_eq!(p.time_base, tb());
        assert!(p.pts.is_none());
        assert!(p.dts.is_none());
        assert!(p.duration.is_none());
        assert_eq!(p.flags, PacketFlags::default());
        assert_eq!(p.data, vec![1, 2, 3]);
        // All accessor convenience helpers default to false.
        assert!(!p.is_keyframe());
        assert!(!p.is_header());
        assert!(!p.is_discard());
    }

    #[test]
    fn builder_chain_sets_every_flag_field() {
        let p = Packet::new(0, tb(), vec![])
            .with_keyframe(true)
            .with_header(true)
            .with_corrupt(true)
            .with_discard(true)
            .with_unit_boundary(true);
        assert!(p.flags.keyframe);
        assert!(p.flags.header);
        assert!(p.flags.corrupt);
        assert!(p.flags.discard);
        assert!(p.flags.unit_boundary);
        assert!(p.is_keyframe());
        assert!(p.is_header());
        assert!(p.is_discard());
    }

    #[test]
    fn with_flags_replaces_full_flag_set() {
        let flags = PacketFlags {
            keyframe: true,
            header: false,
            corrupt: true,
            discard: false,
            unit_boundary: true,
        };
        let p = Packet::new(0, tb(), vec![]).with_flags(flags);
        assert_eq!(p.flags, flags);
        // A second with_flags wipes the prior set rather than OR-ing.
        let cleared = p.with_flags(PacketFlags::default());
        assert_eq!(cleared.flags, PacketFlags::default());
    }

    #[test]
    fn with_stream_index_and_time_base_override() {
        let original = TimeBase::new(1, 1);
        let replacement = TimeBase::new(1, 90_000);
        let p = Packet::new(0, original, vec![])
            .with_stream_index(7)
            .with_time_base(replacement);
        assert_eq!(p.stream_index, 7);
        assert_eq!(p.time_base, replacement);
    }

    #[test]
    fn end_pts_requires_both_pts_and_duration() {
        // Neither set.
        assert_eq!(Packet::new(0, tb(), vec![]).end_pts(), None);
        // pts only.
        assert_eq!(Packet::new(0, tb(), vec![]).with_pts(100).end_pts(), None);
        // duration only.
        assert_eq!(
            Packet::new(0, tb(), vec![]).with_duration(50).end_pts(),
            None
        );
        // Both: returns pts + duration.
        assert_eq!(
            Packet::new(0, tb(), vec![])
                .with_pts(100)
                .with_duration(50)
                .end_pts(),
            Some(150)
        );
    }

    #[test]
    fn end_pts_saturates_on_overflow() {
        // pts + duration would overflow i64::MAX; checked_add returns
        // None so end_pts surfaces that instead of wrapping.
        let p = Packet::new(0, tb(), vec![])
            .with_pts(i64::MAX - 1)
            .with_duration(10);
        assert_eq!(p.end_pts(), None);
    }

    #[test]
    fn shared_bytes_views_share_one_buffer() {
        let file = SharedBytes::from((0u8..32).collect::<Vec<u8>>());
        assert_eq!(file.len(), 32);
        assert_eq!(file.range(), 0..32);
        let a = file.slice(4..12);
        let b = a.slice(2..=3);
        let c = a.slice(..);
        assert_eq!(&a[..], &[4, 5, 6, 7, 8, 9, 10, 11]);
        assert_eq!(&b[..], &[6, 7]);
        assert_eq!(b.range(), 6..8);
        assert_eq!(c, a);
        assert!(a.shares_buffer_with(&file) && b.shares_buffer_with(&file));
        // Same storage, not a copy.
        assert_eq!(a.as_ptr(), file[4..].as_ptr());
        // The surrounding buffer stays reachable from a slice.
        assert_eq!(b.buffer(), file);
        assert_eq!(b.buffer().range(), 0..32);
        // Empty slices at either end are fine; out-of-range is refused.
        assert!(a.slice(8..).is_empty());
        assert!(a.try_slice(0..9).is_none());
        assert!(a.try_slice(..=usize::MAX).is_none());
        #[allow(clippy::reversed_empty_ranges)]
        let reversed = a.try_slice(5..4);
        assert!(reversed.is_none());
        // The buffer outlives the first handle.
        drop(file);
        assert_eq!(b, vec![6u8, 7]);
        assert_eq!(b, [6u8, 7][..]);
        assert!(!b.shares_buffer_with(&SharedBytes::from(vec![6u8, 7])));
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn shared_bytes_slice_panics_past_the_end() {
        let _ = SharedBytes::from(vec![1u8, 2, 3]).slice(2..5);
    }

    #[test]
    fn shared_bytes_constructors_and_into_vec() {
        assert!(SharedBytes::new().is_empty());
        assert!(SharedBytes::default().into_vec().is_empty());

        // A uniquely held whole Vec comes back without a copy.
        let v = vec![9u8; 64];
        let ptr = v.as_ptr();
        let unwrapped = SharedBytes::from(v).into_vec();
        assert_eq!(unwrapped.as_ptr(), ptr);

        // A shared or sliced one is copied.
        let whole = SharedBytes::from(vec![1u8, 2, 3, 4]);
        let keep = whole.clone();
        assert_eq!(whole.into_vec(), vec![1, 2, 3, 4]);
        assert_eq!(keep.slice(1..3).into_vec(), vec![2, 3]);

        let arc: Arc<[u8]> = Arc::from(&[5u8, 6, 7][..]);
        let from_arc = SharedBytes::from(arc.clone());
        assert_eq!(from_arc.as_ptr(), arc.as_ptr());
        assert_eq!(from_arc.slice(1..).into_vec(), vec![6, 7]);

        let arc_vec = Arc::new(vec![8u8, 9]);
        assert_eq!(
            SharedBytes::from(arc_vec.clone()).as_ptr(),
            arc_vec.as_ptr()
        );

        struct Owner([u8; 4]);
        impl AsRef<[u8]> for Owner {
            fn as_ref(&self) -> &[u8] {
                &self.0
            }
        }
        let other = SharedBytes::from_owner(Owner([1, 2, 3, 4]));
        assert_eq!(&other.slice(1..3)[..], &[2, 3]);
        assert!(other.slice(1..3).shares_buffer_with(&other));

        assert_eq!(SharedBytes::copy_from_slice(b"abc"), b"abc"[..]);
        let dbg = format!("{:?}", SharedBytes::from(vec![0u8; 100]).slice(10..90));
        assert!(dbg.contains("len: 80") && dbg.contains("10..90"), "{dbg}");
    }

    #[test]
    fn shared_packet_round_trips_with_packet() {
        let p = Packet::new(2, tb(), vec![1, 2, 3])
            .with_pts(10)
            .with_dts(9)
            .with_duration(5)
            .with_keyframe(true);
        let ptr = p.data.as_ptr();
        let sp = p.clone().into_shared();
        assert_eq!(sp.stream_index, 2);
        assert_eq!(sp.time_base, tb());
        assert_eq!((sp.pts, sp.dts, sp.duration), (Some(10), Some(9), Some(5)));
        assert_eq!(sp.end_pts(), Some(15));
        assert!(sp.is_keyframe() && !sp.is_header() && !sp.is_discard());
        assert_eq!(sp.data, p.data);

        let copy = sp.to_packet();
        assert_eq!(copy.data, p.data);
        assert_eq!(copy.flags, p.flags);

        // Packet → SharedPacket → Packet moves the payload both ways.
        let back: Packet = SharedPacket::from(p).into();
        assert_eq!(back.data.as_ptr(), ptr);
        assert_eq!(back.pts, Some(10));
    }

    #[test]
    fn shared_packets_from_one_buffer_do_not_copy() {
        let file = SharedBytes::from(vec![7u8; 1 << 16]);
        let packets: Vec<SharedPacket> = (0..64usize)
            .map(|i| {
                SharedPacket::new(0, tb(), file.slice(i * 1024..(i + 1) * 1024))
                    .with_pts(i as i64)
                    .with_flags(PacketFlags::default())
                    .with_stream_index(1)
                    .with_time_base(tb())
            })
            .collect();
        for (i, p) in packets.iter().enumerate() {
            assert_eq!(p.data.as_ptr(), file[i * 1024..].as_ptr());
            assert!(p.data.shares_buffer_with(&file));
            assert_eq!(p.clone().data.as_ptr(), p.data.as_ptr());
        }
        let replaced = packets[0].clone().with_data(vec![1u8]);
        assert_eq!(replaced.data, vec![1u8]);
    }

    #[test]
    fn end_pts_handles_negative_pts() {
        // Negative pts is legal (B-frames pre-roll); ensure the sum
        // still works through zero.
        let p = Packet::new(0, tb(), vec![]).with_pts(-25).with_duration(40);
        assert_eq!(p.end_pts(), Some(15));
    }
}
