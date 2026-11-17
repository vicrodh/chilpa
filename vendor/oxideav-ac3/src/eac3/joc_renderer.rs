//! Speaker renderer for reconstructed E-AC-3 JOC objects.
//!
//! ETSI TS 103 420 specifies how to recover object essences and their
//! rendering metadata, but deliberately leaves adaptation to a playback
//! speaker layout to the renderer. Two policies live here:
//!
//! * [`JocLayout::Stereo`] — an equal-power stereo pan: the room/screen X
//!   coordinate pans between L/R and object width spreads a point source
//!   toward both speakers;
//! * every other [`JocLayout`] — the layered equal-power balance panner
//!   of [`super::joc_layout`], driven by the full OAMD object state
//!   (position, size, channel lock, zone constraints) and evaluated on a
//!   32-sample grid with linear interpolation in between.
//!
//! Bed objects (speaker-anchored, clause 5.2.1.4) play from their own
//! speaker when the layout has it and are panned at the speaker's
//! Table B.10 position otherwise.

use std::{f32::consts::FRAC_PI_2, fmt};

use super::{
    chanmap::ChannelLocation,
    joc::JocMetadata,
    joc_layout::{JocLayout, JocSpeaker, MAX_LAYOUT_CHANNELS, PanSource, Panner},
    joc_qmf::{
        Complex32, QMF_RECONSTRUCTION_DELAY, QMF_SUBBANDS, QMF_TIMESLOTS_PER_EAC3_FRAME,
        QmfAnalysis, QmfSynthesis,
    },
    joc_reconstruct::{JocMatrixDecoder, JocReconstructionError},
    oamd::{OamdDecoder, OamdFrame, OamdObjectKind, OamdObjectState, OamdParseError},
};

const SAMPLES_PER_JOC_FRAME: usize = QMF_SUBBANDS * QMF_TIMESLOTS_PER_EAC3_FRAME;
const MAX_JOC_OBJECTS: usize = 16;
const LFE_STEREO_GAIN: f32 = 0.5;
const DIFFUSE_STEREO_GAIN: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// Spacing of the panning-gain grid of the multichannel layouts; OAMD
/// update blocks start on multiples of 32 samples (clause 5.6.2.5).
const GAIN_GRID: usize = 32;
const GAIN_KNOTS: usize = SAMPLES_PER_JOC_FRAME / GAIN_GRID + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// internal — exposed for tests/fuzz; not part of the stable API
#[doc(hidden)]
pub enum JocRenderError {
    Invalid(&'static str),
    Unsupported(&'static str),
    Matrix(JocReconstructionError),
    Oamd(OamdParseError),
}

impl fmt::Display for JocRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(context) => write!(formatter, "invalid {context}"),
            Self::Unsupported(context) => write!(formatter, "unsupported {context}"),
            Self::Matrix(error) => write!(formatter, "JOC matrix: {error}"),
            Self::Oamd(error) => write!(formatter, "OAMD: {error}"),
        }
    }
}

impl From<JocReconstructionError> for JocRenderError {
    fn from(error: JocReconstructionError) -> Self {
        Self::Matrix(error)
    }
}

impl From<OamdParseError> for JocRenderError {
    fn from(error: OamdParseError) -> Self {
        Self::Oamd(error)
    }
}

// internal — exposed for tests/fuzz; not part of the stable API
#[doc(hidden)]
pub struct JocRenderer {
    panner: Panner,
    matrices: JocMatrixDecoder,
    oamd: OamdDecoder,
    analyses: Vec<QmfAnalysis>,
    syntheses: Vec<QmfSynthesis>,
    /// `[object][delay slot][output channel]` panning gains, delayed to
    /// line up with the QMF reconstruction delay.
    gain_delay: Vec<f32>,
    lfe_delay: Vec<f32>,
    delay_position: usize,
    shape: Option<(usize, usize)>,
    last_sequence: Option<u16>,
}

impl Default for JocRenderer {
    fn default() -> Self {
        Self::with_layout(JocLayout::Stereo)
    }
}

impl JocRenderer {
    /// A renderer for `layout`.
    #[doc(hidden)]
    pub fn with_layout(layout: JocLayout) -> Self {
        Self {
            panner: Panner::new(layout),
            matrices: JocMatrixDecoder::default(),
            oamd: OamdDecoder::default(),
            analyses: Vec::new(),
            syntheses: Vec::new(),
            gain_delay: Vec::new(),
            lfe_delay: vec![0.0; QMF_RECONSTRUCTION_DELAY],
            delay_position: 0,
            shape: None,
            last_sequence: None,
        }
    }

    /// The layout this renderer produces.
    #[doc(hidden)]
    pub fn layout(&self) -> JocLayout {
        self.panner.layout()
    }

    #[doc(hidden)]
    pub fn reset(&mut self) {
        self.matrices.reset();
        self.oamd.reset();
        self.analyses.clear();
        self.syntheses.clear();
        self.gain_delay.clear();
        self.lfe_delay.fill(0.0);
        self.delay_position = 0;
        self.shape = None;
        self.last_sequence = None;
    }

    /// Render one 1,536-sample, bitstream-order E-AC-3 frame to interleaved
    /// stereo. Only valid on a [`JocLayout::Stereo`] renderer.
    #[doc(hidden)]
    pub fn render_stereo(
        &mut self,
        metadata: &JocMetadata,
        pcm: &[f32],
        source_channels: usize,
        acmod: u8,
        lfeon: bool,
    ) -> Result<Vec<f32>, JocRenderError> {
        if self.layout() != JocLayout::Stereo {
            return Err(JocRenderError::Invalid("JOC renderer layout"));
        }
        self.render(metadata, pcm, source_channels, acmod, lfeon)
    }

