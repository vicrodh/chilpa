//! Layer / view identity for multi-layer and multi-view video.
//!
//! Scalable and multi-view coding schemes (spatial / quality / view
//! scalability in H.264 Annexes G and H, H.265 Annex F, H.266 Annex F,
//! AV1 operating points with spatial or temporal layers, …) put more
//! than one *layer* of coded pictures in a single stream. Each layer
//! carries an identifier in its coded units (H.265 / H.266
//! `nuh_layer_id`, AV1 `spatial_id`, …); the layer with identifier `0`
//! is the base layer, and a stream's parameter sets may map layer
//! identifiers to *views* (a camera position in stereoscopic or
//! multi-view content), to dependency / quality levels, or to auxiliary
//! pictures (alpha, depth).
//!
//! This module gives decoders a way to tag every output frame with the
//! layer it belongs to — [`LayerIdentity`], attached through
//! [`VideoFrame::layer`](crate::VideoFrame::layer) — and containers /
//! decoders a way to describe the stream's layer structure up front —
//! [`LayerInfo`], carried in
//! [`CodecParameters::layers`](crate::CodecParameters::layers). Both
//! are optional and absent for single-layer streams, which remain
//! byte-for-byte what they always were.
//!
//! The identifiers are the codec's own values passed through verbatim;
//! this crate assigns no meaning of its own to them beyond "`0` is the
//! base layer". A container that selects a subset of layers (an
//! operating point, a single view) filters on these ids; a renderer
//! that shows a stereo pair routes frames by `view_id`; a converter
//! ignores them.

/// Which layer / view a decoded [`VideoFrame`](crate::VideoFrame)
/// belongs to.
///
/// `#[non_exhaustive]`: build it with [`new`](Self::new) and the
/// `with_*` builders, read the public fields directly. `Default` is
/// the base layer with no view and no access-unit index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub struct LayerIdentity {
    /// The codec's layer identifier for the frame (H.265 / H.266
    /// `nuh_layer_id`, AV1 `spatial_id`, …). `0` is the base layer.
    pub layer_id: u16,
    /// The view the frame belongs to, for multi-view content (the
    /// codec's view identifier, e.g. H.265 Annex F `ViewId`). `None`
    /// when the stream is not multi-view or the decoder does not
    /// know the mapping.
    pub view_id: Option<u16>,
    /// Running index of the *access unit* (the set of pictures across
    /// all layers that share one presentation instant) the frame comes
    /// from, when the decoder counts them. Lets a consumer regroup the
    /// per-layer frames of one instant without relying on equal `pts`
    /// values. `None` when not tracked.
    pub access_unit: Option<u64>,
}

impl LayerIdentity {
    /// Size in bytes of the frame side-channel encoding
    /// ([`to_bytes`](Self::to_bytes)).
    pub const WIRE_LEN: usize = 13;

    /// Identity for `layer_id`, with no view and no access-unit index.
    pub const fn new(layer_id: u16) -> Self {
        Self {
            layer_id,
            view_id: None,
            access_unit: None,
        }
    }

    /// The base layer (`layer_id == 0`), no view, no access unit —
    /// identical to `Default`.
    pub const fn base() -> Self {
        Self::new(0)
    }

    /// Builder: set the view identifier.
    pub const fn with_view_id(mut self, view_id: u16) -> Self {
        self.view_id = Some(view_id);
        self
    }

    /// Builder: set the access-unit index.
    pub const fn with_access_unit(mut self, index: u64) -> Self {
        self.access_unit = Some(index);
        self
    }

    /// `true` for the base layer (`layer_id == 0`).
    pub const fn is_base_layer(&self) -> bool {
        self.layer_id == 0
    }

