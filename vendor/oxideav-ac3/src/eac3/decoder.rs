//! E-AC-3 syncframe decoder — rounds 1 + 2 + 3.
//!
//! Round-1 path:
//!
//! 1. Verify the 16-bit syncword (`0x0B77`).
//! 2. Parse the [`super::bsi`] BSI — channel layout, sample rate,
//!    frame size, dialnorm, etc.
//! 3. Parse the [`super::audfrm`] audio-frame element — strategy
//!    flags only (no DSP yet).
//! 4. Emit `bsi.num_blocks × 256 × nchans` interleaved S16 zeros.
//!
//! Round-2 path:
//!
//! 5. Hand the BitReader to [`super::dsp::decode_indep_audblks`],
//!    which walks per-block side-info, runs §7 bit allocation +
//!    mantissa unpack, and applies IMDCT + window + overlap-add via
//!    the AC-3 helpers. On any "unsupported feature" error
//!    (coupling, SPX, AHT, transient processing, …) the decoder
//!    falls back to silent emit so the corpus driver keeps decoding.
//!
//! Round-3 path (this commit):
//!
//! 6. Walk dependent substreams in the same packet, decode each via
//!    the round-2 DSP, and **splice** the resulting channels into
//!    [`Eac3DecoderState::indep_pcm_f32`] per the §E.3.8.2 channel-
//!    and-program-extension rule: each dep coded channel is routed by
//!    its Table E2.5 location (or natural `acmod` order when
//!    `chanmape == 0`); a location already present in the indep
//!    program *replaces* that channel in place, a new location
//!    *extends* the output. The replace-vs-extend partition is what
//!    keeps a real greater-than-5.1 broadcast stream spatially
//!    correct — a dep substream commonly re-codes Center / LFE (or,
//!    with a custom chanmap, even L/R) that the indep 5.1 downmix
//!    already carries, and a naive blind-append would duplicate and
//!    decorrelate those channels.
//!    None of the validator-encoded corpus fixtures exercise dep
//!    substreams (the corpus omits them per
//!    `eac3-5.1-side-768kbps/notes.md`); the in-tree 7.1 encoder
//!    (indep 5.1 + dep [Lrs/Rrs] pair) and the
//!    `splice_*_replace`/`_extend` unit tests cover the path.

use oxideav_core::bits::BitReader;
use oxideav_core::{Error, Result};

use crate::audblk::{Ac3State, SAMPLES_PER_BLOCK};

use super::audfrm::{self, AudFrm};
use super::bsi::{self, Bsi as Eac3Bsi, StreamType};
use super::chanmap::{self, ChannelLocation};
use super::dsp;
use super::joc::{JocMetadata, parse_ec3_extension_type_a, parse_joc_emdf};

/// E-AC-3 syncword — same value as base AC-3 (§E.2.2.1).
// internal — exposed for tests/fuzz; not part of the stable API
#[doc(hidden)]
pub const SYNCWORD: u16 = 0x0B77;

/// Per-decoder state that persists across packets.
///
/// Round 2 adds [`Ac3State`] — the per-channel exponent / bap /
/// overlap-add-delay state shared with the AC-3 decoder. We carry one
/// `Ac3State` per substream id so dependent + independent substreams
/// don't trample each other's delay lines (round 3 wires this).
#[derive(Default, Clone)]
pub struct Eac3DecoderState {
    /// Last successfully-decoded indep substream parameters. Used by
    /// dependent substreams to know how many channels to extend.
    pub last_indep: Option<IndepProgramShape>,
    /// Per-channel persistent DSP state for the **independent**
    /// substream. Carries exponent reuse + 256-sample IMDCT delay line
    /// across blocks and frames.
    indep_state: Ac3State,
    /// Per-channel persistent DSP state of the selected program's
    /// **dependent** substreams, indexed by dependent `substreamid`
    /// (§E.2.3.1.2: up to eight per independent substream, numbered in
    /// bit-stream order). Each substream owns its exponent / overlap
    /// history; the list grows on first sight of an id.
    dep_states: Vec<Ac3State>,
    /// Independent `substreamid` of the program this state decodes
    /// (§E.3.8.3: "The default program selection should always be
    /// Program 1", i.e. independent substream 0). Frames of every other
    /// program — independent substreams with another id and the
    /// dependent substreams that follow them — are skipped.
    program: u8,
    /// §E.3.8.2: "Decoders reproducing 5.1 or fewer channels from a
    /// program containing greater than 5.1 channels shall decode only
    /// independent substream 0 and skip all associated dependent
    /// substreams." `true` skips them.
    skip_dependent: bool,
    /// Scratch substream state for parse-only passes over a frame (the
    /// enhanced-coupling look-ahead, see
    /// [`decode_eac3_packet_with_next`]).
    scratch: Option<Box<Ac3State>>,
    /// Leave [`DecodedFrame::pcm_s16le`] empty: the caller takes the
    /// program from [`Self::indep_pcm_f32`] and does its own packing.
    skip_s16: bool,
    /// §E.3.2: verify the syncframe CRC before decoding and substitute
    /// a concealment signal when it fails. Off for a default state (the
    /// frame-level entry point is also used on hand-built frames); the
    /// registry decoder switches it on.
    verify_crc: bool,
    /// BSI of the last frame of each substream that passed the CRC
    /// check — slot 0 the independent substream, slot `1 + id` the
    /// dependent substream `id`. A frame whose CRC fails is concealed
    /// in the shape of its substream's last good frame: its own header
    /// bits cannot be trusted to describe the channel layout.
    last_good_bsi: Vec<Option<Eac3Bsi>>,
    /// DRC configuration, re-applied to dependent-substream states as
    /// they are created.
    drc: crate::drc::DrcSettings,
    /// Noise seed set through [`Self::reseed_noise`], re-applied to
    /// dependent-substream states as they are created.
    noise_seed: Option<u32>,
    /// Per-frame f32 PCM scratch — the indep substream's PCM in its
    /// native layout immediately after [`decode_indep_substream`]; if
    /// any dep substreams follow in the same packet, their channels
    /// are spliced in by [`decode_dep_substream`], growing this slot
    /// to `indep_nchans + dep_nchans`.
    indep_pcm_f32: Vec<f32>,
    /// Current channel count of [`Self::indep_pcm_f32`] — starts at
    /// the indep BSI's `nchans` and grows as dep substreams splice
    /// their channels in.
    indep_nchans: u16,
    /// Physical channel locations of the **independent** substream's
    /// own channels, in `indep_pcm_f32` slot order (natural
    /// `acmod`/`lfeon` order, Table 5.8). Populated by
    /// [`decode_indep_substream`]. Used by [`splice_dep_into_indep`]
    /// to implement the §E.3.8.2 dependent-substream combination rule:
    /// a dep channel whose location already exists in this list
    /// *replaces* the indep channel in place rather than being appended.
    indep_locations: Vec<ChannelLocation>,
    /// Samples-per-frame (per channel) of [`Self::indep_pcm_f32`].
    indep_samples_per_frame: u32,
    /// Declared `skipfld` byte payloads captured while decoding the most
    /// recent substream. JOC carriage is parsed from this typed boundary
    /// instead of probing arbitrary compressed packet bytes.
    skip_fields: Vec<Vec<u8>>,
    /// Per-frame error string (last seen). Diagnostic only.
    pub last_error: Option<String>,
    /// Cumulative count of substream frames this decoder state emitted
    /// as silence (the zero-fill fallback) because the frame failed to
    /// parse or DSP-decode. A conformant stream decodes with this
    /// counter staying at 0 — callers observing audible dropouts (the
    /// issue-#13 symptom) can poll it to distinguish decoder-side
    /// zero-fill from genuinely silent program audio. Counts each
    /// affected substream frame (indep or dep) once. Since the
    /// concealment rework the substituted signal is the overlap tail of
    /// the last good block followed by silence, not a hard cut.
    pub frames_zero_filled: u64,
    /// Subset of [`Self::frames_zero_filled`]: frames replaced because
    /// their CRC word did not verify (§E.3.2). Only counted while CRC
    /// verification is on.
    pub frames_crc_failed: u64,
    /// Physical channel locations of dep-substream channels that were
    /// spliced into [`Self::indep_pcm_f32`] during the most recent
    /// packet, in the order they were appended. Always one entry per
    /// dep coded channel, matching the §E.2.3.1.8 chanmap expansion
    /// (Table E2.5). Empty when no dep substream contributed, or when
    /// the dep substream's `chanmape == 0` (in which case the dep
    /// channels are routed by the spec's "channel locations apply in
    /// the natural order" default for the dep's `acmod` / `lfeon` —
    /// see [`Self::default_dep_locations`]).
    pub dep_locations: Vec<ChannelLocation>,
}

