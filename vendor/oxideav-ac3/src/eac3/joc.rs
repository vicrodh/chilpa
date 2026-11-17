//! Bounded E-AC-3 JOC signalling and EMDF decoding.
//!
//! ETSI TS 103 420 carries the OAMD and JOC payloads in an EMDF container. The
//! container is carried in an E-AC-3 reserved-data region; deployed streams place
//! it in an audio-block `skipfld`. The E-AC-3 DSP parser passes those declared
//! bytes directly to this module, avoiding any scan of coded audio data.

// Integer fields are narrowed only after `BitCursor::read` has bounded them to
// the bit width stated at each call site (at most 16 bits for narrowed values).
#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]

use std::fmt;

use super::{
    joc_sparse_tables::{
        JOC_HUFF_CODE_5CH_POS_INDEX_SPARSE, JOC_HUFF_CODE_7CH_POS_INDEX_SPARSE,
        JOC_HUFF_CODE_COARSE_COEFF_SPARSE, JOC_HUFF_CODE_FINE_COEFF_SPARSE,
    },
    joc_tables::{JOC_HUFF_CODE_COARSE_GENERIC, JOC_HUFF_CODE_FINE_GENERIC},
};

const EMDF_SYNCWORD: u32 = 0x5838;
const EMDF_SYNC_HEADER_BYTES: usize = 4;
const MAX_EMDF_PAYLOADS: usize = 16;
const MAX_VARIABLE_BITS_GROUPS: usize = 4;
const OAMD_PAYLOAD_ID: u32 = 11;
const JOC_PAYLOAD_ID: u32 = 14;
const MAX_JOC_OBJECTS: u8 = 16;
const JOC_BAND_COUNTS: [usize; 8] = [1, 3, 5, 7, 9, 12, 15, 23];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// internal — exposed for tests/fuzz; not part of the stable API
#[doc(hidden)]
pub struct JocSignal {
    pub(super) complexity_index: u8,
}

#[derive(Clone, Debug, PartialEq)]
// internal — exposed for tests/fuzz; not part of the stable API
#[doc(hidden)]
pub struct JocMetadata {
    pub(super) complexity_index: u8,
    pub(super) group_id: u32,
    pub(super) oamd_payload: Vec<u8>,
    pub(super) joc_payload: Vec<u8>,
    pub(super) downmix_config: u8,
    pub(super) num_objects: u8,
    pub(super) extension_config: u8,
    pub(super) sequence_count: u16,
    pub(super) num_channels: u8,
    pub(super) clip_gain: f32,
    pub(super) object_updates: Vec<JocObjectUpdate>,
}

