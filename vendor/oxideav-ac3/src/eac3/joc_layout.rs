//! Target speaker layouts and the amplitude panner for rendered E-AC-3
//! JOC objects.
//!
//! ETSI TS 103 420 specifies how to recover the object essences and
//! their properties (position, size, gain, channel lock, zone
//! constraints — clause 5.2) and stops at the decoder interface of
//! clause 4.4: adapting the objects to a playback speaker layout is left
//! to the renderer. What the specification does fix, and what this
//! module takes from it, is the geometry the properties are expressed
//! in:
//!
//! * the room-anchored coordinate system of clause 4.2.1 (`x` 0 = left
//!   wall … 1 = right wall, `y` 0 = front wall … 1 = back wall, `z`
//!   −1 = floor … 1 = ceiling);
//! * the speaker labels of clause 4.4 with their positions in Table B.10
//!   (given there on a −1 … 1 scale with `Y` = 1 at the front wall; they
//!   are mapped onto the clause 4.2.1 scale here);
//! * the speaker-to-zone assignment of Table A.7 and the zone-constraint
//!   codes of Tables 20 and 21.
//!
//! The panning law itself is this crate's own design — a **layered
//! equal-power balance panner**:
//!
//! 1. the speakers of a layout are grouped into the listener-plane layer
//!    (`z` = 0) and the top layer (`z` = 1); each layer is a set of rows
//!    (speakers sharing a `y`), each row a set of speakers ordered by `x`;
//! 2. a point source is panned with a sine/cosine (equal-power) law
//!    between the two layers by `z`, between the two rows bracketing its
//!    `y` inside each layer, and between the two speakers bracketing its
//!    `x` inside each row; a coordinate outside the speakers' span is
//!    held at the nearest one. The three stages multiply, so the squared
//!    gains always sum to one;
//! 3. object size spreads the source: the squared gains of up to 3 × 3 × 3
//!    point sources at `position ± size / 2` (per axis, clamped to the
//!    room) are averaged;
//! 4. channel lock sends the object to the single nearest speaker;
//! 5. zone constraints remove speakers before any of the above (a
//!    speaker stays when at least one of its Table A.7 zones is
//!    included); if a constraint would leave no speaker it is ignored.

use std::{f32::consts::FRAC_PI_2, fmt, str::FromStr};

use oxideav_core::{ChannelLayout, ChannelPosition};

use super::oamd::OamdObjectState;

/// Most speakers any [`JocLayout`] carries (9.1.6).
pub(super) const MAX_LAYOUT_CHANNELS: usize = 16;

/// A loudspeaker a rendered JOC object presentation can address.
///
/// The names are the TS 103 420 clause 4.4 speaker labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum JocSpeaker {
    /// Left (`RC_L`).
    Left,
    /// Right (`RC_R`).
    Right,
    /// Centre (`RC_C`).
    Center,
    /// Low-frequency effects (`RC_LFE`).
    Lfe,
    /// Left side / surround (`RC_LS`).
    LeftSurround,
    /// Right side / surround (`RC_RS`).
    RightSurround,
    /// Left back (`RC_LB`).
    LeftBack,
    /// Right back (`RC_RB`).
    RightBack,
    /// Left wide (`RC_LW`).
    LeftWide,
    /// Right wide (`RC_RW`).
    RightWide,
    /// Top front left (`RC_TFL`).
    TopFrontLeft,
    /// Top front right (`RC_TFR`).
    TopFrontRight,
    /// Top side left (`RC_TSL`).
    TopSideLeft,
    /// Top side right (`RC_TSR`).
    TopSideRight,
    /// Top back left (`RC_TBL`).
    TopBackLeft,
    /// Top back right (`RC_TBR`).
    TopBackRight,
}