impl Eac3DecoderState {
    /// Re-seed the decoder-private pseudo-random generators — the
    /// §7.3.4 zero-bit dither and the §E.3.6.4.2 SPX noise — of both
    /// substream states. The spec leaves these sequences to the decoder
    /// ("any reasonably random sequence"), so two decodes of one stream
    /// under different seeds differ exactly by that noise: conformance
    /// harnesses use the pair to measure the decoder-private noise
    /// ceiling of a stream before comparing against another decoder.
    /// `seed == 0` is mapped to a fixed non-zero value (both generators
    /// stall on an all-zero state).
    #[doc(hidden)]
    pub fn reseed_noise(&mut self, seed: u32) {
        let s = if seed == 0 { 0x6D2B_79F5 } else { seed };
        self.noise_seed = Some(s);
        for st in std::iter::once(&mut self.indep_state).chain(self.dep_states.iter_mut()) {
            st.dither_lfsr_state = s;
            st.spx_noise_lfsr = s.rotate_left(16) | 1;
        }
    }

    /// Select the program to decode by its independent `substreamid`
    /// (0..=7, §E.3.8.3 / §E.3.8.4). The default is program 0. Changing
    /// the selection clears the per-substream history.
    pub fn set_program(&mut self, substreamid: u8) {
        let id = substreamid & 7;
        if id != self.program {
            self.program = id;
            self.indep_state.reset_stream_state();
            self.dep_states.clear();
            self.last_good_bsi.clear();
        }
    }

    /// Independent `substreamid` of the program being decoded.
    pub fn program(&self) -> u8 {
        self.program
    }

    /// Reproduce the independent substream alone (`false`) or the
    /// complete program with its dependent substreams (`true`, the
    /// default). §E.3.8.2: a decoder reproducing 5.1 or fewer channels
    /// "shall decode only independent substream 0 and skip all
    /// associated dependent substreams" — a dependent substream may
    /// replace channels of the 5.1 program with the versions meant for
    /// the larger layout.
    pub fn set_dependent_substreams(&mut self, decode: bool) {
        self.skip_dependent = !decode;
    }

    /// Would decoding the access unit `data` benefit from the access
    /// unit after it — does a frame of the selected program end in a
    /// block that uses enhanced coupling (§3.5.5.1)? A caller that can
    /// wait for the next access unit passes it to
    /// [`decode_eac3_packet_with_next`].
    pub fn wants_next(&mut self, data: &[u8]) -> bool {
        let Ok((units, _)) = locate_units(self, data) else {
            return false;
        };
        units.iter().any(|unit| {
            let mut br = BitReader::new(&unit.frame[2..]);
            if br.skip(unit.bsi_bits).is_err() {
                return false;
            }
            let audfrm = audfrm::parse_with(&mut br, &unit.bsi);
            // The CRC pass only when the answer would be yes (a frame
            // that fails it is concealed and needs no look-ahead).
            tail_uses_ecpl(self, unit, &audfrm)
                && (!self.verify_crc
                    || crate::crc::verify_eac3_syncframe(unit.frame, unit.frame.len()).crc2_ok
                        == Some(true))
        })
    }

    /// Skip the 16-bit packing of [`DecodedFrame::pcm_s16le`] (left
    /// empty) for a caller that reads [`Self::indep_pcm_f32`] instead.
    #[doc(hidden)]
    pub fn set_s16_output(&mut self, pack: bool) {
        self.skip_s16 = !pack;
    }

    /// Turn the §E.3.2 CRC check on or off. With the check on, a
    /// syncframe whose CRC word does not verify is not parsed: its
    /// channels carry the concealment signal instead.
    pub fn set_crc_check(&mut self, on: bool) {
        self.verify_crc = on;
    }

    /// DSP state of dependent substream `id`, created on first use with
    /// the decoder's DRC configuration and noise seed.
    fn dep_state_mut(&mut self, id: u8) -> &mut Ac3State {
        let idx = usize::from(id & 7);
        while self.dep_states.len() <= idx {
            let mut st = Ac3State::new();
            st.drc = self.drc;
            if let Some(s) = self.noise_seed {
                st.dither_lfsr_state = s;
                st.spx_noise_lfsr = s.rotate_left(16) | 1;
            }
            self.dep_states.push(st);
        }
        &mut self.dep_states[idx]
    }

    /// Read-only view of the indep substream's per-frame interleaved
    /// f32 PCM (range -1..1). After [`decode_eac3_packet`] this is the
    /// indep substream's PCM in `(acmod, lfeon)` bitstream order; if
    /// any dep substreams contributed in the same packet, their
    /// channels have been combined in per §E.3.8.2 — replacing the
    /// matching indep slot when the location is shared, or extending
    /// the buffer when it is new (see
    /// [`crate::eac3::decoder::splice_dep_into_indep`]).
    ///
    /// `process_eac3_frame` consumes this directly when running the
    /// §7.8 downmix matrix so it can apply per-channel coefficients
    /// before quantising to S16LE — matrix×S16 would lose 6+ dB of
    /// headroom per cascade step on negatively-signed weights.
    pub fn indep_pcm_f32(&self) -> &[f32] {
        &self.indep_pcm_f32
    }

    /// Current channel count of [`Self::indep_pcm_f32`].
    pub fn indep_nchans(&self) -> u16 {
        self.indep_nchans
    }

    /// Propagate §7.7 DRC control settings into both the independent and
    /// dependent substream DSP states so the next packet's `dynrng` /
    /// `compr` decode (in [`crate::eac3::dsp`]) applies the requested
    /// regime. Called by the decoder whenever the caller changes its DRC
    /// configuration.
    pub fn set_drc(&mut self, drc: crate::drc::DrcSettings) {
        self.drc = drc;
        self.indep_state.drc = drc;
        for st in self.dep_states.iter_mut() {
            st.drc = drc;
        }
    }

