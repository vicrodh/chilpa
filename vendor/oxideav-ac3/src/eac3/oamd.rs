//! Object Audio Metadata (OAMD) decoding for E-AC-3 JOC.
//!
//! The syntax and state transitions follow ETSI TS 103 420 V1.2.1,
//! clauses 5.3 through 5.6. This module resolves reuse and differential
//! coding into complete per-update object states; rendering remains a
//! separate policy.

// Narrowing conversions below all follow fixed-width (at most 16-bit) syntax
// fields read through `BitCursor`; floating conversions are of at most 6 bits.
#![allow(clippy::cast_possible_truncation)]

use std::fmt;

use super::joc_layout::JocSpeaker;

const MAX_OAMD_OBJECTS: usize = 128;
const MAX_OAMD_ELEMENTS: usize = 31;
const MAX_VARIABLE_BITS_GROUPS: usize = 4;
const OBJECT_ELEMENT_ID: u8 = 1;
const TRIM_ELEMENT_ID: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OamdObjectKind {
    Lfe,
    Bed,
    Isf,
    Dynamic,
}

impl OamdObjectKind {
    fn has_fixed_render_info(self) -> bool {
        !matches!(self, Self::Dynamic)
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq)]
pub(super) struct OamdObjectState {
    pub(super) active: bool,
    /// `None` represents the specified minus-infinity gain.
    pub(super) gain_db: Option<f32>,
    pub(super) priority: f32,
    /// Normalized room- or screen-relative `(x, y, z)` coordinates.
    pub(super) position: [f32; 3],
    pub(super) size: [f32; 3],
    pub(super) screen_reference: bool,
    pub(super) channel_lock: bool,
    pub(super) zone_constraints: u8,
    pub(super) elevation_enabled: bool,
    pub(super) raw_position: [i16; 3],
}

impl OamdObjectState {
    /// Initial state of an object. A bed object is speaker-anchored
    /// (clause 5.2.1.4): its position is the room position of its
    /// speaker label and never changes.
    fn default_for(kind: OamdObjectKind, label: Option<JocSpeaker>) -> Self {
        let position = match (kind, label) {
            (OamdObjectKind::Lfe, _) => [-1.0, 1.0, -1.0],
            (OamdObjectKind::Bed, Some(speaker)) => speaker.room_position(),
            _ => [0.5, 0.5, 0.0],
        };
        Self {
            active: true,
            gain_db: Some(0.0),
            priority: 1.0,
            position,
            size: [0.0; 3],
            screen_reference: false,
            channel_lock: false,
            zone_constraints: 0,
            elevation_enabled: true,
            raw_position: [31, 31, 0],
        }
    }

