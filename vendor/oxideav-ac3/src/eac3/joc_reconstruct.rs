//! Stateful interpolation of decoded JOC reconstruction matrices.
//!
//! This implements ETSI TS 103 420 V1.2.1 clauses 6.5 and 6.6.5.

use std::fmt;

use super::{
    joc::{JocMetadata, JocObjectUpdate},
    joc_qmf::{QMF_SUBBANDS, QMF_TIMESLOTS_PER_EAC3_FRAME},
};

const MAX_JOC_OBJECTS: usize = 16;
const MAX_JOC_CHANNELS: usize = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// internal — exposed for tests/fuzz; not part of the stable API
#[doc(hidden)]
pub enum JocReconstructionError {
    Invalid(&'static str),
    Limit(&'static str),
}

impl fmt::Display for JocReconstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(context) => write!(formatter, "invalid {context}"),
            Self::Limit(context) => write!(formatter, "{context} exceeds reconstruction limit"),
        }
    }
}

/// One E-AC-3 frame of interpolated reconstruction coefficients, stored in
/// `[object][timeslot][channel][subband]` order.
pub(super) struct JocMatrixFrame {
    num_objects: usize,
    num_channels: usize,
    values: Vec<f32>,
}

impl JocMatrixFrame {
    pub(super) fn num_objects(&self) -> usize {
        self.num_objects
    }

    pub(super) fn num_channels(&self) -> usize {
        self.num_channels
    }

    #[cfg(test)]
    pub(super) fn coefficient(
        &self,
        object: usize,
        timeslot: usize,
        channel: usize,
        subband: usize,
    ) -> f32 {
        self.values[self.index(object, timeslot, channel, subband)]
    }

    /// The coefficients of one object / timeslot / channel over the 64
    /// subbands.
    pub(super) fn subbands(&self, object: usize, timeslot: usize, channel: usize) -> &[f32] {
        let start = self.index(object, timeslot, channel, 0);
        &self.values[start..start + QMF_SUBBANDS]
    }

    fn index(&self, object: usize, timeslot: usize, channel: usize, subband: usize) -> usize {
        (((object * QMF_TIMESLOTS_PER_EAC3_FRAME + timeslot) * self.num_channels + channel)
            * QMF_SUBBANDS)
            + subband
    }
}

pub(super) struct JocMatrixDecoder {
    previous: Vec<f32>,
    last_sequence: Option<u16>,
    previous_shape: Option<(usize, usize)>,
}

impl Default for JocMatrixDecoder {
    fn default() -> Self {
        Self {
            previous: vec![0.0; MAX_JOC_OBJECTS * MAX_JOC_CHANNELS * QMF_SUBBANDS],
            last_sequence: None,
            previous_shape: None,
        }
    }
}

impl JocMatrixDecoder {
    pub(super) fn reset(&mut self) {
        self.previous.fill(0.0);
        self.last_sequence = None;
        self.previous_shape = None;
    }

