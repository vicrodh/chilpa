//! Container traits (demuxer + muxer) and a registry.
//!
//! This module defines the abstract [`Demuxer`] / [`Muxer`] traits that
//! every container implementation (oxideav-mp4, oxideav-mkv,
//! oxideav-flac, oxideav-ogg, …) fulfils, plus a
//! [`ContainerRegistry`] that consumers of the framework use to pick a
//! demuxer by probe bytes or filename hint.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};

use crate::{CodecResolver, Error, Packet, Result, StreamInfo};

// ───────────────────────── traits ─────────────────────────

/// Reads a container and emits packets per stream.
pub trait Demuxer: Send {
    /// Name of the container format (e.g., `"wav"`).
    fn format_name(&self) -> &str;

    /// Streams in this container. Stable across the lifetime of the demuxer.
    fn streams(&self) -> &[StreamInfo];

    /// Read the next packet from any stream. Returns `Error::Eof` at end.
    fn next_packet(&mut self) -> Result<Packet>;

    /// Read the next packet as a [`SharedPacket`](crate::packet::SharedPacket),
    /// whose payload is a cheaply clonable view of a shared buffer.
    ///
    /// The default wraps [`next_packet`](Self::next_packet) (moving its
    /// payload, no copy), so every demuxer answers this. A demuxer that
    /// already holds the bytes it hands out — a whole file or a large
    /// region kept in memory — overrides it to return views of that one
    /// buffer instead of a fresh copy per packet. Both methods read the
    /// same packet sequence and advance the same position; a caller uses
    /// one or the other, or mixes them freely.
    fn next_shared_packet(&mut self) -> Result<crate::packet::SharedPacket> {
        self.next_packet().map(crate::packet::SharedPacket::from)
    }

    /// Hint that only the listed stream indices will be consumed by the
    /// pipeline. Demuxers that can efficiently skip inactive streams at
    /// the container level (e.g., MKV cluster-aware, MP4 trak-aware)
    /// should override this. The default is a no-op — the pipeline
    /// drops unwanted packets on the floor.
    fn set_active_streams(&mut self, _indices: &[u32]) {}

    /// Seek to the nearest keyframe at or before `pts` (in the given
    /// stream's time base). Returns the actual timestamp seeked to, or
    /// `Error::Unsupported` if this demuxer can't seek.
    fn seek_to(&mut self, _stream_index: u32, _pts: i64) -> Result<i64> {
        Err(Error::unsupported("this demuxer does not support seeking"))
    }

    /// Container-level metadata as ordered (key, value) pairs.
    /// Keys follow a loose convention borrowed from Vorbis comments:
    /// `title`, `artist`, `album`, `comment`, `date`, `sample_name:<n>`,
    /// `channels`, `n_patterns`, etc. Demuxers that carry no metadata
    /// return an empty slice (the default).
    fn metadata(&self) -> &[(String, String)] {
        &[]
    }
    /// Container-level duration, if known. Default is `None` — callers
    /// may fall back to the longest per-stream duration. Expressed as
    /// microseconds for portability; convert to seconds at the edge.
    fn duration_micros(&self) -> Option<i64> {
        None
    }

    /// Attached pictures (cover art, artist photos, ...) embedded in
    /// the container. Returns an empty slice (the default) when the
    /// container carries none or doesn't support them. Containers that
    /// do — ID3v2 on MP3, `METADATA_BLOCK_PICTURE` on FLAC, `covr`
    /// atoms on MP4, etc. — override this to expose the images.
    fn attached_pictures(&self) -> &[crate::AttachedPicture] {
        &[]
    }

    /// Structured chapter / cue list. Default returns an empty slice
    /// for back-compat; demuxers that carry chapters (MKV `Chapters`,
    /// MP4 chapter track, Ogg `CHAPTERnn=` Vorbis comments, …) should
    /// override and return [`Chapter`](crate::Chapter) records in
    /// presentation order. Coexists with the legacy `chapter:N:*`
    /// flat-metadata keys; new consumers should prefer this.
    fn chapters(&self) -> &[crate::Chapter] {
        &[]
    }

    /// Structured attachment list. Default returns an empty slice for
    /// back-compat; demuxers that carry attachments (MKV `Attachments`,
    /// …) should override and return [`Attachment`](crate::Attachment)
    /// records in container order. Coexists with the legacy
    /// `attachment:N:*` flat-metadata keys; new consumers should prefer
    /// this.
    fn attachments(&self) -> &[crate::Attachment] {
        &[]
    }
}

/// Writes packets into a container.
pub trait Muxer: Send {
    /// Registered name of the container format being written.
    fn format_name(&self) -> &str;

    /// Write the container header. Must be called after stream configuration
    /// and before the first `write_packet`.
    fn write_header(&mut self) -> Result<()>;

    /// Write one compressed packet into the container.
    fn write_packet(&mut self, packet: &Packet) -> Result<()>;

    /// Write one compressed packet handed over as a
    /// [`SharedPacket`](crate::packet::SharedPacket). The default
    /// converts it (moving the payload when the packet is its only
    /// holder, copying the viewed bytes otherwise) and calls
    /// [`write_packet`](Self::write_packet); a muxer that only reads the
    /// payload overrides it to write straight from the view.
    fn write_shared_packet(&mut self, packet: crate::packet::SharedPacket) -> Result<()> {
        self.write_packet(&packet.into_packet())
    }