impl JocSpeaker {
    /// Short conventional label (`"L"`, `"Ls"`, `"Tfl"`, …).
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "L",
            Self::Right => "R",
            Self::Center => "C",
            Self::Lfe => "LFE",
            Self::LeftSurround => "Ls",
            Self::RightSurround => "Rs",
            Self::LeftBack => "Lb",
            Self::RightBack => "Rb",
            Self::LeftWide => "Lw",
            Self::RightWide => "Rw",
            Self::TopFrontLeft => "Tfl",
            Self::TopFrontRight => "Tfr",
            Self::TopSideLeft => "Tsl",
            Self::TopSideRight => "Tsr",
            Self::TopBackLeft => "Tbl",
            Self::TopBackRight => "Tbr",
        }
    }

    /// The matching `oxideav-core` channel position, when core has one.
    /// The wide and top-side speakers have no core position yet and
    /// answer `None`.
    pub fn channel_position(self) -> Option<ChannelPosition> {
        Some(match self {
            Self::Left => ChannelPosition::FrontLeft,
            Self::Right => ChannelPosition::FrontRight,
            Self::Center => ChannelPosition::FrontCenter,
            Self::Lfe => ChannelPosition::LowFrequency,
            Self::LeftSurround => ChannelPosition::SideLeft,
            Self::RightSurround => ChannelPosition::SideRight,
            Self::LeftBack => ChannelPosition::BackLeft,
            Self::RightBack => ChannelPosition::BackRight,
            Self::TopFrontLeft => ChannelPosition::TopFrontLeft,
            Self::TopFrontRight => ChannelPosition::TopFrontRight,
            Self::TopBackLeft => ChannelPosition::TopBackLeft,
            Self::TopBackRight => ChannelPosition::TopBackRight,
            Self::LeftWide | Self::RightWide | Self::TopSideLeft | Self::TopSideRight => {
                return None;
            }
        })
    }

    /// Room-anchored position (clause 4.2.1 scale) of the speaker label:
    /// Table B.10 mapped from its −1 … 1 / front-positive scale.
    pub fn room_position(self) -> [f32; 3] {
        // Table B.10 (X, Y, Z).
        let (x, y, z): (f32, f32, f32) = match self {
            Self::Left => (-1.0, 1.0, 0.0),
            Self::Right => (1.0, 1.0, 0.0),
            Self::Center => (0.0, 1.0, 0.0),
            Self::Lfe => (-1.0, 1.0, -1.0),
            Self::LeftSurround => (-1.0, 0.0, 0.0),
            Self::RightSurround => (1.0, 0.0, 0.0),
            Self::LeftBack => (-1.0, -1.0, 0.0),
            Self::RightBack => (1.0, -1.0, 0.0),
            Self::LeftWide => (-1.0, 0.677_419, 0.0),
            Self::RightWide => (1.0, 0.677_419, 0.0),
            Self::TopFrontLeft => (-1.0, 1.0, 1.0),
            Self::TopFrontRight => (1.0, 1.0, 1.0),
            Self::TopSideLeft => (-1.0, 0.0, 1.0),
            Self::TopSideRight => (1.0, 0.0, 1.0),
            Self::TopBackLeft => (-1.0, -1.0, 1.0),
            Self::TopBackRight => (1.0, -1.0, 1.0),
        };
        [(x + 1.0) * 0.5, (1.0 - y) * 0.5, z]
    }

    fn is_top(self) -> bool {
        matches!(
            self,
            Self::TopFrontLeft
                | Self::TopFrontRight
                | Self::TopSideLeft
                | Self::TopSideRight
                | Self::TopBackLeft
                | Self::TopBackRight
        )
    }
}

impl fmt::Display for JocSpeaker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Speaker layout a JOC object presentation is rendered to.
///
/// Channels are interleaved in the order [`speakers`](Self::speakers)
/// lists them: `L, R, C, LFE, Ls, Rs`, then `Lb, Rb`, then `Lw, Rw`,
/// then the top speakers front to back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum JocLayout {
    /// 2.0 — `L, R`.
    #[default]
    Stereo,
    /// 5.1 — `L, R, C, LFE, Ls, Rs`.
    Surround51,
    /// 5.1.2 — 5.1 plus `Tsl, Tsr`.
    Surround512,
    /// 5.1.4 — 5.1 plus `Tfl, Tfr, Tbl, Tbr`.
    Surround514,
    /// 7.1 — `L, R, C, LFE, Ls, Rs, Lb, Rb`.
    Surround71,
    /// 7.1.2 — 7.1 plus `Tsl, Tsr`.
    Surround712,
    /// 7.1.4 — 7.1 plus `Tfl, Tfr, Tbl, Tbr`.
    Surround714,
    /// 9.1.6 — 7.1 plus `Lw, Rw` and `Tfl, Tfr, Tsl, Tsr, Tbl, Tbr`.
    Surround916,
}

impl JocLayout {
    /// Every layout, in ascending channel count.
    pub const ALL: [Self; 8] = [
        Self::Stereo,
        Self::Surround51,
        Self::Surround512,
        Self::Surround71,
        Self::Surround514,
        Self::Surround712,
        Self::Surround714,
        Self::Surround916,
    ];