    /// Render one 1,536-sample, bitstream-order E-AC-3 frame to the
    /// renderer's layout, interleaved in [`JocLayout::speakers`] order.
    /// For the currently standardized five-channel JOC configurations the
    /// input (bitstream) order is `L, C, R, Ls, Rs, (LFE)`.
    #[allow(clippy::too_many_lines)]
    #[doc(hidden)]
    pub fn render(
        &mut self,
        metadata: &JocMetadata,
        pcm: &[f32],
        source_channels: usize,
        acmod: u8,
        lfeon: bool,
    ) -> Result<Vec<f32>, JocRenderError> {
        if metadata.num_channels != 5 || !matches!(metadata.downmix_config, 0 | 3) {
            return Err(JocRenderError::Unsupported(
                "non-5.X JOC downmix configuration",
            ));
        }
        if acmod != 7 {
            return Err(JocRenderError::Unsupported(
                "JOC base presentation other than 3/2",
            ));
        }
        let expected_channels = 5 + usize::from(lfeon);
        if source_channels != expected_channels {
            return Err(JocRenderError::Invalid("JOC source channel count"));
        }
        if pcm.len() != SAMPLES_PER_JOC_FRAME * source_channels {
            return Err(JocRenderError::Invalid("JOC PCM frame length"));
        }

        let num_objects = usize::from(metadata.num_objects);
        let num_channels = usize::from(metadata.num_channels);
        if !(1..=MAX_JOC_OBJECTS).contains(&num_objects) {
            return Err(JocRenderError::Invalid("JOC object count"));
        }

        let shape = (num_channels, num_objects);
        let expected_sequence = self
            .last_sequence
            .map(|sequence| if sequence == 1023 { 1 } else { sequence + 1 });
        if metadata.sequence_count != 0
            && (expected_sequence != Some(metadata.sequence_count) || self.shape != Some(shape))
        {
            self.reset();
            return Err(JocRenderError::Unsupported(
                "JOC sequence without an initialization frame",
            ));
        }
        if metadata.sequence_count == 0 {
            self.reset();
        }
        self.ensure_shape(shape);
        self.last_sequence = Some(metadata.sequence_count);

        let oamd = self.oamd.decode(&metadata.oamd_payload)?;
        if oamd
            .kinds
            .iter()
            .any(|kind| matches!(kind, OamdObjectKind::Isf))
        {
            // The intermediate spatial format's stacked-ring geometry is
            // not defined by TS 103 420.
            return Err(JocRenderError::Unsupported(
                "intermediate-spatial-format OAMD program assignment",
            ));
        }
        let object_indices: Vec<_> = oamd.joc_object_indices().collect();
        if object_indices.len() != num_objects
            || oamd.starting_objects.len() != oamd.kinds.len()
            || oamd.labels.len() != oamd.kinds.len()
            || oamd
                .updates
                .iter()
                .any(|update| update.objects.len() != oamd.kinds.len())
        {
            return Err(JocRenderError::Invalid("OAMD/JOC object mapping"));
        }
        if oamd
            .updates
            .windows(2)
            .any(|updates| updates[0].timing.start_sample > updates[1].timing.start_sample)
        {
            return Err(JocRenderError::Invalid("OAMD update ordering"));
        }
        validate_oamd_timing(&oamd)?;

        let matrices = self.matrices.interpolate(metadata)?;
        if matrices.num_channels() != num_channels || matrices.num_objects() != num_objects {
            return Err(JocRenderError::Invalid("JOC matrix shape"));
        }

        let mut object_pcm = vec![0.0_f32; num_objects * SAMPLES_PER_JOC_FRAME];
        let mut inputs = vec![[Complex32::default(); QMF_SUBBANDS]; num_channels];
        for timeslot in 0..QMF_TIMESLOTS_PER_EAC3_FRAME {
            for (channel, input) in inputs.iter_mut().enumerate() {
                let source_channel = joc_to_bitstream_channel(channel);
                let mut block = [0.0_f32; QMF_SUBBANDS];
                for (sample, value) in block.iter_mut().enumerate() {
                    let frame = timeslot * QMF_SUBBANDS + sample;
                    *value = pcm[frame * source_channels + source_channel];
                }
                *input = self.analyses[channel].process(&block);
            }

            for (object, synthesis) in self.syntheses.iter_mut().enumerate() {
                // Each subband sums its channels in ascending order; the
                // channel loop is outermost so the subband loop runs over
                // independent accumulators.
                let mut reconstructed = [Complex32::default(); QMF_SUBBANDS];
                for (channel, input) in inputs.iter().enumerate() {
                    let coefficients = matrices.subbands(object, timeslot, channel);
                    for ((value, sample), &coefficient) in
                        reconstructed.iter_mut().zip(input).zip(coefficients)
                    {
                        value.re = sample.re.mul_add(coefficient, value.re);
                        value.im = sample.im.mul_add(coefficient, value.im);
                    }
                }
                if reconstructed.iter().all(|sample| {
                    sample.re.abs() <= f32::EPSILON && sample.im.abs() <= f32::EPSILON
                }) && synthesis.is_silent()
                {
                    continue;
                }
                let output = synthesis.process(&reconstructed);
                let start = object * SAMPLES_PER_JOC_FRAME + timeslot * QMF_SUBBANDS;
                object_pcm[start..start + QMF_SUBBANDS].copy_from_slice(&output);
            }
        }

        let channels = self.panner.channels();
        let stereo = self.layout() == JocLayout::Stereo;
        // Multichannel layouts: panning gains on the 32-sample grid.
        let mut knots = Vec::new();
        if !stereo {
            knots.resize(num_objects * GAIN_KNOTS * channels, 0.0_f32);
            for (object, &oamd_index) in object_indices.iter().enumerate() {
                for knot in 0..GAIN_KNOTS {
                    let gains = layout_gains_at(&self.panner, &oamd, oamd_index, knot * GAIN_GRID);
                    let start = (object * GAIN_KNOTS + knot) * channels;
                    knots[start..start + channels].copy_from_slice(&gains[..channels]);
                }
            }
        }

        type RampKey = (*const OamdObjectState, *const OamdObjectState, u32);
        let mut stereo_memo: Vec<(Option<RampKey>, [f32; 2])> = vec![(None, [0.0; 2]); num_objects];
        let mut output = vec![0.0_f32; SAMPLES_PER_JOC_FRAME * channels];
        for (sample, mixed) in output.chunks_exact_mut(channels).enumerate() {
            let delay = self.delay_position;
            for (object, &oamd_index) in object_indices.iter().enumerate() {
                let mut current_gain = [0.0_f32; MAX_LAYOUT_CHANNELS];
                if stereo {
                    // The gains are a pure function of the ramp the sample
                    // falls in and its position there: outside a ramp
                    // they repeat sample after sample.
                    let (from, to, alpha) = ramp_at(&oamd, oamd_index, sample);
                    let key = (
                        std::ptr::from_ref(from),
                        std::ptr::from_ref(to),
                        alpha.to_bits(),
                    );
                    let memo = &mut stereo_memo[object];
                    if memo.0 != Some(key) {
                        *memo = (Some(key), stereo_gains(from, to, alpha));
                    }
                    current_gain[..2].copy_from_slice(&memo.1);
                } else {
                    let knot = sample / GAIN_GRID;
                    let fraction = (sample % GAIN_GRID) as f32 / GAIN_GRID as f32;
                    let start = (object * GAIN_KNOTS + knot) * channels;
                    let (from, to) = knots[start..start + 2 * channels].split_at(channels);
                    for ((gain, &from), &to) in current_gain.iter_mut().zip(from).zip(to) {
                        *gain = from + (to - from) * fraction;
                    }
                }
                let delay_index = (object * QMF_RECONSTRUCTION_DELAY + delay) * channels;
                let delayed = &mut self.gain_delay[delay_index..delay_index + channels];
                let value = object_pcm[object * SAMPLES_PER_JOC_FRAME + sample];
                for ((out, delayed_gain), &gain) in
                    mixed.iter_mut().zip(delayed.iter_mut()).zip(&current_gain)
                {
                    *out = value.mul_add(*delayed_gain, *out);
                    *delayed_gain = gain;
                }
            }

            if lfeon {
                // E-AC-3 keeps LFE after every full-bandwidth channel in
                // bitstream order, so acmod 3/2 + LFE uses slot 5.
                let current_lfe = pcm[sample * source_channels + source_channels - 1];
                let delayed_lfe = self.lfe_delay[delay];
                self.lfe_delay[delay] = current_lfe;
                if stereo {
                    mixed[0] = delayed_lfe.mul_add(LFE_STEREO_GAIN, mixed[0]);
                    mixed[1] = delayed_lfe.mul_add(LFE_STEREO_GAIN, mixed[1]);
                } else if let Some(lfe_channel) = self.panner.lfe_channel() {
                    mixed[lfe_channel] += delayed_lfe;
                }
            }

            self.delay_position += 1;
            if self.delay_position == QMF_RECONSTRUCTION_DELAY {
                self.delay_position = 0;
            }
        }
        Ok(output)
    }