    pub(super) fn interpolate(
        &mut self,
        metadata: &JocMetadata,
    ) -> Result<JocMatrixFrame, JocReconstructionError> {
        let num_objects = usize::from(metadata.num_objects);
        let num_channels = usize::from(metadata.num_channels);
        if !(1..=MAX_JOC_OBJECTS).contains(&num_objects) {
            return Err(JocReconstructionError::Limit("JOC object count"));
        }
        if !matches!(num_channels, 5 | 7) {
            return Err(JocReconstructionError::Invalid("JOC channel count"));
        }
        if metadata.object_updates.len() != num_objects {
            return Err(JocReconstructionError::Invalid("JOC object-update count"));
        }

        let expected_sequence = self
            .last_sequence
            .map(|sequence| if sequence == 1023 { 1 } else { sequence + 1 });
        let shape = (num_objects, num_channels);
        if metadata.sequence_count == 0
            || expected_sequence.is_some_and(|expected| expected != metadata.sequence_count)
            || self
                .previous_shape
                .is_some_and(|previous| previous != shape)
        {
            self.previous.fill(0.0);
        }
        self.last_sequence = Some(metadata.sequence_count);
        self.previous_shape = Some(shape);

        let coefficient_count = num_objects
            .checked_mul(QMF_TIMESLOTS_PER_EAC3_FRAME)
            .and_then(|count| count.checked_mul(num_channels))
            .and_then(|count| count.checked_mul(QMF_SUBBANDS))
            .ok_or(JocReconstructionError::Limit(
                "JOC interpolated coefficient count",
            ))?;
        let mut frame = JocMatrixFrame {
            num_objects,
            num_channels,
            values: vec![0.0; coefficient_count],
        };

        for (object, update) in metadata.object_updates.iter().enumerate() {
            match update {
                JocObjectUpdate::Reuse => {
                    for timeslot in 0..QMF_TIMESLOTS_PER_EAC3_FRAME {
                        for channel in 0..num_channels {
                            for subband in 0..QMF_SUBBANDS {
                                let value =
                                    self.previous[Self::previous_index(object, channel, subband)];
                                let index = frame.index(object, timeslot, channel, subband);
                                frame.values[index] = value;
                            }
                        }
                    }
                }
                JocObjectUpdate::Parameters {
                    num_bands,
                    slope,
                    offsets,
                    coefficients,
                } => {
                    let values_per_point = num_channels.checked_mul(*num_bands).ok_or(
                        JocReconstructionError::Limit("JOC data-point coefficient count"),
                    )?;
                    if values_per_point == 0 || coefficients.len() % values_per_point != 0 {
                        return Err(JocReconstructionError::Invalid(
                            "JOC data-point coefficient count",
                        ));
                    }
                    let num_data_points = coefficients.len() / values_per_point;
                    if !matches!(num_data_points, 1 | 2) {
                        return Err(JocReconstructionError::Invalid("JOC data-point count"));
                    }
                    validate_offsets(*slope, offsets, num_data_points)?;

                    for channel in 0..num_channels {
                        for subband in 0..QMF_SUBBANDS {
                            let parameter_band = parameter_band_for_subband(*num_bands, subband)
                                .ok_or(JocReconstructionError::Invalid(
                                    "JOC parameter-band mapping",
                                ))?;
                            let previous_index = Self::previous_index(object, channel, subband);
                            let previous = self.previous[previous_index];
                            let first = coefficients[channel * *num_bands + parameter_band];
                            let second = (num_data_points == 2).then(|| {
                                coefficients
                                    [values_per_point + channel * *num_bands + parameter_band]
                            });

                            for timeslot in 0..QMF_TIMESLOTS_PER_EAC3_FRAME {
                                let value = interpolate_value(
                                    previous, first, second, *slope, offsets, timeslot,
                                );
                                let index = frame.index(object, timeslot, channel, subband);
                                frame.values[index] = value;
                            }
                            self.previous[previous_index] = second.unwrap_or(first);
                        }
                    }
                }
            }
        }

        Ok(frame)
    }

    fn previous_index(object: usize, channel: usize, subband: usize) -> usize {
        ((object * MAX_JOC_CHANNELS + channel) * QMF_SUBBANDS) + subband
    }
}

fn validate_offsets(
    slope: bool,
    offsets: &[u8],
    num_data_points: usize,
) -> Result<(), JocReconstructionError> {
    if !slope {
        if !offsets.is_empty() {
            return Err(JocReconstructionError::Invalid(
                "smooth JOC data-point offsets",
            ));
        }
        return Ok(());
    }
    if offsets.len() != num_data_points
        || offsets
            .iter()
            .any(|&offset| usize::from(offset) > QMF_TIMESLOTS_PER_EAC3_FRAME)
        || offsets.windows(2).any(|pair| pair[0] > pair[1])
    {
        return Err(JocReconstructionError::Invalid(
            "steep JOC data-point offsets",
        ));
    }
    Ok(())
}

fn interpolate_value(
    previous: f32,
    first: f32,
    second: Option<f32>,
    slope: bool,
    offsets: &[u8],
    timeslot: usize,
) -> f32 {
    if slope {
        if timeslot < usize::from(offsets[0]) {
            return previous;
        }
        return match second {
            None => first,
            Some(_) if timeslot < usize::from(offsets[1]) => first,
            Some(second) => second,
        };
    }

    match second {
        None => {
            let ratio = timeslot_ratio(timeslot + 1, QMF_TIMESLOTS_PER_EAC3_FRAME);
            (first - previous).mul_add(ratio, previous)
        }
        Some(second) => {
            let midpoint = QMF_TIMESLOTS_PER_EAC3_FRAME / 2;
            if timeslot < midpoint {
                let ratio = timeslot_ratio(timeslot + 1, midpoint);
                (first - previous).mul_add(ratio, previous)
            } else {
                let ratio = timeslot_ratio(
                    timeslot - midpoint + 1,
                    QMF_TIMESLOTS_PER_EAC3_FRAME - midpoint,
                );
                (second - first).mul_add(ratio, first)
            }
        }
    }
}

fn timeslot_ratio(numerator: usize, denominator: usize) -> f32 {
    let numerator = u8::try_from(numerator).expect("JOC frame has only 24 QMF timeslots");
    let denominator = u8::try_from(denominator).expect("JOC frame has only 24 QMF timeslots");
    f32::from(numerator) / f32::from(denominator)
}