    /// Finalize the file (write index, patch in total sizes, etc.).
    fn write_trailer(&mut self) -> Result<()>;

    /// Hand the muxer container-level string metadata: ordered
    /// `(key, value)` pairs in the vocabulary
    /// [`Demuxer::metadata`] reports (`title`, `artist`, `album`,
    /// `comment`, `date`, …), so a remux is
    /// `mux.set_metadata(demux.metadata())`. Call it after
    /// construction and **before** [`write_header`](Self::write_header):
    /// most formats write their text block in the header or need to
    /// reserve room for it there.
    ///
    /// Formats differ in what they can hold — a free-form tag block
    /// stores every pair, a fixed header with a title field stores one
    /// key and has nowhere for the rest, a format with no text at all
    /// stores nothing — so the contract is best-effort and explicit
    /// about the "nothing" case:
    ///
    /// - The default returns [`Error::Unsupported`]: the format carries
    ///   no text metadata and nothing was stored. Callers treat that as
    ///   "not carried", never as a failure of the write.
    /// - A muxer that carries text overrides this, keeps the pairs it
    ///   has a field for and returns `Ok(())`. Keys it has no field for
    ///   are dropped without an error; its docs say which keys it
    ///   writes, and [`carries_metadata_key`](Self::carries_metadata_key)
    ///   answers the same question programmatically. The two overrides
    ///   go together.
    /// - Opaque payloads (an ICC profile, an Exif block, a format's own
    ///   binary table) are not text: they travel as
    ///   [`MetadataBlob`](crate::MetadataBlob)s on the stream's
    ///   [`CodecParameters::blobs`](crate::CodecParameters::blobs) and on
    ///   frames, not through this hook — and a format's native *text*
    ///   fields (a comment extension, an annotation chunk, an author
    ///   field in a trailer) belong here, not in a blob.
    /// - A later call replaces the earlier set; `&[]` clears it.
    ///   Implementations are free to refuse a call after `write_header`
    ///   with [`Error::InvalidData`]; the default never checks.
    fn set_metadata(&mut self, _metadata: &[(String, String)]) -> Result<()> {
        Err(Error::unsupported(
            "this muxer does not carry container metadata",
        ))
    }

    /// `true` when [`set_metadata`](Self::set_metadata) stores a pair
    /// under `key` — the "will the title survive this save?" question,
    /// answerable before anything is written. The default is `false`
    /// for every key, matching the default `set_metadata`. A free-form
    /// tag block answers `true` for every key; a fixed-field format
    /// lists its keys (`matches!(key, "title" | "comment")`).
    fn carries_metadata_key(&self, _key: &str) -> bool {
        false
    }
}

/// Factory that tries to open a stream as a particular container format.
///
/// Implementations should read the minimum needed to confirm the format and
/// return `Error::InvalidData` if the stream is not in this format.
///
/// The `codecs` parameter carries a resolver that converts container-
/// level codec tags (FourCCs, WAVEFORMATEX wFormatTag, Matroska
/// CodecIDs, …) into [`CodecId`](crate::CodecId) values.
pub type OpenDemuxerFn =
    fn(input: Box<dyn ReadSeek>, codecs: &dyn CodecResolver) -> Result<Box<dyn Demuxer>>;

/// Factory that creates a muxer for a set of streams.
pub type OpenMuxerFn =
    fn(output: Box<dyn WriteSeek>, streams: &[StreamInfo]) -> Result<Box<dyn Muxer>>;

/// Information passed to a content-based [`ContainerProbeFn`].
///
/// `buf` holds the first few KB of the input — enough to recognise the
/// magic bytes of any container we know about. `ext` carries the file
/// extension as a hint (lowercase, no leading dot); some containers
/// (raw MP3 with no ID3v2, headerless tracker formats) need it to break
/// ties with otherwise weak signatures.
pub struct ProbeData<'a> {
    /// First few KB of the input, for magic-byte matching.
    pub buf: &'a [u8],
    /// File-extension hint (lowercase, no leading dot), when known.
    pub ext: Option<&'a str>,
}

/// Confidence score returned by a [`ContainerProbeFn`]. `0` means no match.
/// Higher means more certain. Conventional values:
///
/// * `100` – unambiguous magic bytes at a known offset
/// * `75`  – signature match corroborated by file extension
/// * `50`  – signature match without extension corroboration
/// * `25`  – extension match only (no content signature available)
pub type ProbeScore = u8;

/// Maximum probe score (alias for `100`).
pub const MAX_PROBE_SCORE: ProbeScore = 100;
/// Default score returned when only the file extension matches.
pub const PROBE_SCORE_EXTENSION: ProbeScore = 25;

/// Content-based format detection function.
///
/// Returns a [`ProbeScore`] in `0..=100`. Implementations should be
/// pure (no I/O, no allocation beyond the stack) and fast — they may
/// be invoked once per registered demuxer on every input file.
pub type ContainerProbeFn = fn(probe: &ProbeData) -> ProbeScore;