    /// Fixed-size wire form used by the [`VideoFrame`](crate::VideoFrame)
    /// side-channel record: one flags byte (bit 0 = `view_id`
    /// present, bit 1 = `access_unit` present), then `layer_id` as
    /// two little-endian bytes, `view_id` as two (zero when absent)
    /// and `access_unit` as eight (zero when absent).
    pub fn to_bytes(&self) -> [u8; Self::WIRE_LEN] {
        let mut out = [0u8; Self::WIRE_LEN];
        let mut flags = 0u8;
        if self.view_id.is_some() {
            flags |= 1;
        }
        if self.access_unit.is_some() {
            flags |= 2;
        }
        out[0] = flags;
        out[1..3].copy_from_slice(&self.layer_id.to_le_bytes());
        out[3..5].copy_from_slice(&self.view_id.unwrap_or(0).to_le_bytes());
        out[5..13].copy_from_slice(&self.access_unit.unwrap_or(0).to_le_bytes());
        out
    }

    /// Inverse of [`to_bytes`](Self::to_bytes). Accepts any slice of at
    /// least [`WIRE_LEN`](Self::WIRE_LEN) bytes (extra trailing bytes
    /// are reserved and ignored); returns `None` for a shorter slice.
    /// Unknown flag bits are ignored.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let b = bytes.get(..Self::WIRE_LEN)?;
        let flags = b[0];
        let layer_id = u16::from_le_bytes([b[1], b[2]]);
        let view_id = (flags & 1 != 0).then(|| u16::from_le_bytes([b[3], b[4]]));
        let access_unit = (flags & 2 != 0).then(|| {
            let mut au = [0u8; 8];
            au.copy_from_slice(&b[5..13]);
            u64::from_le_bytes(au)
        });
        Some(Self {
            layer_id,
            view_id,
            access_unit,
        })
    }
}

impl std::fmt::Display for LayerIdentity {
    /// `layer N`, `layer N view V`, optionally `@au A`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "layer {}", self.layer_id)?;
        if let Some(v) = self.view_id {
            write!(f, " view {v}")?;
        }
        if let Some(au) = self.access_unit {
            write!(f, " @au {au}")?;
        }
        Ok(())
    }
}

/// Stream-level description of one layer of a multi-layer video
/// stream, carried in
/// [`CodecParameters::layers`](crate::CodecParameters::layers).
///
/// `#[non_exhaustive]`: build it with [`new`](Self::new) and the
/// `with_*` builders, read the public fields directly.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub struct LayerInfo {
    /// The codec's layer identifier (the value frames of this layer
    /// report in [`LayerIdentity::layer_id`]). `0` is the base layer.
    pub layer_id: u16,
    /// The view this layer codes, for multi-view content; `None` when
    /// the layer is not a view (spatial / quality enhancement,
    /// auxiliary pictures) or the mapping is unknown.
    pub view_id: Option<u16>,
    /// Layer identifiers this layer predicts from directly (its
    /// reference layers). Empty for the base layer and for streams
    /// whose dependency structure is not described. A consumer that
    /// wants to decode this layer must also feed every layer reachable
    /// through `depends_on`.
    pub depends_on: Vec<u16>,
}

impl LayerInfo {
    /// Description of layer `layer_id` with no view and no declared
    /// dependencies.
    pub fn new(layer_id: u16) -> Self {
        Self {
            layer_id,
            view_id: None,
            depends_on: Vec::new(),
        }
    }

    /// Builder: set the view identifier.
    pub fn with_view_id(mut self, view_id: u16) -> Self {
        self.view_id = Some(view_id);
        self
    }

    /// Builder: replace the direct reference-layer list.
    pub fn with_depends_on(mut self, layers: impl Into<Vec<u16>>) -> Self {
        self.depends_on = layers.into();
        self
    }

    /// `true` for the base layer (`layer_id == 0`).
    pub fn is_base_layer(&self) -> bool {
        self.layer_id == 0
    }