    /// The speakers of the layout, in output channel order.
    pub fn speakers(self) -> &'static [JocSpeaker] {
        use JocSpeaker::{
            Center, Left, LeftBack, LeftSurround, LeftWide, Lfe, Right, RightBack, RightSurround,
            RightWide, TopBackLeft, TopBackRight, TopFrontLeft, TopFrontRight, TopSideLeft,
            TopSideRight,
        };
        match self {
            Self::Stereo => &[Left, Right],
            Self::Surround51 => &[Left, Right, Center, Lfe, LeftSurround, RightSurround],
            Self::Surround512 => &[
                Left,
                Right,
                Center,
                Lfe,
                LeftSurround,
                RightSurround,
                TopSideLeft,
                TopSideRight,
            ],
            Self::Surround514 => &[
                Left,
                Right,
                Center,
                Lfe,
                LeftSurround,
                RightSurround,
                TopFrontLeft,
                TopFrontRight,
                TopBackLeft,
                TopBackRight,
            ],
            Self::Surround71 => &[
                Left,
                Right,
                Center,
                Lfe,
                LeftSurround,
                RightSurround,
                LeftBack,
                RightBack,
            ],
            Self::Surround712 => &[
                Left,
                Right,
                Center,
                Lfe,
                LeftSurround,
                RightSurround,
                LeftBack,
                RightBack,
                TopSideLeft,
                TopSideRight,
            ],
            Self::Surround714 => &[
                Left,
                Right,
                Center,
                Lfe,
                LeftSurround,
                RightSurround,
                LeftBack,
                RightBack,
                TopFrontLeft,
                TopFrontRight,
                TopBackLeft,
                TopBackRight,
            ],
            Self::Surround916 => &[
                Left,
                Right,
                Center,
                Lfe,
                LeftSurround,
                RightSurround,
                LeftBack,
                RightBack,
                LeftWide,
                RightWide,
                TopFrontLeft,
                TopFrontRight,
                TopSideLeft,
                TopSideRight,
                TopBackLeft,
                TopBackRight,
            ],
        }
    }

    /// Number of output channels.
    pub fn channel_count(self) -> u16 {
        self.speakers().len() as u16
    }

    /// Canonical name (`"stereo"`, `"5.1"`, `"5.1.2"`, `"5.1.4"`, `"7.1"`,
    /// `"7.1.2"`, `"7.1.4"`, `"9.1.6"`) — the value of the `joc_layout`
    /// decoder option.
    pub fn name(self) -> &'static str {
        match self {
            Self::Stereo => "stereo",
            Self::Surround51 => "5.1",
            Self::Surround512 => "5.1.2",
            Self::Surround514 => "5.1.4",
            Self::Surround71 => "7.1",
            Self::Surround712 => "7.1.2",
            Self::Surround714 => "7.1.4",
            Self::Surround916 => "9.1.6",
        }
    }

    /// The `oxideav-core` channel layout describing the output.
    ///
    /// Stereo, 5.1 and 7.1 map to their named core layouts. Core has no
    /// layouts with height or wide speakers yet, so the others answer
    /// [`ChannelLayout::DiscreteN`] with the channel count; the per-slot
    /// meaning is then given by [`speakers`](Self::speakers).
    pub fn channel_layout(self) -> ChannelLayout {
        match self {
            Self::Stereo => ChannelLayout::Stereo,
            Self::Surround51 => ChannelLayout::Surround51,
            Self::Surround71 => ChannelLayout::Surround71,
            other => ChannelLayout::DiscreteN(other.channel_count()),
        }
    }

    fn has_back_pair(self) -> bool {
        self.speakers().contains(&JocSpeaker::LeftBack)
    }
}

impl fmt::Display for JocLayout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// Error of the [`JocLayout`] `FromStr` impl: the name is not a layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseJocLayoutError(pub String);

impl fmt::Display for ParseJocLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unrecognised JOC layout {:?} (expected one of stereo, 5.1, 5.1.2, 5.1.4, 7.1, 7.1.2, 7.1.4, 9.1.6)",
            self.0
        )
    }
}

impl std::error::Error for ParseJocLayoutError {}

impl FromStr for JocLayout {
    type Err = ParseJocLayoutError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let lower = text.trim().to_ascii_lowercase();
        Ok(match lower.as_str() {
            "stereo" | "2.0" => Self::Stereo,
            "5.1" => Self::Surround51,
            "5.1.2" => Self::Surround512,
            "5.1.4" => Self::Surround514,
            "7.1" => Self::Surround71,
            "7.1.2" => Self::Surround712,
            "7.1.4" => Self::Surround714,
            "9.1.6" => Self::Surround916,
            _ => return Err(ParseJocLayoutError(text.to_owned())),
        })
    }
}

// --- Zones (Table A.7, Tables 20 / 21) --------------------------------

const ZONE_SCREEN: u8 = 1 << 0;
const ZONE_SIDE: u8 = 1 << 1;
const ZONE_SURROUND: u8 = 1 << 2;
const ZONE_BACK: u8 = 1 << 3;
/// The centre speaker's membership of the centre-and-back zone (the back
/// speakers belong to it through `ZONE_BACK`, Table A.7 note 1).
const ZONE_CENTRE_OF_CENTRE_AND_BACK: u8 = 1 << 4;
const ZONE_TOP_BOTTOM: u8 = 1 << 5;
const ZONE_ALL_HORIZONTAL: u8 =
    ZONE_SCREEN | ZONE_SIDE | ZONE_SURROUND | ZONE_BACK | ZONE_CENTRE_OF_CENTRE_AND_BACK;

/// Table A.7: the zones a speaker belongs to in `layout`.
fn speaker_zones(speaker: JocSpeaker, layout: JocLayout) -> u8 {
    match speaker {
        JocSpeaker::Left | JocSpeaker::Right => ZONE_SCREEN,
        JocSpeaker::Center => ZONE_SCREEN | ZONE_CENTRE_OF_CENTRE_AND_BACK,
        // Table A.7 note 2: Back for 5.X(.X) or less, Side otherwise.
        JocSpeaker::LeftSurround | JocSpeaker::RightSurround => {
            ZONE_SURROUND
                | if layout.has_back_pair() {
                    ZONE_SIDE
                } else {
                    ZONE_BACK
                }
        }
        JocSpeaker::LeftBack | JocSpeaker::RightBack => ZONE_BACK,
        JocSpeaker::LeftWide | JocSpeaker::RightWide => ZONE_SIDE,
        JocSpeaker::Lfe => 0,
        _ => ZONE_TOP_BOTTOM,
    }
}