impl JocMetadata {
    /// Synthetic 5.X metadata: object `n` is reconstructed from JOC
    /// channel `routing[n] % 5` alone (order L, R, C, Ls, Rs) with unit
    /// gain in every parameter band, and `oamd_payload` carries the
    /// object properties. For tests, benches and fuzz harnesses.
    #[doc(hidden)]
    pub fn synthetic(sequence_count: u16, oamd_payload: Vec<u8>, routing: &[usize]) -> Self {
        Self {
            complexity_index: 1,
            group_id: 0,
            oamd_payload,
            joc_payload: Vec::new(),
            downmix_config: 0,
            num_objects: routing.len().min(255) as u8,
            extension_config: 0,
            sequence_count,
            num_channels: 5,
            clip_gain: 1.0,
            object_updates: routing
                .iter()
                .map(|&channel| {
                    let mut coefficients = vec![0.0; 5];
                    coefficients[channel % 5] = 1.0;
                    JocObjectUpdate::Parameters {
                        num_bands: 1,
                        slope: false,
                        offsets: Vec::new(),
                        coefficients,
                    }
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JocObjectUpdate {
    /// No side information was sent for this object; retain the previous frame's
    /// reconstruction matrix.
    Reuse,
    Parameters {
        num_bands: usize,
        slope: bool,
        /// QMF timeslot offsets for steep transitions. Empty for smooth updates.
        offsets: Vec<u8>,
        /// Dequantized coefficients in `[data_point][channel][parameter_band]`
        /// order.
        coefficients: Vec<f32>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// internal — exposed for tests/fuzz; not part of the stable API
#[doc(hidden)]
pub enum JocParseError {
    Truncated(&'static str),
    Invalid(&'static str),
    Limit(&'static str),
    Unsupported(&'static str),
}

impl fmt::Display for JocParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(context) => write!(formatter, "truncated {context}"),
            Self::Invalid(context) => write!(formatter, "invalid {context}"),
            Self::Limit(context) => write!(formatter, "{context} exceeds parser limit"),
            Self::Unsupported(context) => write!(formatter, "unsupported {context}"),
        }
    }
}

/// Parse the extension type A fields carried in E-AC-3 `addbsi`.
// internal — exposed for tests/fuzz; not part of the stable API
#[doc(hidden)]
pub fn parse_ec3_extension_type_a(addbsi: &[u8]) -> Result<Option<JocSignal>, JocParseError> {
    let Some(&flags) = addbsi.first() else {
        return Ok(None);
    };
    if flags & 1 == 0 {
        return Ok(None);
    }
    if flags >> 1 != 0 {
        return Err(JocParseError::Invalid(
            "E-AC-3 extension type A reserved bits",
        ));
    }

    let complexity_index = *addbsi
        .get(1)
        .ok_or(JocParseError::Truncated("E-AC-3 extension type A"))?;
    if complexity_index > MAX_JOC_OBJECTS {
        return Err(JocParseError::Invalid(
            "E-AC-3 extension type A complexity index",
        ));
    }

    Ok(Some(JocSignal { complexity_index }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EmdfPayloadConfig {
    sample_offset: Option<u16>,
    duration: Option<u32>,
    group_id: Option<u32>,
    codec_data: Option<u8>,
    discard_unknown: bool,
    frame_aligned: Option<bool>,
    create_duplicate: bool,
    remove_duplicate: bool,
    priority: Option<u8>,
    processing_allowed: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EmdfPayload {
    id: u32,
    config: EmdfPayloadConfig,
    data: Vec<u8>,
}

#[allow(clippy::too_many_lines)]
// internal — exposed for tests/fuzz; not part of the stable API
#[doc(hidden)]
pub fn parse_joc_emdf(container: &[u8], signal: JocSignal) -> Result<JocMetadata, JocParseError> {
    if container.len() < EMDF_SYNC_HEADER_BYTES {
        return Err(JocParseError::Truncated("EMDF synchronization header"));
    }
    if u16::from_be_bytes([container[0], container[1]]) != EMDF_SYNCWORD as u16 {
        return Err(JocParseError::Invalid("EMDF syncword"));
    }
    let declared_body_len = usize::from(u16::from_be_bytes([container[2], container[3]]));
    if declared_body_len != container.len() - EMDF_SYNC_HEADER_BYTES {
        return Err(JocParseError::Invalid("EMDF container length"));
    }

    let mut bits = BitCursor::new(&container[EMDF_SYNC_HEADER_BYTES..]);
    let mut version = bits.read(2, "EMDF version")?;
    if version == 3 {
        version = version
            .checked_add(read_variable_bits(&mut bits, 2, "extended EMDF version")?)
            .ok_or(JocParseError::Limit("EMDF version"))?;
    }
    // TS 102 366 Annex H defines version zero; accepting later versions here
    // would make the following field walk ambiguous.
    if version != 0 {
        return Err(JocParseError::Invalid("unsupported EMDF version"));
    }

    let mut key_id = bits.read(3, "EMDF key id")?;
    if key_id == 7 {
        key_id = key_id
            .checked_add(read_variable_bits(&mut bits, 3, "extended EMDF key id")?)
            .ok_or(JocParseError::Limit("EMDF key id"))?;
    }
    let _ = key_id;

    let mut payloads = Vec::new();
    loop {
        let mut payload_id = bits.read(5, "EMDF payload id")?;
        if payload_id == 0 {
            break;
        }
        if payload_id == 0x1f {
            payload_id = payload_id
                .checked_add(read_variable_bits(
                    &mut bits,
                    5,
                    "extended EMDF payload id",
                )?)
                .ok_or(JocParseError::Limit("EMDF payload id"))?;
        }
        if payloads.len() >= MAX_EMDF_PAYLOADS {
            return Err(JocParseError::Limit("EMDF payload count"));
        }

        let config = parse_payload_config(&mut bits)?;
        let payload_size = usize::try_from(read_variable_bits(&mut bits, 8, "EMDF payload size")?)
            .map_err(|_| JocParseError::Limit("EMDF payload size"))?;
        if payload_size > declared_body_len
            || payload_size
                .checked_mul(8)
                .is_none_or(|payload_bits| payload_bits > bits.remaining())
        {
            return Err(JocParseError::Truncated("EMDF payload"));
        }

        let mut data = Vec::with_capacity(payload_size);
        for _ in 0..payload_size {
            data.push(bits.read(8, "EMDF payload byte")? as u8);
        }
        payloads.push(EmdfPayload {
            id: payload_id,
            config,
            data,
        });
    }

    parse_protection_and_padding(&mut bits)?;

    let mut oamd = None;
    let mut joc = None;
    for payload in payloads {
        match payload.id {
            OAMD_PAYLOAD_ID if oamd.is_none() => oamd = Some(payload),
            JOC_PAYLOAD_ID if joc.is_none() => joc = Some(payload),
            _ => {}
        }
    }
    let oamd = oamd.ok_or(JocParseError::Invalid("missing OAMD payload"))?;
    let joc = joc.ok_or(JocParseError::Invalid("missing JOC payload"))?;
    validate_joc_payload_config(&oamd.config)?;
    validate_joc_payload_config(&joc.config)?;

    let oamd_group = oamd
        .config
        .group_id
        .ok_or(JocParseError::Invalid("missing OAMD group id"))?;
    let joc_group = joc
        .config
        .group_id
        .ok_or(JocParseError::Invalid("missing JOC group id"))?;
    if oamd_group != joc_group {
        return Err(JocParseError::Invalid("OAMD/JOC group id mismatch"));
    }

    let decoded = parse_joc_payload(&joc.data)?;
    Ok(JocMetadata {
        complexity_index: signal.complexity_index,
        group_id: joc_group,
        oamd_payload: oamd.data,
        joc_payload: joc.data,
        downmix_config: decoded.downmix_config,
        num_objects: decoded.num_objects,
        extension_config: decoded.extension_config,
        sequence_count: decoded.sequence_count,
        num_channels: decoded.num_channels,
        clip_gain: decoded.clip_gain,
        object_updates: decoded.object_updates,
    })
}

fn parse_payload_config(bits: &mut BitCursor<'_>) -> Result<EmdfPayloadConfig, JocParseError> {
    let sample_offset_exists = bits.read_flag("EMDF sample-offset flag")?;
    let sample_offset = if sample_offset_exists {
        let value = bits.read(11, "EMDF sample offset")? as u16;
        if bits.read(1, "EMDF sample-offset reserved bit")? != 0 {
            return Err(JocParseError::Invalid("EMDF sample-offset reserved bit"));
        }
        Some(value)
    } else {
        None
    };

    let duration = if bits.read_flag("EMDF duration flag")? {
        Some(read_variable_bits_limited(bits, 11, 2, "EMDF duration")?)
    } else {
        None
    };
    let group_id = if bits.read_flag("EMDF group-id flag")? {
        Some(read_variable_bits(bits, 2, "EMDF group id")?)
    } else {
        None
    };
    let codec_data = if bits.read_flag("EMDF codec-data flag")? {
        Some(bits.read(8, "EMDF codec data")? as u8)
    } else {
        None
    };
    let discard_unknown = bits.read_flag("EMDF discard-unknown flag")?;

    let mut frame_aligned = None;
    let mut create_duplicate = false;
    let mut remove_duplicate = false;
    let mut priority = None;
    let mut processing_allowed = None;
    if !discard_unknown {
        if !sample_offset_exists {
            let aligned = bits.read_flag("EMDF frame-aligned flag")?;
            frame_aligned = Some(aligned);
            if aligned {
                create_duplicate = bits.read_flag("EMDF create-duplicate flag")?;
                remove_duplicate = bits.read_flag("EMDF remove-duplicate flag")?;
            }
        }
        if sample_offset_exists || frame_aligned == Some(true) {
            priority = Some(bits.read(5, "EMDF priority")? as u8);
            processing_allowed = Some(bits.read(2, "EMDF processing allowance")? as u8);
        }
    }

    Ok(EmdfPayloadConfig {
        sample_offset,
        duration,
        group_id,
        codec_data,
        discard_unknown,
        frame_aligned,
        create_duplicate,
        remove_duplicate,
        priority,
        processing_allowed,
    })
}

fn validate_joc_payload_config(config: &EmdfPayloadConfig) -> Result<(), JocParseError> {
    let aligned_profile = config.frame_aligned == Some(true)
        && config.priority == Some(0)
        && config.processing_allowed == Some(0);
    // Commercial EC-3/JOC files also use the base EMDF frame-alignment defaults,
    // omitting the transcode fields. Their payload cadence is still one container
    // per E-AC-3 frame, as required by TS 103 420.
    let deployed_default_profile = config.frame_aligned == Some(false)
        && config.priority.is_none()
        && config.processing_allowed.is_none();
    if config.sample_offset.is_some()
        || config.duration.is_some()
        || config.group_id.is_none()
        || config.discard_unknown
        || config.codec_data.is_some_and(|codec_data| codec_data != 0)
        || config.create_duplicate
        || config.remove_duplicate
        || !(aligned_profile || deployed_default_profile)
    {
        return Err(JocParseError::Invalid(
            "JOC/OAMD EMDF payload configuration",
        ));
    }
    Ok(())
}

fn parse_protection_and_padding(bits: &mut BitCursor<'_>) -> Result<(), JocParseError> {
    let primary_code = bits.read(2, "EMDF primary protection length")?;
    let secondary_code = bits.read(2, "EMDF secondary protection length")?;
    let primary_bits = match primary_code {
        1 => 8,
        2 => 32,
        3 => 128,
        _ => return Err(JocParseError::Invalid("EMDF primary protection length")),
    };
    let secondary_bits = match secondary_code {
        0 => 0,
        1 => 8,
        2 => 32,
        3 => 128,
        _ => unreachable!("2-bit value"),
    };
    bits.skip(primary_bits, "EMDF primary protection data")?;
    bits.skip(secondary_bits, "EMDF secondary protection data")?;
    while bits.remaining() > 0 {
        if bits.read(1, "EMDF padding")? != 0 {
            return Err(JocParseError::Invalid("non-zero EMDF padding"));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
struct ParsedJocFrame {
    downmix_config: u8,
    num_objects: u8,
    extension_config: u8,
    sequence_count: u16,
    num_channels: u8,
    clip_gain: f32,
    object_updates: Vec<JocObjectUpdate>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct JocObjectInfo {
    num_bands: usize,
    sparse: bool,
    quant_index: u8,
    slope: bool,
    num_data_points: usize,
    offsets: Vec<u8>,
}

fn parse_joc_payload(payload: &[u8]) -> Result<ParsedJocFrame, JocParseError> {
    let mut bits = BitCursor::new(payload);
    let downmix_config = bits.read(3, "JOC downmix configuration")? as u8;
    let num_channels = match downmix_config {
        0 | 3 => 5,
        1 | 2 | 4 => 7,
        _ => return Err(JocParseError::Invalid("reserved JOC downmix configuration")),
    };
    let num_objects_bits = bits.read(6, "JOC object count")? as u8;
    if num_objects_bits >= MAX_JOC_OBJECTS {
        return Err(JocParseError::Invalid("JOC object count"));
    }
    let extension_config = bits.read(3, "JOC extension configuration")? as u8;
    if extension_config != 0 {
        return Err(JocParseError::Unsupported("JOC extension configuration"));
    }

    let clip_gain_x = bits.read(3, "JOC clip-gain exponent")? as i32;
    let clip_gain_y = bits.read(5, "JOC clip-gain mantissa")? as u8;
    if clip_gain_x < 4 {
        return Err(JocParseError::Invalid("JOC clip gain"));
    }
    let clip_gain = (1.0 + f32::from(clip_gain_y) / 32.0) * 2.0_f32.powi(clip_gain_x - 4);
    let sequence_count = bits.read(10, "JOC sequence count")? as u16;

    let num_objects = num_objects_bits + 1;
    let mut object_info = Vec::with_capacity(usize::from(num_objects));
    for _ in 0..num_objects {
        if !bits.read_flag("JOC object-present flag")? {
            object_info.push(None);
            continue;
        }

        let band_index = bits.read(3, "JOC parameter-band count")? as usize;
        let num_bands = JOC_BAND_COUNTS[band_index];
        let sparse = bits.read_flag("JOC sparse-coding flag")?;
        let quant_index = bits.read(1, "JOC quantizer index")? as u8;
        let slope = bits.read_flag("JOC interpolation slope")?;
        let num_data_points = bits.read(1, "JOC data-point count")? as usize + 1;
        let mut offsets = Vec::with_capacity(if slope { num_data_points } else { 0 });
        if slope {
            for _ in 0..num_data_points {
                offsets.push(bits.read(5, "JOC data-point offset")? as u8 + 1);
            }
        }
        object_info.push(Some(JocObjectInfo {
            num_bands,
            sparse,
            quant_index,
            slope,
            num_data_points,
            offsets,
        }));
    }

    let mut object_updates = Vec::with_capacity(usize::from(num_objects));
    for info in object_info {
        let Some(info) = info else {
            object_updates.push(JocObjectUpdate::Reuse);
            continue;
        };
        let coefficients = if info.sparse {
            decode_sparse_coefficients(&mut bits, &info, num_channels)?
        } else {
            decode_dense_coefficients(&mut bits, &info, num_channels)?
        };
        object_updates.push(JocObjectUpdate::Parameters {
            num_bands: info.num_bands,
            slope: info.slope,
            offsets: info.offsets,
            coefficients,
        });
    }

    if bits.remaining() > 7 {
        return Err(JocParseError::Invalid("JOC trailing data"));
    }
    while bits.remaining() > 0 {
        if bits.read(1, "JOC padding")? != 0 {
            return Err(JocParseError::Invalid("non-zero JOC padding"));
        }
    }

    Ok(ParsedJocFrame {
        downmix_config,
        num_objects,
        extension_config,
        sequence_count,
        num_channels,
        clip_gain,
        object_updates,
    })
}

fn decode_dense_coefficients(
    bits: &mut BitCursor<'_>,
    info: &JocObjectInfo,
    num_channels: u8,
) -> Result<Vec<f32>, JocParseError> {
    let table = match info.quant_index {
        0 => JOC_HUFF_CODE_COARSE_GENERIC,
        1 => JOC_HUFF_CODE_FINE_GENERIC,
        _ => unreachable!("quantizer index is one bit"),
    };
    let nquant = quantizer_steps(info.quant_index);
    let offset = nquant / 2;
    let capacity = info
        .num_data_points
        .checked_mul(usize::from(num_channels))
        .and_then(|count| count.checked_mul(info.num_bands))
        .ok_or(JocParseError::Limit("JOC coefficient count"))?;
    let mut coefficients = Vec::with_capacity(capacity);

    for _ in 0..info.num_data_points {
        for _ in 0..num_channels {
            let mut previous = offset;
            for parameter_band in 0..info.num_bands {
                let delta = decode_huffman(bits, table)?;
                let base = if parameter_band == 0 {
                    offset
                } else {
                    previous
                };
                previous = (base + delta) % nquant;
                coefficients.push(dequantize(previous, nquant, info.quant_index));
            }
        }
    }
    Ok(coefficients)
}

fn decode_sparse_coefficients(
    bits: &mut BitCursor<'_>,
    info: &JocObjectInfo,
    num_channels: u8,
) -> Result<Vec<f32>, JocParseError> {
    let position_table = match num_channels {
        5 => JOC_HUFF_CODE_5CH_POS_INDEX_SPARSE,
        7 => JOC_HUFF_CODE_7CH_POS_INDEX_SPARSE,
        _ => return Err(JocParseError::Invalid("JOC sparse channel count")),
    };
    let vector_table = match info.quant_index {
        0 => JOC_HUFF_CODE_COARSE_COEFF_SPARSE,
        1 => JOC_HUFF_CODE_FINE_COEFF_SPARSE,
        _ => unreachable!("quantizer index is one bit"),
    };
    let nquant = quantizer_steps(info.quant_index);
    let offset = if info.quant_index == 0 { 50 } else { 100 };
    let channels = usize::from(num_channels);
    let per_point = channels
        .checked_mul(info.num_bands)
        .ok_or(JocParseError::Limit("JOC sparse coefficient count"))?;
    let capacity = info
        .num_data_points
        .checked_mul(per_point)
        .ok_or(JocParseError::Limit("JOC sparse coefficient count"))?;
    let mut coefficients = Vec::with_capacity(capacity);

    for _ in 0..info.num_data_points {
        let first_channel = bits.read(3, "JOC sparse channel index")? as u8;
        if first_channel >= num_channels {
            return Err(JocParseError::Invalid("JOC sparse channel index"));
        }
        let mut channel_deltas = Vec::with_capacity(info.num_bands);
        channel_deltas.push(first_channel);
        for _ in 1..info.num_bands {
            channel_deltas.push(
                u8::try_from(decode_huffman(bits, position_table)?)
                    .expect("sparse position symbols fit u8"),
            );
        }

        let mut vector = Vec::with_capacity(info.num_bands);
        for _ in 0..info.num_bands {
            vector.push(decode_huffman(bits, vector_table)?);
        }

        let mut quantized = vec![offset; per_point];
        for parameter_band in 0..info.num_bands {
            let active_channel = if parameter_band == 0 {
                usize::from(first_channel)
            } else {
                (usize::from(channel_deltas[parameter_band - 1])
                    + usize::from(channel_deltas[parameter_band]))
                    % channels
            };
            let index = active_channel * info.num_bands + parameter_band;
            let base = if parameter_band == 0 {
                offset
            } else {
                quantized[index - 1]
            };
            quantized[index] = (base + vector[parameter_band]) % nquant;
        }
        coefficients.extend(
            quantized
                .into_iter()
                .map(|value| dequantize(value, nquant, info.quant_index)),
        );
    }
    Ok(coefficients)
}

fn decode_huffman(bits: &mut BitCursor<'_>, table: &[[i16; 2]]) -> Result<u16, JocParseError> {
    let mut node = 0_usize;
    for _ in 0..=table.len() {
        let branch = usize::from(bits.read_flag("JOC Huffman codeword")?);
        let next = *table
            .get(node)
            .and_then(|branches| branches.get(branch))
            .ok_or(JocParseError::Invalid("JOC Huffman tree node"))?;
        if next < 0 {
            return u16::try_from(-i32::from(next) - 1)
                .map_err(|_| JocParseError::Invalid("JOC Huffman symbol"));
        }
        if next == 0 {
            return Err(JocParseError::Invalid("JOC Huffman tree cycle"));
        }
        node = usize::try_from(next).map_err(|_| JocParseError::Invalid("JOC Huffman node"))?;
    }
    Err(JocParseError::Invalid("JOC Huffman codeword length"))
}

fn quantizer_steps(quant_index: u8) -> u16 {
    if quant_index == 0 { 96 } else { 192 }
}

fn dequantize(value: u16, nquant: u16, quant_index: u8) -> f32 {
    let centered = i16::try_from(i32::from(value) - i32::from(nquant / 2))
        .expect("JOC quantized coefficient range fits i16");
    f32::from(centered) * 820.0 / (4096.0 * f32::from(1 + quant_index))
}

fn read_variable_bits(
    bits: &mut BitCursor<'_>,
    width: usize,
    context: &'static str,
) -> Result<u32, JocParseError> {
    read_variable_bits_limited(bits, width, MAX_VARIABLE_BITS_GROUPS, context)
}

fn read_variable_bits_limited(
    bits: &mut BitCursor<'_>,
    width: usize,
    max_groups: usize,
    context: &'static str,
) -> Result<u32, JocParseError> {
    let mut value = 0_u32;
    for group in 0..max_groups {
        value = value
            .checked_add(bits.read(width, context)?)
            .ok_or(JocParseError::Limit(context))?;
        if !bits.read_flag(context)? {
            return Ok(value);
        }
        if group + 1 == max_groups {
            return Err(JocParseError::Limit(context));
        }
        value = value
            .checked_shl(u32::try_from(width).expect("field width fits u32"))
            .and_then(|shifted| shifted.checked_add(1_u32 << width))
            .ok_or(JocParseError::Limit(context))?;
    }
    unreachable!("variable-bit loop returns from every final-group path")
}

#[derive(Clone, Copy)]
struct BitCursor<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> BitCursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn remaining(self) -> usize {
        self.data.len() * 8 - self.position
    }

    fn read(&mut self, width: usize, context: &'static str) -> Result<u32, JocParseError> {
        if width > 32 {
            return Err(JocParseError::Limit(context));
        }
        let end = self
            .position
            .checked_add(width)
            .ok_or(JocParseError::Limit(context))?;
        if end > self.data.len() * 8 {
            return Err(JocParseError::Truncated(context));
        }

        let value = read_bits_at(self.data, self.position, width)
            .ok_or(JocParseError::Truncated(context))?;
        self.position = end;
        Ok(value)
    }

    fn read_flag(&mut self, context: &'static str) -> Result<bool, JocParseError> {
        Ok(self.read(1, context)? != 0)
    }

    fn skip(&mut self, width: usize, context: &'static str) -> Result<(), JocParseError> {
        let end = self
            .position
            .checked_add(width)
            .ok_or(JocParseError::Limit(context))?;
        if end > self.data.len() * 8 {
            return Err(JocParseError::Truncated(context));
        }
        self.position = end;
        Ok(())
    }
}

fn read_bits_at(data: &[u8], start_bit: usize, width: usize) -> Option<u32> {
    if width > 32 || start_bit.checked_add(width)? > data.len().checked_mul(8)? {
        return None;
    }
    let mut value = 0_u32;
    for bit in start_bit..start_bit + width {
        value = (value << 1) | u32::from((data[bit / 8] >> (7 - bit % 8)) & 1);
    }
    Some(value)
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // Synthetic bitstreams decode to exact table values.
mod tests {
    use super::*;

    #[test]
    fn parses_extension_type_a_signal() {
        assert_eq!(parse_ec3_extension_type_a(&[]), Ok(None));
        assert_eq!(parse_ec3_extension_type_a(&[0, 16]), Ok(None));
        assert_eq!(
            parse_ec3_extension_type_a(&[1, 16]),
            Ok(Some(JocSignal {
                complexity_index: 16
            }))
        );
        assert!(parse_ec3_extension_type_a(&[1]).is_err());
        assert!(parse_ec3_extension_type_a(&[1, 17]).is_err());
        assert!(parse_ec3_extension_type_a(&[0x81, 16]).is_err());
    }

    #[test]
    fn parses_joc_emdf_from_declared_skip_field_bytes() {
        let emdf = make_test_emdf(3, 7, 0, &[0xaa, 0xbb]);
        let metadata = parse_joc_emdf(
            &emdf,
            JocSignal {
                complexity_index: 8,
            },
        )
        .expect("parse synthetic EMDF");

        assert_eq!(metadata.complexity_index, 8);
        assert_eq!(metadata.group_id, 3);
        assert_eq!(metadata.oamd_payload, [0x40, 0xaa, 0xbb]);
        assert!(metadata.joc_payload.len() >= 4);
        assert_eq!(metadata.downmix_config, 0);
        assert_eq!(metadata.num_objects, 7);
        assert_eq!(metadata.extension_config, 0);
        assert_eq!(metadata.sequence_count, 0x155);
        assert_eq!(metadata.num_channels, 5);
        assert_eq!(metadata.clip_gain, 1.0);
        assert_eq!(metadata.object_updates.len(), 7);
        assert!(metadata.object_updates.iter().all(|update| matches!(
            update,
            JocObjectUpdate::Parameters {
                num_bands: 1,
                slope: false,
                offsets,
                coefficients,
            } if offsets.is_empty() && coefficients.len() == 5
        )));
    }

    #[test]
    fn rejects_non_emdf_skip_field_data() {
        let emdf = make_test_emdf(1, 1, 0, &[]);
        let mut skip_field = vec![0; 3];
        skip_field.extend(emdf);
        assert!(matches!(
            parse_joc_emdf(
                &skip_field,
                JocSignal {
                    complexity_index: 1
                }
            ),
            Err(JocParseError::Invalid("EMDF syncword"))
        ));
    }

    #[test]
    fn rejects_mismatched_oamd_and_joc_groups() {
        let emdf = make_test_emdf_with_groups(2, 3, 2, 0, &[]);
        assert!(matches!(
            parse_joc_emdf(
                &emdf,
                JocSignal {
                    complexity_index: 2
                }
            ),
            Err(JocParseError::Invalid("OAMD/JOC group id mismatch"))
        ));
    }

    #[test]
    fn arbitrary_skip_field_bytes_are_bounded_and_panic_free() {
        let signal = JocSignal {
            complexity_index: MAX_JOC_OBJECTS,
        };
        let mut state = 0x4d59_5df4_u32;

        // `skipl` is nine bits, so 511 bytes is the largest declared
        // E-AC-3 skip-field payload this parser can receive.
        for length in 0..=511 {
            let mut data = vec![0_u8; length];
            for byte in &mut data {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                *byte = state as u8;
            }
            let _ = parse_ec3_extension_type_a(&data);
            let _ = parse_joc_emdf(&data, signal);
        }
    }

    fn make_test_emdf(
        group_id: u32,
        num_objects: u8,
        downmix_config: u8,
        oamd_tail: &[u8],
    ) -> Vec<u8> {
        make_test_emdf_with_groups(group_id, group_id, num_objects, downmix_config, oamd_tail)
    }

    fn make_test_emdf_with_groups(
        oamd_group: u32,
        joc_group: u32,
        num_objects: u8,
        downmix_config: u8,
        oamd_tail: &[u8],
    ) -> Vec<u8> {
        let mut body = TestBitWriter::default();
        body.write(0, 2); // emdf_version
        body.write(0, 3); // key_id

        let mut oamd = vec![0x40];
        oamd.extend_from_slice(oamd_tail);
        write_payload(&mut body, OAMD_PAYLOAD_ID, oamd_group, &oamd);

        let mut joc = TestBitWriter::default();
        joc.write(u32::from(downmix_config), 3);
        joc.write(u32::from(num_objects.saturating_sub(1)), 6);
        joc.write(0, 3); // extension config
        joc.write(4, 3); // clip-gain x
        joc.write(0, 5); // clip-gain y
        joc.write(0x155, 10); // sequence count
        for _ in 0..num_objects {
            joc.write(1, 1); // object update is present
            joc.write(0, 3); // one parameter band
            joc.write(0, 1); // dense matrix
            joc.write(0, 1); // coarse quantizer
            joc.write(0, 1); // smooth interpolation
            joc.write(0, 1); // one data point
        }
        // Symbol zero is the root's zero branch in both generic tables.
        for _ in 0..usize::from(num_objects) * 5 {
            joc.write(0, 1);
        }
        let joc = joc.into_bytes();
        write_payload(&mut body, JOC_PAYLOAD_ID, joc_group, &joc);

        body.write(0, 5); // container end
        body.write(1, 2); // 8-bit primary protection
        body.write(0, 2); // no secondary protection
        body.write(0xa5, 8);
        let body = body.into_bytes();

        let mut container = Vec::with_capacity(EMDF_SYNC_HEADER_BYTES + body.len());
        container.extend_from_slice(&(EMDF_SYNCWORD as u16).to_be_bytes());
        container.extend_from_slice(
            &u16::try_from(body.len())
                .expect("synthetic body fits EMDF length")
                .to_be_bytes(),
        );
        container.extend(body);
        container
    }

    fn write_payload(writer: &mut TestBitWriter, id: u32, group_id: u32, data: &[u8]) {
        writer.write(id, 5);
        writer.write(0, 1); // no sample offset
        writer.write(0, 1); // no duration
        writer.write(1, 1); // group id exists
        writer.write_variable(group_id, 2);
        writer.write(1, 1); // codec data exists
        writer.write(0, 8); // codec data
        writer.write(0, 1); // retain unknown payload
        writer.write(1, 1); // frame aligned
        writer.write(0, 1); // do not create duplicate
        writer.write(0, 1); // do not remove duplicate
        writer.write(0, 5); // highest priority
        writer.write(0, 2); // processing not allowed
        writer.write_variable(
            u32::try_from(data.len()).expect("synthetic payload length fits u32"),
            8,
        );
        for byte in data {
            writer.write(u32::from(*byte), 8);
        }
    }

    #[derive(Default)]
    struct TestBitWriter {
        bits: Vec<bool>,
    }

    impl TestBitWriter {
        fn write(&mut self, value: u32, width: usize) {
            assert!(width <= 32);
            for shift in (0..width).rev() {
                self.bits.push((value >> shift) & 1 != 0);
            }
        }

        fn write_variable(&mut self, value: u32, width: usize) {
            assert!(value < 1_u32 << width, "test helper only needs one group");
            self.write(value, width);
            self.write(0, 1);
        }

        fn into_bytes(mut self) -> Vec<u8> {
            while !self.bits.len().is_multiple_of(8) {
                self.bits.push(false);
            }
            self.bits
                .as_chunks::<8>()
                .0
                .iter()
                .map(|bits| {
                    bits.iter()
                        .fold(0_u8, |byte, bit| (byte << 1) | u8::from(*bit))
                })
                .collect()
        }
    }
}