/// One ranked entry from [`ContainerRegistry::probe_candidates`].
///
/// Candidates are ordered by the registry's resolution rule —
/// `score` descending, then `priority` ascending (lower is
/// preferred), then `order` ascending (earlier registration wins) —
/// so the first element is exactly what
/// [`ContainerRegistry::probe_input`] would open. The struct is
/// `#[non_exhaustive]`: read it by field, construct it never.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProbeCandidate<'a> {
    /// Container name the probe was registered under.
    pub name: &'a str,
    /// Score the probe returned for this input (never `0` — non-
    /// matching probes are not candidates).
    pub score: ProbeScore,
    /// Resolution priority attached at registration (lower is
    /// preferred; [`DEFAULT_PRIORITY`](crate::DEFAULT_PRIORITY) unless
    /// [`ContainerRegistry::register_probe_with_priority`] was used).
    pub priority: i32,
    /// 0-based position of this container's *first* probe
    /// registration in the registry — the final tie-break.
    pub order: usize,
}

/// Internal probe record: `probes` is kept as a registration-ordered
/// vector (not a map) so equal-score ties resolve identically in every
/// process — see [`ContainerRegistry::probe_input`].
struct ProbeEntry {
    name: String,
    probe: ContainerProbeFn,
    priority: i32,
}

/// One ranked entry from [`ContainerRegistry::extension_candidates`].
///
/// Extension hints are a *replacement* map, not an evidence-scored
/// probe: every claim of an extension is equally (weakly) justified,
/// so the rule is `priority` ascending (lower is preferred), then
/// **most recent** registration first — the historical
/// last-registration-wins contract of
/// [`ContainerRegistry::register_extension`], preserved so existing
/// registrations resolve as they always did. The first element is
/// what [`ContainerRegistry::container_for_extension`] returns.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtensionCandidate<'a> {
    /// Container name this claim maps the extension to.
    pub container: &'a str,
    /// Resolution priority attached at registration (lower is
    /// preferred; [`DEFAULT_PRIORITY`](crate::DEFAULT_PRIORITY) unless
    /// [`ContainerRegistry::register_extension_with_priority`] was used).
    pub priority: i32,
    /// 0-based sequence number of this claim among *all* extension
    /// registrations in the registry (later claims have larger
    /// numbers). Among equal priorities the largest wins.
    pub order: usize,
}

/// Internal extension-claim record; see [`ExtensionCandidate`].
struct ExtensionClaim {
    container: String,
    priority: i32,
    order: usize,
}

/// Convenience trait bundle for seekable readers.
pub trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

/// Convenience trait bundle for seekable writers.
pub trait WriteSeek: Write + Seek + Send {}
impl<T: Write + Seek + Send> WriteSeek for T {}

// ───────────────────────── ContainerRegistry ─────────────────────────

/// Registry of container formats: demuxer/muxer factories keyed by
/// format name, plus the extension map and content probes that back
/// input auto-detection.
#[derive(Default)]
pub struct ContainerRegistry {
    /// Demuxer factories in registration order; names are unique
    /// (re-registration replaces in place). Linear lookup — a registry
    /// holds on the order of a hundred names and `open_demuxer` runs
    /// once per input, so a hash index would buy nothing but an
    /// unspecified enumeration order.
    demuxers: Vec<(String, OpenDemuxerFn)>,
    /// Muxer factories, same shape as `demuxers`.
    muxers: Vec<(String, OpenMuxerFn)>,
    /// Lowercase file extension → every claim made on it, in
    /// registration order (e.g. "wav" → [wav]). Resolution picks the
    /// lowest priority number, then the most recent claim.
    extensions: HashMap<String, Vec<ExtensionClaim>>,
    /// Number of extension claims registered so far — the source of
    /// [`ExtensionClaim::order`].
    extension_seq: usize,
    /// Content-probe records in registration order. Optional —
    /// containers without a probe still work but require an extension
    /// hint or an explicit format name. Names are unique: re-
    /// registering a name replaces its probe/priority in place (the
    /// name keeps its original position in the order).
    probes: Vec<ProbeEntry>,
}

impl ContainerRegistry {
    /// An empty registry (same as `Default`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a demuxer factory under a container format name.
    /// Re-registering a name replaces its factory in place; the name
    /// keeps its original position in
    /// [`demuxer_names`](Self::demuxer_names).
    pub fn register_demuxer(&mut self, name: &str, open: OpenDemuxerFn) {
        match self.demuxers.iter_mut().find(|(n, _)| n == name) {
            Some(slot) => slot.1 = open,
            None => self.demuxers.push((name.to_owned(), open)),
        }
    }

    /// Register a muxer factory under a container format name. Same
    /// replacement contract as [`register_demuxer`](Self::register_demuxer).
    pub fn register_muxer(&mut self, name: &str, open: OpenMuxerFn) {
        match self.muxers.iter_mut().find(|(n, _)| n == name) {
            Some(slot) => slot.1 = open,
            None => self.muxers.push((name.to_owned(), open)),
        }
    }