    /// Channel-location list assigned to the dep substream when
    /// `chanmape == 0` (no custom channel map present), per
    /// §E.2.3.1.7: "the channel map for a dependent substream shall
    /// be defined by the audio coding mode" (the dep substream's
    /// `acmod` + `lfeon`).
    ///
    /// Maps the dep substream's `acmod` to the natural Table 5.8
    /// channel order (L, C, R, Ls, Rs, …) plus LFE when `lfeon`.
    /// `acmod == 0` (1+1 dual mono) is treated as two anonymous
    /// channels assigned to `Left` / `Right` slots.
    pub fn default_dep_locations(acmod: u8, lfeon: bool) -> Vec<ChannelLocation> {
        let mut out: Vec<ChannelLocation> = Vec::with_capacity(6);
        match acmod {
            0 => {
                // 1+1 dual mono — two independent channels with no
                // canonical assignment. Default to L / R.
                out.push(ChannelLocation::Left);
                out.push(ChannelLocation::Right);
            }
            1 => {
                // 1/0 mono — Center.
                out.push(ChannelLocation::Center);
            }
            2 => {
                // 2/0 stereo — L, R.
                out.push(ChannelLocation::Left);
                out.push(ChannelLocation::Right);
            }
            3 => {
                // 3/0 — L, C, R.
                out.push(ChannelLocation::Left);
                out.push(ChannelLocation::Center);
                out.push(ChannelLocation::Right);
            }
            4 => {
                // 2/1 — L, R, S (treated as center-surround per
                // Table E2.5 bit 7).
                out.push(ChannelLocation::Left);
                out.push(ChannelLocation::Right);
                out.push(ChannelLocation::CenterSurround);
            }
            5 => {
                // 3/1 — L, C, R, S.
                out.push(ChannelLocation::Left);
                out.push(ChannelLocation::Center);
                out.push(ChannelLocation::Right);
                out.push(ChannelLocation::CenterSurround);
            }
            6 => {
                // 2/2 — L, R, Ls, Rs.
                out.push(ChannelLocation::Left);
                out.push(ChannelLocation::Right);
                out.push(ChannelLocation::LeftSurround);
                out.push(ChannelLocation::RightSurround);
            }
            7 => {
                // 3/2 — L, C, R, Ls, Rs.
                out.push(ChannelLocation::Left);
                out.push(ChannelLocation::Center);
                out.push(ChannelLocation::Right);
                out.push(ChannelLocation::LeftSurround);
                out.push(ChannelLocation::RightSurround);
            }
            _ => {}
        }
        if lfeon {
            out.push(ChannelLocation::Lfe);
        }
        out
    }
}

/// Snapshot of the most recent independent substream's output shape
/// — channels, sample rate, samples per frame.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct IndepProgramShape {
    pub nchans: u16,
    pub sample_rate: u32,
    pub samples_per_frame: u32,
}

impl IndepProgramShape {
    /// Build a [`IndepProgramShape`] from every field.
    pub fn new(nchans: u16, sample_rate: u32, samples_per_frame: u32) -> Self {
        Self {
            nchans,
            sample_rate,
            samples_per_frame,
        }
    }
}

/// Result of decoding one E-AC-3 packet.
#[derive(Clone, Debug)]
pub struct DecodedFrame {
    pub sample_rate: u32,
    pub channels: u16,
    /// PCM samples per channel (= `bsi.num_blocks * 256`).
    pub samples: u32,
    /// Interleaved S16LE bytes — `samples * channels * 2` total.
    pub pcm_s16le: Vec<u8>,
    /// Independent substream's `acmod` field (Table E1.2 §E.1.2.1).
    /// Surfaced so callers that need to reorder bitstream-order
    /// multichannel layouts into WAV-mask order can pick the right
    /// permutation via [`crate::wave_order`]. For dep-substream-extended
    /// programs (e.g. 7.1 emitted as indep 5.1 + dep [Lb,Rb]) this
    /// reflects the **indep** acmod only — any dep channels at *new*
    /// locations are appended at the end of the PCM buffer per the
    /// §E.3.8.2 rule in `splice_dep_into_indep` and are not covered by
    /// Table 5.8 (dep channels at a *shared* location replace the
    /// matching indep slot in place and do not grow the layout).
    pub acmod: u8,
    /// Independent substream's `lfeon` flag (1 bit, §E.1.2.1).
    pub lfeon: bool,
    /// Independent substream's `nfchans` (= `acmod_nfchans(acmod)`).
    /// Surfaced so callers building a [`crate::downmix::Downmix`] don't
    /// have to re-derive it from `channels - lfeon as u16`.
    pub nfchans: u8,
    /// Independent substream's Annex E mixmdata refinement
    /// (§E.2.3.1.3-6). `Some` only when `mixmdate == 1` AND at least
    /// one of the four 3-bit codes' per-channel guards fired
    /// (`acmod > 2` for center codes, `acmod & 0x4` for surround
    /// codes). When `Some`, the [`crate::downmix::Downmix::from_eac3_bsi`]
    /// constructor uses these as overrides for the §7.8.2 fixed-0.707
    /// LtRt defaults and the 0.707 LoRo defaults.
    pub annex_e_mix_levels: Option<crate::bsi::AnnexDMixLevels>,
    /// Physical channel locations for the dep-substream channels that
    /// *extended* the indep program (i.e. were appended because their
    /// location was new), in append order (§E.3.8.2 / §E.2.3.1.7-8).
    /// One entry per extending dep coded channel, derived from
    /// `chanmap` (Table E2.5) when `chanmape == 1` or from the dep
    /// substream's `acmod`/`lfeon` natural order when `chanmape == 0`.
    /// Empty when no dep substream contributed, or when every dep
    /// channel *replaced* a shared indep location (§E.3.8.2 replace).
    ///
    /// The indep program's channels occupy slots `0..indep_nchans`
    /// (replacements overwrite the matching slot in place); the
    /// extending dep channels occupy the appended slots in this order.
    pub dep_locations: Vec<ChannelLocation>,
    /// Independent substream's 5-bit `dialnorm` word (§E.2.3.1.x, headroom
    /// in dB below digital 100%, `1..=31`). Surfaced so a caller that owns
    /// the playback gain can apply §7.6 dialogue normalisation toward a
    /// chosen target level. Advisory — the core decode does not scale by
    /// it (the spec leaves dialnorm to the reproduction system).
    pub dialnorm: u8,
    /// Validated JOC/OAMD metadata for this packet, when present. Kept
    /// crate-private until a standalone metadata API is designed; the
    /// opt-in stereo presentation consumes it internally.
    pub(crate) joc_metadata: Option<JocMetadata>,
}

/// One syncframe of the selected program, located by the packet walk.
struct Unit<'a> {
    /// The syncframe's bytes.
    frame: &'a [u8],
    bsi: Eac3Bsi,
    /// Bits of `frame[2..]` the BSI occupies.
    bsi_bits: u32,
    is_indep: bool,
}

/// What a dependent substream contributes to its program.
struct DepOutput {
    floats: Vec<f32>,
    nchans: usize,
    chanmap: Option<u16>,
    locations: Vec<ChannelLocation>,
    joc: Option<JocMetadata>,
}