fn parameter_band_for_subband(num_bands: usize, subband: usize) -> Option<usize> {
    if subband >= QMF_SUBBANDS {
        return None;
    }
    let parameter_band = match num_bands {
        1 => 0,
        3 => match subband {
            0..=2 => 0,
            3..=13 => 1,
            _ => 2,
        },
        5 => match subband {
            0 => 0,
            1..=2 => 1,
            3..=8 => 2,
            9..=22 => 3,
            _ => 4,
        },
        7 => match subband {
            0 => 0,
            1 => 1,
            2..=3 => 2,
            4..=7 => 3,
            8..=13 => 4,
            14..=22 => 5,
            _ => 6,
        },
        9 => match subband {
            0 => 0,
            1 => 1,
            2 => 2,
            3..=4 => 3,
            5..=6 => 4,
            7..=8 => 5,
            9..=13 => 6,
            14..=22 => 7,
            _ => 8,
        },
        12 => match subband {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 3,
            4..=5 => 4,
            6..=7 => 5,
            8..=10 => 6,
            11..=13 => 7,
            14..=17 => 8,
            18..=22 => 9,
            23..=34 => 10,
            _ => 11,
        },
        15 => match subband {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 3,
            4 => 4,
            5 => 5,
            6 => 6,
            7 => 7,
            8 => 8,
            9..=10 => 9,
            11..=13 => 10,
            14..=17 => 11,
            18..=22 => 12,
            23..=34 => 13,
            _ => 14,
        },
        23 => match subband {
            0..=11 => subband,
            12..=13 => 12,
            14..=15 => 13,
            16..=17 => 14,
            18..=19 => 15,
            20..=22 => 16,
            23..=25 => 17,
            26..=29 => 18,
            30..=34 => 19,
            35..=40 => 20,
            41..=47 => 21,
            _ => 22,
        },
        _ => return None,
    };
    Some(parameter_band)
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // Tests intentionally exercise exact hold/step cases.
mod tests {
    use super::*;

    fn metadata(sequence_count: u16, update: JocObjectUpdate) -> JocMetadata {
        JocMetadata {
            complexity_index: 1,
            group_id: 0,
            oamd_payload: Vec::new(),
            joc_payload: Vec::new(),
            downmix_config: 0,
            num_objects: 1,
            extension_config: 0,
            sequence_count,
            num_channels: 5,
            clip_gain: 1.0,
            object_updates: vec![update],
        }
    }

    #[test]
    fn smooth_update_reaches_target_and_reuse_holds_it() {
        let mut decoder = JocMatrixDecoder::default();
        let first = decoder
            .interpolate(&metadata(
                0,
                JocObjectUpdate::Parameters {
                    num_bands: 1,
                    slope: false,
                    offsets: Vec::new(),
                    coefficients: vec![1.0, 2.0, 3.0, 4.0, 5.0],
                },
            ))
            .expect("interpolate first frame");
        assert_eq!(first.num_objects(), 1);
        assert_eq!(first.num_channels(), 5);
        assert!((first.coefficient(0, 0, 0, 0) - 1.0 / 24.0).abs() < 1.0e-6);
        assert_eq!(first.coefficient(0, 23, 0, 63), 1.0);
        assert_eq!(first.coefficient(0, 23, 4, 31), 5.0);

        let reused = decoder
            .interpolate(&metadata(1, JocObjectUpdate::Reuse))
            .expect("reuse matrix");
        assert_eq!(reused.coefficient(0, 0, 0, 0), 1.0);
        assert_eq!(reused.coefficient(0, 23, 4, 63), 5.0);
    }

    #[test]
    fn steep_two_point_update_switches_at_signalled_offsets() {
        let mut decoder = JocMatrixDecoder::default();
        let frame = decoder
            .interpolate(&metadata(
                0,
                JocObjectUpdate::Parameters {
                    num_bands: 1,
                    slope: true,
                    offsets: vec![3, 17],
                    coefficients: vec![1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 2.0],
                },
            ))
            .expect("interpolate steep frame");
        assert_eq!(frame.coefficient(0, 2, 0, 0), 0.0);
        assert_eq!(frame.coefficient(0, 3, 0, 0), 1.0);
        assert_eq!(frame.coefficient(0, 16, 0, 0), 1.0);
        assert_eq!(frame.coefficient(0, 17, 0, 0), 2.0);
    }

    #[test]
    fn parameter_band_mapping_matches_table_54_boundaries() {
        assert_eq!(parameter_band_for_subband(12, 0), Some(0));
        assert_eq!(parameter_band_for_subband(12, 5), Some(4));
        assert_eq!(parameter_band_for_subband(12, 22), Some(9));
        assert_eq!(parameter_band_for_subband(12, 23), Some(10));
        assert_eq!(parameter_band_for_subband(12, 63), Some(11));
        assert_eq!(parameter_band_for_subband(23, 47), Some(21));
        assert_eq!(parameter_band_for_subband(23, 48), Some(22));
        assert_eq!(parameter_band_for_subband(2, 0), None);
        assert_eq!(parameter_band_for_subband(12, 64), None);
    }
}