    /// The [`LayerIdentity`] a frame of this layer would carry
    /// (`layer_id` and `view_id`; no access-unit index).
    pub fn identity(&self) -> LayerIdentity {
        LayerIdentity {
            layer_id: self.layer_id,
            view_id: self.view_id,
            access_unit: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_identity_is_the_base_layer() {
        let id = LayerIdentity::default();
        assert_eq!(id, LayerIdentity::base());
        assert_eq!(id, LayerIdentity::new(0));
        assert!(id.is_base_layer());
        assert_eq!(id.layer_id, 0);
        assert_eq!(id.view_id, None);
        assert_eq!(id.access_unit, None);
        assert_eq!(id.to_string(), "layer 0");
    }

    #[test]
    fn builders_set_fields() {
        let id = LayerIdentity::new(3).with_view_id(1).with_access_unit(42);
        assert_eq!(id.layer_id, 3);
        assert_eq!(id.view_id, Some(1));
        assert_eq!(id.access_unit, Some(42));
        assert!(!id.is_base_layer());
        assert_eq!(id.to_string(), "layer 3 view 1 @au 42");
        assert_eq!(
            LayerIdentity::new(2).with_access_unit(7).to_string(),
            "layer 2 @au 7"
        );
    }

    #[test]
    fn wire_bytes_round_trip() {
        for id in [
            LayerIdentity::base(),
            LayerIdentity::new(0x1234),
            LayerIdentity::new(1).with_view_id(0),
            LayerIdentity::new(1).with_view_id(0xBEEF),
            LayerIdentity::new(5).with_access_unit(0),
            LayerIdentity::new(5).with_access_unit(u64::MAX),
            LayerIdentity::new(u16::MAX)
                .with_view_id(u16::MAX)
                .with_access_unit(0x0102_0304_0506_0708),
        ] {
            let bytes = id.to_bytes();
            assert_eq!(LayerIdentity::from_bytes(&bytes), Some(id), "{id}");
        }
        // Explicit layout check.
        let b = LayerIdentity::new(0x0201)
            .with_view_id(0x0403)
            .with_access_unit(0x0C0B_0A09_0807_0605)
            .to_bytes();
        assert_eq!(b, [3, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        // A present-but-zero view is distinguishable from an absent one.
        assert_eq!(LayerIdentity::new(1).with_view_id(0).to_bytes()[0], 1);
        assert_eq!(LayerIdentity::new(1).to_bytes()[0], 0);
    }

    #[test]
    fn wire_bytes_tolerate_extra_and_reject_short() {
        let mut long = LayerIdentity::new(9).to_bytes().to_vec();
        long.extend_from_slice(&[0xFF; 5]);
        assert_eq!(
            LayerIdentity::from_bytes(&long),
            Some(LayerIdentity::new(9))
        );
        assert_eq!(LayerIdentity::from_bytes(&long[..12]), None);
        assert_eq!(LayerIdentity::from_bytes(&[]), None);
        // Unknown flag bits are ignored.
        let mut b = LayerIdentity::new(1).to_bytes();
        b[0] |= 0xF0;
        assert_eq!(LayerIdentity::from_bytes(&b), Some(LayerIdentity::new(1)));
    }

    #[test]
    fn layer_info_builders_and_identity() {
        let base = LayerInfo::new(0);
        assert!(base.is_base_layer());
        assert_eq!(base, LayerInfo::default());
        assert!(base.depends_on.is_empty());
        assert_eq!(base.identity(), LayerIdentity::base());

        let right = LayerInfo::new(1).with_view_id(1).with_depends_on([0u16]);
        assert_eq!(right.layer_id, 1);
        assert_eq!(right.view_id, Some(1));
        assert_eq!(right.depends_on, vec![0]);
        assert_eq!(right.identity(), LayerIdentity::new(1).with_view_id(1));

        let quality = LayerInfo::new(2).with_depends_on(vec![0, 1]);
        assert_eq!(quality.depends_on, vec![0, 1]);
        assert_eq!(quality.clone(), quality);
    }
}