    pub(super) fn linear_gain(&self) -> f32 {
        self.gain_db
            .map_or(0.0, |decibels| 10.0_f32.powf(decibels / 20.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OamdTiming {
    pub(super) start_sample: u16,
    pub(super) ramp_duration: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct OamdUpdateBlock {
    pub(super) timing: OamdTiming,
    pub(super) objects: Vec<OamdObjectState>,
}

#[derive(Clone, Debug, PartialEq)]
// internal — exposed for tests/fuzz; not part of the stable API
#[doc(hidden)]
pub struct OamdFrame {
    pub(super) kinds: Vec<OamdObjectKind>,
    /// Speaker label of each bed object (`None` for every other kind).
    pub(super) labels: Vec<Option<JocSpeaker>>,
    pub(super) starting_objects: Vec<OamdObjectState>,
    pub(super) updates: Vec<OamdUpdateBlock>,
}

impl OamdFrame {
    pub(super) fn joc_object_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.kinds
            .iter()
            .enumerate()
            .filter_map(|(index, kind)| (!matches!(kind, OamdObjectKind::Lfe)).then_some(index))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// internal — exposed for tests/fuzz; not part of the stable API
#[doc(hidden)]
pub enum OamdParseError {
    Truncated(&'static str),
    Invalid(&'static str),
    Limit(&'static str),
    Unsupported(&'static str),
}

impl fmt::Display for OamdParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(context) => write!(formatter, "truncated {context}"),
            Self::Invalid(context) => write!(formatter, "invalid {context}"),
            Self::Limit(context) => write!(formatter, "{context} exceeds parser limit"),
            Self::Unsupported(context) => write!(formatter, "unsupported {context}"),
        }
    }
}

#[derive(Default)]
// internal — exposed for tests/fuzz; not part of the stable API
#[doc(hidden)]
pub struct OamdDecoder {
    kinds: Vec<OamdObjectKind>,
    labels: Vec<Option<JocSpeaker>>,
    objects: Vec<OamdObjectState>,
}

impl OamdDecoder {
    #[doc(hidden)]
    pub fn reset(&mut self) {
        self.kinds.clear();
        self.labels.clear();
        self.objects.clear();
    }

    #[allow(clippy::too_many_lines)]
    #[doc(hidden)]
    pub fn decode(&mut self, payload: &[u8]) -> Result<OamdFrame, OamdParseError> {
        let mut bits = BitCursor::new(payload);
        let mut version = bits.read(2, "OAMD version")?;
        if version == 3 {
            version = version
                .checked_add(bits.read(3, "extended OAMD version")?)
                .ok_or(OamdParseError::Limit("OAMD version"))?;
        }
        if version != 0 {
            return Err(OamdParseError::Unsupported("OAMD version"));
        }

        let mut object_count_bits = bits.read(5, "OAMD object count")?;
        if object_count_bits == 0x1f {
            object_count_bits = object_count_bits
                .checked_add(bits.read(7, "extended OAMD object count")?)
                .ok_or(OamdParseError::Limit("OAMD object count"))?;
        }
        let object_count = usize::try_from(object_count_bits)
            .ok()
            .and_then(|count| count.checked_add(1))
            .ok_or(OamdParseError::Limit("OAMD object count"))?;
        if object_count > MAX_OAMD_OBJECTS {
            return Err(OamdParseError::Limit("OAMD object count"));
        }

        let (kinds, labels) = parse_program_assignment(&mut bits, object_count)?;
        let same_program =
            self.kinds == kinds && self.labels == labels && self.objects.len() == object_count;
        let default_objects = || -> Vec<OamdObjectState> {
            kinds
                .iter()
                .zip(&labels)
                .map(|(&kind, &label)| OamdObjectState::default_for(kind, label))
                .collect()
        };
        let alternate_data = bits.read_flag("OAMD alternate-object-data flag")?;
        let mut element_count = bits.read(4, "OAMD element count")?;
        if element_count == 0x0f {
            element_count = element_count
                .checked_add(bits.read(5, "extended OAMD element count")?)
                .ok_or(OamdParseError::Limit("OAMD element count"))?;
        }
        let element_count = usize::try_from(element_count)
            .map_err(|_| OamdParseError::Limit("OAMD element count"))?;
        if element_count > MAX_OAMD_ELEMENTS {
            return Err(OamdParseError::Limit("OAMD element count"));
        }

        let mut starting_objects = if same_program {
            self.objects.clone()
        } else {
            default_objects()
        };
        let mut parsed_updates = None;

        for _ in 0..element_count {
            let element_id = bits.read(4, "OAMD element id")? as u8;
            let size_bits = read_variable_bits_max(&mut bits, 4, "OAMD element size")?;
            let element_bytes = usize::try_from(size_bits)
                .ok()
                .and_then(|size| size.checked_add(1))
                .ok_or(OamdParseError::Limit("OAMD element size"))?;
            let element_bits = element_bytes
                .checked_mul(8)
                .ok_or(OamdParseError::Limit("OAMD element size"))?;
            let element_end = bits
                .position
                .checked_add(element_bits)
                .ok_or(OamdParseError::Limit("OAMD element position"))?;
            if element_end > bits.limit {
                return Err(OamdParseError::Truncated("OAMD element"));
            }

            let mut element = BitCursor {
                data: payload,
                position: bits.position,
                limit: element_end,
            };
            if alternate_data {
                let alternate_id = element.read(4, "OAMD alternate-object-data id")?;
                if alternate_id != 0 {
                    return Err(OamdParseError::Unsupported(
                        "OAMD alternate-object-data type",
                    ));
                }
            }
            let _discard_unknown = element.read_flag("OAMD discard-unknown-element flag")?;

            match element_id {
                OBJECT_ELEMENT_ID => {
                    if parsed_updates.is_some() {
                        return Err(OamdParseError::Invalid("duplicate OAMD object element"));
                    }
                    let updates = parse_object_element(&mut element, &kinds, &starting_objects)?;
                    starting_objects = updates
                        .last()
                        .map_or(starting_objects, |update| update.objects.clone());
                    parsed_updates = Some(updates);
                }
                TRIM_ELEMENT_ID => parse_trim_element(&mut element, object_count)?,
                _ => {
                    // Unknown/reserved elements are explicitly length-delimited.
                    element.position = element.limit;
                }
            }
            if matches!(element_id, OBJECT_ELEMENT_ID | TRIM_ELEMENT_ID) {
                element.consume_zero_padding("OAMD element padding")?;
            }
            bits.position = element_end;
        }
        bits.consume_zero_padding("OAMD payload padding")?;

        let updates =
            parsed_updates.ok_or(OamdParseError::Invalid("missing OAMD object element"))?;
        let frame_start = if same_program {
            self.objects.clone()
        } else {
            default_objects()
        };
        self.objects = updates
            .last()
            .map_or(frame_start.clone(), |update| update.objects.clone());
        self.kinds.clone_from(&kinds);
        self.labels.clone_from(&labels);

        Ok(OamdFrame {
            kinds,
            labels,
            starting_objects: frame_start,
            updates,
        })
    }
}

/// Speaker labels of the `bed_channel_assignment[]` array elements
/// (Table 12), by array index.
const STANDARD_BED_LABELS: [&[JocSpeaker]; 10] = [
    &[JocSpeaker::Lfe], // RC_LFE2
    &[JocSpeaker::LeftWide, JocSpeaker::RightWide],
    &[JocSpeaker::TopBackLeft, JocSpeaker::TopBackRight],
    &[JocSpeaker::TopSideLeft, JocSpeaker::TopSideRight],
    &[JocSpeaker::TopFrontLeft, JocSpeaker::TopFrontRight],
    &[JocSpeaker::LeftBack, JocSpeaker::RightBack],
    &[JocSpeaker::LeftSurround, JocSpeaker::RightSurround],
    &[JocSpeaker::Lfe],
    &[JocSpeaker::Center],
    &[JocSpeaker::Left, JocSpeaker::Right],
];

/// Speaker labels of the `nonstd_bed_channel_assignment[]` array
/// elements (Table 13), by array index.
const NONSTANDARD_BED_LABELS: [JocSpeaker; 17] = [
    JocSpeaker::Lfe, // RC_LFE2
    JocSpeaker::RightWide,
    JocSpeaker::LeftWide,
    JocSpeaker::TopBackRight,
    JocSpeaker::TopBackLeft,
    JocSpeaker::TopSideRight,
    JocSpeaker::TopSideLeft,
    JocSpeaker::TopFrontRight,
    JocSpeaker::TopFrontLeft,
    JocSpeaker::RightBack,
    JocSpeaker::LeftBack,
    JocSpeaker::RightSurround,
    JocSpeaker::LeftSurround,
    JocSpeaker::Lfe,
    JocSpeaker::Center,
    JocSpeaker::Right,
    JocSpeaker::Left,
];

type ProgramAssignment = (Vec<OamdObjectKind>, Vec<Option<JocSpeaker>>);

fn parse_program_assignment(
    bits: &mut BitCursor<'_>,
    object_count: usize,
) -> Result<ProgramAssignment, OamdParseError> {
    let dynamic_only = bits.read_flag("OAMD dynamic-only-program flag")?;
    let mut kinds = Vec::with_capacity(object_count);
    let mut labels = Vec::with_capacity(object_count);
    if dynamic_only {
        if bits.read_flag("OAMD LFE-present flag")? {
            // TS 103 420 places the optional LFE first and excludes it from
            // the JOC matrix; deployed EC-3/JOC streams rely on this ordering.
            kinds.push(OamdObjectKind::Lfe);
        }
        kinds.resize(object_count, OamdObjectKind::Dynamic);
        labels.resize(object_count, None);
        return Ok((kinds, labels));
    }

    let mut push_bed = |kinds: &mut Vec<OamdObjectKind>, speaker: JocSpeaker| {
        kinds.push(if speaker == JocSpeaker::Lfe {
            OamdObjectKind::Lfe
        } else {
            OamdObjectKind::Bed
        });
        labels.push(Some(speaker));
    };
    let content = bits.read(4, "OAMD content description")? as u8;
    if content & 0b1000 != 0 {
        let _distribute = bits.read_flag("OAMD bed-channel-distribution flag")?;
        let multiple_beds = bits.read_flag("OAMD multiple-bed-instances flag")?;
        let bed_count = if multiple_beds {
            bits.read(3, "OAMD bed-instance count")? as usize + 2
        } else {
            1
        };
        for _ in 0..bed_count {
            if bits.read_flag("OAMD LFE-only-bed flag")? {
                push_bed(&mut kinds, JocSpeaker::Lfe);
                continue;
            }
            // Tables 12 / 13: the labels are listed in array-element
            // order starting with element 0.
            if bits.read_flag("OAMD standard-bed-assignment flag")? {
                let assignment = bits.read(10, "OAMD standard-bed assignment")?;
                for (index, speakers) in STANDARD_BED_LABELS.iter().enumerate() {
                    if assignment & (1 << index) != 0 {
                        for &speaker in *speakers {
                            push_bed(&mut kinds, speaker);
                        }
                    }
                }
            } else {
                let assignment = bits.read(17, "OAMD nonstandard-bed assignment")?;
                for (index, &speaker) in NONSTANDARD_BED_LABELS.iter().enumerate() {
                    if assignment & (1 << index) != 0 {
                        push_bed(&mut kinds, speaker);
                    }
                }
            }
        }
    }
    if content & 0b0100 != 0 {
        let format = bits.read(3, "OAMD intermediate-spatial-format index")? as usize;
        let count = *[4_usize, 8, 10, 14, 15, 30]
            .get(format)
            .ok_or(OamdParseError::Invalid(
                "OAMD intermediate-spatial-format index",
            ))?;
        kinds.extend(std::iter::repeat_n(OamdObjectKind::Isf, count));
    }
    if content & 0b0010 != 0 {
        let mut dynamic_count = bits.read(5, "OAMD dynamic-object count")?;
        if dynamic_count == 0x1f {
            dynamic_count = dynamic_count
                .checked_add(bits.read(7, "extended OAMD dynamic-object count")?)
                .ok_or(OamdParseError::Limit("OAMD dynamic-object count"))?;
        }
        let count = usize::try_from(dynamic_count)
            .ok()
            .and_then(|count| count.checked_add(1))
            .ok_or(OamdParseError::Limit("OAMD dynamic-object count"))?;
        kinds.extend(std::iter::repeat_n(OamdObjectKind::Dynamic, count));
    }
    if content & 1 != 0 {
        let reserved_bytes = bits.read(4, "OAMD reserved-data size")? as usize + 1;
        bits.skip(reserved_bytes * 8, "OAMD program-assignment reserved data")?;
    }
    if kinds.len() != object_count {
        return Err(OamdParseError::Invalid(
            "OAMD program-assignment object count",
        ));
    }
    labels.resize(object_count, None);
    Ok((kinds, labels))
}

fn parse_object_element(
    bits: &mut BitCursor<'_>,
    kinds: &[OamdObjectKind],
    starting_objects: &[OamdObjectState],
) -> Result<Vec<OamdUpdateBlock>, OamdParseError> {
    let sample_offset = match bits.read(2, "OAMD sample-offset code")? {
        0 => 0,
        1 => [8_u16, 16, 18, 24][bits.read(2, "OAMD sample-offset index")? as usize],
        2 => bits.read(5, "OAMD sample offset")? as u16,
        _ => return Err(OamdParseError::Invalid("OAMD sample-offset code")),
    };
    let block_count = bits.read(3, "OAMD update-block count")? as usize + 1;
    let mut timings = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        let block_offset = bits.read(6, "OAMD block-offset factor")? as u16 * 32;
        let ramp_duration = match bits.read(2, "OAMD ramp-duration code")? {
            0 => 0,
            1 => 512,
            2 => 1536,
            _ if bits.read_flag("OAMD indexed-ramp-duration flag")? => {
                const DURATIONS: [u16; 16] = [
                    32, 64, 128, 256, 320, 480, 1000, 1001, 1024, 1600, 1601, 1602, 1920, 2000,
                    2002, 2048,
                ];
                DURATIONS[bits.read(4, "OAMD ramp-duration index")? as usize]
            }
            _ => bits.read(11, "OAMD ramp duration")? as u16,
        };
        timings.push(OamdTiming {
            start_sample: sample_offset
                .checked_add(block_offset)
                .ok_or(OamdParseError::Limit("OAMD update start sample"))?,
            ramp_duration,
        });
    }

    if !bits.read_flag("OAMD reserved-data-not-present flag")? {
        let reserved = bits.read(5, "OAMD object-element reserved data")?;
        if reserved != 0 {
            return Err(OamdParseError::Invalid("OAMD object-element reserved data"));
        }
    }

    let mut updates: Vec<OamdUpdateBlock> = timings
        .into_iter()
        .map(|timing| OamdUpdateBlock {
            timing,
            objects: starting_objects.to_vec(),
        })
        .collect();
    let mut previous_object_gain = vec![Some(0.0_f32); block_count];

    for (object, &kind) in kinds.iter().enumerate() {
        for block in 0..block_count {
            let mut state = if block == 0 {
                starting_objects[object].clone()
            } else {
                updates[block - 1].objects[object].clone()
            };
            let inactive = bits.read_flag("OAMD object-not-active flag")?;
            if inactive {
                state.active = false;
                state.gain_db = None;
                state.priority = 0.0;
            } else {
                state.active = true;
                let basic_status = if block == 0 {
                    1
                } else {
                    bits.read(2, "OAMD basic-info status")? as u8
                };
                parse_basic_info(bits, basic_status, &mut state, previous_object_gain[block])?;

                if !kind.has_fixed_render_info() {
                    let render_status = if block == 0 {
                        1
                    } else {
                        bits.read(2, "OAMD render-info status")? as u8
                    };
                    parse_render_info(bits, render_status, block, &mut state)?;
                }
            }
            previous_object_gain[block] = state.gain_db;

            if bits.read_flag("OAMD additional-table-data flag")? {
                let bytes = bits.read(4, "OAMD additional-table-data size")? as usize + 1;
                bits.skip(bytes * 8, "OAMD additional table data")?;
            }
            updates[block].objects[object] = state;
        }
    }
    Ok(updates)
}

fn parse_basic_info(
    bits: &mut BitCursor<'_>,
    status: u8,
    state: &mut OamdObjectState,
    previous_object_gain: Option<f32>,
) -> Result<(), OamdParseError> {
    let flags = match status {
        0 => {
            state.gain_db = None;
            state.priority = 0.0;
            return Ok(());
        }
        1 => 0b11,
        2 => return Ok(()),
        3 => bits.read(2, "OAMD mixed-basic-info flags")? as u8,
        _ => unreachable!("two-bit status"),
    };
    if flags & 0b10 != 0 {
        state.gain_db = match bits.read(2, "OAMD object-gain index")? {
            0 => Some(0.0),
            1 => None,
            2 => {
                let word = bits.read(6, "OAMD object gain")? as i16;
                Some(f32::from(if word <= 14 { 15 - word } else { 14 - word }))
            }
            3 => previous_object_gain.or(Some(0.0)),
            _ => unreachable!("two-bit index"),
        };
    }
    if flags & 1 != 0 {
        state.priority = if bits.read_flag("OAMD default-object-priority flag")? {
            1.0
        } else {
            f32::from(bits.read(5, "OAMD object priority")? as u8) / 32.0
        };
    }
    Ok(())
}

fn parse_render_info(
    bits: &mut BitCursor<'_>,
    status: u8,
    block: usize,
    state: &mut OamdObjectState,
) -> Result<(), OamdParseError> {
    let flags = match status {
        0 => {
            reset_render_info(state);
            return Ok(());
        }
        1 => 0b1111,
        2 => return Ok(()),
        3 => bits.read(4, "OAMD mixed-render-info flags")? as u8,
        _ => unreachable!("two-bit status"),
    };

    if flags & 0b1000 != 0 {
        let differential = block != 0 && bits.read_flag("OAMD differential-position flag")?;
        if differential {
            let x = state.raw_position[0] + read_signed(bits, 3, "OAMD differential X")?;
            let y = state.raw_position[1] + read_signed(bits, 3, "OAMD differential Y")?;
            let z = state.raw_position[2] + read_signed(bits, 3, "OAMD differential Z")?;
            state.raw_position = [x.clamp(0, 62), y.clamp(0, 62), z.clamp(-15, 15)];
        } else {
            let x = bits.read(6, "OAMD position X")? as i16;
            let y = bits.read(6, "OAMD position Y")? as i16;
            let sign = if bits.read_flag("OAMD position-Z sign")? {
                1
            } else {
                -1
            };
            let z = sign * bits.read(4, "OAMD position Z")? as i16;
            state.raw_position = [x.min(62), y.min(62), z.clamp(-15, 15)];
        }
        state.position = [
            f32::from(state.raw_position[0]) / 62.0,
            f32::from(state.raw_position[1]) / 62.0,
            f32::from(state.raw_position[2]) / 15.0,
        ];
        if bits.read_flag("OAMD object-distance-specified flag")?
            && !bits.read_flag("OAMD object-at-infinity flag")?
        {
            let _distance = bits.read(4, "OAMD distance-factor index")?;
        }
    }
    if flags & 0b0100 != 0 {
        state.zone_constraints = bits.read(3, "OAMD zone-constraints index")? as u8;
        if state.zone_constraints > 5 {
            return Err(OamdParseError::Invalid("OAMD zone-constraints index"));
        }
        state.elevation_enabled = bits.read_flag("OAMD elevation-enabled flag")?;
    }
    if flags & 0b0010 != 0 {
        state.size = match bits.read(2, "OAMD object-size index")? {
            0 => [0.0; 3],
            1 => {
                let size = f32::from(bits.read(5, "OAMD object size")? as u8) / 31.0;
                [size; 3]
            }
            2 => [
                f32::from(bits.read(5, "OAMD object width")? as u8) / 31.0,
                f32::from(bits.read(5, "OAMD object depth")? as u8) / 31.0,
                f32::from(bits.read(5, "OAMD object height")? as u8) / 31.0,
            ],
            _ => return Err(OamdParseError::Invalid("OAMD object-size index")),
        };
    }
    if flags & 1 != 0 {
        state.screen_reference = bits.read_flag("OAMD screen-reference flag")?;
        if state.screen_reference {
            let _screen_factor = bits.read(3, "OAMD screen factor")?;
            let _depth_factor = bits.read(2, "OAMD depth-factor index")?;
        }
    }
    state.channel_lock = bits.read_flag("OAMD object-snap flag")?;
    Ok(())
}

fn reset_render_info(state: &mut OamdObjectState) {
    state.position = [0.5, 0.5, 0.0];
    state.raw_position = [31, 31, 0];
    state.size = [0.0; 3];
    state.zone_constraints = 0;
    state.elevation_enabled = true;
    state.screen_reference = false;
    state.channel_lock = false;
}

fn parse_trim_element(bits: &mut BitCursor<'_>, object_count: usize) -> Result<(), OamdParseError> {
    let warp_mode = bits.read(2, "OAMD trim warp mode")?;
    if warp_mode > 1 {
        return Err(OamdParseError::Invalid("OAMD trim warp mode"));
    }
    if bits.read(2, "OAMD trim reserved bits")? != 0 {
        return Err(OamdParseError::Invalid("OAMD trim reserved bits"));
    }
    let global_mode = bits.read(2, "OAMD global-trim mode")?;
    if global_mode == 2 {
        return Err(OamdParseError::Unsupported(
            "custom OAMD trim configuration",
        ));
    }
    if global_mode == 3 {
        return Err(OamdParseError::Invalid("OAMD global-trim mode"));
    }
    if bits.read_flag("OAMD per-object-trim-disable flag")? {
        bits.skip(object_count, "OAMD per-object trim-disable flags")?;
    }
    Ok(())
}

fn read_signed(
    bits: &mut BitCursor<'_>,
    width: usize,
    context: &'static str,
) -> Result<i16, OamdParseError> {
    let value = bits.read(width, context)? as i16;
    let sign_bit = 1_i16 << (width - 1);
    Ok(if value & sign_bit != 0 {
        value - (1_i16 << width)
    } else {
        value
    })
}

fn read_variable_bits_max(
    bits: &mut BitCursor<'_>,
    width: usize,
    context: &'static str,
) -> Result<u32, OamdParseError> {
    let mut value = bits.read(width, context)?;
    let mut groups = 1;
    let mut more = bits.read_flag(context)?;
    while more {
        if groups >= MAX_VARIABLE_BITS_GROUPS {
            return Err(OamdParseError::Limit(context));
        }
        value = value
            .checked_shl(width as u32)
            .and_then(|shifted| shifted.checked_add(1 << width))
            .and_then(|shifted| shifted.checked_add(bits.read(width, context).ok()?))
            .ok_or(OamdParseError::Limit(context))?;
        groups += 1;
        more = bits.read_flag(context)?;
    }
    Ok(value)
}

#[derive(Clone, Copy)]
struct BitCursor<'a> {
    data: &'a [u8],
    position: usize,
    limit: usize,
}

impl<'a> BitCursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            position: 0,
            limit: data.len() * 8,
        }
    }

    fn read(&mut self, width: usize, context: &'static str) -> Result<u32, OamdParseError> {
        if width > 32 {
            return Err(OamdParseError::Limit(context));
        }
        let end = self
            .position
            .checked_add(width)
            .ok_or(OamdParseError::Limit(context))?;
        if end > self.limit {
            return Err(OamdParseError::Truncated(context));
        }
        let mut value = 0_u32;
        for bit in self.position..end {
            value = (value << 1) | u32::from((self.data[bit / 8] >> (7 - bit % 8)) & 1);
        }
        self.position = end;
        Ok(value)
    }

    fn read_flag(&mut self, context: &'static str) -> Result<bool, OamdParseError> {
        Ok(self.read(1, context)? != 0)
    }

    fn skip(&mut self, width: usize, context: &'static str) -> Result<(), OamdParseError> {
        let end = self
            .position
            .checked_add(width)
            .ok_or(OamdParseError::Limit(context))?;
        if end > self.limit {
            return Err(OamdParseError::Truncated(context));
        }
        self.position = end;
        Ok(())
    }

    fn consume_zero_padding(&mut self, context: &'static str) -> Result<(), OamdParseError> {
        while self.position < self.limit {
            if self.read(1, context)? != 0 {
                return Err(OamdParseError::Invalid(context));
            }
        }
        Ok(())
    }
}

/// Synthetic OAMD payloads: the writer behind the unit tests of the
/// parser and renderer, the renderer throughput example and the
/// structure-aware fuzz harness (no JOC bitstream is available to the
/// project, so object metadata has to be made here).
// internal — exposed for tests/fuzz/benches; not part of the stable API
#[doc(hidden)]
pub mod synth {
    use super::OBJECT_ELEMENT_ID;

    /// Render information of one dynamic test object (one update block,
    /// zero-dB gain).
    #[derive(Clone, Copy, Debug)]
    pub struct TestObject {
        /// Raw `pos3D_X/Y` (0..=62) and signed `pos3D_Z` (−15..=15).
        pub position: [i16; 3],
        /// `zone_constraints_idx` (0..=5).
        pub zone_constraints: u32,
        /// `b_enable_elevation`.
        pub elevation: bool,
        /// `object_size_bits` (0..=31) for all three axes.
        pub size: u32,
        /// `b_object_snap` (channel lock).
        pub snap: bool,
    }

    impl TestObject {
        /// A point object at `position` without constraints.
        pub fn at(position: [i16; 3]) -> Self {
            Self {
                position,
                zone_constraints: 0,
                elevation: true,
                size: 0,
                snap: false,
            }
        }
    }

    /// The program assignment of a test payload.
    #[derive(Clone, Copy, Debug)]
    pub enum TestProgram {
        /// `b_dyn_object_only_program` with an optional leading LFE.
        Dynamic { lfe: bool },
        /// One standard bed (`bed_channel_assignment[]` mask) followed by
        /// the dynamic objects.
        Bed { assignment: u32, objects: usize },
    }

    /// A one-update-block payload: `fixed` objects without render
    /// information (LFE / bed) followed by `objects`, all ramping over
    /// `ramp_code` (0 = none, 1 = 512, 2 = 1536 samples).
    pub fn payload(
        program: TestProgram,
        fixed: usize,
        objects: &[TestObject],
        ramp_code: u32,
    ) -> Vec<u8> {
        // The unescaped 5-bit count fields carry at most 31 objects.
        let fixed = fixed.min(31);
        let objects = &objects[..objects.len().min(31 - fixed)];
        if fixed + objects.len() == 0 {
            return Vec::new();
        }
        let mut object = TestBitWriter::default();
        object.write(0, 2); // sample offset zero
        object.write(0, 3); // one update block
        object.write(0, 6); // block offset zero
        object.write(ramp_code, 2);
        object.write(1, 1); // reserved data absent
        for _ in 0..fixed {
            write_basic_info(&mut object);
            object.write(0, 1); // no additional table data
        }
        for test in objects {
            write_basic_info(&mut object);
            object.write(u32::from(test.position[0].unsigned_abs()), 6);
            object.write(u32::from(test.position[1].unsigned_abs()), 6);
            object.write(u32::from(test.position[2] >= 0), 1);
            object.write(u32::from(test.position[2].unsigned_abs()), 4);
            object.write(0, 1); // no distance
            object.write(test.zone_constraints, 3);
            object.write(u32::from(test.elevation), 1);
            if test.size == 0 {
                object.write(0, 2); // point object
            } else {
                object.write(1, 2);
                object.write(test.size, 5);
            }
            object.write(0, 1); // room coordinates
            object.write(u32::from(test.snap), 1);
            object.write(0, 1); // no additional table data
        }
        let object_data = object.into_bytes();

        let mut payload = TestBitWriter::default();
        payload.write(0, 2); // version zero
        let count = fixed + objects.len();
        payload.write(count as u32 - 1, 5);
        match program {
            TestProgram::Dynamic { lfe } => {
                payload.write(1, 1); // dynamic-only program
                payload.write(u32::from(lfe), 1);
            }
            TestProgram::Bed {
                assignment,
                objects,
            } => {
                payload.write(0, 1);
                payload.write(if objects > 0 { 0b1010 } else { 0b1000 }, 4);
                payload.write(0, 1); // no bed-channel distribution
                payload.write(0, 1); // one bed instance
                payload.write(0, 1); // not LFE-only
                payload.write(1, 1); // standard assignment
                payload.write(assignment, 10);
                if objects > 0 {
                    payload.write((objects.clamp(1, 31) - 1) as u32, 5);
                }
            }
        }
        payload.write(0, 1); // no alternate data
        payload.write(1, 4); // one element
        payload.write(u32::from(OBJECT_ELEMENT_ID), 4);
        // The element is one byte longer than the object data: the
        // discard flag, the data, seven bits of zero padding.
        payload.write_variable(object_data.len() as u32);
        payload.write(0, 1); // retain unknown element
        for byte in &object_data {
            payload.write(u32::from(*byte), 8);
        }
        payload.write(0, 7);
        payload.into_bytes()
    }

    fn write_basic_info(bits: &mut TestBitWriter) {
        bits.write(0, 1); // active
        bits.write(0, 2); // zero-dB gain
        bits.write(1, 1); // default priority
    }

    #[derive(Default)]
    struct TestBitWriter {
        bits: Vec<bool>,
    }

    impl TestBitWriter {
        fn write(&mut self, value: u32, width: usize) {
            for shift in (0..width).rev() {
                self.bits.push((value >> shift) & 1 != 0);
            }
        }

        /// `variable_bits_max(4, ..)` (clause 5.5.1), one or two groups.
        fn write_variable(&mut self, value: u32) {
            if value < 16 {
                self.write(value, 4);
                self.write(0, 1);
            } else {
                // At most 32 objects of at most 35 bits: well under 272.
                let rest = (value - 16).min(255);
                self.write(rest >> 4, 4);
                self.write(1, 1);
                self.write(rest & 15, 4);
                self.write(0, 1);
            }
        }

        fn into_bytes(mut self) -> Vec<u8> {
            while !self.bits.len().is_multiple_of(8) {
                self.bits.push(false);
            }
            self.bits
                .as_chunks::<8>()
                .0
                .iter()
                .map(|byte| {
                    byte.iter()
                        .fold(0_u8, |value, bit| (value << 1) | u8::from(*bit))
                })
                .collect()
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // Synthetic integer coordinates have exact decoded values.
mod tests {
    use super::*;

    #[test]
    fn parses_dynamic_objects_with_leading_lfe_and_full_updates() {
        let payload = make_dynamic_test_payload();
        let mut decoder = OamdDecoder::default();
        let frame = decoder.decode(&payload).expect("decode synthetic OAMD");

        assert_eq!(frame.kinds.len(), 3);
        assert_eq!(frame.kinds[0], OamdObjectKind::Lfe);
        assert_eq!(frame.kinds[1..], [OamdObjectKind::Dynamic; 2]);
        assert_eq!(frame.joc_object_indices().collect::<Vec<_>>(), [1, 2]);
        assert_eq!(frame.updates.len(), 1);
        assert_eq!(frame.updates[0].timing.start_sample, 0);
        assert_eq!(frame.updates[0].timing.ramp_duration, 1536);
        assert_eq!(frame.updates[0].objects[1].position, [0.0, 0.5, 0.0]);
        assert_eq!(frame.updates[0].objects[2].position, [1.0, 0.5, 0.0]);
        assert_eq!(frame.updates[0].objects[1].linear_gain(), 1.0);
    }

    /// Table 12: a standard bed lists its labels in array-element order
    /// and every bed object is anchored at its speaker.
    #[test]
    fn standard_bed_assignment_yields_speaker_labels_and_positions() {
        use super::synth::{TestObject, TestProgram, payload};
        // Elements 9 (L/R), 8 (C), 7 (LFE), 6 (Ls/Rs) and 4 (Tfl/Tfr).
        let assignment = 0b11_1101_0000;
        let payload = payload(
            TestProgram::Bed {
                assignment,
                objects: 1,
            },
            8,
            &[TestObject::at([62, 0, 15])],
            0,
        );
        let mut decoder = OamdDecoder::default();
        let frame = decoder.decode(&payload).expect("decode bed OAMD");
        let labels: Vec<_> = frame.labels.iter().flatten().copied().collect();
        assert_eq!(
            labels,
            [
                JocSpeaker::TopFrontLeft,
                JocSpeaker::TopFrontRight,
                JocSpeaker::LeftSurround,
                JocSpeaker::RightSurround,
                JocSpeaker::Lfe,
                JocSpeaker::Center,
                JocSpeaker::Left,
                JocSpeaker::Right,
            ]
        );
        assert_eq!(frame.kinds[4], OamdObjectKind::Lfe);
        assert_eq!(frame.kinds[8], OamdObjectKind::Dynamic);
        assert_eq!(frame.labels[8], None);
        assert_eq!(frame.joc_object_indices().count(), 8);
        let last = &frame.updates[0].objects;
        assert_eq!(last[0].position, JocSpeaker::TopFrontLeft.room_position());
        assert_eq!(last[5].position, [0.5, 0.0, 0.0]);
        assert_eq!(last[8].position, [1.0, 0.0, 1.0]);
    }

    fn make_dynamic_test_payload() -> Vec<u8> {
        let mut object = TestBitWriter::default();
        object.write(0, 2); // sample offset zero
        object.write(0, 3); // one update block
        object.write(0, 6); // block offset zero
        object.write(2, 2); // 1536-sample ramp
        object.write(1, 1); // reserved data absent

        // Object zero: LFE, with basic information but no render information.
        write_basic_info(&mut object);
        object.write(0, 1); // no additional table data
        for x in [0, 62] {
            write_basic_info(&mut object);
            object.write(x, 6);
            object.write(31, 6);
            object.write(1, 1); // positive Z
            object.write(0, 4);
            object.write(0, 1); // no distance
            object.write(0, 3); // no zone constraints
            object.write(1, 1); // elevation enabled
            object.write(0, 2); // point object
            object.write(0, 1); // room coordinates
            object.write(0, 1); // no channel lock
            object.write(0, 1); // no additional table data
        }
        let object_data = object.into_bytes();

        let mut payload = TestBitWriter::default();
        payload.write(0, 2); // version zero
        payload.write(2, 5); // three OAMD objects
        payload.write(1, 1); // dynamic-only program
        payload.write(1, 1); // leading LFE
        payload.write(0, 1); // no alternate data
        payload.write(1, 4); // one element
        payload.write(u32::from(OBJECT_ELEMENT_ID), 4);
        payload.write_variable(
            u32::try_from(object_data.len()).expect("test element size"),
            4,
        );
        payload.write(0, 1); // retain unknown element
        for byte in object_data {
            payload.write(u32::from(byte), 8);
        }
        payload.write(0, 7); // element padding after the one-bit discard flag
        payload.into_bytes()
    }

    fn write_basic_info(bits: &mut TestBitWriter) {
        bits.write(0, 1); // active
        bits.write(0, 2); // zero-dB gain
        bits.write(1, 1); // default priority
    }

    #[derive(Default)]
    struct TestBitWriter {
        bits: Vec<bool>,
    }

    impl TestBitWriter {
        fn write(&mut self, value: u32, width: usize) {
            for shift in (0..width).rev() {
                self.bits.push((value >> shift) & 1 != 0);
            }
        }

        fn write_variable(&mut self, value: u32, width: usize) {
            assert!(value < 1 << width);
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
                .map(|byte| {
                    byte.iter()
                        .fold(0_u8, |value, bit| (value << 1) | u8::from(*bit))
                })
                .collect()
        }
    }
}