/// Decode one or more concatenated E-AC-3 syncframes contained in a
/// single packet: one access unit — the selected program's independent
/// syncframe and the dependent syncframes that follow it (frames of
/// other programs are skipped).
///
/// The packet must begin with a 16-bit `0x0B77` syncword. Subsequent
/// syncframes are located via the BSI's `frame_bytes` field. Returns
/// the program's PCM: the independent substream's channels, with the
/// dependent substreams' channels combined in per §E.3.8.2 unless
/// [`Eac3DecoderState::set_dependent_substreams`] switched them off.
pub fn decode_eac3_packet(state: &mut Eac3DecoderState, data: &[u8]) -> Result<DecodedFrame> {
    decode_eac3_packet_with_next(state, data, None)
}

/// Locate the syncframes of the selected program in `data` (§E.3.8.3 /
/// §E.3.8.4: the selected independent substream and the dependent
/// substreams that follow it; every other program's frames are
/// skipped). A defect after the program's independent frame (trailing
/// bytes that are not a syncframe, a frame cut short by the packet)
/// ends the walk, keeps the frames found and is returned as a note;
/// the same defect before it is an error.
fn locate_units<'a>(
    state: &Eac3DecoderState,
    data: &'a [u8],
) -> Result<(Vec<Unit<'a>>, Option<String>)> {
    let mut units: Vec<Unit<'a>> = Vec::new();
    let mut have_program = false;
    let mut in_selected_program = false;
    let mut off = 0usize;
    let mut note = None;
    while off + 4 <= data.len() {
        // §E.2.2.1 — syncword.
        let sync = u16::from_be_bytes([data[off], data[off + 1]]);
        let mut br = BitReader::new(&data[off + 2..]);
        let defect = if sync != SYNCWORD {
            Some(format!(
                "eac3: bad syncword 0x{sync:04X} at offset {off} (expected 0x0B77)"
            ))
        } else {
            None
        };
        // BSI starts at byte off+2.
        let parsed = match defect {
            Some(msg) => Err(Error::invalid(msg)),
            None => bsi::parse_with(&mut br).and_then(|bsi| {
                let frame_bytes = bsi.frame_bytes as usize;
                if off + frame_bytes > data.len() {
                    Err(Error::invalid(format!(
                        "eac3: syncframe at offset {off} claims {frame_bytes} bytes but packet has only {}",
                        data.len() - off
                    )))
                } else if matches!(bsi.strmtyp, StreamType::Reserved) {
                    Err(Error::invalid("eac3: strmtyp '11' is reserved"))
                } else {
                    Ok(bsi)
                }
            }),
        };
        let bsi = match parsed {
            Ok(bsi) => bsi,
            Err(e) if have_program => {
                note = Some(format!("{e}"));
                break;
            }
            Err(e) => return Err(e),
        };
        let frame_bytes = bsi.frame_bytes as usize;
        let is_indep = !matches!(bsi.strmtyp, StreamType::Dependent);
        if is_indep {
            in_selected_program = bsi.substreamid == state.program;
        }
        if in_selected_program && (is_indep || !state.skip_dependent) {
            have_program |= is_indep;
            units.push(Unit {
                frame: &data[off..off + frame_bytes],
                bsi_bits: br.bit_position() as u32,
                bsi,
                is_indep,
            });
        }
        off += frame_bytes;
    }
    Ok((units, note))
}

/// [`decode_eac3_packet`] with a look at the access unit that follows.
///
/// Enhanced coupling (§3.5.5.1) builds the coupling channel of a block
/// from the previous, the current and the NEXT block, so the last
/// block of a syncframe needs the first block of the following one.
/// `next` is the following access unit of the same stream (or `None`
/// at the end of the stream / when it is not at hand): its first block
/// is parsed, not rendered, on a scratch state. Without it that block
/// is taken not to use enhanced coupling, which leaves time-domain
/// aliasing in the last block of every frame of a stream that does.
/// Streams without enhanced coupling decode identically either way.
pub fn decode_eac3_packet_with_next(
    state: &mut Eac3DecoderState,
    data: &[u8],
    next: Option<&[u8]>,
) -> Result<DecodedFrame> {
    if data.len() < 4 {
        return Err(Error::invalid("eac3: packet too short for syncinfo"));
    }
    state.last_error = None;
    state.dep_locations.clear();

    // ---- Pass 1: locate the selected program's syncframes. ----
    let (units, note) = locate_units(state, data)?;
    if note.is_some() {
        state.last_error = note;
    }
    // The following access unit's frames, for the look-ahead: its
    // first period only.
    let next_units: Vec<Unit<'_>> = match next {
        Some(bytes) if bytes.len() >= 4 => {
            let mut found = locate_units(state, bytes).map_or_else(|_| Vec::new(), |(u, _)| u);
            if let Some(end) = found.iter().skip(1).position(|u| u.is_indep) {
                found.truncate(end + 1);
            }
            found
        }
        _ => Vec::new(),
    };

    // ---- Pass 2: decode, one period (independent frame + its
    // dependent frames) at a time. ----
    let mut indep_pcm: Option<DecodedFrame> = None;
    let mut joc_metadata: Option<JocMetadata> = None;
    let mut spliced = false;
    let mut i = 0usize;
    while i < units.len() {
        if !units[i].is_indep {
            // A dependent frame with no independent frame before it in
            // the packet has nothing to extend.
            state.last_error = Some(
                "eac3 dep: dependent substream with no preceding independent substream".into(),
            );
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < units.len() && !units[j].is_indep {
            j += 1;
        }
        let (indep, deps) = (&units[i], &units[i + 1..j]);
        state.dep_locations.clear();
        spliced = false;

        // §E.3.8.5: with dependent substreams reproduced, the dynamic
        // range words of the LAST one govern the whole program — so it
        // is decoded first, and its per-block gains replace every
        // other substream's own.
        let mut dep_out: Vec<Option<DepOutput>> = Vec::new();
        dep_out.resize_with(deps.len(), || None);
        let mut program_gains = None;
        if let Some((last, earlier)) = deps.split_last() {
            let (out, gains) = decode_dep_unit(state, last, &indep.bsi, None, &next_units);
            program_gains = gains;
            dep_out[deps.len() - 1] = out;
            for (slot, unit) in dep_out.iter_mut().zip(earlier) {
                *slot = decode_dep_unit(state, unit, &indep.bsi, program_gains, &next_units).0;
            }
        }

        let (bsi, audfrm, mut br) = open_unit(state, indep);
        let carrier = next_carrier(state, indep, &audfrm, &next_units);
        let pcm = decode_indep_substream(
            state,
            &bsi,
            audfrm,
            &mut br,
            program_gains,
            carrier.as_deref(),
        );
        if let Some(metadata) = decode_substream_joc(&bsi, &state.skip_fields) {
            joc_metadata = Some(metadata);
        }
        state.last_indep = Some(IndepProgramShape {
            nchans: pcm.channels,
            sample_rate: pcm.sample_rate,
            samples_per_frame: pcm.samples,
        });
        indep_pcm = Some(pcm);

        // Combine the dependent substreams in bit-stream order
        // (§E.3.8.2). Only the *extending* channel locations are
        // recorded: replaced channels keep their slot.
        for out in dep_out.into_iter().flatten() {
            let extended =
                splice_dep_into_indep(state, out.nchans, out.chanmap, &out.floats, &out.locations);
            state.dep_locations.extend(extended);
            spliced = true;
            if out.joc.is_some() {
                joc_metadata = out.joc;
            }
        }
        i = j;
    }

    let mut pcm = indep_pcm.ok_or_else(|| {
        Error::invalid(format!(
            "eac3: packet contains no frame of independent substream {}",
            state.program
        ))
    })?;

    // Rebuild the final S16 buffer from the [`indep_pcm_f32`] scratch
    // when a dependent substream was combined in: its channels replaced
    // slots of the independent program and / or extended it.
    if spliced {
        pcm.channels = state.indep_nchans;
        if !state.skip_s16 {
            pcm.pcm_s16le = pack_f32_to_s16le(&state.indep_pcm_f32);
        }
    }
    // Surface the accumulated dep-channel locations so callers know
    // the physical assignment of the appended channels without
    // re-parsing the chanmap. Empty when no dep substream
    // contributed.
    pcm.dep_locations.clone_from(&state.dep_locations);
    pcm.joc_metadata = joc_metadata;
    Ok(pcm)
}

