//! Opaque named metadata blobs — ICC profiles, Exif, XMP, IPTC, cover
//! art and anything else a format embeds as bytes it does not interpret.
//!
//! Containers and image formats carry per-stream metadata that the
//! framework cannot (and should not) parse: an ICC profile is consumed
//! by a colour-management system, an Exif block by a tag reader, cover
//! art by an image decoder. What the framework *can* do is carry such
//! payloads unchanged from the producer that found them (a demuxer, a
//! still-image decoder) to the consumer that wants them (a muxer
//! re-embedding the profile, a viewer reading orientation, a gateway
//! exposing `metadata()`), on the same path the samples travel.
//!
//! [`MetadataBlob`] is that carriage: a [`BlobKind`] name, an optional
//! media type and the raw bytes. The mechanism is **format-agnostic** —
//! the well-known kinds are examples with a pinned payload convention
//! so that a blob lifted from one format drops into another, and
//! [`BlobKind::custom`] names anything else. Blobs live
//! **stream-level** on [`CodecParameters::blobs`](crate::CodecParameters::blobs)
//! (the whole-stream profile, the file's Exif, an audio track's cover
//! art) and **per frame** as a [`VideoFrame`](crate::VideoFrame)
//! side-channel record (a multi-page TIFF's per-page Exif, HEIF burst
//! items with their own profile) through
//! [`VideoFrame::blobs`](crate::VideoFrame::blobs); a per-frame blob of
//! some kind refines the stream-level blob of the same kind.
//!
//! # Payload conventions for the well-known kinds
//!
//! The bytes are the format-neutral payload, with the carrying format's
//! own framing (marker segments, chunk headers, box headers,
//! compression) removed:
//!
//! | kind | payload |
//! |------|---------|
//! | [`BlobKind::ICC`] | a complete ICC profile file (ICC.1 `acsp` structure) — PNG `iCCP` inflated, JPEG `APP2 ICC_PROFILE` chunks reassembled in sequence order, ISOBMFF `colr` `prof`/`rICC` body, TIFF tag 34675 |
//! | [`BlobKind::EXIF`] | a TIFF-structured Exif block starting at the byte-order mark (`II*\0` / `MM\0*`) — JPEG `APP1` without its `Exif\0\0` prefix, PNG `eXIf` verbatim, HEIF `Exif` item without its 4-byte offset header, WebP `EXIF` chunk |
//! | [`BlobKind::XMP`] | the XMP packet: UTF-8 RDF/XML, `<?xpacket` wrapper included when the source had one — JPEG `APP1` without the `http://ns.adobe.com/xap/1.0/\0` namespace prefix, PNG `iTXt XML:com.adobe.xmp` text, ISOBMFF `mime` item, TIFF tag 700, WebP `XMP ` chunk |
//! | [`BlobKind::IPTC`] | IPTC-IIM datasets (the `1C 02 …` record stream) — Photoshop `8BIM` resource 0x0404 body, TIFF tag 33723 |
//! | [`BlobKind::COVER_ART`] | one encoded picture file (JPEG, PNG, …) with [`mime`](MetadataBlob::mime) set — ID3v2 `APIC`, FLAC `METADATA_BLOCK_PICTURE`, MP4 `covr` |
//!
//! Producers that cannot meet a convention (they hold the data in a
//! form they cannot normalise) use a [`custom`](BlobKind::custom) kind
//! naming the native form rather than a well-known kind with foreign
//! framing. Consumers never parse `data` here: they route it by
//! `kind` (and `mime`) to whatever understands it.
//!
//! # Custom kinds: the `<producer>-<chunk>` rule
//!
//! A custom kind names a payload in a *format's own* structure — the
//! framing is the payload. Its name is `<producer>-<chunk>`: the
//! producing crate's short format name, a hyphen, then the native
//! chunk / segment / field the bytes come from, all lower-case ASCII
//! letters, digits and hyphens (no leading, trailing or doubled
//! hyphen). [`BlobKind::format_native`] builds one,
//! [`BlobKind::producer`] reads the prefix back and
//! [`BlobKind::follows_naming_rule`] checks the spelling. Two
//! producers never share a prefix, so a consumer that understands one
//! format's chunk can route by prefix without sniffing the bytes, and
//! a kind that spells the same reads the same everywhere.
//!
//! Kinds in use under the rule (the producing crate's README holds the
//! exact payload; these are the established names, listed so a new
//! producer reuses rather than reinvents):
//!
//! | kind | payload |
//! |------|---------|
//! | `gif-comment` | the text of a GIF Comment Extension, sub-blocks reassembled |
//! | `tga-extension-area` | a TGA 2.0 extension area as stored |
//! | `tga-developer-area` | a TGA 2.0 developer area (directory + fields) as stored |
//! | `iff-<chunk>` (`iff-camg`, `iff-aspect`, `iff-copyright`, …) | one IFF chunk body as stored, named by its lower-cased chunk id (a non-alphanumeric id spelled out: `(c) ` → `copyright`) |
//!
//! Text that a container stores as *string metadata* (a title, an
//! author, a comment with a key) is not a blob at all: it goes
//! through [`Muxer::set_metadata`](crate::Muxer::set_metadata) and
//! [`Demuxer::metadata`](crate::Demuxer::metadata).