    /// Map a file extension (case-insensitive) to a registered
    /// container name, for extension-hint lookups.
    ///
    /// Registered at the default resolution priority
    /// ([`DEFAULT_PRIORITY`](crate::DEFAULT_PRIORITY)). When several
    /// containers claim the same extension at equal priority the most
    /// recent registration wins (the historical contract, kept
    /// unchanged); use
    /// [`register_extension_with_priority`](Self::register_extension_with_priority)
    /// to pin a winner independent of registration order.
    pub fn register_extension(&mut self, ext: &str, container_name: &str) {
        self.register_extension_with_priority(ext, container_name, crate::DEFAULT_PRIORITY);
    }

    /// [`register_extension`](Self::register_extension) with an explicit
    /// resolution priority. **Lower numbers are preferred**: a claim
    /// with a smaller `priority` beats every claim with a larger one
    /// regardless of registration order; equal priorities fall through
    /// to most-recent-wins. Every claim is retained (see
    /// [`extension_candidates`](Self::extension_candidates)), so a later
    /// default-priority claim never hides an earlier prioritised one.
    pub fn register_extension_with_priority(
        &mut self,
        ext: &str,
        container_name: &str,
        priority: i32,
    ) {
        let order = self.extension_seq;
        self.extension_seq += 1;
        self.extensions
            .entry(ext.to_lowercase())
            .or_default()
            .push(ExtensionClaim {
                container: container_name.to_owned(),
                priority,
                order,
            });
    }