/// Does the frame `unit` (whose audfrm is `audfrm`) need to look at the
/// first block of its substream's next frame — i.e. can its last block
/// use enhanced coupling? Coupling must be in use in that block, and
/// the substream must not be known to end its frames without enhanced
/// coupling; when nothing is known yet (stream start, after a
/// concealed frame) the frame itself is parsed on the scratch state to
/// find out.
fn tail_uses_ecpl(state: &mut Eac3DecoderState, unit: &Unit<'_>, audfrm: &Result<AudFrm>) -> bool {
    let Ok(audfrm) = audfrm else {
        return false;
    };
    let last = (unit.bsi.num_blocks as usize).saturating_sub(1);
    if !audfrm.cplinu_blk[last] {
        return false;
    }
    let known = if unit.is_indep {
        state.indep_state.ecpl_at_frame_end
    } else {
        state
            .dep_states
            .get(usize::from(unit.bsi.substreamid & 7))
            .and_then(|st| st.ecpl_at_frame_end)
    };
    if let Some(known) = known {
        return known;
    }
    let scratch = state
        .scratch
        .get_or_insert_with(|| Box::new(Ac3State::new()));
    let mut br = BitReader::new(&unit.frame[2..]);
    let fits = br.skip(unit.bsi_bits).is_ok()
        && audfrm::parse_with(&mut br, &unit.bsi).is_ok_and(|af| {
            dsp::probe_frame(
                &unit.bsi,
                &af,
                &mut br,
                scratch,
                crate::audblk::Probe::TailCoupling,
            )
        });
    fits && scratch.ecpl_at_frame_end == Some(true)
}

/// The enhanced-coupling channel coefficients of the first block of
/// the frame that follows `unit` in its substream, when `unit`'s last
/// block needs them (§3.5.5.1) and that frame is among `next_units`,
/// intact, and opens with enhanced coupling.
fn next_carrier(
    state: &mut Eac3DecoderState,
    unit: &Unit<'_>,
    audfrm: &Result<AudFrm>,
    next_units: &[Unit<'_>],
) -> Option<Box<[f32; crate::audblk::N_COEFFS]>> {
    let follower = next_units
        .iter()
        .find(|n| n.is_indep == unit.is_indep && n.bsi.substreamid == unit.bsi.substreamid)?;
    if !tail_uses_ecpl(state, unit, audfrm) {
        return None;
    }
    let frame = follower.frame;
    if state.verify_crc
        && crate::crc::verify_eac3_syncframe(frame, frame.len()).crc2_ok != Some(true)
    {
        return None;
    }
    let mut br = BitReader::new(&frame[2..]);
    br.skip(follower.bsi_bits).ok()?;
    let audfrm = audfrm::parse_with(&mut br, &follower.bsi).ok()?;
    if !audfrm.cplinu_blk[0] {
        return None;
    }
    let scratch = state
        .scratch
        .get_or_insert_with(|| Box::new(Ac3State::new()));
    dsp::probe_frame(
        &follower.bsi,
        &audfrm,
        &mut br,
        scratch,
        crate::audblk::Probe::FirstBlockCarrier,
    );
    scratch.probe_carrier.take()
}

/// Prepare one syncframe for decoding: CRC verdict (§E.3.2), the BSI
/// whose shape the frame is rendered in, the parsed audfrm (or the
/// reason there is none) and a bit reader — bounded to this syncframe,
/// so unpacking can never continue into the next one (§7.10.2 item 21)
/// — positioned at the first audio block.
fn open_unit<'u, 'a>(
    state: &mut Eac3DecoderState,
    unit: &'u Unit<'a>,
) -> (std::borrow::Cow<'u, Eac3Bsi>, Result<AudFrm>, BitReader<'a>) {
    let frame = unit.frame;
    let crc_ok = !state.verify_crc
        || crate::crc::verify_eac3_syncframe(frame, frame.len()).crc2_ok == Some(true);
    let mut br = BitReader::new(&frame[2.min(frame.len())..]);
    let header_fits = br.skip(unit.bsi_bits).is_ok();
    let slot = if unit.is_indep {
        0
    } else {
        1 + usize::from(unit.bsi.substreamid & 7)
    };
    if state.last_good_bsi.len() <= slot {
        state.last_good_bsi.resize(slot + 1, None);
    }
    // The check only condemns a frame once the substream has shown a
    // CRC word that verifies: a stream whose CRC words are unusable
    // throughout (never one good frame) is decoded as it stands rather
    // than muted.
    let crc_failed = !crc_ok && state.last_good_bsi[slot].is_some();
    if crc_failed {
        state.frames_crc_failed += 1;
        // Conceal in the substream's established shape when one is
        // known and spans the same number of blocks: a damaged frame's
        // own header bits cannot be trusted to describe the layout.
        let shape = match &state.last_good_bsi[slot] {
            Some(good) if good.num_blocks == unit.bsi.num_blocks => {
                std::borrow::Cow::Owned(good.clone())
            }
            _ => std::borrow::Cow::Borrowed(&unit.bsi),
        };
        return (
            shape,
            Err(Error::invalid("eac3: syncframe CRC mismatch")),
            br,
        );
    }
    if state.verify_crc && crc_ok && state.last_good_bsi[slot].as_ref() != Some(&unit.bsi) {
        state.last_good_bsi[slot] = Some(unit.bsi.clone());
    }
    // Audfrm follows BSI in the same bit cursor.
    let audfrm = if header_fits {
        audfrm::parse_with(&mut br, &unit.bsi)
    } else {
        Err(Error::invalid("eac3: syncframe shorter than its BSI"))
    };
    (std::borrow::Cow::Borrowed(&unit.bsi), audfrm, br)
}

/// Decode the Extension Type A signal and a matching JOC/OAMD EMDF
/// container carried by a declared audio-block skip field. Optional
/// metadata never makes the compatibility presentation fail: malformed or
/// unsupported payloads simply return `None` and the caller keeps the
/// channel-based decode.
fn decode_substream_joc(bsi: &Eac3Bsi, skip_fields: &[Vec<u8>]) -> Option<JocMetadata> {
    let signal = parse_ec3_extension_type_a(bsi.addbsi.as_ref()?.payload())
        .ok()
        .flatten()?;
    skip_fields
        .iter()
        .filter(|field| field.starts_with(&[0x58, 0x38]))
        .find_map(|field| parse_joc_emdf(field, signal).ok())
}

/// Decode one independent substream's audblks. A frame that cannot be
/// decoded — a failed CRC, an audfrm that does not parse, a DSP error —
/// is replaced by the concealment signal (the last good block's overlap
/// tail, then silence) and the substream's stream state is cleared so
/// the next frame starts clean; the returned frame always has the
/// shape the BSI announces.
fn decode_indep_substream(
    state: &mut Eac3DecoderState,
    bsi: &Eac3Bsi,
    audfrm: Result<AudFrm>,
    br: &mut BitReader<'_>,
    program_gains: Option<[f32; crate::audblk::BLOCKS_PER_FRAME]>,
    next_carrier: Option<&[f32; crate::audblk::N_COEFFS]>,
) -> DecodedFrame {
    let samples = bsi.num_blocks as u32 * SAMPLES_PER_BLOCK as u32;
    let nchans = bsi.nchans as usize;
    let mut floats = std::mem::take(&mut state.indep_pcm_f32);
    floats.clear();
    floats.resize(samples as usize * nchans, 0.0);

    let dsp_result = decode_or_conceal(
        bsi,
        audfrm,
        br,
        &mut state.indep_state,
        &mut floats,
        &mut state.skip_fields,
        FrameContext {
            program_gains,
            next_carrier,
        },
    );
    if let Err(e) = &dsp_result {
        state.last_error = Some(format!("{e}"));
        state.frames_zero_filled += 1;
    }

    // Cache the indep f32 PCM in its native (acmod, lfeon) layout
    // so any subsequent dep substream in the same packet can append
    // its chanmap-routed channels (round 3).
    state.indep_pcm_f32 = floats;
    state.indep_nchans = nchans as u16;
    state.indep_samples_per_frame = samples;
    // Record the indep substream's own per-channel locations (natural
    // acmod/lfeon order, Table 5.8) so a following dep substream can
    // tell which of its channels *replace* an indep channel (shared
    // location, §E.3.8.2) versus *extend* the program (new location).
    state.indep_locations = Eac3DecoderState::default_dep_locations(bsi.acmod, bsi.lfeon);

    // Pack indep PCM only. If a dep substream follows, the packet-
    // level driver will rebuild the final S16 buffer in
    // `decode_eac3_packet` after all substreams are walked.
    let pcm_s16le = if state.skip_s16 {
        Vec::new()
    } else {
        pack_f32_to_s16le(&state.indep_pcm_f32)
    };

    DecodedFrame {
        sample_rate: bsi.sample_rate,
        channels: bsi.nchans as u16,
        samples,
        pcm_s16le,
        acmod: bsi.acmod,
        lfeon: bsi.lfeon,
        nfchans: bsi.nfchans,
        annex_e_mix_levels: bsi.annex_e_mix_levels,
        // Indep-substream-only emit: no dep channels yet. The
        // packet-level driver overwrites this with the accumulated
        // `state.dep_locations` if dep substreams follow.
        dep_locations: Vec::new(),
        dialnorm: bsi.dialnorm,
        joc_metadata: None,
    }
}

/// What a substream frame takes from the rest of its program and
/// stream.
#[derive(Clone, Copy)]
struct FrameContext<'a> {
    /// §E.3.8.5: the program's per-block dynamic-range gains, when
    /// they replace the substream's own.
    program_gains: Option<[f32; crate::audblk::BLOCKS_PER_FRAME]>,
    /// §3.5.5.1: the enhanced-coupling channel of the first block of
    /// the substream's next frame.
    next_carrier: Option<&'a [f32; crate::audblk::N_COEFFS]>,
}