    /// Route a decoded channel program (no object metadata) to the
    /// renderer's layout: every coded channel is treated as a bed object
    /// at its own speaker. `pcm` is interleaved in bitstream order — the
    /// `acmod` channels, the LFE when `lfeon`, then one channel per
    /// entry of `extra` (dependent-substream extensions).
    #[doc(hidden)]
    pub fn render_channel_bed(
        &self,
        pcm: &[f32],
        source_channels: usize,
        acmod: u8,
        lfeon: bool,
        extra: &[ChannelLocation],
    ) -> Vec<f32> {
        let channels = self.panner.channels();
        if source_channels == 0 {
            return Vec::new();
        }
        let frames = pcm.len() / source_channels;
        let mut sources: Vec<BedSource> = match acmod & 7 {
            0 | 2 => vec![
                BedSource::Speaker(JocSpeaker::Left),
                BedSource::Speaker(JocSpeaker::Right),
            ],
            1 => vec![BedSource::Speaker(JocSpeaker::Center)],
            3 => vec![
                BedSource::Speaker(JocSpeaker::Left),
                BedSource::Speaker(JocSpeaker::Center),
                BedSource::Speaker(JocSpeaker::Right),
            ],
            4 => vec![
                BedSource::Speaker(JocSpeaker::Left),
                BedSource::Speaker(JocSpeaker::Right),
                BedSource::Position(REAR_CENTRE),
            ],
            5 => vec![
                BedSource::Speaker(JocSpeaker::Left),
                BedSource::Speaker(JocSpeaker::Center),
                BedSource::Speaker(JocSpeaker::Right),
                BedSource::Position(REAR_CENTRE),
            ],
            6 => vec![
                BedSource::Speaker(JocSpeaker::Left),
                BedSource::Speaker(JocSpeaker::Right),
                BedSource::Speaker(JocSpeaker::LeftSurround),
                BedSource::Speaker(JocSpeaker::RightSurround),
            ],
            _ => vec![
                BedSource::Speaker(JocSpeaker::Left),
                BedSource::Speaker(JocSpeaker::Center),
                BedSource::Speaker(JocSpeaker::Right),
                BedSource::Speaker(JocSpeaker::LeftSurround),
                BedSource::Speaker(JocSpeaker::RightSurround),
            ],
        };
        if lfeon {
            sources.push(BedSource::Speaker(JocSpeaker::Lfe));
        }
        sources.extend(
            extra
                .iter()
                .map(|&location| BedSource::from_location(location)),
        );
        sources.truncate(source_channels);

        let matrix: Vec<[f32; MAX_LAYOUT_CHANNELS]> = sources
            .iter()
            .map(|source| source.gains(&self.panner))
            .collect();
        let mut output = vec![0.0_f32; frames * channels];
        for (input, mixed) in pcm
            .chunks_exact(source_channels)
            .zip(output.chunks_exact_mut(channels))
        {
            for (&value, gains) in input.iter().zip(&matrix) {
                for (out, &gain) in mixed.iter_mut().zip(gains) {
                    *out = value.mul_add(gain, *out);
                }
            }
        }
        output
    }