/// Tables 20 and 21: the set of included zones.
fn included_zones(zone_constraints_idx: u8, elevation_enabled: bool) -> u8 {
    let horizontal = match zone_constraints_idx {
        1 => ZONE_ALL_HORIZONTAL & !ZONE_BACK,
        2 => ZONE_ALL_HORIZONTAL & !ZONE_SIDE,
        3 => ZONE_BACK | ZONE_CENTRE_OF_CENTRE_AND_BACK,
        4 => ZONE_SCREEN,
        5 => ZONE_SURROUND,
        _ => ZONE_ALL_HORIZONTAL,
    };
    horizontal
        | if elevation_enabled {
            ZONE_TOP_BOTTOM
        } else {
            0
        }
}

// --- Panner ------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct PannedSpeaker {
    /// Output channel index.
    channel: usize,
    position: [f32; 3],
    zones: u8,
    top: bool,
}

/// Most rows of equal depth in one layer (9.1.6: front, wide, side,
/// back).
const MAX_LAYER_ROWS: usize = 4;

/// The allowed speakers of one layer of a layout, grouped by row.
struct PanLayer {
    rows: usize,
    row_y: [f32; MAX_LAYER_ROWS],
    /// Speakers of row `r` are `row_start[r]..row_start[r + 1]`.
    row_start: [usize; MAX_LAYER_ROWS + 1],
    x: [f32; MAX_LAYOUT_CHANNELS],
    channel: [usize; MAX_LAYOUT_CHANNELS],
}

/// Precomputed geometry of one [`JocLayout`].
#[derive(Clone, Debug)]
pub(super) struct Panner {
    layout: JocLayout,
    /// Full-bandwidth speakers sorted by (layer, y, x).
    speakers: Vec<PannedSpeaker>,
    lfe_channel: Option<usize>,
    channels: usize,
}

/// What an object contributes at one instant.
#[derive(Clone, Copy, Debug)]
pub(super) struct PanSource {
    pub(super) position: [f32; 3],
    pub(super) size: [f32; 3],
    pub(super) zone_constraints: u8,
    pub(super) elevation_enabled: bool,
    pub(super) channel_lock: bool,
}

impl PanSource {
    pub(super) fn point(position: [f32; 3]) -> Self {
        Self {
            position,
            size: [0.0; 3],
            zone_constraints: 0,
            elevation_enabled: true,
            channel_lock: false,
        }
    }

    /// The properties of an OAMD ramp at `alpha` between two states:
    /// coordinates and size are interpolated, the discrete properties
    /// switch at the ramp's midpoint.
    pub(super) fn interpolated(from: &OamdObjectState, to: &OamdObjectState, alpha: f32) -> Self {
        let lerp3 = |a: [f32; 3], b: [f32; 3]| {
            [
                a[0] + (b[0] - a[0]) * alpha,
                a[1] + (b[1] - a[1]) * alpha,
                a[2] + (b[2] - a[2]) * alpha,
            ]
        };
        let discrete = if alpha >= 0.5 { to } else { from };
        Self {
            position: lerp3(from.position, to.position),
            size: lerp3(from.size, to.size),
            zone_constraints: discrete.zone_constraints,
            elevation_enabled: discrete.elevation_enabled,
            channel_lock: discrete.channel_lock,
        }
    }
}