/// Run the audblk DSP for one substream frame into `out`; on any
/// failure render the concealment signal instead and clear the
/// substream's stream state (decoder configuration and noise
/// generators survive). Returns the failure, if any, for the caller's
/// bookkeeping — `out` is fully written either way.
fn decode_or_conceal(
    bsi: &Eac3Bsi,
    audfrm: Result<AudFrm>,
    br: &mut BitReader<'_>,
    st: &mut Ac3State,
    out: &mut [f32],
    skip_fields: &mut Vec<Vec<u8>>,
    context: FrameContext<'_>,
) -> Result<()> {
    let FrameContext {
        program_gains,
        next_carrier,
    } = context;
    // The overlap tails as they stood before this frame: a frame that
    // fails part-way has already advanced them through blocks whose
    // content cannot be trusted.
    let tails: [[f32; SAMPLES_PER_BLOCK]; crate::audblk::MAX_CHANNELS] =
        std::array::from_fn(|ch| st.channels[ch].delay);
    st.gain_override = program_gains;
    let result = audfrm.and_then(|audfrm| {
        dsp::decode_audblks(bsi, &audfrm, br, st, out, skip_fields, next_carrier)
    });
    st.gain_override = None;
    if result.is_err() {
        skip_fields.clear();
        for (ch, tail) in tails.iter().enumerate() {
            st.channels[ch].delay = *tail;
        }
        crate::audblk::conceal_frame(st, bsi.nfchans as usize, bsi.lfeon, out);
    }
    result
}