use std::borrow::Cow;
use std::fmt;

/// The name of a [`MetadataBlob`]'s payload family.
///
/// Compared byte-for-byte; well-known names are lower-case ASCII with
/// `-` separators and are provided as constants ([`ICC`](Self::ICC),
/// [`EXIF`](Self::EXIF), [`XMP`](Self::XMP), [`IPTC`](Self::IPTC),
/// [`COVER_ART`](Self::COVER_ART)). Anything else is a
/// [`custom`](Self::custom) kind — use a name that says which format's
/// native structure the bytes have (`"png-text"`, `"exr-attributes"`)
/// so a consumer can tell the payload apart without sniffing.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlobKind(Cow<'static, str>);

impl BlobKind {
    /// An ICC colour profile (complete ICC.1 profile file).
    pub const ICC: Self = Self::from_static("icc");
    /// A TIFF-structured Exif block starting at the byte-order mark.
    pub const EXIF: Self = Self::from_static("exif");
    /// An XMP packet (UTF-8 RDF/XML).
    pub const XMP: Self = Self::from_static("xmp");
    /// IPTC-IIM datasets.
    pub const IPTC: Self = Self::from_static("iptc");
    /// One encoded picture file (cover art); `mime` names its format.
    pub const COVER_ART: Self = Self::from_static("cover-art");

    /// The well-known kinds, in the order documented on the module.
    pub const WELL_KNOWN: [Self; 5] = [
        Self::ICC,
        Self::EXIF,
        Self::XMP,
        Self::IPTC,
        Self::COVER_ART,
    ];

    /// A kind from a `'static` name, allocation-free (how the
    /// constants are built). Equal to [`custom`](Self::custom) of the
    /// same string.
    pub const fn from_static(name: &'static str) -> Self {
        Self(Cow::Borrowed(name))
    }

    /// A kind not covered by the constants. The name is stored
    /// verbatim; compare it byte-for-byte. For a format's own chunk
    /// prefer [`format_native`](Self::format_native), which spells the
    /// name by the module's `<producer>-<chunk>` rule.
    pub fn custom(name: impl Into<String>) -> Self {
        Self(Cow::Owned(name.into()))
    }