    fn ensure_shape(&mut self, shape: (usize, usize)) {
        if self.shape == Some(shape) {
            return;
        }
        let (num_channels, num_objects) = shape;
        self.analyses = vec![QmfAnalysis::default(); num_channels];
        self.syntheses = vec![QmfSynthesis::default(); num_objects];
        self.gain_delay =
            vec![0.0; num_objects * QMF_RECONSTRUCTION_DELAY * self.panner.channels()];
        self.lfe_delay.fill(0.0);
        self.delay_position = 0;
        self.shape = Some(shape);
    }
}

fn validate_oamd_timing(frame: &OamdFrame) -> Result<(), JocRenderError> {
    let mut previous_end = 0_usize;
    for update in &frame.updates {
        let start = usize::from(update.timing.start_sample);
        let end = start
            .checked_add(usize::from(update.timing.ramp_duration))
            .ok_or(JocRenderError::Invalid("OAMD ramp timing"))?;
        if start < previous_end {
            return Err(JocRenderError::Unsupported("overlapping OAMD ramps"));
        }
        if start > SAMPLES_PER_JOC_FRAME || end > SAMPLES_PER_JOC_FRAME {
            return Err(JocRenderError::Unsupported("cross-frame OAMD ramp"));
        }
        previous_end = end;
    }
    Ok(())
}

fn joc_to_bitstream_channel(channel: usize) -> usize {
    // TS 103 420 JOC order is L, R, C, Ls, Rs. The E-AC-3 acmod=7
    // decoder scratch is in bitstream order L, C, R, Ls, Rs, (LFE).
    match channel {
        0 => 0,
        1 => 2,
        2 => 1,
        3 => 3,
        4 => 4,
        _ => channel,
    }
}

/// Room position of a single rear-centre source (mono surround, Cs).
const REAR_CENTRE: [f32; 3] = [0.5, 1.0, 0.0];

/// Where a coded channel of a channel-based program plays.
#[derive(Clone, Copy, Debug)]
enum BedSource {
    Speaker(JocSpeaker),
    Position([f32; 3]),
}

impl BedSource {
    /// Table E2.5 channel locations. The locations with a TS 103 420
    /// speaker label map to it; the others are placed in the room by
    /// their name (this renderer's choice).
    fn from_location(location: ChannelLocation) -> Self {
        match location {
            ChannelLocation::Left => Self::Speaker(JocSpeaker::Left),
            ChannelLocation::Center => Self::Speaker(JocSpeaker::Center),
            ChannelLocation::Right => Self::Speaker(JocSpeaker::Right),
            ChannelLocation::LeftSurround | ChannelLocation::LeftSurroundDirect => {
                Self::Speaker(JocSpeaker::LeftSurround)
            }
            ChannelLocation::RightSurround | ChannelLocation::RightSurroundDirect => {
                Self::Speaker(JocSpeaker::RightSurround)
            }
            ChannelLocation::LeftCenter => Self::Position([0.25, 0.0, 0.0]),
            ChannelLocation::RightCenter => Self::Position([0.75, 0.0, 0.0]),
            ChannelLocation::LeftRearSurround => Self::Speaker(JocSpeaker::LeftBack),
            ChannelLocation::RightRearSurround => Self::Speaker(JocSpeaker::RightBack),
            ChannelLocation::CenterSurround => Self::Position(REAR_CENTRE),
            ChannelLocation::TopSurround => Self::Position([0.5, 0.5, 1.0]),
            ChannelLocation::LeftWide => Self::Speaker(JocSpeaker::LeftWide),
            ChannelLocation::RightWide => Self::Speaker(JocSpeaker::RightWide),
            ChannelLocation::VerticalHeightLeft => Self::Speaker(JocSpeaker::TopFrontLeft),
            ChannelLocation::VerticalHeightRight => Self::Speaker(JocSpeaker::TopFrontRight),
            ChannelLocation::VerticalHeightCenter => Self::Position([0.5, 0.0, 1.0]),
            ChannelLocation::TopSurroundLeft => Self::Speaker(JocSpeaker::TopSideLeft),
            ChannelLocation::TopSurroundRight => Self::Speaker(JocSpeaker::TopSideRight),
            ChannelLocation::Lfe2 | ChannelLocation::Lfe => Self::Speaker(JocSpeaker::Lfe),
        }
    }

    fn gains(self, panner: &Panner) -> [f32; MAX_LAYOUT_CHANNELS] {
        let mut gains = [0.0; MAX_LAYOUT_CHANNELS];
        match self {
            Self::Speaker(speaker) => speaker_gains(panner, speaker, &mut gains),
            Self::Position(position) => panner.gains(&PanSource::point(position), &mut gains),
        }
        gains
    }
}

/// A speaker-anchored source: its own speaker when the layout has it,
/// a point source at the speaker's room position otherwise. An LFE
/// source is dropped by a layout without an LFE channel.
fn speaker_gains(panner: &Panner, speaker: JocSpeaker, gains: &mut [f32; MAX_LAYOUT_CHANNELS]) {
    if let Some(channel) = panner.channel_of(speaker) {
        gains.fill(0.0);
        gains[channel] = 1.0;
    } else if speaker == JocSpeaker::Lfe {
        gains.fill(0.0);
    } else {
        panner.gains(&PanSource::point(speaker.room_position()), gains);
    }
}

/// The OAMD ramp `sample` falls in: the states it runs between and the
/// position inside it.
fn ramp_at(
    frame: &OamdFrame,
    object: usize,
    sample: usize,
) -> (&OamdObjectState, &OamdObjectState, f32) {
    let mut from = &frame.starting_objects[object];
    let mut to = from;
    let mut alpha = 1.0_f32;
    for update in &frame.updates {
        let start = usize::from(update.timing.start_sample);
        if sample < start {
            break;
        }
        to = &update.objects[object];
        let duration = update.timing.ramp_duration;
        alpha = if duration == 0 {
            1.0
        } else {
            let elapsed = u16::try_from(sample - start)
                .unwrap_or(u16::MAX)
                .min(duration);
            f32::from(elapsed) / f32::from(duration)
        };
        if alpha < 1.0 {
            break;
        }
        from = to;
    }
    (from, to, alpha)
}