/// Decode one dependent substream of the period whose independent
/// frame has the BSI `indep`. Returns what it contributes to the
/// program — `None` when it cannot be placed (block count unlike the
/// independent frame's, unresolvable channel map) — and, when it
/// decoded, the per-block dynamic-range gains its own words resolved
/// to (§E.3.8.5). A frame that cannot be decoded contributes the
/// concealment signal, so the program keeps its channel layout.
fn decode_dep_unit(
    state: &mut Eac3DecoderState,
    unit: &Unit<'_>,
    indep: &Eac3Bsi,
    program_gains: Option<[f32; crate::audblk::BLOCKS_PER_FRAME]>,
    next_units: &[Unit<'_>],
) -> (
    Option<DepOutput>,
    Option<[f32; crate::audblk::BLOCKS_PER_FRAME]>,
) {
    let (bsi, audfrm, mut br) = open_unit(state, unit);
    let carrier = next_carrier(state, unit, &audfrm, next_units);
    // §E.2.3.1.2: dependent substreams "must contain the same number
    // of blocks per syncframe ... as the independent substream with
    // which they are associated".
    if bsi.num_blocks != indep.num_blocks {
        state.last_error = Some(format!(
            "eac3 dep: {} blocks per syncframe, independent substream has {}",
            bsi.num_blocks, indep.num_blocks
        ));
        return (None, None);
    }
    let samples = bsi.num_blocks as usize * SAMPLES_PER_BLOCK;
    let nchans = bsi.nchans as usize;

    // Resolve the dep substream's per-channel location list per
    // §E.2.3.1.7-8. When `chanmape == 1` the chanmap field selects
    // locations from Table E2.5 and the expanded count MUST equal the
    // coded channel count (spec invariant). When `chanmape == 0` the
    // locations default to the natural-acmod order (e.g. acmod=2 →
    // [Left, Right]).
    let locations = match bsi.chanmap {
        Some(map) => match chanmap::expand_chanmap_locations(map, nchans as u8) {
            Ok(locs) => locs,
            Err(e) => {
                state.last_error = Some(format!("eac3 dep chanmap: {e}"));
                return (None, None);
            }
        },
        None => Eac3DecoderState::default_dep_locations(bsi.acmod, bsi.lfeon),
    };

    let mut floats = vec![0.0f32; samples * nchans];
    let mut skip_fields = Vec::new();
    let st = state.dep_state_mut(bsi.substreamid);
    let result = decode_or_conceal(
        &bsi,
        audfrm,
        &mut br,
        st,
        &mut floats,
        &mut skip_fields,
        FrameContext {
            program_gains,
            next_carrier: carrier.as_deref(),
        },
    );
    let gains = result.is_ok().then_some(st.block_gains);
    if let Err(e) = &result {
        state.last_error = Some(format!("{e}"));
        state.frames_zero_filled += 1;
    }
    let joc = decode_substream_joc(&bsi, &skip_fields);
    (
        Some(DepOutput {
            floats,
            nchans,
            chanmap: bsi.chanmap,
            locations,
            joc,
        }),
        gains,
    )
}

/// Pack interleaved f32 PCM (range -1..1) into S16LE bytes.
fn pack_f32_to_s16le(floats: &[f32]) -> Vec<u8> {
    let mut out = vec![0u8; floats.len() * 2];
    for (dst, s) in out.as_chunks_mut::<2>().0.iter_mut().zip(floats) {
        *dst = ((s * 32767.0).clamp(-32768.0, 32767.0) as i16).to_le_bytes();
    }
    out
}

/// Splice the dep substream's channels into the indep PCM scratch per
/// the §E.3.8.2 channel-and-program-extension combination rule.
///
/// A/52:2018 §E.3.8.2 (Decoding a Single Program with Greater than 5.1
/// Channels):
///
/// > If channels are present in the dependent substream that
/// > correspond to channels in the associated independent substream,
/// > then the dependent substream data for those channels replaces the
/// > independent substream data for the corresponding channels. All
/// > channels present in the dependent substream that do not correspond
/// > to channels in the independent substream are used to enable output
/// > for speaker configurations with greater than 5.1 channels.
///
/// The same paragraph also covers `chanmape == 0`, where the dep
/// substream's `acmod`/`lfeon` identify its channels and "the
/// corresponding audio channels in the independent substream are
/// overwritten with the dependent audio channel data". Both cases
/// therefore reduce to: for each dep coded channel, *replace in place*
/// when its [`ChannelLocation`] already exists in the indep program,
/// otherwise *append* it as a new output channel.
///
/// Replacing rather than blind-appending is what keeps a real
/// greater-than-5.1 broadcast stream spatially correct — a dep
/// substream commonly re-codes the Center / LFE (or, with a custom
/// chanmap, even L/R; Table E2.5 shaded entries) that the indep
/// program already carries. A naive append duplicates and reorders
/// those channels, decorrelating the rendered layout.
///
/// `dep_locations[ch]` gives the physical location of dep coded
/// channel `ch` (resolved from the chanmap, or the natural acmod
/// order when `chanmape == 0`). Returns the locations of only the
/// channels that *extended* the program (the ones that were appended),
/// in append order, so the caller can record them on
/// [`Eac3DecoderState::dep_locations`].
fn splice_dep_into_indep(
    state: &mut Eac3DecoderState,
    dep_nchans: usize,
    chanmap: Option<u16>,
    dep_floats: &[f32],
    dep_locations: &[ChannelLocation],
) -> Vec<ChannelLocation> {
    let samples = state.indep_samples_per_frame as usize;
    let indep_nchans = state.indep_nchans as usize;

    // Partition the dep channels into "replace existing indep slot"
    // versus "extend". For replacement, find the indep slot index that
    // carries the same location. The indep_locations list is in
    // indep_pcm_f32 slot order, so its index *is* the slot index.
    // Defensive: if locations weren't recorded (shouldn't happen on a
    // real decode), fall back to extend-only so we never lose audio.
    let mut replace_slot: Vec<Option<usize>> = Vec::with_capacity(dep_nchans);
    let mut extend_locs: Vec<ChannelLocation> = Vec::new();
    for loc in dep_locations.iter().copied().take(dep_nchans) {
        match state.indep_locations.iter().position(|&l| l == loc) {
            Some(slot) => replace_slot.push(Some(slot)),
            None => {
                replace_slot.push(None);
                extend_locs.push(loc);
            }
        }
    }
    // Any dep channels beyond the resolved location list (malformed /
    // missing locations) are extended with an unknown location to
    // preserve their audio rather than drop it.
    for _ in dep_locations.len().min(dep_nchans)..dep_nchans {
        replace_slot.push(None);
        extend_locs.push(ChannelLocation::Left);
    }

    let num_extend = extend_locs.len();
    let new_nchans = indep_nchans + num_extend;

    let mut grown = vec![0.0f32; samples * new_nchans];
    for n in 0..samples {
        // Start from the indep program (which the in-place replacements
        // below overwrite slot-by-slot).
        for ch in 0..indep_nchans {
            grown[n * new_nchans + ch] = state.indep_pcm_f32[n * indep_nchans + ch];
        }
        let mut extend_cursor = 0usize;
        for (dch, slot) in replace_slot.iter().copied().enumerate() {
            let sample = dep_floats[n * dep_nchans + dch];
            match slot {
                // §E.3.8.2 replace: overwrite the matching indep slot.
                Some(s) => grown[n * new_nchans + s] = sample,
                // §E.3.8.2 extend: append in extend order.
                None => {
                    grown[n * new_nchans + indep_nchans + extend_cursor] = sample;
                    extend_cursor += 1;
                }
            }
        }
    }
    state.indep_pcm_f32 = grown;
    state.indep_nchans = new_nchans as u16;
    // The extended channels grow the location list; replaced channels
    // already have their location in indep_locations and keep their slot.
    state.indep_locations.extend(extend_locs.iter().copied());

    // Diagnostic: log the chanmap for any future bug hunts.
    if let Some(map) = chanmap
        && crate::audblk::debug_env("EAC3_TRACE_CHANMAP").is_some()
    {
        eprintln!(
            "TRACE-CHANMAP dep substream: chanmap=0x{map:04X} dep_nchans={dep_nchans} \
                 replaced={} extended={num_extend} → indep grown to {new_nchans} channels",
            dep_nchans - num_extend,
        );
    }

    extend_locs
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideav_core::bits::BitWriter;

    /// Build a minimal valid E-AC-3 syncframe of `frame_bytes` bytes:
    /// syncword + the BSI from `bsi_bits` + zero-padding through the
    /// payload. Used by parser smoke tests; not bit-stream conformant
    /// past the BSI/audfrm boundary.
    fn build_syncframe(frame_bytes: usize, bsi_bits: &[(u32, u32)]) -> Vec<u8> {
        let mut bw = BitWriter::with_capacity(frame_bytes);
        bw.write_u32(SYNCWORD as u32, 16);
        for &(n, v) in bsi_bits {
            bw.write_u32(v, n);
        }
        let mut buf = bw.into_bytes();
        if buf.len() < frame_bytes {
            buf.resize(frame_bytes, 0);
        } else {
            buf.truncate(frame_bytes);
        }
        buf
    }

    /// Build a stereo 6-block 768-byte indep syncframe + a stripped
    /// audfrm with zero strategy flags — enough that the round-1
    /// decoder produces a silent buffer of the right shape.
    fn stereo_768_indep() -> Vec<u8> {
        let bsi_bits: &[(u32, u32)] = &[
            (2, 0),    // strmtyp = indep
            (3, 0),    // substreamid
            (11, 383), // frmsiz → 768 bytes
            (2, 0),    // fscod = 48 kHz
            (2, 3),    // numblkscod = 3 → 6 blocks
            (3, 2),    // acmod = 2 (2/0)
            (1, 0),    // lfeon
            (5, 16),   // bsid
            (5, 27),   // dialnorm
            (1, 0),    // compre
            (1, 0),    // mixmdate
            (1, 0),    // infomdate
            (1, 0),    // addbsie
            // ----- audfrm -----
            (1, 1), // expstre
            (1, 0), // ahte
            (2, 0), // snroffststr
            (1, 0), // transproce
            (1, 1), // blkswe
            (1, 1), // dithflage
            (1, 1), // bamode
            (1, 0), // frmfgaincode
            (1, 1), // dbaflde
            (1, 1), // skipflde
            (1, 0), // spxattene
            // acmod>1 → cplinu[0] (1) + 5 × cplstre (1 each)
            (1, 0),
            (1, 0),
            (1, 0),
            (1, 0),
            (1, 0),
            (1, 0),
            // expstre==1 → per-block chexpstr lives in audblk(), NOT
            // audfrm. (Round-2 fix: round 1 over-consumed these bits.)
            // strmtyp=0 + numblkscod=3 → convexpstre implicit 1
            // → 2 × convexpstr (5 bits)
            (5, 0),
            (5, 0),
            // snroffststr=0 → frmcsnroffst (6) + frmfsnroffst (4)
            (6, 15),
            (4, 0),
            // num_blocks > 1 → blkstrtinfoe (1, 0)
            (1, 0),
        ];
        build_syncframe(768, bsi_bits)
    }

    #[test]
    fn decode_silent_indep_smoke() {
        let pkt = stereo_768_indep();
        let mut st = Eac3DecoderState::default();
        let frm = decode_eac3_packet(&mut st, &pkt).unwrap();
        assert_eq!(frm.sample_rate, 48_000);
        assert_eq!(frm.channels, 2);
        assert_eq!(frm.samples, 6 * 256);
        assert_eq!(frm.pcm_s16le.len(), (6 * 256) * 2 * 2);
        assert!(frm.pcm_s16le.iter().all(|&b| b == 0));
    }

    #[test]
    fn rejects_bad_syncword() {
        let mut data = stereo_768_indep();
        data[0] = 0xFF;
        let mut st = Eac3DecoderState::default();
        assert!(decode_eac3_packet(&mut st, &data).is_err());
    }

    /// Set up a decoder state with a 1-sample, 3-channel indep program
    /// at the given locations and the given per-channel constant values.
    fn indep_state_3ch(locs: [ChannelLocation; 3], vals: [f32; 3]) -> Eac3DecoderState {
        Eac3DecoderState {
            indep_samples_per_frame: 1,
            indep_nchans: 3,
            indep_pcm_f32: vals.to_vec(),
            indep_locations: locs.to_vec(),
            ..Default::default()
        }
    }

    /// §E.3.8.2: a dep channel whose location is NOT in the indep
    /// program *extends* the output (grows the channel count). The 7.1
    /// case: indep 5.1 (here shrunk to L,C,R for the test) + a dep
    /// [Lrs, Rrs] pair → 5 channels, the two new ones appended at the
    /// end, and `dep_locations` reports exactly [Lrs, Rrs].
    #[test]
    fn splice_extend_appends_new_locations() {
        use ChannelLocation::*;
        let mut st = indep_state_3ch([Left, Center, Right], [1.0, 2.0, 3.0]);
        // Dep substream: 2 coded channels at the rear-surround pair.
        // Table E2.5 bit 6 = Lrs/Rrs.
        let chanmap = Some(1u16 << (15 - 6));
        let dep_floats = [4.0f32, 5.0f32]; // 1 sample × 2 dep channels
        let locs = vec![LeftRearSurround, RightRearSurround];
        let extended = splice_dep_into_indep(&mut st, 2, chanmap, &dep_floats, &locs);
        assert_eq!(st.indep_nchans, 5, "indep 3 + 2 new dep = 5 channels");
        // Indep channels untouched; dep pair appended in order.
        assert_eq!(st.indep_pcm_f32, vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(extended, vec![LeftRearSurround, RightRearSurround]);
        assert_eq!(
            st.indep_locations,
            vec![Left, Center, Right, LeftRearSurround, RightRearSurround]
        );
    }

    /// §E.3.8.2: a dep channel whose location ALREADY exists in the
    /// indep program *replaces* the matching indep slot in place — it
    /// does NOT grow the channel count, and the original indep sample
    /// at that slot is overwritten with the dep data. This is the case
    /// the spec spells out explicitly ("the center channel audio data
    /// carried in the dependent stream will replace the center channel
    /// audio data carried in the independent stream"). A naive append
    /// would have produced [L,C,R, C'] (4 channels, duplicated +
    /// decorrelated center) — the broadcast-decode defect this fixes.
    #[test]
    fn splice_replace_overwrites_shared_location_in_place() {
        use ChannelLocation::*;
        let mut st = indep_state_3ch([Left, Center, Right], [1.0, 2.0, 3.0]);
        // Dep substream: a single channel re-coding the Center.
        // Table E2.5 bit 1 = Center.
        let chanmap = Some(1u16 << (15 - 1));
        let dep_floats = [9.0f32]; // 1 sample × 1 dep channel
        let locs = vec![Center];
        let extended = splice_dep_into_indep(&mut st, 1, chanmap, &dep_floats, &locs);
        assert_eq!(
            st.indep_nchans, 3,
            "replace must NOT grow the channel count"
        );
        // Center slot (index 1) overwritten; L and R unchanged.
        assert_eq!(st.indep_pcm_f32, vec![1.0, 9.0, 3.0]);
        assert!(
            extended.is_empty(),
            "a replaced channel does not extend the program"
        );
        assert_eq!(st.indep_locations, vec![Left, Center, Right]);
    }

    /// §E.3.8.2 mixed case: a dep substream carrying both a shared
    /// location (Center, replaced in place) and a new location (Cs,
    /// appended). Verifies the partition is per-channel, not all-or-
    /// nothing, and that the appended channel lands after the indep
    /// program while the replaced one stays in its original slot.
    #[test]
    fn splice_mixed_replace_and_extend() {
        use ChannelLocation::*;
        let mut st = indep_state_3ch([Left, Center, Right], [1.0, 2.0, 3.0]);
        // Table E2.5 bit 1 = Center, bit 7 = Cs.
        let chanmap = Some((1u16 << (15 - 1)) | (1u16 << (15 - 7)));
        let dep_floats = [7.0f32, 8.0f32]; // Center', Cs
        let locs = vec![Center, CenterSurround];
        let extended = splice_dep_into_indep(&mut st, 2, chanmap, &dep_floats, &locs);
        assert_eq!(st.indep_nchans, 4, "1 replace + 1 extend → 3 + 1 = 4");
        // Center (slot 1) replaced with 7.0; Cs appended at slot 3.
        assert_eq!(st.indep_pcm_f32, vec![1.0, 7.0, 3.0, 8.0]);
        assert_eq!(extended, vec![CenterSurround]);
        assert_eq!(
            st.indep_locations,
            vec![Left, Center, Right, CenterSurround]
        );
    }
}