    /// A custom kind for `chunk` of the format `producer` produces,
    /// spelled by the `<producer>-<chunk>` rule (see the [module
    /// docs](self)): the two parts joined by a hyphen and lower-cased
    /// (ASCII). Nothing else is normalised — a chunk id with
    /// characters outside letters, digits and hyphens is the caller's
    /// to spell out (`(c) ` → `copyright`), and
    /// [`follows_naming_rule`](Self::follows_naming_rule) reports the
    /// result.
    ///
    /// ```
    /// use oxideav_core::BlobKind;
    /// let k = BlobKind::format_native("iff", "CAMG");
    /// assert_eq!(k, "iff-camg");
    /// assert_eq!(k.producer(), Some("iff"));
    /// assert!(k.follows_naming_rule());
    /// assert_eq!(BlobKind::format_native("gif", "comment"), BlobKind::custom("gif-comment"));
    /// ```
    pub fn format_native(producer: &str, chunk: &str) -> Self {
        let mut name = String::with_capacity(producer.len() + 1 + chunk.len());
        name.push_str(&producer.to_ascii_lowercase());
        name.push('-');
        name.push_str(&chunk.to_ascii_lowercase());
        Self(Cow::Owned(name))
    }

    /// The `<producer>` prefix of a custom kind spelled by the
    /// `<producer>-<chunk>` rule — the text before the first hyphen —
    /// or `None` for a well-known kind (their names are format-neutral
    /// even when hyphenated, like `cover-art`) and for a custom name
    /// with no hyphen.
    pub fn producer(&self) -> Option<&str> {
        if self.is_well_known() {
            return None;
        }
        let (producer, _) = self.0.split_once('-')?;
        Some(producer)
    }

    /// `true` when the name is spelled by the module's rule: non-empty
    /// lower-case ASCII letters, digits and hyphens, starting and
    /// ending with a letter or digit, no doubled hyphen, and — for a
    /// custom kind — at least one hyphen separating `<producer>` from
    /// `<chunk>`. Every well-known constant passes. A lexical check
    /// only: it does not know which producers exist.
    pub fn follows_naming_rule(&self) -> bool {
        let name = self.0.as_bytes();
        let Some((&first, &last)) = name.first().zip(name.last()) else {
            return false;
        };
        let alnum = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
        if !alnum(first) || !alnum(last) {
            return false;
        }
        if !name.iter().all(|&b| alnum(b) || b == b'-') || name.windows(2).any(|w| w == b"--") {
            return false;
        }
        self.is_well_known() || name.contains(&b'-')
    }

    /// The kind's name.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `true` for one of the documented well-known kinds.
    pub fn is_well_known(&self) -> bool {
        Self::WELL_KNOWN.iter().any(|k| k == self)
    }
}

impl fmt::Display for BlobKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for BlobKind {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&'static str> for BlobKind {
    fn from(name: &'static str) -> Self {
        Self::from_static(name)
    }
}

impl From<String> for BlobKind {
    fn from(name: String) -> Self {
        Self(Cow::Owned(name))
    }
}

impl PartialEq<str> for BlobKind {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for BlobKind {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// One opaque metadata payload attached to a stream or a frame.
///
/// `#[non_exhaustive]`: build it with [`new`](Self::new) and the
/// `with_*` builders. The framework never interprets `data`; see the
/// [module docs](self) for the payload convention of each well-known
/// [`kind`](Self::kind).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct MetadataBlob {
    /// What the payload is — routes it to the consumer that understands
    /// it.
    pub kind: BlobKind,
    /// IANA media type of `data` when the producer knows it
    /// (`"image/jpeg"` for cover art, `"application/rdf+xml"` for an
    /// XMP packet, …). `None` means "unspecified", which is the normal
    /// state for kinds whose payload convention fixes the format.
    pub mime: Option<String>,
    /// The raw payload, per the kind's convention.
    pub data: Vec<u8>,
}

impl MetadataBlob {
    /// A blob of `kind` holding `data`, with no media type.
    pub fn new(kind: impl Into<BlobKind>, data: impl Into<Vec<u8>>) -> Self {
        Self {
            kind: kind.into(),
            mime: None,
            data: data.into(),
        }
    }

    /// Builder: set the payload's [`mime`](Self::mime) type.
    pub fn with_mime(mut self, mime: impl Into<String>) -> Self {
        self.mime = Some(mime.into());
        self
    }