/// Panning gains of OAMD object `object` at `sample` over the channels of
/// a multichannel layout, object gain included.
fn layout_gains_at(
    panner: &Panner,
    frame: &OamdFrame,
    object: usize,
    sample: usize,
) -> [f32; MAX_LAYOUT_CHANNELS] {
    let (from, to, alpha) = ramp_at(frame, object, sample);
    let alpha = alpha.clamp(0.0, 1.0);
    let from_gain = if from.active { from.linear_gain() } else { 0.0 };
    let to_gain = if to.active { to.linear_gain() } else { 0.0 };
    let gain = from_gain + (to_gain - from_gain) * alpha;
    let mut gains = [0.0; MAX_LAYOUT_CHANNELS];
    if gain == 0.0 || !gain.is_finite() {
        return gains;
    }
    match (frame.kinds[object], frame.labels[object]) {
        (OamdObjectKind::Bed, Some(speaker)) => speaker_gains(panner, speaker, &mut gains),
        _ => panner.gains(&PanSource::interpolated(from, to, alpha), &mut gains),
    }
    for value in &mut gains {
        *value *= gain;
    }
    gains
}

fn stereo_gains(from: &OamdObjectState, to: &OamdObjectState, alpha: f32) -> [f32; 2] {
    let alpha = alpha.clamp(0.0, 1.0);
    let from_gain = if from.active { from.linear_gain() } else { 0.0 };
    let to_gain = if to.active { to.linear_gain() } else { 0.0 };
    let gain = from_gain + (to_gain - from_gain) * alpha;
    if gain == 0.0 {
        return [0.0; 2];
    }

    let x = (from.position[0] + (to.position[0] - from.position[0]) * alpha).clamp(0.0, 1.0);
    let width = (from.size[0] + (to.size[0] - from.size[0]) * alpha).clamp(0.0, 1.0);
    let mut left = (x * FRAC_PI_2).cos();
    let mut right = (x * FRAC_PI_2).sin();

    // Width one approaches an equal diffuse pair; width zero remains a
    // point source. Renormalize so spreading does not change energy.
    left += (DIFFUSE_STEREO_GAIN - left) * width;
    right += (DIFFUSE_STEREO_GAIN - right) * width;
    let norm = left.hypot(right);
    if norm > 0.0 {
        left /= norm;
        right /= norm;
    }
    [left * gain, right * gain]
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // Endpoint panning has exact values.
mod tests {
    use super::*;

    fn object(x: f32, gain_db: Option<f32>, width: f32) -> OamdObjectState {
        OamdObjectState {
            active: true,
            gain_db,
            priority: 1.0,
            position: [x, 0.0, 0.0],
            size: [width, 0.0, 0.0],
            screen_reference: false,
            channel_lock: false,
            zone_constraints: 0,
            elevation_enabled: true,
            raw_position: [0; 3],
        }
    }

    #[test]
    fn point_pan_is_equal_power() {
        let left = stereo_gains(
            &object(0.0, Some(0.0), 0.0),
            &object(0.0, Some(0.0), 0.0),
            1.0,
        );
        let center = stereo_gains(
            &object(0.5, Some(0.0), 0.0),
            &object(0.5, Some(0.0), 0.0),
            1.0,
        );
        let right = stereo_gains(
            &object(1.0, Some(0.0), 0.0),
            &object(1.0, Some(0.0), 0.0),
            1.0,
        );
        assert_eq!(left, [1.0, 0.0]);
        assert!((center[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1.0e-6);
        assert!((center[1] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1.0e-6);
        assert!(right[0].abs() < 1.0e-6);
        assert!((right[1] - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn gain_and_position_ramp_together() {
        let from = object(0.0, None, 0.0);
        let to = object(1.0, Some(0.0), 0.0);
        let midpoint = stereo_gains(&from, &to, 0.5);
        let expected = 0.5 * std::f32::consts::FRAC_1_SQRT_2;
        assert!((midpoint[0] - expected).abs() < 1.0e-6);
        assert!((midpoint[1] - expected).abs() < 1.0e-6);
    }

    #[test]
    fn maps_joc_order_to_eac3_bitstream_order() {
        assert_eq!(
            (0..5).map(joc_to_bitstream_channel).collect::<Vec<_>>(),
            [0, 2, 1, 3, 4]
        );
    }

    #[test]
    fn accepts_frame_local_oamd_ramps_and_rejects_ambiguous_timing() {
        let mut frame = OamdFrame {
            kinds: Vec::new(),
            labels: Vec::new(),
            starting_objects: Vec::new(),
            updates: vec![super::super::oamd::OamdUpdateBlock {
                timing: super::super::oamd::OamdTiming {
                    start_sample: 0,
                    ramp_duration: 1536,
                },
                objects: Vec::new(),
            }],
        };
        assert_eq!(validate_oamd_timing(&frame), Ok(()));

        frame.updates[0].timing.start_sample = 1;
        assert!(matches!(
            validate_oamd_timing(&frame),
            Err(JocRenderError::Unsupported("cross-frame OAMD ramp"))
        ));

        frame.updates[0].timing = super::super::oamd::OamdTiming {
            start_sample: 0,
            ramp_duration: 1000,
        };
        frame.updates.push(super::super::oamd::OamdUpdateBlock {
            timing: super::super::oamd::OamdTiming {
                start_sample: 512,
                ramp_duration: 0,
            },
            objects: Vec::new(),
        });
        assert!(matches!(
            validate_oamd_timing(&frame),
            Err(JocRenderError::Unsupported("overlapping OAMD ramps"))
        ));
    }

    // --- Layout rendering ------------------------------------------------

    use super::super::oamd::synth::{TestObject, TestProgram, payload};

    const FRAME: usize = SAMPLES_PER_JOC_FRAME;

    /// JOC metadata reconstructing object `n` from JOC channel
    /// `routing[n]` alone (order L, R, C, Ls, Rs) with unit gain.
    fn routed_metadata(sequence: u16, oamd_payload: &[u8], routing: &[usize]) -> JocMetadata {
        JocMetadata::synthetic(sequence, oamd_payload.to_vec(), routing)
    }

    /// Bitstream-order (L, C, R, Ls, Rs, LFE) test program: one tone per
    /// channel, continuous across frames.
    fn program_sample(channel: usize, time: usize) -> f32 {
        let frequency = [440.0, 660.0, 1000.0, 1500.0, 2300.0, 60.0][channel];
        let phase = 2.0 * std::f32::consts::PI * frequency * time as f32 / 48_000.0;
        0.25 * phase.sin()
    }

    fn program_frame(frame: usize, silent: &[usize]) -> Vec<f32> {
        let mut pcm = vec![0.0; FRAME * 6];
        for sample in 0..FRAME {
            for channel in 0..6 {
                if !silent.contains(&channel) {
                    pcm[sample * 6 + channel] = program_sample(channel, frame * FRAME + sample);
                }
            }
        }
        pcm
    }

    /// Render `frames` frames and return the last one.
    fn render_frames(
        layout: JocLayout,
        oamd_payload: &[u8],
        routing: &[usize],
        silent: &[usize],
        frames: usize,
    ) -> Vec<f32> {
        let mut renderer = JocRenderer::with_layout(layout);
        let mut last = Vec::new();
        for frame in 0..frames {
            let metadata = routed_metadata(frame as u16, oamd_payload, routing);
            last = renderer
                .render(&metadata, &program_frame(frame, silent), 6, 7, true)
                .expect("render frame");
            assert_eq!(last.len(), FRAME * usize::from(layout.channel_count()));
        }
        last
    }

    /// Residual of output channel `out` of the frame `frame` against the
    /// delayed bitstream channel `source` (or silence), relative to the
    /// source tone's power.
    fn residual(
        rendered: &[f32],
        channels: usize,
        out: usize,
        source: Option<(usize, f32)>,
        frame: usize,
    ) -> f32 {
        let mut error = 0.0_f32;
        for sample in 0..FRAME {
            let time = frame * FRAME + sample;
            let want = source.map_or(0.0, |(channel, gain)| {
                gain * program_sample(channel, time - QMF_RECONSTRUCTION_DELAY)
            });
            let got = rendered[sample * channels + out];
            error += (got - want) * (got - want);
        }
        // Mean power of a 0.25-amplitude tone.
        error / (FRAME as f32 * 0.25 * 0.25 * 0.5)
    }

    fn channel_of(layout: JocLayout, speaker: JocSpeaker) -> usize {
        layout
            .speakers()
            .iter()
            .position(|&candidate| candidate == speaker)
            .expect("speaker in layout")
    }

    /// Two dynamic objects on speaker positions of a 7.1.4 layout: each
    /// object's essence comes out of exactly that speaker, the LFE is
    /// bypassed to the LFE channel, everything else stays silent.
    #[test]
    fn dynamic_objects_reach_their_speakers_in_7_1_4() {
        let layout = JocLayout::Surround714;
        let oamd = payload(
            TestProgram::Dynamic { lfe: true },
            1,
            &[
                TestObject::at([0, 0, 15]),  // top front left
                TestObject::at([62, 62, 0]), // right back
            ],
            0,
        );
        // Object 0 <- L (JOC channel 0), object 1 <- R (JOC channel 1).
        // The other bitstream channels carry tones the matrix ignores.
        let rendered = render_frames(layout, &oamd, &[0, 1], &[], 4);
        let channels = usize::from(layout.channel_count());
        let expect = [
            (JocSpeaker::TopFrontLeft, 0), // bitstream L
            (JocSpeaker::RightBack, 2),    // bitstream R
            (JocSpeaker::Lfe, 5),
        ];
        for (index, &speaker) in layout.speakers().iter().enumerate() {
            let source = expect
                .iter()
                .find(|(want, _)| *want == speaker)
                .map(|&(_, channel)| (channel, 1.0));
            let error = residual(&rendered, channels, index, source, 3);
            assert!(error < 1.0e-3, "{speaker}: residual {error}");
        }
    }

    /// The same object lands on different speakers as the layout shrinks.
    #[test]
    fn one_object_across_every_layout_keeps_its_power() {
        let oamd = payload(
            TestProgram::Dynamic { lfe: true },
            1,
            &[TestObject::at([0, 31, 15])], // left wall, mid depth, ceiling
            0,
        );
        for layout in JocLayout::ALL {
            let rendered = render_frames(layout, &oamd, &[0], &[5], 4);
            let channels = usize::from(layout.channel_count());
            let mut power = vec![0.0_f32; channels];
            for frame in rendered.chunks_exact(channels) {
                for (sum, value) in power.iter_mut().zip(frame) {
                    *sum += value * value;
                }
            }
            let total: f32 = power.iter().sum::<f32>() / (FRAME as f32 * 0.25 * 0.25 * 0.5);
            assert!((total - 1.0).abs() < 0.02, "{layout}: total power {total}");
            let loudest = power
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(index, _)| layout.speakers()[index])
                .expect("channels");
            let want: &[JocSpeaker] = match layout {
                JocLayout::Stereo => &[JocSpeaker::Left],
                JocLayout::Surround51 | JocLayout::Surround71 => {
                    &[JocSpeaker::Left, JocSpeaker::LeftSurround]
                }
                JocLayout::Surround512 | JocLayout::Surround712 | JocLayout::Surround916 => {
                    &[JocSpeaker::TopSideLeft]
                }
                _ => &[JocSpeaker::TopFrontLeft, JocSpeaker::TopBackLeft],
            };
            assert!(want.contains(&loudest), "{layout}: loudest {loudest}");
            // Nothing on the right half of the room.
            for (index, speaker) in layout.speakers().iter().enumerate() {
                if speaker.room_position()[0] > 0.5 && *speaker != JocSpeaker::Lfe {
                    assert!(
                        power[index] < 1.0e-3,
                        "{layout}: {speaker} {}",
                        power[index]
                    );
                }
            }
        }
    }

    /// A 5.1 bed program is the identity on a 5.1 layout and keeps every
    /// bed channel on its own speaker of a larger one.
    #[test]
    fn bed_objects_play_from_their_own_speakers() {
        // Elements 9 (L/R), 8 (C), 7 (LFE), 6 (Ls/Rs): objects in order
        // Ls, Rs, LFE, C, L, R — the JOC objects are the five non-LFE ones.
        let oamd = payload(
            TestProgram::Bed {
                assignment: 0b11_1100_0000,
                objects: 0,
            },
            6,
            &[],
            0,
        );
        // JOC channel order L, R, C, Ls, Rs.
        let routing = [3, 4, 2, 0, 1];
        // Bitstream channel feeding each speaker.
        let expect = [
            (JocSpeaker::Left, 0),
            (JocSpeaker::Center, 1),
            (JocSpeaker::Right, 2),
            (JocSpeaker::LeftSurround, 3),
            (JocSpeaker::RightSurround, 4),
            (JocSpeaker::Lfe, 5),
        ];
        for layout in [JocLayout::Surround51, JocLayout::Surround916] {
            let rendered = render_frames(layout, &oamd, &routing, &[], 4);
            let channels = usize::from(layout.channel_count());
            for (index, &speaker) in layout.speakers().iter().enumerate() {
                let source = expect
                    .iter()
                    .find(|(want, _)| *want == speaker)
                    .map(|&(_, channel)| (channel, 1.0));
                let error = residual(&rendered, channels, index, source, 3);
                assert!(error < 1.0e-3, "{layout} {speaker}: residual {error}");
            }
        }

        // Stereo: bed objects pan by the X of their speaker.
        let rendered = render_frames(JocLayout::Stereo, &oamd, &routing, &[1, 2, 3, 4, 5], 4);
        assert!(residual(&rendered, 2, 0, Some((0, 1.0)), 3) < 1.0e-3);
        assert!(residual(&rendered, 2, 1, None, 3) < 1.0e-3);
    }

    /// Channel lock and zone constraints reach the panner.
    #[test]
    fn channel_lock_and_zone_constraints_steer_the_object() {
        let layout = JocLayout::Surround714;
        let channels = usize::from(layout.channel_count());
        let mut object = TestObject::at([12, 4, 0]); // near the left front
        object.snap = true;
        let oamd = payload(TestProgram::Dynamic { lfe: true }, 1, &[object], 0);
        let rendered = render_frames(layout, &oamd, &[0], &[5], 4);
        let left = channel_of(layout, JocSpeaker::Left);
        assert!(residual(&rendered, channels, left, Some((0, 1.0)), 3) < 1.0e-3);

        // Screen zone only, elevation off: a source at the back top
        // collapses onto the front right speaker.
        let mut object = TestObject::at([62, 62, 15]);
        object.zone_constraints = 4;
        object.elevation = false;
        let oamd = payload(TestProgram::Dynamic { lfe: true }, 1, &[object], 0);
        let rendered = render_frames(layout, &oamd, &[0], &[5], 4);
        let right = channel_of(layout, JocSpeaker::Right);
        assert!(residual(&rendered, channels, right, Some((0, 1.0)), 3) < 1.0e-3);
    }

    /// A frame-long OAMD ramp moves the object smoothly: the total power
    /// stays put while the energy crosses from one speaker to the other.
    #[test]
    fn position_ramp_crossfades_between_speakers() {
        let layout = JocLayout::Surround51;
        let channels = usize::from(layout.channel_count());
        let left = payload(
            TestProgram::Dynamic { lfe: true },
            1,
            &[TestObject::at([0, 0, 0])],
            0,
        );
        let right = payload(
            TestProgram::Dynamic { lfe: true },
            1,
            &[TestObject::at([62, 0, 0])],
            2, // 1536-sample ramp
        );
        let mut renderer = JocRenderer::with_layout(layout);
        let mut frames = Vec::new();
        for frame in 0..6 {
            let oamd = if frame < 3 { &left } else { &right };
            let metadata = routed_metadata(frame as u16, oamd, &[0]);
            frames.push(
                renderer
                    .render(&metadata, &program_frame(frame, &[5]), 6, 7, true)
                    .expect("render"),
            );
        }
        let power_of = |frame: &[f32], out: usize, range: std::ops::Range<usize>| -> f32 {
            range
                .map(|sample| frame[sample * channels + out].powi(2))
                .sum()
        };
        let (l, c, r) = (
            channel_of(layout, JocSpeaker::Left),
            channel_of(layout, JocSpeaker::Center),
            channel_of(layout, JocSpeaker::Right),
        );
        // Before the ramp: all left. Two frames later: all right.
        assert!(power_of(&frames[2], l, 0..FRAME) > 20.0);
        assert!(power_of(&frames[2], r, 0..FRAME) < 1.0e-3);
        assert!(power_of(&frames[5], r, 0..FRAME) > 20.0);
        assert!(power_of(&frames[5], l, 0..FRAME) < 1.0e-3);
        // During the ramp (delayed by the QMF latency) the centre speaker
        // carries the object as it passes the middle of the screen.
        let middle = QMF_RECONSTRUCTION_DELAY + FRAME / 2;
        let window = middle - 64..middle + 64;
        let centre = power_of(&frames[3], c, window.clone());
        assert!(centre > 10.0 * power_of(&frames[3], l, window.clone()));
        assert!(centre > 10.0 * power_of(&frames[3], r, window));
        // No discontinuity: the largest sample-to-sample step of the
        // ramp frame stays in the range of the tone itself.
        let tone_step = 0.25 * 2.0 * std::f32::consts::PI * 440.0 / 48_000.0;
        for out in [l, c, r] {
            for sample in 1..FRAME {
                let step = (frames[3][sample * channels + out]
                    - frames[3][(sample - 1) * channels + out])
                    .abs();
                assert!(
                    step < 2.0 * tone_step,
                    "channel {out} sample {sample}: {step}"
                );
            }
        }
    }

    /// `render_stereo` is the stereo renderer's entry point only, and is
    /// the general entry point on a stereo renderer.
    #[test]
    fn stereo_only_entry_point_rejects_other_layouts() {
        let oamd = payload(
            TestProgram::Dynamic { lfe: true },
            1,
            &[TestObject::at([0, 0, 0])],
            0,
        );
        let metadata = routed_metadata(0, &oamd, &[0]);
        let pcm = program_frame(0, &[]);
        let mut renderer = JocRenderer::with_layout(JocLayout::Surround51);
        assert!(matches!(
            renderer.render_stereo(&metadata, &pcm, 6, 7, true),
            Err(JocRenderError::Invalid("JOC renderer layout"))
        ));
        let mut stereo = JocRenderer::default();
        let by_layout = JocRenderer::with_layout(JocLayout::Stereo)
            .render(&metadata, &pcm, 6, 7, true)
            .expect("render");
        assert_eq!(
            stereo.render_stereo(&metadata, &pcm, 6, 7, true),
            Ok(by_layout)
        );
    }

    /// Corrupted OAMD payloads (bit flips of valid dynamic and bed
    /// programs) either fail to parse or render to finite samples of
    /// the right shape, in every layout.
    #[test]
    fn corrupted_oamd_payloads_never_break_the_renderer() {
        let mut object = TestObject::at([40, 9, -7]);
        object.size = 19;
        object.zone_constraints = 3;
        let seeds = [
            payload(
                TestProgram::Dynamic { lfe: true },
                1,
                &[object, TestObject::at([3, 60, 15])],
                2,
            ),
            payload(
                TestProgram::Bed {
                    assignment: 0b11_1111_0110,
                    objects: 2,
                },
                14,
                &[object, TestObject::at([62, 0, 0])],
                1,
            ),
        ];
        let mut state = 0x9e37_79b9_u32;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let pcm = program_frame(0, &[]);
        let mut rendered = 0;
        for round in 0..160 {
            let mut bytes = seeds[round % seeds.len()].clone();
            for _ in 0..(round % 4) {
                let bit = next() as usize % (bytes.len() * 8);
                bytes[bit / 8] ^= 1 << (bit % 8);
            }
            let layout = JocLayout::ALL[round % JocLayout::ALL.len()];
            let channels = usize::from(layout.channel_count());
            for objects in [2_usize, 15] {
                let routing: Vec<usize> = (0..objects).map(|object| object % 5).collect();
                let mut renderer = JocRenderer::with_layout(layout);
                for sequence in 0..2 {
                    let metadata = routed_metadata(sequence, &bytes, &routing);
                    if let Ok(samples) = renderer.render(&metadata, &pcm, 6, 7, true) {
                        assert_eq!(samples.len(), FRAME * channels);
                        assert!(samples.iter().all(|sample| sample.is_finite()));
                        rendered += 1;
                    }
                }
            }
        }
        // The uncorrupted rounds alone render.
        assert!(rendered >= 80, "only {rendered} renders succeeded");
    }

    /// A channel program without objects is routed channel by channel.
    #[test]
    fn channel_bed_routing_places_coded_channels() {
        let renderer = JocRenderer::with_layout(JocLayout::Surround714);
        let layout = renderer.layout();
        let channels = usize::from(layout.channel_count());
        // One frame of 3/2 + LFE, value = channel index + 1.
        let pcm: Vec<f32> = (0..6).map(|channel| channel as f32 + 1.0).collect();
        let out = renderer.render_channel_bed(&pcm, 6, 7, true, &[]);
        assert_eq!(out.len(), channels);
        let at = |speaker| out[channel_of(layout, speaker)];
        assert_eq!(at(JocSpeaker::Left), 1.0);
        assert_eq!(at(JocSpeaker::Center), 2.0);
        assert_eq!(at(JocSpeaker::Right), 3.0);
        assert_eq!(at(JocSpeaker::LeftSurround), 4.0);
        assert_eq!(at(JocSpeaker::RightSurround), 5.0);
        assert_eq!(at(JocSpeaker::Lfe), 6.0);
        assert_eq!(at(JocSpeaker::LeftBack), 0.0);
        assert_eq!(at(JocSpeaker::TopFrontLeft), 0.0);

        // A 7.1 program (5.1 + Lrs/Rrs extension) fills the back pair.
        let pcm: Vec<f32> = (0..8).map(|channel| channel as f32 + 1.0).collect();
        let out = renderer.render_channel_bed(
            &pcm,
            8,
            7,
            true,
            &[
                ChannelLocation::LeftRearSurround,
                ChannelLocation::RightRearSurround,
            ],
        );
        assert_eq!(out[channel_of(layout, JocSpeaker::LeftBack)], 7.0);
        assert_eq!(out[channel_of(layout, JocSpeaker::RightBack)], 8.0);
        // On 5.1 the same back pair folds onto the surround pair.
        let small = JocRenderer::with_layout(JocLayout::Surround51);
        let out = small.render_channel_bed(
            &pcm,
            8,
            7,
            true,
            &[
                ChannelLocation::LeftRearSurround,
                ChannelLocation::RightRearSurround,
            ],
        );
        assert_eq!(
            out[channel_of(JocLayout::Surround51, JocSpeaker::LeftSurround)],
            4.0 + 7.0
        );

        // 3/1: the mono surround splits with equal power.
        let pcm = [0.0, 0.0, 0.0, 1.0];
        let out = small.render_channel_bed(&pcm, 4, 5, false, &[]);
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let ls = out[channel_of(JocLayout::Surround51, JocSpeaker::LeftSurround)];
        let rs = out[channel_of(JocLayout::Surround51, JocSpeaker::RightSurround)];
        assert!((ls - half).abs() < 1.0e-6 && (rs - half).abs() < 1.0e-6);
        // Hostile shapes do not panic.
        assert!(small.render_channel_bed(&[], 0, 7, true, &[]).is_empty());
        let _ = small.render_channel_bed(&[1.0; 7], 3, 7, true, &[]);
        let _ = small.render_channel_bed(&[1.0; 40], 20, 0, false, &[]);
    }
}