impl Panner {
    pub(super) fn new(layout: JocLayout) -> Self {
        let mut speakers = Vec::new();
        let mut lfe_channel = None;
        for (channel, &speaker) in layout.speakers().iter().enumerate() {
            if speaker == JocSpeaker::Lfe {
                lfe_channel = Some(channel);
                continue;
            }
            let mut position = speaker.room_position();
            // Without a back pair the surround pair is the rearmost row:
            // it takes the back wall so the front/back pan uses the whole
            // room depth (Table A.7 note 2 likewise counts it as Back).
            if matches!(
                speaker,
                JocSpeaker::LeftSurround | JocSpeaker::RightSurround
            ) && !layout.has_back_pair()
                && layout != JocLayout::Stereo
            {
                position[1] = 1.0;
            }
            speakers.push(PannedSpeaker {
                channel,
                position,
                zones: speaker_zones(speaker, layout),
                top: speaker.is_top(),
            });
        }
        speakers.sort_by(|a, b| {
            (a.top, a.position[1], a.position[0])
                .partial_cmp(&(b.top, b.position[1], b.position[0]))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Self {
            layout,
            speakers,
            lfe_channel,
            channels: layout.speakers().len(),
        }
    }

    pub(super) fn layout(&self) -> JocLayout {
        self.layout
    }

    pub(super) fn channels(&self) -> usize {
        self.channels
    }

    pub(super) fn lfe_channel(&self) -> Option<usize> {
        self.lfe_channel
    }

    /// Output channel of `speaker`, when the layout has it.
    pub(super) fn channel_of(&self, speaker: JocSpeaker) -> Option<usize> {
        self.layout.speakers().iter().position(|&s| s == speaker)
    }

    /// Unit-power gains of `source` over the layout's channels (the LFE
    /// channel always gets zero). `gains` is overwritten.
    pub(super) fn gains(&self, source: &PanSource, gains: &mut [f32; MAX_LAYOUT_CHANNELS]) {
        gains.fill(0.0);
        let included = included_zones(source.zone_constraints, source.elevation_enabled);
        let mut allowed = [false; MAX_LAYOUT_CHANNELS];
        let mut any = false;
        for (slot, speaker) in allowed.iter_mut().zip(&self.speakers) {
            *slot = speaker.zones & included != 0;
            any |= *slot;
        }
        if !any {
            // The constraint excludes every speaker of this layout.
            allowed[..self.speakers.len()].fill(true);
        }
        let allowed = &allowed[..self.speakers.len()];

        let position = [
            sanitize(source.position[0], 0.5).clamp(0.0, 1.0),
            sanitize(source.position[1], 0.5).clamp(0.0, 1.0),
            sanitize(source.position[2], 0.0).clamp(-1.0, 1.0),
        ];
        if source.channel_lock {
            self.snap(position, allowed, gains);
            return;
        }

        let size = [
            sanitize(source.size[0], 0.0).clamp(0.0, 1.0),
            sanitize(source.size[1], 0.0).clamp(0.0, 1.0),
            sanitize(source.size[2], 0.0).clamp(0.0, 1.0),
        ];
        // Extent: average the power of point sources across the object.
        // A point object is the one-source case of the same sum.
        let offsets = |extent: f32| -> ([f32; 3], usize) {
            if extent > 0.0 {
                ([-0.5 * extent, 0.0, 0.5 * extent], 3)
            } else {
                ([0.0; 3], 1)
            }
        };
        let (dx, nx) = offsets(size[0]);
        let (dy, ny) = offsets(size[1]);
        let (dz, nz) = offsets(size[2]);
        let weight = 1.0 / (nx * ny * nz) as f32;

        // The three pan stages are separable, so each is evaluated once
        // per distinct coordinate of the source grid instead of once per
        // source: the layer split for every z, the row bracket of each
        // layer for every y, the speaker bracket of each row for every x.
        let layers = [self.layer(false, allowed), self.layer(true, allowed)];
        let (has_plane, has_top) = (layers[0].rows > 0, layers[1].rows > 0);
        let mut top_shares = [0.0_f32; 3];
        for (share, &oz) in top_shares.iter_mut().zip(&dz[..nz]) {
            let z = (position[2] + oz).clamp(-1.0, 1.0);
            *share = if has_plane && has_top {
                (z.clamp(0.0, 1.0) * FRAC_PI_2).sin().powi(2)
            } else if has_top {
                1.0
            } else {
                0.0
            };
        }
        let mut row_brackets = [[(0_usize, 0_usize, 0.0_f32); 3]; 2];
        let mut speaker_brackets = [[[(0_usize, 0_usize, 0.0_f32); 3]; MAX_LAYER_ROWS]; 2];
        for (layer_index, layer) in layers.iter().enumerate() {
            if layer.rows == 0 {
                continue;
            }
            for (bracketed, &oy) in row_brackets[layer_index].iter_mut().zip(&dy[..ny]) {
                let y = (position[1] + oy).clamp(0.0, 1.0);
                *bracketed = bracket(&layer.row_y[..layer.rows], y);
            }
            for row in 0..layer.rows {
                let speakers = layer.row_start[row]..layer.row_start[row + 1];
                for (bracketed, &ox) in speaker_brackets[layer_index][row].iter_mut().zip(&dx[..nx])
                {
                    let x = (position[0] + ox).clamp(0.0, 1.0);
                    let (left, right, share) = bracket(&layer.x[speakers.clone()], x);
                    *bracketed = (
                        layer.channel[speakers.start + left],
                        layer.channel[speakers.start + right],
                        share,
                    );
                }
            }
        }

        for &top_share in &top_shares[..nz] {
            for y_index in 0..ny {
                for x_index in 0..nx {
                    for (layer_index, layer_share) in [(0, 1.0 - top_share), (1, top_share)] {
                        if layer_share <= 0.0 {
                            continue;
                        }
                        let layer_weight = weight * layer_share;
                        let (low, high, share) = row_brackets[layer_index][y_index];
                        for (row, row_share) in [(low, 1.0 - share), (high, share)] {
                            if row_share <= 0.0 {
                                continue;
                            }
                            let (left, right, x_share) =
                                speaker_brackets[layer_index][row][x_index];
                            gains[left] += layer_weight * row_share * (1.0 - x_share);
                            gains[right] += layer_weight * row_share * x_share;
                        }
                    }
                }
            }
        }
        for gain in gains.iter_mut().take(self.channels) {
            *gain = gain.sqrt();
        }
    }

    /// The allowed speakers of one layer, grouped into rows of equal
    /// depth (the speaker list is sorted by layer, y, x).
    fn layer(&self, top: bool, allowed: &[bool]) -> PanLayer {
        let mut layer = PanLayer {
            rows: 0,
            row_y: [0.0; MAX_LAYER_ROWS],
            row_start: [0; MAX_LAYER_ROWS + 1],
            x: [0.0; MAX_LAYOUT_CHANNELS],
            channel: [0; MAX_LAYOUT_CHANNELS],
        };
        let mut count = 0;
        for (speaker, &ok) in self.speakers.iter().zip(allowed) {
            if !ok || speaker.top != top {
                continue;
            }
            let y = speaker.position[1];
            if layer.rows == 0 || layer.row_y[layer.rows - 1] != y {
                if layer.rows == MAX_LAYER_ROWS {
                    // No layout has more rows; fold any excess into the
                    // last one rather than index out of bounds.
                    layer.rows -= 1;
                }
                layer.row_y[layer.rows] = y;
                layer.row_start[layer.rows] = count;
                layer.rows += 1;
            }
            layer.x[count] = speaker.position[0];
            layer.channel[count] = speaker.channel;
            count += 1;
            layer.row_start[layer.rows] = count;
        }
        layer
    }

    fn snap(&self, position: [f32; 3], allowed: &[bool], gains: &mut [f32; MAX_LAYOUT_CHANNELS]) {
        let mut best = None;
        for (speaker, &ok) in self.speakers.iter().zip(allowed) {
            if !ok {
                continue;
            }
            let distance: f32 = speaker
                .position
                .iter()
                .zip(position)
                .map(|(a, b)| (a - b) * (a - b))
                .sum();
            if best.is_none_or(|(_, nearest)| distance < nearest) {
                best = Some((speaker.channel, distance));
            }
        }
        if let Some((channel, _)) = best {
            gains[channel] = 1.0;
        }
    }
}

fn sanitize(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

/// Indices of the two ascending `stops` bracketing `value` and the
/// equal-power share (a squared gain, `sin²`) of the upper one. `stops`
/// is never empty when called (a layer is only visited when it has an
/// allowed speaker).
fn bracket(stops: &[f32], value: f32) -> (usize, usize, f32) {
    let last = stops.len().saturating_sub(1);
    if stops.is_empty() || value <= stops[0] {
        return (0, 0, 0.0);
    }
    if value >= stops[last] {
        return (last, last, 0.0);
    }
    let upper = stops
        .iter()
        .position(|&stop| stop > value)
        .unwrap_or(last)
        .max(1);
    let lower = upper - 1;
    let span = stops[upper] - stops[lower];
    let fraction = if span > 0.0 {
        (value - stops[lower]) / span
    } else {
        0.0
    };
    (lower, upper, (fraction * FRAC_PI_2).sin().powi(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gains_of(layout: JocLayout, source: &PanSource) -> Vec<f32> {
        let panner = Panner::new(layout);
        let mut gains = [0.0; MAX_LAYOUT_CHANNELS];
        panner.gains(source, &mut gains);
        gains[..panner.channels()].to_vec()
    }

    fn power(gains: &[f32]) -> f32 {
        gains.iter().map(|gain| gain * gain).sum()
    }

    fn channel(layout: JocLayout, speaker: JocSpeaker) -> usize {
        layout
            .speakers()
            .iter()
            .position(|&s| s == speaker)
            .expect("speaker in layout")
    }

    #[test]
    fn layout_names_round_trip_and_counts_match() {
        for layout in JocLayout::ALL {
            assert_eq!(layout.name().parse::<JocLayout>(), Ok(layout));
            assert_eq!(layout.to_string(), layout.name());
            assert_eq!(
                layout.channel_layout().channel_count(),
                layout.channel_count()
            );
        }
        assert_eq!(JocLayout::Surround916.channel_count(), 16);
        assert_eq!(JocLayout::Surround714.channel_count(), 12);
        assert_eq!(JocLayout::Surround514.channel_count(), 10);
        assert_eq!(JocLayout::Surround512.channel_count(), 8);
        assert!("11.1".parse::<JocLayout>().is_err());
        assert_eq!(" 7.1.4 ".parse::<JocLayout>(), Ok(JocLayout::Surround714));
    }

    #[test]
    fn named_core_layouts_agree_on_channel_order() {
        for layout in [
            JocLayout::Stereo,
            JocLayout::Surround51,
            JocLayout::Surround71,
        ] {
            let positions: Vec<_> = layout
                .speakers()
                .iter()
                .map(|speaker| speaker.channel_position().expect("core position"))
                .collect();
            assert_eq!(positions, layout.channel_layout().positions());
        }
    }

    #[test]
    fn speaker_positions_follow_table_b10() {
        assert_eq!(JocSpeaker::Left.room_position(), [0.0, 0.0, 0.0]);
        assert_eq!(JocSpeaker::Center.room_position(), [0.5, 0.0, 0.0]);
        assert_eq!(JocSpeaker::RightSurround.room_position(), [1.0, 0.5, 0.0]);
        assert_eq!(JocSpeaker::LeftBack.room_position(), [0.0, 1.0, 0.0]);
        assert_eq!(JocSpeaker::TopBackRight.room_position(), [1.0, 1.0, 1.0]);
        let wide = JocSpeaker::LeftWide.room_position();
        assert!((wide[1] - 0.161_290_5).abs() < 1.0e-6);
    }

    /// A point source at a speaker's own position plays from that speaker
    /// alone, in every layout that has it.
    #[test]
    fn a_source_on_a_speaker_uses_only_that_speaker() {
        for layout in JocLayout::ALL {
            let panner = Panner::new(layout);
            for speaker in &panner.speakers {
                let gains = gains_of(layout, &PanSource::point(speaker.position));
                for (index, gain) in gains.iter().enumerate() {
                    let want = if index == speaker.channel { 1.0 } else { 0.0 };
                    assert!(
                        (gain - want).abs() < 1.0e-6,
                        "{layout}: speaker channel {} gains {gains:?}",
                        speaker.channel
                    );
                }
            }
        }
    }

    /// Unit power everywhere in the room, with and without extent, and
    /// never anything on the LFE channel.
    #[test]
    fn gains_are_unit_power_across_the_room() {
        for layout in JocLayout::ALL {
            let lfe = Panner::new(layout).lfe_channel();
            for step in 0..=1000 {
                let t = step as f32 / 1000.0;
                let position = [
                    (t * 7.0).fract(),
                    (t * 13.0).fract(),
                    (t * 3.0).fract() * 2.0 - 1.0,
                ];
                for size in [[0.0; 3], [0.3, 0.0, 0.0], [0.2, 0.6, 1.0], [1.0; 3]] {
                    for zone_constraints in 0..=5 {
                        let source = PanSource {
                            position,
                            size,
                            zone_constraints,
                            elevation_enabled: step % 3 != 0,
                            channel_lock: step % 11 == 0,
                        };
                        let gains = gains_of(layout, &source);
                        assert!(
                            (power(&gains) - 1.0).abs() < 1.0e-4,
                            "{layout} {source:?}: {gains:?}"
                        );
                        assert!(gains.iter().all(|gain| (0.0..=1.0001).contains(gain)));
                        if let Some(lfe) = lfe {
                            assert_eq!(gains[lfe], 0.0);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn height_pans_into_the_top_layer_only_where_one_exists() {
        let overhead = PanSource::point([0.0, 0.0, 1.0]);
        let gains = gains_of(JocLayout::Surround714, &overhead);
        let tfl = channel(JocLayout::Surround714, JocSpeaker::TopFrontLeft);
        assert!((gains[tfl] - 1.0).abs() < 1.0e-6, "{gains:?}");

        // Half-way up: equal power between L and Tfl.
        let gains = gains_of(JocLayout::Surround714, &PanSource::point([0.0, 0.0, 0.5]));
        let left = channel(JocLayout::Surround714, JocSpeaker::Left);
        assert!((gains[left] - gains[tfl]).abs() < 1.0e-6, "{gains:?}");
        assert!((gains[left] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1.0e-6);

        // No top layer: the same source stays in the listener plane.
        let gains = gains_of(JocLayout::Surround51, &overhead);
        assert!((gains[channel(JocLayout::Surround51, JocSpeaker::Left)] - 1.0).abs() < 1.0e-6);

        // Elevation disabled (Table 21): the top layer is excluded.
        let mut flat = overhead;
        flat.elevation_enabled = false;
        let gains = gains_of(JocLayout::Surround714, &flat);
        assert!((gains[left] - 1.0).abs() < 1.0e-6, "{gains:?}");

        // Below the listener plane there are no speakers: held at z = 0.
        let gains = gains_of(JocLayout::Surround714, &PanSource::point([0.0, 0.0, -1.0]));
        assert!((gains[left] - 1.0).abs() < 1.0e-6, "{gains:?}");
    }

    #[test]
    fn depth_pans_front_to_side_to_back() {
        let layout = JocLayout::Surround71;
        let ch = |speaker| channel(layout, speaker);
        // Left wall, a quarter of the way back: equal power L / Ls.
        let gains = gains_of(layout, &PanSource::point([0.0, 0.25, 0.0]));
        assert!((gains[ch(JocSpeaker::Left)] - gains[ch(JocSpeaker::LeftSurround)]).abs() < 1.0e-6);
        // Back wall: the back pair only.
        let gains = gains_of(layout, &PanSource::point([0.5, 1.0, 0.0]));
        let (lb, rb) = (ch(JocSpeaker::LeftBack), ch(JocSpeaker::RightBack));
        assert!((gains[lb] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1.0e-6);
        assert!((gains[rb] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1.0e-6);

        // 5.1: the surround pair is the back of the room.
        let layout = JocLayout::Surround51;
        let gains = gains_of(layout, &PanSource::point([1.0, 1.0, 0.0]));
        assert!((gains[channel(layout, JocSpeaker::RightSurround)] - 1.0).abs() < 1.0e-6);
        // Front centre is the centre speaker alone.
        let gains = gains_of(layout, &PanSource::point([0.5, 0.0, 0.0]));
        assert!((gains[channel(layout, JocSpeaker::Center)] - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn zone_constraints_follow_tables_20_and_a7() {
        let layout = JocLayout::Surround714;
        let ch = |speaker| channel(layout, speaker);
        let at = |position, zone_constraints| {
            let mut source = PanSource::point(position);
            source.zone_constraints = zone_constraints;
            gains_of(layout, &source)
        };
        // 1: back zone excluded — a source at the back wall stops at the
        // side surrounds.
        let gains = at([0.0, 1.0, 0.0], 1);
        assert!(
            (gains[ch(JocSpeaker::LeftSurround)] - 1.0).abs() < 1.0e-6,
            "{gains:?}"
        );
        // 4: only the screen zone — everything collapses onto L / C / R.
        let gains = at([1.0, 1.0, 0.0], 4);
        assert!(
            (gains[ch(JocSpeaker::Right)] - 1.0).abs() < 1.0e-6,
            "{gains:?}"
        );
        // 5: only the surround zone — a front source plays from Ls / Rs.
        let gains = at([0.0, 0.0, 0.0], 5);
        assert!(
            (gains[ch(JocSpeaker::LeftSurround)] - 1.0).abs() < 1.0e-6,
            "{gains:?}"
        );
        // 3: centre-and-back — front centre keeps C, the rest goes back.
        let gains = at([0.5, 0.0, 0.0], 3);
        assert!(
            (gains[ch(JocSpeaker::Center)] - 1.0).abs() < 1.0e-6,
            "{gains:?}"
        );
        let gains = at([0.0, 0.6, 0.0], 3);
        assert!(gains[ch(JocSpeaker::LeftBack)] > 0.5, "{gains:?}");
        assert_eq!(gains[ch(JocSpeaker::LeftSurround)], 0.0);
        assert_eq!(gains[ch(JocSpeaker::Left)], 0.0);

        // 2: side zone excluded — on 9.1.6 the wides drop out, the side
        // surrounds (also in the surround zone) stay.
        let layout = JocLayout::Surround916;
        let wide = JocSpeaker::LeftWide.room_position();
        let mut source = PanSource::point(wide);
        let gains = gains_of(layout, &source);
        assert!((gains[channel(layout, JocSpeaker::LeftWide)] - 1.0).abs() < 1.0e-6);
        source.zone_constraints = 2;
        let gains = gains_of(layout, &source);
        assert_eq!(gains[channel(layout, JocSpeaker::LeftWide)], 0.0);
        assert!(gains[channel(layout, JocSpeaker::Left)] > 0.5, "{gains:?}");

        // 5.1: the surround pair counts as the back zone (note 2), but it
        // is also the surround zone, so excluding "back" keeps it.
        let layout = JocLayout::Surround51;
        let mut source = PanSource::point([0.0, 1.0, 0.0]);
        source.zone_constraints = 1;
        let gains = gains_of(layout, &source);
        assert!((gains[channel(layout, JocSpeaker::LeftSurround)] - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn channel_lock_selects_the_nearest_speaker() {
        let mut source = PanSource::point([0.4, 0.1, 0.1]);
        source.channel_lock = true;
        let gains = gains_of(JocLayout::Surround714, &source);
        let centre = channel(JocLayout::Surround714, JocSpeaker::Center);
        assert_eq!(gains[centre], 1.0);
        assert_eq!(power(&gains), 1.0);
    }

    #[test]
    fn extent_spreads_power_over_neighbouring_speakers() {
        let layout = JocLayout::Surround714;
        let mut source = PanSource::point([0.5, 0.0, 0.0]);
        source.size = [1.0, 0.0, 0.0];
        let gains = gains_of(layout, &source);
        let third = (1.0_f32 / 3.0).sqrt();
        for speaker in [JocSpeaker::Left, JocSpeaker::Center, JocSpeaker::Right] {
            assert!(
                (gains[channel(layout, speaker)] - third).abs() < 1.0e-6,
                "{gains:?}"
            );
        }
        // Full-room extent reaches every full-bandwidth speaker.
        source.position = [0.5, 0.5, 0.0];
        source.size = [1.0; 3];
        let gains = gains_of(layout, &source);
        let lfe = channel(layout, JocSpeaker::Lfe);
        for (index, gain) in gains.iter().enumerate() {
            assert_eq!(*gain > 0.0, index != lfe, "{gains:?}");
        }
    }

    #[test]
    fn hostile_coordinates_are_sanitized() {
        for layout in JocLayout::ALL {
            let source = PanSource {
                position: [f32::NAN, f32::INFINITY, -f32::INFINITY],
                size: [f32::NAN, 9.0, -3.0],
                zone_constraints: 200,
                elevation_enabled: true,
                channel_lock: false,
            };
            let gains = gains_of(layout, &source);
            assert!((power(&gains) - 1.0).abs() < 1.0e-4, "{layout}: {gains:?}");
        }
    }
}