    /// Every container that claimed `ext` (case-insensitive, no
    /// leading dot), ranked by the extension rule: priority ascending,
    /// then most recent registration first. Empty when the extension
    /// is unclaimed. The first element is what
    /// [`container_for_extension`](Self::container_for_extension)
    /// returns; a list whose two heads share a priority is a claim the
    /// registry settled by registration order alone.
    pub fn extension_candidates(&self, ext: &str) -> Vec<ExtensionCandidate<'_>> {
        let Some(claims) = self.extensions.get(&ext.to_lowercase()) else {
            return Vec::new();
        };
        let mut out: Vec<ExtensionCandidate<'_>> = claims
            .iter()
            .map(|c| ExtensionCandidate {
                container: c.container.as_str(),
                priority: c.priority,
                order: c.order,
            })
            .collect();
        // `order` is unique across the registry, so the key is total.
        out.sort_by(|a, b| a.priority.cmp(&b.priority).then(b.order.cmp(&a.order)));
        out
    }

    /// Attach a content-based probe to a registered demuxer. Called by
    /// the registry's [`probe_input`](Self::probe_input) to detect the
    /// container format from the first few KB of an input stream.
    ///
    /// The probe is registered at the default resolution priority
    /// ([`DEFAULT_PRIORITY`](crate::DEFAULT_PRIORITY)); equal-score
    /// ties against other default-priority probes go to the earlier
    /// registration. Use
    /// [`register_probe_with_priority`](Self::register_probe_with_priority)
    /// to win (or yield) such ties explicitly. Re-registering a name
    /// replaces its probe and resets its priority to the default; the
    /// name keeps the registration-order slot of its first
    /// registration.
    pub fn register_probe(&mut self, container_name: &str, probe: ContainerProbeFn) {
        self.register_probe_with_priority(container_name, probe, crate::DEFAULT_PRIORITY);
    }

    /// [`register_probe`](Self::register_probe) with an explicit
    /// resolution priority. **Lower numbers are preferred**, the same
    /// convention as [`CodecCapabilities::priority`](crate::CodecCapabilities::priority):
    /// when two probes return the same non-zero score for an input,
    /// the one with the smaller `priority` wins; equal priorities fall
    /// through to registration order (earlier wins). Priority never
    /// out-ranks score — a higher-scoring probe always wins regardless
    /// of priority.
    ///
    /// Typical use: two containers that share a signature family
    /// where one is the more specific reading register with a
    /// priority below the default so the specific container wins the
    /// tie whatever order the two crates happened to register in.
    pub fn register_probe_with_priority(
        &mut self,
        container_name: &str,
        probe: ContainerProbeFn,
        priority: i32,
    ) {
        if let Some(entry) = self.probes.iter_mut().find(|e| e.name == container_name) {
            entry.probe = probe;
            entry.priority = priority;
        } else {
            self.probes.push(ProbeEntry {
                name: container_name.to_owned(),
                probe,
                priority,
            });
        }
    }

    /// Resolution priority currently attached to `container_name`'s
    /// probe, or `None` when no probe is registered under that name.
    pub fn probe_priority(&self, container_name: &str) -> Option<i32> {
        self.probes
            .iter()
            .find(|e| e.name == container_name)
            .map(|e| e.priority)
    }

    /// Score every registered probe against `data` and return the
    /// non-zero results ranked by the resolution rule: score
    /// descending, then priority ascending, then registration order
    /// ascending. The first element is the container
    /// [`probe_input`](Self::probe_input) would pick (before its
    /// extension-table fallback); the rest show who lost and by how
    /// much — the hook for collision audits that want to prove a tie
    /// is broken the intended way rather than merely broken.
    ///
    /// Pure and allocation-light: the caller supplies the buffer, so
    /// this can run against synthetic inputs without any I/O.
    pub fn probe_candidates(&self, data: &ProbeData) -> Vec<ProbeCandidate<'_>> {
        let mut out: Vec<ProbeCandidate<'_>> = self
            .probes
            .iter()
            .enumerate()
            .filter_map(|(order, e)| {
                let score = (e.probe)(data);
                (score != 0).then_some(ProbeCandidate {
                    name: e.name.as_str(),
                    score,
                    priority: e.priority,
                    order,
                })
            })
            .collect();
        // Stable sort on a total key; `order` is unique so the result
        // is fully determined by the registrations, never by memory
        // layout or hashing.
        out.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(a.priority.cmp(&b.priority))
                .then(a.order.cmp(&b.order))
        });
        out
    }

    /// Iterate the registered demuxer format names in registration
    /// order (stable across processes — suitable for listings and
    /// audits without a sort).
    pub fn demuxer_names(&self) -> impl Iterator<Item = &str> {
        self.demuxers.iter().map(|(n, _)| n.as_str())
    }

    /// Iterate the registered muxer format names in registration order.
    pub fn muxer_names(&self) -> impl Iterator<Item = &str> {
        self.muxers.iter().map(|(n, _)| n.as_str())
    }

    /// Open a demuxer explicitly by format name. The `codecs` resolver
    /// is passed through to the demuxer so it can translate the
    /// container's in-stream codec tags (FourCCs / wFormatTag /
    /// Matroska CodecIDs) into [`CodecId`](crate::CodecId)
    /// values. Demuxers that don't need tag resolution can ignore it.
    pub fn open_demuxer(
        &self,
        name: &str,
        input: Box<dyn ReadSeek>,
        codecs: &dyn CodecResolver,
    ) -> Result<Box<dyn Demuxer>> {
        let open = self
            .demuxers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, f)| *f)
            .ok_or_else(|| Error::FormatNotFound(name.to_owned()))?;
        open(input, codecs)
    }

    /// Open a muxer by format name.
    pub fn open_muxer(
        &self,
        name: &str,
        output: Box<dyn WriteSeek>,
        streams: &[StreamInfo],
    ) -> Result<Box<dyn Muxer>> {
        let open = self
            .muxers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, f)| *f)
            .ok_or_else(|| Error::FormatNotFound(name.to_owned()))?;
        open(output, streams)
    }

    /// Look up a container name from a file extension (no leading dot).
    /// Among several claims the lowest priority number wins, then the
    /// most recent registration — see
    /// [`extension_candidates`](Self::extension_candidates).
    pub fn container_for_extension(&self, ext: &str) -> Option<&str> {
        // Same rule as `extension_candidates` without the allocation:
        // strictly-lower priority replaces; equal priority replaces
        // only when more recent (larger order), which a forward walk
        // over the registration-ordered claims gives for free with
        // `<=`.
        let claims = self.extensions.get(&ext.to_lowercase())?;
        let mut best: Option<&ExtensionClaim> = None;
        for c in claims {
            best = match best {
                Some(b) if c.priority > b.priority => Some(b),
                _ => Some(c),
            };
        }
        best.map(|c| c.container.as_str())
    }

    /// Detect the container format by reading the first ~256 KB of the
    /// input, scoring each registered probe, and returning the highest-
    /// scoring container's name. The extension is passed to probes as a
    /// hint — they may use it to break ties when their signature is weak.
    ///
    /// Equal top scores are resolved deterministically: the probe with
    /// the lower registration priority number wins, then the earlier
    /// registration (see [`probe_candidates`](Self::probe_candidates)
    /// for the full ranked list). The result therefore depends only on
    /// the input bytes and the sequence of `register_probe*` calls —
    /// never on process-specific hash state.
    ///
    /// Falls back to the extension table if no probe scores above zero.
    /// The input cursor is restored to its starting position on success
    /// and on the I/O failure paths that allow it.
    pub fn probe_input(&self, input: &mut dyn ReadSeek, ext_hint: Option<&str>) -> Result<String> {
        const PROBE_BUF_SIZE: usize = 256 * 1024;

        let saved_pos = input.stream_position()?;
        input.seek(SeekFrom::Start(0))?;
        let mut buf = vec![0u8; PROBE_BUF_SIZE];
        let mut got = 0;
        while got < buf.len() {
            match input.read(&mut buf[got..]) {
                Ok(0) => break,
                Ok(n) => got += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    let _ = input.seek(SeekFrom::Start(saved_pos));
                    return Err(e.into());
                }
            }
        }
        buf.truncate(got);
        input.seek(SeekFrom::Start(saved_pos))?;

        let ext_lower = ext_hint.map(|s| s.to_ascii_lowercase());
        let probe_data = ProbeData {
            buf: &buf,
            ext: ext_lower.as_deref(),
        };

        if let Some(best) = self.probe_candidates(&probe_data).first() {
            return Ok(best.name.to_owned());
        }

        // Fall back to extension lookup with the conventional weak score.
        if let Some(ext) = ext_hint
            && let Some(name) = self.container_for_extension(ext)
        {
            return Ok(name.to_owned());
        }

        Err(Error::FormatNotFound(
            "no registered demuxer recognises this input".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyDemuxer;

    impl Demuxer for DummyDemuxer {
        fn format_name(&self) -> &str {
            "dummy"
        }
        fn streams(&self) -> &[StreamInfo] {
            &[]
        }
        fn next_packet(&mut self) -> Result<Packet> {
            Err(Error::Eof)
        }
    }

    struct DummyMuxer;

    impl Muxer for DummyMuxer {
        fn format_name(&self) -> &str {
            "dummy"
        }
        fn write_header(&mut self) -> Result<()> {
            Ok(())
        }
        fn write_packet(&mut self, _packet: &Packet) -> Result<()> {
            Ok(())
        }
        fn write_trailer(&mut self) -> Result<()> {
            Ok(())
        }
    }

    /// A muxer with one text field: it keeps `title` and drops the
    /// rest, the fixed-header shape the contract describes.
    struct TitleOnlyMuxer {
        title: Option<String>,
        header_written: bool,
    }

    impl Muxer for TitleOnlyMuxer {
        fn format_name(&self) -> &str {
            "title-only"
        }
        fn write_header(&mut self) -> Result<()> {
            self.header_written = true;
            Ok(())
        }
        fn write_packet(&mut self, _packet: &Packet) -> Result<()> {
            Ok(())
        }
        fn write_trailer(&mut self) -> Result<()> {
            Ok(())
        }
        fn set_metadata(&mut self, metadata: &[(String, String)]) -> Result<()> {
            if self.header_written {
                return Err(Error::invalid("metadata must precede the header"));
            }
            self.title = metadata
                .iter()
                .find(|(k, _)| k == "title")
                .map(|(_, v)| v.clone());
            Ok(())
        }
        fn carries_metadata_key(&self, key: &str) -> bool {
            key == "title"
        }
    }

    #[test]
    fn default_muxer_metadata_hook_is_unsupported_and_carries_nothing() {
        let mut m = DummyMuxer;
        assert!(!m.carries_metadata_key("title"));
        assert!(!m.carries_metadata_key("comment"));
        let pairs = vec![("title".to_string(), "x".to_string())];
        match m.set_metadata(&pairs) {
            Err(Error::Unsupported(_)) => {}
            other => panic!("expected Unsupported, got {other:?}"),
        }
        // The hook takes exactly what `Demuxer::metadata` returns, so a
        // remux hands the slice straight across.
        let d = DummyDemuxer;
        match m.set_metadata(d.metadata()) {
            Err(Error::Unsupported(_)) => {}
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn overriding_muxer_keeps_its_keys_and_drops_the_rest() {
        let mut m = TitleOnlyMuxer {
            title: None,
            header_written: false,
        };
        assert!(m.carries_metadata_key("title"));
        assert!(!m.carries_metadata_key("artist"));
        let pairs = vec![
            ("artist".to_string(), "someone".to_string()),
            ("title".to_string(), "a picture".to_string()),
            ("comment".to_string(), "dropped".to_string()),
        ];
        m.set_metadata(&pairs).unwrap();
        assert_eq!(m.title.as_deref(), Some("a picture"));
        // A later call replaces; an empty set clears.
        m.set_metadata(&[]).unwrap();
        assert_eq!(m.title, None);
        // This implementation refuses late calls, as the docs allow.
        m.write_header().unwrap();
        assert!(matches!(m.set_metadata(&pairs), Err(Error::InvalidData(_))));
    }

    #[test]
    fn default_seek_to_is_unsupported() {
        let mut d = DummyDemuxer;
        match d.seek_to(0, 0) {
            Err(Error::Unsupported(_)) => {}
            other => panic!(
                "expected default seek_to to return Unsupported, got {:?}",
                other
            ),
        }
    }

    // ───────────── resolution-order tests (synthetic registrations) ─────────────
    //
    // Every registration below is invented for the test: the rule is
    // format-agnostic and must be provable without naming any real
    // container. Probe fns are non-capturing closures coerced to
    // `ContainerProbeFn`.

    fn always(score: ProbeScore) -> ContainerProbeFn {
        match score {
            100 => |_: &ProbeData| 100,
            90 => |_: &ProbeData| 90,
            50 => |_: &ProbeData| 50,
            _ => |_: &ProbeData| 0,
        }
    }

    fn names<'a>(cands: &'a [ProbeCandidate<'a>]) -> Vec<&'a str> {
        cands.iter().map(|c| c.name).collect()
    }

    fn probe_cursor(reg: &ContainerRegistry, bytes: &[u8]) -> String {
        let mut cur = std::io::Cursor::new(bytes.to_vec());
        reg.probe_input(&mut cur, None).expect("some probe matches")
    }

    #[test]
    fn equal_scores_resolve_by_registration_order_across_fresh_registries() {
        // Six probes that all claim the same bytes at the same score.
        // The winner must be the first-registered one in every one of
        // 1000 independently built registries — the property the old
        // `HashMap` walk could not offer.
        const ORDER: [&str; 6] = ["zeta", "alpha", "omicron", "beta", "kappa", "mu"];
        for _ in 0..1000 {
            let mut ctx = crate::RuntimeContext::new();
            for name in ORDER {
                ctx.containers.register_probe(name, always(100));
            }
            assert_eq!(probe_cursor(&ctx.containers, b"any bytes"), "zeta");
            let cands = ctx.containers.probe_candidates(&ProbeData {
                buf: b"any bytes",
                ext: None,
            });
            assert_eq!(names(&cands), ORDER.to_vec());
        }
    }

    #[test]
    fn registration_order_is_the_only_remaining_discriminator() {
        // Same two probes, same score, same (default) priority: the
        // winner flips with the registration order and with nothing
        // else — proof that names never enter the ranking.
        let mut a = ContainerRegistry::new();
        a.register_probe("second-by-name", always(100));
        a.register_probe("first-by-name", always(100));
        assert_eq!(probe_cursor(&a, b"x"), "second-by-name");

        let mut b = ContainerRegistry::new();
        b.register_probe("first-by-name", always(100));
        b.register_probe("second-by-name", always(100));
        assert_eq!(probe_cursor(&b, b"x"), "first-by-name");
    }

    #[test]
    fn lower_priority_number_beats_registration_order() {
        let mut reg = ContainerRegistry::new();
        reg.register_probe("earlier", always(100));
        reg.register_probe_with_priority("later-but-preferred", always(100), 10);
        assert_eq!(probe_cursor(&reg, b"x"), "later-but-preferred");
        let cands = reg.probe_candidates(&ProbeData {
            buf: b"x",
            ext: None,
        });
        assert_eq!(names(&cands), vec!["later-but-preferred", "earlier"]);
        assert_eq!(cands[0].priority, 10);
        assert_eq!(cands[0].order, 1);
        assert_eq!(cands[1].priority, crate::DEFAULT_PRIORITY);
        assert_eq!(cands[1].order, 0);
    }

    #[test]
    fn score_always_beats_priority() {
        // A stronger signature wins even against a probe that asked
        // for the best possible priority.
        let mut reg = ContainerRegistry::new();
        reg.register_probe_with_priority("confident-but-yielding", always(90), i32::MIN);
        reg.register_probe("weakly-prioritised-but-certain", always(100));
        assert_eq!(probe_cursor(&reg, b"x"), "weakly-prioritised-but-certain");
    }

    #[test]
    fn probe_candidates_excludes_zero_scores_and_ranks_fully() {
        let mut reg = ContainerRegistry::new();
        reg.register_probe("silent", always(0));
        reg.register_probe("half", always(50));
        reg.register_probe_with_priority("full-late-preferred", always(100), 1);
        reg.register_probe("full-early", always(100));
        reg.register_probe("half-again", always(50));
        // Insertion order: silent(0) half(1) full-late-preferred(2)
        // full-early(3) half-again(4). Ranking: 100s first (priority
        // 1 before default), then 50s in registration order; the
        // zero-scorer is absent.
        let cands = reg.probe_candidates(&ProbeData {
            buf: b"x",
            ext: None,
        });
        assert_eq!(
            names(&cands),
            vec!["full-late-preferred", "full-early", "half", "half-again"]
        );
        assert_eq!(
            cands.iter().map(|c| c.order).collect::<Vec<_>>(),
            vec![2, 3, 1, 4]
        );
        assert!(cands.iter().all(|c| c.score != 0));
    }

    #[test]
    fn re_registering_a_name_replaces_in_place() {
        // The name keeps its original order slot; probe and priority
        // are replaced (priority resets to the default when the plain
        // `register_probe` form is used).
        let mut reg = ContainerRegistry::new();
        reg.register_probe_with_priority("first", always(100), 5);
        reg.register_probe("second", always(100));
        assert_eq!(reg.probe_priority("first"), Some(5));
        assert_eq!(probe_cursor(&reg, b"x"), "first");

        // Replace `first` with a probe that no longer matches.
        reg.register_probe("first", always(0));
        assert_eq!(reg.probe_priority("first"), Some(crate::DEFAULT_PRIORITY));
        assert_eq!(probe_cursor(&reg, b"x"), "second");

        // Replace it again with a matching probe: it is still slot 0,
        // so it beats `second` on registration order.
        reg.register_probe("first", always(100));
        let cands = reg.probe_candidates(&ProbeData {
            buf: b"x",
            ext: None,
        });
        assert_eq!(names(&cands), vec!["first", "second"]);
        assert_eq!(cands[0].order, 0);
        assert_eq!(reg.probe_priority("never-registered"), None);
    }

    #[test]
    fn probe_input_falls_back_to_extension_when_nothing_scores() {
        let mut reg = ContainerRegistry::new();
        reg.register_probe("silent", always(0));
        reg.register_extension("xyz", "by-extension");
        let mut cur = std::io::Cursor::new(b"bytes".to_vec());
        assert_eq!(
            reg.probe_input(&mut cur, Some("XYZ")).unwrap(),
            "by-extension"
        );
        assert!(matches!(
            reg.probe_input(&mut cur, Some("abc")),
            Err(Error::FormatNotFound(_))
        ));
    }

    #[test]
    fn extension_equal_priority_most_recent_wins_deterministically() {
        // The historical contract, now with a visible candidate list.
        for _ in 0..1000 {
            let mut ctx = crate::RuntimeContext::new();
            ctx.containers.register_extension("SYN", "first");
            ctx.containers.register_extension("syn", "second");
            ctx.containers.register_extension("Syn", "third");
            assert_eq!(ctx.containers.container_for_extension("syn"), Some("third"));
            assert_eq!(ctx.containers.container_for_extension("SYN"), Some("third"));
            let cands = ctx.containers.extension_candidates("syn");
            assert_eq!(
                cands.iter().map(|c| c.container).collect::<Vec<_>>(),
                vec!["third", "second", "first"]
            );
            assert_eq!(
                cands.iter().map(|c| c.order).collect::<Vec<_>>(),
                vec![2, 1, 0]
            );
        }
    }

    #[test]
    fn extension_priority_beats_registration_recency() {
        let mut reg = ContainerRegistry::new();
        reg.register_extension_with_priority("syn", "pinned", 10);
        reg.register_extension("syn", "later-default");
        reg.register_extension("syn", "latest-default");
        assert_eq!(reg.container_for_extension("syn"), Some("pinned"));
        let cands = reg.extension_candidates("syn");
        assert_eq!(
            cands.iter().map(|c| c.container).collect::<Vec<_>>(),
            vec!["pinned", "latest-default", "later-default"]
        );
        assert_eq!(cands[0].priority, 10);
        assert_eq!(cands[1].priority, crate::DEFAULT_PRIORITY);

        // A later, even-lower priority claim takes over; an unrelated
        // extension is unaffected; an unclaimed one is empty / None.
        reg.register_extension_with_priority("syn", "pinned-harder", 1);
        assert_eq!(reg.container_for_extension("syn"), Some("pinned-harder"));
        reg.register_extension("other", "elsewhere");
        assert_eq!(reg.container_for_extension("other"), Some("elsewhere"));
        assert!(reg.extension_candidates("nope").is_empty());
        assert_eq!(reg.container_for_extension("nope"), None);
    }

    #[test]
    fn extension_order_is_global_across_extensions() {
        // `order` counts every claim in the registry, so two claims on
        // different extensions never share a sequence number.
        let mut reg = ContainerRegistry::new();
        reg.register_extension("a", "x");
        reg.register_extension("b", "y");
        reg.register_extension("a", "z");
        let a = reg.extension_candidates("a");
        let b = reg.extension_candidates("b");
        assert_eq!(a.iter().map(|c| c.order).collect::<Vec<_>>(), vec![2, 0]);
        assert_eq!(b[0].order, 1);
    }

    fn open_dummy(
        _input: Box<dyn ReadSeek>,
        _codecs: &dyn CodecResolver,
    ) -> Result<Box<dyn Demuxer>> {
        Ok(Box::new(DummyDemuxer))
    }

    fn open_dummy_mux(
        _output: Box<dyn WriteSeek>,
        _streams: &[StreamInfo],
    ) -> Result<Box<dyn Muxer>> {
        Err(Error::unsupported("dummy muxer"))
    }

    #[test]
    fn name_listings_follow_registration_order_and_replace_in_place() {
        const ORDER: [&str; 5] = ["zeta", "alpha", "omicron", "beta", "kappa"];
        for _ in 0..200 {
            let mut reg = ContainerRegistry::new();
            for n in ORDER {
                reg.register_demuxer(n, open_dummy);
                reg.register_muxer(n, open_dummy_mux);
            }
            // Re-registering keeps the slot.
            reg.register_demuxer("alpha", open_dummy);
            reg.register_muxer("omicron", open_dummy_mux);
            assert_eq!(reg.demuxer_names().collect::<Vec<_>>(), ORDER.to_vec());
            assert_eq!(reg.muxer_names().collect::<Vec<_>>(), ORDER.to_vec());
        }
        let mut reg = ContainerRegistry::new();
        reg.register_demuxer("only", open_dummy);
        let d = reg
            .open_demuxer(
                "only",
                Box::new(std::io::Cursor::new(Vec::new())),
                &crate::NullCodecResolver,
            )
            .unwrap();
        assert_eq!(d.format_name(), "dummy");
        assert!(matches!(
            reg.open_demuxer(
                "missing",
                Box::new(std::io::Cursor::new(Vec::new())),
                &crate::NullCodecResolver,
            ),
            Err(Error::FormatNotFound(_))
        ));
        assert!(matches!(
            reg.open_muxer("missing", Box::new(std::io::Cursor::new(Vec::new())), &[]),
            Err(Error::FormatNotFound(_))
        ));
    }

    #[test]
    fn default_chapters_and_attachments_are_empty() {
        // A demuxer that overrides nothing must compile and return
        // empty slices for both structured accessors. This is the
        // back-compat contract that lets every existing demuxer pick
        // up the new API without source changes.
        let d = DummyDemuxer;
        assert!(d.chapters().is_empty());
        assert!(d.attachments().is_empty());
        assert!(d.attached_pictures().is_empty());
        assert!(d.metadata().is_empty());
        assert_eq!(d.duration_micros(), None);
    }
}