    /// `true` when this blob is of `kind`.
    pub fn is(&self, kind: &BlobKind) -> bool {
        self.kind == *kind
    }

    /// Length of the wire form [`encode_blobs`] produces for this blob
    /// alone.
    fn wire_len(&self) -> usize {
        8 + self.kind.as_str().len()
            + 1
            + self.mime.as_ref().map_or(0, |m| 8 + m.len())
            + 8
            + self.data.len()
    }
}

/// Serialise a list of blobs into the wire form the
/// [`VideoFrame`](crate::VideoFrame) blob side-channel carries.
///
/// Little-endian throughout: `count: u64`, then each blob in order as
/// `kind_len: u64`, `kind` bytes, `has_mime: u8` (`0` / `1`), and if
/// set `mime_len: u64`, `mime` bytes, then `data_len: u64`, `data`
/// bytes — no padding. The empty list is the empty byte string (no
/// count), which is what lets a frame drop the record instead of
/// carrying an empty one. Every in-memory blob is representable
/// (lengths are `usize`), so encoding is total.
pub fn encode_blobs(blobs: &[MetadataBlob]) -> Vec<u8> {
    fn put(out: &mut Vec<u8>, bytes: &[u8]) {
        out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    if blobs.is_empty() {
        return Vec::new();
    }
    let total: usize = 8 + blobs.iter().map(MetadataBlob::wire_len).sum::<usize>();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&(blobs.len() as u64).to_le_bytes());
    for b in blobs {
        put(&mut out, b.kind.as_str().as_bytes());
        match &b.mime {
            Some(m) => {
                out.push(1);
                put(&mut out, m.as_bytes());
            }
            None => out.push(0),
        }
        put(&mut out, &b.data);
    }
    out
}

/// Parse the wire form produced by [`encode_blobs`].
///
/// Strict: the count and every length must be satisfied exactly,
/// `kind` and `mime` must be valid UTF-8 and `has_mime` must be `0` or
/// `1`; any truncation (even at a blob boundary — the count catches
/// it), trailing bytes or invalid flag makes the whole record
/// unreadable (`None`), so a consumer never sees half a list. The
/// empty byte string is the empty list. Nothing is pre-allocated from
/// the declared lengths, so a hostile record costs no memory.
pub fn decode_blobs(mut bytes: &[u8]) -> Option<Vec<MetadataBlob>> {
    fn take<'a>(bytes: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
        let (head, tail) = bytes.split_at_checked(n)?;
        *bytes = tail;
        Some(head)
    }
    fn take_len(bytes: &mut &[u8]) -> Option<usize> {
        let b = take(bytes, 8)?;
        usize::try_from(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
        .ok()
    }
    fn take_str<'a>(bytes: &mut &'a [u8]) -> Option<&'a str> {
        let n = take_len(bytes)?;
        std::str::from_utf8(take(bytes, n)?).ok()
    }

    if bytes.is_empty() {
        return Some(Vec::new());
    }
    let count = take_len(&mut bytes)?;
    let mut out = Vec::new();
    for _ in 0..count {
        let kind = take_str(&mut bytes)?.to_owned();
        let mime = match take(&mut bytes, 1)?[0] {
            0 => None,
            1 => Some(take_str(&mut bytes)?.to_owned()),
            _ => return None,
        };
        let len = take_len(&mut bytes)?;
        let data = take(&mut bytes, len)?.to_vec();
        out.push(MetadataBlob {
            kind: BlobKind::from(kind),
            mime,
            data,
        });
    }
    bytes.is_empty().then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_compare_by_name_whatever_their_storage() {
        assert_eq!(BlobKind::ICC, BlobKind::custom("icc"));
        assert_eq!(BlobKind::ICC, BlobKind::from(String::from("icc")));
        assert_eq!(BlobKind::from("icc"), BlobKind::ICC);
        assert_ne!(BlobKind::ICC, BlobKind::EXIF);
        assert_eq!(BlobKind::COVER_ART.as_str(), "cover-art");
        assert_eq!(BlobKind::COVER_ART.to_string(), "cover-art");
        assert!(BlobKind::COVER_ART == "cover-art");
        assert!(BlobKind::XMP.is_well_known());
        assert!(!BlobKind::custom("png-text").is_well_known());
        // Case matters: the convention is lower-case.
        assert_ne!(BlobKind::custom("ICC"), BlobKind::ICC);
        assert_eq!(BlobKind::WELL_KNOWN.len(), 5);
        let known = BlobKind::WELL_KNOWN;
        let mut names: Vec<&str> = known.iter().map(BlobKind::as_str).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 5, "well-known names are distinct");
    }

    #[test]
    fn format_native_kinds_follow_the_producer_chunk_rule() {
        let k = BlobKind::format_native("gif", "comment");
        assert_eq!(k, BlobKind::custom("gif-comment"));
        assert_eq!(k.producer(), Some("gif"));
        assert!(k.follows_naming_rule());
        assert!(!k.is_well_known());
        // Lower-casing is the only normalisation.
        assert_eq!(BlobKind::format_native("IFF", "CAMG"), "iff-camg");
        assert_eq!(
            BlobKind::format_native("tga", "extension-area"),
            "tga-extension-area"
        );
        assert_eq!(
            BlobKind::format_native("tga", "extension-area").producer(),
            Some("tga")
        );
        // A chunk id the caller did not spell out fails the lexical
        // check but is still a usable (byte-compared) kind.
        let raw = BlobKind::format_native("iff", "(c) ");
        assert_eq!(raw, "iff-(c) ");
        assert!(!raw.follows_naming_rule());
        assert_eq!(raw.producer(), Some("iff"));
        // Names are the identity: a format-native spelling of a
        // well-known name IS that kind.
        assert_eq!(BlobKind::format_native("cover", "art"), BlobKind::COVER_ART);
        assert!(BlobKind::format_native("cover", "art").is_well_known());
    }

    #[test]
    fn naming_rule_and_producer_edge_cases() {
        for k in BlobKind::WELL_KNOWN {
            assert!(k.follows_naming_rule(), "{k}");
            assert_eq!(k.producer(), None, "{k}");
        }
        for ok in [
            "png-text",
            "exr-attributes",
            "iff-copyright",
            "a-1",
            "x2-y3-z4",
        ] {
            assert!(BlobKind::custom(ok).follows_naming_rule(), "{ok}");
        }
        for bad in [
            "",
            "-",
            "exr",         // custom without a producer prefix
            "GIF-comment", // upper case
            "gif comment", // space
            "gif_comment", // underscore
            "-gif-comment",
            "gif-comment-",
            "gif--comment",
            "iff-(c)",
        ] {
            assert!(!BlobKind::custom(bad).follows_naming_rule(), "{bad:?}");
        }
        assert_eq!(BlobKind::custom("exr").producer(), None);
        assert_eq!(BlobKind::custom("").producer(), None);
        assert_eq!(BlobKind::custom("-x").producer(), Some(""));
        assert_eq!(BlobKind::custom("x2-y3-z4").producer(), Some("x2"));
    }

    #[test]
    fn blob_constructor_and_builders() {
        let b = MetadataBlob::new(BlobKind::COVER_ART, vec![0xFF, 0xD8]).with_mime("image/jpeg");
        assert!(b.is(&BlobKind::COVER_ART));
        assert!(!b.is(&BlobKind::ICC));
        assert_eq!(b.mime.as_deref(), Some("image/jpeg"));
        assert_eq!(b.data, [0xFF, 0xD8]);
        // `kind` accepts a &'static str and a byte slice for the data.
        let c = MetadataBlob::new("exr-attributes", &b"abc"[..]);
        assert_eq!(c.kind, BlobKind::custom("exr-attributes"));
        assert_eq!(c.mime, None);
        assert_eq!(c.data, b"abc");
    }

    #[test]
    fn wire_form_round_trips_including_empty_and_mime() {
        let blobs = vec![
            MetadataBlob::new(BlobKind::ICC, vec![1, 2, 3, 4]),
            MetadataBlob::new(BlobKind::COVER_ART, vec![0xFF, 0xD8, 0xFF]).with_mime("image/jpeg"),
            MetadataBlob::new(BlobKind::custom("png-text"), Vec::new()),
            MetadataBlob::new(BlobKind::EXIF, b"II*\0\x08\0\0\0".to_vec()).with_mime(""),
        ];
        let wire = encode_blobs(&blobs);
        assert_eq!(
            wire.len(),
            8 + blobs.iter().map(MetadataBlob::wire_len).sum::<usize>()
        );
        assert_eq!(&wire[..8], 4u64.to_le_bytes());
        assert_eq!(decode_blobs(&wire), Some(blobs.clone()));

        // Layout of the first blob, byte for byte: u64 LE lengths.
        let first = encode_blobs(&blobs[..1]);
        let mut expect = vec![1, 0, 0, 0, 0, 0, 0, 0];
        expect.extend_from_slice(&[3, 0, 0, 0, 0, 0, 0, 0, b'i', b'c', b'c', 0]);
        expect.extend_from_slice(&[4, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4]);
        assert_eq!(first, expect);
        // With mime: flag 1, u64 length, bytes.
        let second = &encode_blobs(&blobs[1..2])[8..];
        assert_eq!(&second[..8], [9, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&second[8..17], b"cover-art");
        assert_eq!(&second[17..26], [1, 10, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&second[26..36], b"image/jpeg");
        assert_eq!(&second[36..], [3, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xD8, 0xFF]);

        // Empty list ⇄ empty bytes.
        assert_eq!(encode_blobs(&[]), Vec::<u8>::new());
        assert_eq!(decode_blobs(&[]), Some(Vec::new()));
    }

    #[test]
    fn wire_form_rejects_truncation_trailing_bytes_and_bad_flags() {
        let blobs = vec![
            MetadataBlob::new(BlobKind::ICC, vec![1, 2, 3, 4]),
            MetadataBlob::new(BlobKind::XMP, vec![5]).with_mime("application/rdf+xml"),
        ];
        let wire = encode_blobs(&blobs);
        // Every proper prefix is unreadable — never half a list, and
        // the count catches a cut exactly at the blob boundary.
        let boundary = 8 + blobs[..1].iter().map(MetadataBlob::wire_len).sum::<usize>();
        assert_eq!(decode_blobs(&wire[..boundary]), None, "boundary cut");
        for cut in 1..wire.len() {
            assert_eq!(decode_blobs(&wire[..cut]), None, "prefix of {cut} bytes");
        }
        // One trailing byte is unreadable too.
        let mut longer = wire.clone();
        longer.push(0);
        assert_eq!(decode_blobs(&longer), None);
        // has_mime must be 0 or 1 (byte 11 of the first blob, after
        // the 8-byte count).
        let mut bad_flag = wire.clone();
        bad_flag[19] = 2;
        assert_eq!(decode_blobs(&bad_flag), None);
        // kind must be UTF-8.
        let mut bad_utf8 = wire.clone();
        bad_utf8[16] = 0xFF;
        assert_eq!(decode_blobs(&bad_utf8), None);
        // A count or length far beyond the record is a truncation, not
        // an allocation: it reads as None without reserving memory.
        let mut huge_count = wire.clone();
        huge_count[..8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(decode_blobs(&huge_count), None);
        let mut huge_name = wire.clone();
        huge_name[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(decode_blobs(&huge_name), None);
        let mut huge_data = encode_blobs(&blobs[..1]);
        huge_data[20..28].copy_from_slice(&(1u64 << 40).to_le_bytes());
        assert_eq!(decode_blobs(&huge_data), None);
        // A count smaller than the blobs present leaves trailing bytes.
        let mut short_count = wire.clone();
        short_count[..8].copy_from_slice(&1u64.to_le_bytes());
        assert_eq!(decode_blobs(&short_count), None);
    }
}
