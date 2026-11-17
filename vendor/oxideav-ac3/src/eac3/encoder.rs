//! Enhanced AC-3 (E-AC-3 / Dolby Digital Plus) encoder per ATSC A/52
//! Annex E. Scope: independent substream (`strmtyp=0`, `substreamid=0`)
//! for 1.0/2.0/5.1 layouts, plus a paired indep+dep substream emission
//! for 7.1 input (5.1 downmix in indep, Lb/Rb in dep with `chanmap`
//! bit 6 = Lrs/Rrs pair set). `bsid=16`, 6 audio blocks per syncframe
//! (`numblkscod=3`), no coupling, no Adaptive Hybrid Transform.
//! Spectral extension (§E.2.3.3 / §E.3.6) is available opt-in through
//! [`make_encoder_with_spx`] — every fbw channel of every substream is
//! then coded only up to the SPX begin frequency and the decoder
//! synthesizes the extension region from the [`super::spxenc`]
//! energy-matching coordinates.
//!
//! Differences from AC-3 (`super::encoder`) at this scope:
//!
//! * **Sync frame size** is signalled in **bytes** via the 11-bit
//!   `frmsiz` field (§E.2.3.1.3) — `frmsiz = (frame_size_in_words - 1)`.
//!   AC-3's `frmsizecod` 6-bit table (§5.4.1.4) is gone. The encoder is
//!   free to pick any size between 64 and 4096 bytes (32–2048 words);
//!   we pick a per-bitrate value that matches the AC-3 lookup so PSNR
//!   comparisons against AC-3 are like-for-like.
//! * **No `crc1`**. The 4-byte syncinfo is just `syncword(16) +
//!   strmtyp(2) + substreamid(3) + frmsiz(11)`. The errorcheck() field
//!   carries only `encinfo(1) + crc2(16)` (§E.2.2.6).
//! * **`audfrm()`** sits between `bsi()` and the audio blocks and
//!   carries frame-level strategy flags (`expstre`, `ahte`,
//!   `snroffststr`, `transproce`, `blkswe`, `dithflage`, `bamode`,
//!   `frmfgaincode`, `dbaflde`, `skipflde`, `spxattene`). This encoder
//!   sets them so per-block strategy data still travels with each
//!   block — i.e. `expstre=1`, `blkswe=1`, `dithflage=1`, `bamode=1`,
//!   `frmfgaincode=1`, `dbaflde=1`, `skipflde=1`. AHT/SPX/transient
//!   pre-noise-processing all set to 0 (out of round-1 scope).
//! * **`audblk()`** for E-AC-3 inserts a few new fields that
//!   we leave at their disabled defaults (no SPX, no enhanced
//!   coupling).
//!
//! The DSP pipeline (windowing, MDCT, exponent extraction, parametric
//! bit allocation, mantissa quantisation) is identical to AC-3 and we
//! reuse the helpers from `super::encoder` directly.

use oxideav_core::Encoder;
use oxideav_core::bits::BitWriter;
use oxideav_core::{
    CodecId, CodecParameters, Error, Frame, Packet, Result, SampleFormat, TimeBase,
};

use crate::audblk::{BLOCKS_PER_FRAME, N_COEFFS, SAMPLES_PER_BLOCK, remat_band_count_spx};
// The 6-block frame constant (1536) only survives in the test suites'
// fixture builders; the library paths are block-count-generic now.
#[cfg(test)]
use crate::decoder::SAMPLES_PER_FRAME;
use crate::encoder::{
    BitAllocParams, CADENCE_CSNR_INIT, CouplingPlan, DbaPlan, LFE_END_MANT, TransientDetector,
    ac3_crc_update, bound_shared_exponents, build_dba_plan, cadence_reference_ba, compute_bap,
    compute_bap_cpl, decode_input_samples, elect_exponent_cadence, extract_exponent,
    mantissa_bits_total, overhead_bits_for, overhead_bits_for_ends, pick_mstrcplco,
    pick_strategy_for_block, preprocess_d15, quantise_cplco, quantise_exponents_to_grpsize,
    quantise_mantissa, reconstruct_cplco, tune_snroffst_with_plan_ends, write_exponents_cpl,
    write_exponents_grouped, write_mantissa_stream,
};
use crate::mdct::{mdct_256_pair, mdct_512};
use crate::tables::WINDOW;

use super::ahtenc::{compute_hebap, dct_ii_6, plan_aht_channel, write_aht_channel};
use super::dsp::DEFAULT_SPX_BNDSTRC;
use super::ecpl::{DEFAULT_ECPL_BNDSTRC, reconstruct_carrier};
use super::ecplenc::{
    BandStats, EcplGeometry, EcplParams, band_cross_stats, band_mdct_energies, build_carrier_block,
    carrier_band_gains, chaos_amp_factor, chaos_code_for, quantise_amp, quantise_angle,
    region_restrict,
};
use super::spxenc::{
    SpxGeometry, SpxParams, band_coord_targets_span_atten, choose_mstrspxco, quantise_coord,
};

/// Codec id string used by the E-AC-3 encoder (registered separately from
/// the AC-3 decoder/encoder in [`crate::register`]).
pub const CODEC_ID_STR: &str = "eac3";

// `EAC3_BSID` is the canonical bsid value for E-AC-3 streams (= 16).
// It lives in [`super::bsi`] now and is re-exported through
// `eac3::EAC3_BSID` in `mod.rs`. Re-import locally for the bit
// writer call-site below.
use super::bsi::EAC3_BSID;

/// Build an E-AC-3 encoder.
///
/// Required parameters:
/// * `sample_rate` — 48 000 / 44 100 / 32 000 Hz
/// * `channels`    — 1 (mono), 2 (stereo), 6 (5.1 = L,C,R,Ls,Rs,LFE),
///   or 8 (7.1 = L,C,R,Ls,Rs,LFE,Lb,Rb — emits an indep+dep substream
///   pair per Annex E §E.3.8.2: indep carries 5.1 with Ls/Rs taken
///   directly from the source, dep carries Lb/Rb with `chanmape=1`
///   and `chanmap` bit 6 (Lrs/Rrs pair) set).
///
/// Optional `bit_rate` (selects the syncframe size). Defaults:
///   * mono   →  96 kbps
///   * stereo → 192 kbps
///   * 5.1    → 384 kbps
///   * 7.1    → 384 kbps indep + 192 kbps dep = 576 kbps total
///
/// Spectral extension (§E.2.3.3 / §E.3.6) can be enabled through
/// `params.options` so the registry path reaches it too (the typed
/// alternative is [`make_encoder_with_spx`]):
///
/// | key | value | meaning |
/// | --- | --- | --- |
/// | `spx` | `1`/`true` | enable SPX with [`SpxParams::default`] |
/// | `spx_begf` | `0..=7` | §E.2.3.3.5 begin frequency code |
/// | `spx_endf` | `0..=7` | §E.2.3.3.6 end frequency code |
/// | `spx_strtf` | `0..=3` | §E.2.3.3.4 copy start code |
/// | `spx_blnd` | `0..=31` | §E.2.3.3.10 noise blend offset |
/// | `spx_atten` | `0..=31` | §3.6.4.2.3 attenuation code (Table E3.14) |
/// | `spx_adaptive_copy_start` | `1`/`true` | per-frame `spxstrtf` re-selection |
/// | `spx_explicit_band_structure` | `1`/`true` | emit `spxbndstrce = 1` |
///
/// The sub-keys imply `spx` unless it is explicitly `0`/`false`.
///
/// Fractional syncframes (§E.2.3.1.5 / Table E2.4) via the `blocks`
/// option = `1`/`2`/`3`/`6` — audio blocks per syncframe (the typed
/// alternative is [`make_encoder_with_blocks`]).
pub fn make_encoder(params: &CodecParameters) -> Result<Box<dyn Encoder>> {
    let mut concrete = build_concrete_encoder(params)?;
    concrete.meta = eac3_metadata_from_options(params)?;
    if let Some(spx) = spx_params_from_options(params)? {
        // Same construction-time validation as make_encoder_with_spx.
        SpxGeometry::derive(&spx, &DEFAULT_SPX_BNDSTRC)?;
        validate_spx_channel_mask(&spx, params.channels.unwrap_or(0))?;
        concrete.spx = Some(spx);
    }
    match params.options.get("aht") {
        None | Some("0") | Some("false") => {}
        Some("1") | Some("true") => concrete.aht = true,
        Some(v) => {
            return Err(Error::invalid(format!(
                "eac3 encoder: option aht={v} (expected 1/true/0/false)"
            )));
        }
    }
    match params.options.get("dither") {
        None | Some("1") | Some("true") => {}
        Some("0") | Some("false") => concrete.dither = false,
        Some(v) => {
            return Err(Error::invalid(format!(
                "eac3 encoder: option dither={v} (expected 1/true/0/false)"
            )));
        }
    }
    match params.options.get("tpnp") {
        None | Some("0") | Some("false") => {}
        Some("1") | Some("true") => concrete.tpnp = true,
        Some(v) => {
            return Err(Error::invalid(format!(
                "eac3 encoder: option tpnp={v} (expected 1/true/0/false)"
            )));
        }
    }
    if let Some(ecpl) = ecpl_params_from_options(params)? {
        validate_ecpl(&ecpl, params.channels.unwrap_or(0))?;
        concrete.ecpl = Some(ecpl);
    }
    if concrete.aht && concrete.spx.is_some() {
        return Err(Error::Unsupported(
            "eac3 encoder: aht and spx cannot be combined (single-subsystem scope)".into(),
        ));
    }
    if concrete.ecpl.is_some() && concrete.aht {
        return Err(Error::Unsupported(
            "eac3 encoder: ecpl cannot be combined with aht (single-subsystem scope)".into(),
        ));
    }
    if let (Some(ecpl), Some(spx)) = (&concrete.ecpl, &concrete.spx) {
        validate_spx_ecpl(spx, ecpl)?;
    }
    if let Some(cpl) = std_cpl_params_from_options(params)? {
        validate_std_cpl(&concrete, &cpl, params.channels.unwrap_or(0))?;
        concrete.stdcpl = Some(cpl);
    }
    validate_num_blocks_tools(&concrete)?;
    validate_frame_fit(&mut concrete)?;
    Ok(Box::new(concrete))
}

/// Build an E-AC-3 encoder emitting fractional syncframes
/// (§E.2.3.1.5 `numblkscod`, Table E2.4): `blocks` audio blocks —
/// 256 PCM samples each — per syncframe. 1, 2, and 3 give the
/// low-latency fractional-frame shapes (`numblkscod` 0/1/2, with the
/// §E.2.3.1.64 `convsync` cadence marking each 6-block AC-3
/// conversion-group head); 6 is the [`make_encoder`] default. The
/// syncframe byte budget scales by `blocks / 6` so the bit rate is
/// unchanged. Fractional frames use the standard coding path: AHT is
/// spec-implicit-off (`ahte = 0` when `numblkscod != 0x3`, Table
/// E1.3), and this encoder's SPX / enhanced coupling stay 6-block
/// scope (their coordinate anchors ride blocks 0/3).
///
/// Also reachable through the registry path via the
/// `CodecParameters::options` key `blocks` = `1`/`2`/`3`/`6`.
pub fn make_encoder_with_blocks(
    params: &CodecParameters,
    blocks: usize,
) -> Result<Box<dyn Encoder>> {
    if !matches!(blocks, 1 | 2 | 3 | 6) {
        return Err(Error::invalid(format!(
            "eac3 encoder: blocks={blocks} (expected 1, 2, 3, or 6 — Table E2.4)"
        )));
    }
    let mut concrete = build_concrete_encoder(params)?;
    concrete.num_blocks = blocks;
    // Recompute the per-substream frame size for the new block count
    // (build_concrete_encoder sized it from the options-derived count).
    let fscod = concrete.fscod;
    match &mut concrete.layout {
        Layout::Indep(s) => {
            s.frame_bytes = ac3_frame_bytes_blocks(fscod, s.kbps, blocks).ok_or_else(|| {
                Error::Unsupported(format!(
                    "eac3 encoder: bit rate {} kbps has no {blocks}-block frame-size mapping",
                    s.kbps
                ))
            })? as usize;
        }
        Layout::Pair { indep, dep } => {
            for s in [&mut *indep, &mut *dep] {
                s.frame_bytes = ac3_frame_bytes_blocks(fscod, s.kbps, blocks).ok_or_else(|| {
                    Error::Unsupported(format!(
                        "eac3 encoder: bit rate {} kbps has no {blocks}-block frame-size mapping",
                        s.kbps
                    ))
                })? as usize;
            }
        }
    }
    match &concrete.layout {
        Layout::Indep(s) => validate_min_rate(s, blocks, 0)?,
        Layout::Pair { indep, dep } => {
            validate_min_rate(indep, blocks, 0)?;
            validate_min_rate(dep, blocks, 0)?;
        }
    }
    validate_num_blocks_tools(&concrete)?;
    validate_frame_fit(&mut concrete)?;
    Ok(Box::new(concrete))
}

/// Construction-time floor for the (frame size × layout × block
/// count) combination: even with every anchor demoted to D45 (the
/// cheapest legal exponent strategy), the per-frame fixed syntax +
/// exponent payload must fit the syncframe with room to spare for at
/// least a token mantissa payload. Fractional frames concentrate the
/// per-frame overhead onto fewer samples, so low rates that are fine
/// at 6 blocks become impossible at 1 block — reject those up front
/// instead of erroring on the first frame.
fn validate_min_rate(sub: &SubstreamLayout, nblks: usize, meta_bits: u32) -> Result<()> {
    let frame_strat: Vec<u8> = (0..nblks).map(|b| u8::from(b == 0)).collect();
    let d45_anchor = {
        let mut a = [0u8; BLOCKS_PER_FRAME];
        a[0] = 3;
        a
    };
    let plan = vec![d45_anchor; sub.nfchans];
    let floor = overhead_bits_for_ends(
        &frame_strat,
        Some(&plan),
        253, // full coded bandwidth (chbwcod = 60)
        None,
        sub.nfchans,
        &crate::encoder::CouplingPlan::default(),
        &crate::encoder::DbaPlan::default(),
        sub.acmod,
        sub.lfeon,
    ) + meta_bits
        + 192;
    if floor as usize >= sub.frame_bytes * 8 {
        return Err(Error::Unsupported(format!(
            "eac3 encoder: {} kbps is too low for a {nblks}-block acmod={} frame — \
             the fixed syntax + minimum exponent payload + metadata ({floor} bits) \
             exceeds the {}-bit syncframe; raise the bit rate or the block count",
            sub.kbps,
            sub.acmod,
            sub.frame_bytes * 8,
        )));
    }
    Ok(())
}

/// §3.7.1 transient-location analysis for one channel's frame of PCM.
///
/// Scans 4-sample group energies for the first group that jumps a
/// factor of 16 above the running average of the preceding groups
/// (with an absolute floor so noise-level content never triggers).
/// Returns `(transprocloc, transproclen)` — the group index (already
/// in the §2.3.2.22 4-sample units) and the time-scaling length in
/// samples — or `None` when the frame carries no usable transient
/// (none found, or the transient sits on a block boundary, which this
/// encoder leaves to the transform).
///
/// The length is the transient's offset inside its coding block
/// (capped at the 8-bit field). In the decoder (§3.7.2) the replacement
/// audio for sample `d` is sample `d − (TC1 + PN − transproclen)`, with
/// `PN` counted from the leading edge of the block before the
/// transient's block (`PN = 256 + offset`): this choice makes the copy
/// distance a constant 512 samples — two block hops — whatever the
/// offset. A waveform-matched length (the clause's "time scaling
/// synthesis analysis") is encoder tuning this encoder does not do.
fn detect_tpnp_transient(pcm: &[f32]) -> Option<(u16, u16)> {
    const GROUP: usize = 4;
    const WARMUP_GROUPS: usize = 16;
    const RATIO: f32 = 16.0;
    const ABS_FLOOR: f32 = 1e-4;
    let ngroups = pcm.len() / GROUP;
    if ngroups <= WARMUP_GROUPS {
        return None;
    }
    let energy = |g: usize| -> f32 {
        pcm[g * GROUP..(g + 1) * GROUP]
            .iter()
            .map(|x| x * x)
            .sum::<f32>()
    };
    let mut avg = 0.0f32;
    for g in 0..WARMUP_GROUPS {
        avg += energy(g);
    }
    avg /= WARMUP_GROUPS as f32;
    for g in WARMUP_GROUPS..ngroups {
        let e = energy(g);
        if e > ABS_FLOOR && e > RATIO * avg.max(ABS_FLOOR / RATIO) {
            let transloc = g * GROUP;
            let pnlen = transloc % SAMPLES_PER_BLOCK;
            if pnlen == 0 {
                return None;
            }
            return Some((g as u16, pnlen.min(255) as u16));
        }
        // Exponential running average (¼ new) tracks slow level
        // changes without letting the transient itself dilute the
        // reference before it is tested.
        avg = 0.75 * avg + 0.25 * e;
    }
    None
}

/// Construction-time dry run: encode one syncframe of deterministic
/// full-scale noise (the worst practical exponent-loudness case)
/// through the real emission pipeline, then reset every state field
/// the run touched. The analytical floor in [`validate_min_rate`]
/// bounds the bare shape, but tool syntax (SPX / enhanced-coupling
/// coordinates, AHT gain words) and content-driven strategy choices
/// only surface in the actual packer — a config whose fixed syntax
/// cannot fit its frame fails HERE instead of on the caller's first
/// frame (found by the encode→decode fuzz target on a 7.1 96 kbps
/// spx+ecpl config).
fn validate_frame_fit(enc: &mut Eac3Encoder) -> Result<()> {
    let n = enc.frame_samples();
    let mut s = 0x1234_5678u32;
    for ch in 0..enc.total_pcm_chans {
        enc.pending_samples[ch] = (0..n)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((s >> 16) as u16 as i16 as f32) / 32768.0
            })
            .collect();
    }
    let r = enc.emit_syncframe();
    for d in enc.delay_line.iter_mut() {
        d.iter_mut().for_each(|x| *x = 0.0);
    }
    for p in enc.pending_samples.iter_mut() {
        p.clear();
    }
    enc.transient_state = (0..enc.total_pcm_chans)
        .map(|_| TransientDetector::default())
        .collect();
    enc.packet_queue.clear();
    enc.pts = 0;
    enc.frames_emitted = 0;
    enc.ecpl_carry = vec![None, None];
    enc.cadence_csnr = vec![CADENCE_CSNR_INIT; 2];
    enc.last_blksw = vec![false; enc.total_pcm_chans];
    r.map(|_| ()).map_err(|e| {
        Error::Unsupported(format!(
            "eac3 encoder: this configuration cannot fit its syncframe budget \
             (worst-case dry-run failed: {e:?}); raise the bit rate, the block \
             count, or drop a coding tool"
        ))
    })
}

/// Fractional frames (§E.2.3.1.5 `numblkscod != 0x3`) use the
/// standard coding path only: `ahte` is implicit-0 on the wire (Table
/// E1.3), and this encoder's SPX / enhanced-coupling coordinate
/// planning anchors on blocks 0/3 (6-block scope).
fn validate_num_blocks_tools(enc: &Eac3Encoder) -> Result<()> {
    // Re-run the frame-budget floor with the ACTUAL metadata reserve:
    // the per-block dynrng word and the mixing / informational blocks
    // consume real frame bits, and a shape that fits bare can overflow
    // once they ride along (found by the encode→decode fuzz target on
    // a 1-block mono 96 kbps config with dynrng + dialnorm set).
    let subs: [Option<&SubstreamLayout>; 2] = match &enc.layout {
        Layout::Indep(s) => [Some(s), None],
        Layout::Pair { indep, dep } => [Some(indep), Some(dep)],
    };
    for s in subs.into_iter().flatten() {
        let meta_bits = enc.meta.reserve_bits(s.acmod, s.lfeon, s.strmtyp == 0);
        validate_min_rate(s, enc.num_blocks, meta_bits)?;
    }
    if enc.num_blocks == BLOCKS_PER_FRAME {
        return Ok(());
    }
    if enc.aht {
        return Err(Error::Unsupported(
            "eac3 encoder: aht requires 6-block syncframes (Table E1.3 — \
             ahte is implicit 0 when numblkscod != 0x3)"
                .into(),
        ));
    }
    if enc.spx.is_some() || enc.ecpl.is_some() || enc.stdcpl.is_some() {
        return Err(Error::Unsupported(
            "eac3 encoder: spx / ecpl / cpl require 6-block syncframes (blocks = 6)".into(),
        ));
    }
    Ok(())
}

/// SPX + enhanced coupling co-active (§3.6.1: "coupling for a mid-range
/// portion of the frequency spectrum and spectral extension for the
/// higher-range portion"): the enhanced-coupling region ends at the SPX
/// begin frequency (`ecplendf` is not transmitted). Every fbw channel
/// must participate in BOTH tools — a coupled channel outside SPX would
/// have no content above the coupling region at all (its coded
/// mantissas stop at the coupling begin), so a mixed `channel_mask` is
/// rejected in this configuration.
fn validate_spx_ecpl(spx: &SpxParams, ecpl: &EcplParams) -> Result<()> {
    if spx.channel_mask.is_some() {
        return Err(Error::Unsupported(
            "eac3 encoder: spx channel_mask cannot be combined with ecpl — \
             every coupled channel must also be in SPX"
                .into(),
        ));
    }
    // The combined geometry must be valid (non-empty coupling region
    // below the SPX begin).
    EcplGeometry::derive_with_spx(ecpl, spx.spxbegf, &DEFAULT_ECPL_BNDSTRC)?;
    Ok(())
}

/// Build an E-AC-3 encoder with **spectral extension AND enhanced
/// coupling co-active** (§3.6.1): channels are waveform-coded below the
/// enhanced-coupling begin frequency, carried by the shared coupling
/// channel + coordinates from there to the SPX begin frequency, and
/// SPX-synthesized above it. See [`make_encoder_with_spx`] and
/// [`make_encoder_with_ecpl`] for the individual tools.
pub fn make_encoder_with_spx_ecpl(
    params: &CodecParameters,
    spx: SpxParams,
    ecpl: EcplParams,
) -> Result<Box<dyn Encoder>> {
    SpxGeometry::derive(&spx, &DEFAULT_SPX_BNDSTRC)?;
    validate_spx_channel_mask(&spx, params.channels.unwrap_or(0))?;
    validate_ecpl(&ecpl, params.channels.unwrap_or(0))?;
    validate_spx_ecpl(&spx, &ecpl)?;
    let mut concrete = build_concrete_encoder(params)?;
    concrete.spx = Some(spx);
    concrete.ecpl = Some(ecpl);
    validate_num_blocks_tools(&concrete)?;
    validate_frame_fit(&mut concrete)?;
    Ok(Box::new(concrete))
}

/// Build an E-AC-3 encoder with **enhanced coupling** enabled
/// (§E.2.3.3.16-26 syntax / §E.3.5.5 decode model). Every fbw channel
/// of the independent substream is coupled: below the enhanced-coupling
/// begin frequency each channel is waveform-coded as usual; above it a
/// single shared **carrier** channel is coded through the standard
/// exponent / bit-allocation / mantissa path and each coupled channel
/// is reconstructed from it via per-band amplitude + angle coordinates
/// (chaos = 0, no random de-correlation — deterministic decode). The
/// carrier is phase-locked to the first coupled channel (whose angle is
/// spec-fixed to 0) and scaled per band so the loudest channel maps to
/// the Table E3.10 amplitude ceiling. Coordinates refresh on the
/// exponent anchor blocks (0 and 3) and are thrifted (`ecplparam1e = 0`)
/// when the block-3 refresh quantises identically. Long transforms are
/// forced while enhanced coupling is on.
///
/// Requires at least 2 fbw channels (stereo / 5.1 / 7.1; the 7.1 pair's
/// dependent substream stays plain). Also reachable through the
/// registry path via `CodecParameters::options` keys `ecpl` =
/// `1`/`true`, `ecpl_begf` (2..=13) and `ecpl_endf` (0..=15).
pub fn make_encoder_with_ecpl(
    params: &CodecParameters,
    ecpl: EcplParams,
) -> Result<Box<dyn Encoder>> {
    // Same construction-time validation as the options path.
    EcplGeometry::derive(&ecpl, &DEFAULT_ECPL_BNDSTRC)?;
    validate_ecpl(&ecpl, params.channels.unwrap_or(0))?;
    let mut concrete = build_concrete_encoder(params)?;
    concrete.ecpl = Some(ecpl);
    validate_num_blocks_tools(&concrete)?;
    validate_frame_fit(&mut concrete)?;
    Ok(Box::new(concrete))
}

/// Parse the `ecpl*` codec options (see [`make_encoder`]) into an
/// [`EcplParams`], or `None` when enhanced coupling is not requested.
/// Sub-keys imply `ecpl` unless it is explicitly `0`/`false`; the
/// resulting geometry is validated via [`EcplGeometry::derive`].
fn ecpl_params_from_options(params: &CodecParameters) -> Result<Option<EcplParams>> {
    let opts = &params.options;
    let enabled = match opts.get("ecpl") {
        None => None,
        Some("1") | Some("true") => Some(true),
        Some("0") | Some("false") => Some(false),
        Some(v) => {
            return Err(Error::invalid(format!(
                "eac3 encoder: option ecpl={v} (expected 1/true/0/false)"
            )));
        }
    };
    let parse_u8 = |key: &str| -> Result<Option<u8>> {
        match opts.get(key) {
            None => Ok(None),
            Some(v) => match v.parse::<u8>() {
                Ok(n) if n <= 15 => Ok(Some(n)),
                _ => Err(Error::invalid(format!(
                    "eac3 encoder: option {key}={v} (expected 0..=15)"
                ))),
            },
        }
    };
    let begf = parse_u8("ecpl_begf")?;
    let endf = parse_u8("ecpl_endf")?;
    let chaos = match opts.get("ecpl_chaos") {
        None => None,
        Some("1") | Some("true") => Some(true),
        Some("0") | Some("false") => Some(false),
        Some(v) => {
            return Err(Error::invalid(format!(
                "eac3 encoder: option ecpl_chaos={v} (expected 1/true/0/false)"
            )));
        }
    };
    let on = match enabled {
        Some(v) => v,
        None => begf.is_some() || endf.is_some() || chaos.is_some(),
    };
    if !on {
        return Ok(None);
    }
    let d = EcplParams::default();
    let p = EcplParams {
        ecplbegf: begf.unwrap_or(d.ecplbegf),
        ecplendf: endf.unwrap_or(d.ecplendf),
        chaos: chaos.unwrap_or(d.chaos),
    };
    EcplGeometry::derive(&p, &DEFAULT_ECPL_BNDSTRC)?;
    Ok(Some(p))
}

/// Table E1.2 mixing-metadata block (`mixmdate = 1`) for encoder-side
/// emission on the independent substream. The acmod-gated arms
/// (`dmixmod` needs > 2 channels, the centre / surround mix levels
/// need those channels present, `lfemixlevcod` needs `lfeon`) are only
/// emitted when their guard holds; configured values are ignored
/// otherwise. The advisory `mixdef` body, pan information and
/// per-block mixing configuration are emitted as absent
/// (`mixdef = 0`, `paninfoe = 0`, `frmmixcfginfoe = 0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Eac3MixMetadata {
    /// `dmixmod` (2 bits, §E.2.3.1.11) — preferred stereo downmix
    /// (0 = not indicated, 1 = LtRt, 2 = LoRo; 3 is reserved).
    pub dmixmod: u8,
    /// `ltrtcmixlev` (3 bits, Table D2.3 scale: 0 = +3 dB … 6 = −6 dB,
    /// 7 = mute).
    pub ltrtcmixlev: u8,
    /// `lorocmixlev` (3 bits, Table D2.5 — same scale).
    pub lorocmixlev: u8,
    /// `ltrtsurmixlev` (3 bits, Table D2.4: 3 = −1.5 dB … 6 = −6 dB,
    /// 7 = mute; 0..=2 are reserved).
    pub ltrtsurmixlev: u8,
    /// `lorosurmixlev` (3 bits, Table D2.6 — same scale).
    pub lorosurmixlev: u8,
    /// `lfemixlevcode`/`lfemixlevcod` (5 bits, §E.2.3.1.x): LFE mix
    /// level is `10 − lfemixlevcod` dB.
    pub lfemixlevcod: Option<u8>,
    /// `pgmscle`/`pgmscl` (6 bits, §E.2.3.1.12-13): program scale
    /// factor `code − 51` dB (`0` = mute).
    pub pgmscl: Option<u8>,
    /// `extpgmscle`/`extpgmscl` (6 bits, §E.2.3.1.16-17) — external
    /// program scale, same wire scale as `pgmscl`.
    pub extpgmscl: Option<u8>,
}

impl Eac3MixMetadata {
    /// Builder: set [`dmixmod`](Self::dmixmod).
    #[must_use]
    pub fn with_dmixmod(mut self, v: u8) -> Self {
        self.dmixmod = v;
        self
    }

    /// Builder: set [`ltrtcmixlev`](Self::ltrtcmixlev).
    #[must_use]
    pub fn with_ltrtcmixlev(mut self, v: u8) -> Self {
        self.ltrtcmixlev = v;
        self
    }

    /// Builder: set [`lorocmixlev`](Self::lorocmixlev).
    #[must_use]
    pub fn with_lorocmixlev(mut self, v: u8) -> Self {
        self.lorocmixlev = v;
        self
    }

    /// Builder: set [`ltrtsurmixlev`](Self::ltrtsurmixlev).
    #[must_use]
    pub fn with_ltrtsurmixlev(mut self, v: u8) -> Self {
        self.ltrtsurmixlev = v;
        self
    }

    /// Builder: set [`lorosurmixlev`](Self::lorosurmixlev).
    #[must_use]
    pub fn with_lorosurmixlev(mut self, v: u8) -> Self {
        self.lorosurmixlev = v;
        self
    }

    /// Builder: set [`lfemixlevcod`](Self::lfemixlevcod).
    #[must_use]
    pub fn with_lfemixlevcod(mut self, v: Option<u8>) -> Self {
        self.lfemixlevcod = v;
        self
    }

    /// Builder: set [`pgmscl`](Self::pgmscl).
    #[must_use]
    pub fn with_pgmscl(mut self, v: Option<u8>) -> Self {
        self.pgmscl = v;
        self
    }

    /// Builder: set [`extpgmscl`](Self::extpgmscl).
    #[must_use]
    pub fn with_extpgmscl(mut self, v: Option<u8>) -> Self {
        self.extpgmscl = v;
        self
    }
}

impl Default for Eac3MixMetadata {
    /// −3 dB centre / −3 dB surround downmix levels on both LtRt and
    /// LoRo targets, downmix preference not indicated, no LFE mix
    /// level, no program scale factors.
    fn default() -> Self {
        Self {
            dmixmod: 0,
            ltrtcmixlev: 4,
            lorocmixlev: 4,
            ltrtsurmixlev: 4,
            lorosurmixlev: 4,
            lfemixlevcod: None,
            pgmscl: None,
            extpgmscl: None,
        }
    }
}

/// §E.2.3.1.62+ informational-metadata block (`infomdate = 1`) for
/// encoder-side emission on the independent substream. The 2/0-only
/// (`dsurmod` / `dheadphonmod`) and ≥ 6-channel (`dsurexmod`) arms are
/// emitted only when acmod carries them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Eac3InfoMetadata {
    /// `bsmod` (3 bits, Table 5.7) — bit-stream mode.
    pub bsmod: u8,
    /// `copyrightb` (§5.4.2.24 semantics).
    pub copyrightb: bool,
    /// `origbs` (§5.4.2.25 semantics).
    pub origbs: bool,
    /// `dsurmod` (2 bits, Table 5.11; 2/0 only; 3 is reserved).
    pub dsurmod: u8,
    /// `dheadphonmod` (2 bits; 2/0 only; 0 = not indicated, 1 = not
    /// encoded, 2 = encoded, 3 reserved).
    pub dheadphonmod: u8,
    /// `dsurexmod` (2 bits; acmod ≥ 6 only; 3 is reserved).
    pub dsurexmod: u8,
    /// `audprodie` body: `mixlevel` (5 bits), `roomtyp` (2 bits,
    /// 0..=2), `adconvtyp` (1 bit, HDCD A/D converter).
    pub audprod: Option<Eac3AudioProduction>,
    /// `sourcefscod` (1 bit) — source was sampled at twice the rate.
    pub sourcefscod: bool,
}

impl Eac3InfoMetadata {
    /// Builder: set [`bsmod`](Self::bsmod).
    #[must_use]
    pub fn with_bsmod(mut self, v: u8) -> Self {
        self.bsmod = v;
        self
    }

    /// Builder: set [`copyrightb`](Self::copyrightb).
    #[must_use]
    pub fn with_copyrightb(mut self, v: bool) -> Self {
        self.copyrightb = v;
        self
    }

    /// Builder: set [`origbs`](Self::origbs).
    #[must_use]
    pub fn with_origbs(mut self, v: bool) -> Self {
        self.origbs = v;
        self
    }

    /// Builder: set [`dsurmod`](Self::dsurmod).
    #[must_use]
    pub fn with_dsurmod(mut self, v: u8) -> Self {
        self.dsurmod = v;
        self
    }

    /// Builder: set [`dheadphonmod`](Self::dheadphonmod).
    #[must_use]
    pub fn with_dheadphonmod(mut self, v: u8) -> Self {
        self.dheadphonmod = v;
        self
    }

    /// Builder: set [`dsurexmod`](Self::dsurexmod).
    #[must_use]
    pub fn with_dsurexmod(mut self, v: u8) -> Self {
        self.dsurexmod = v;
        self
    }

    /// Builder: set [`audprod`](Self::audprod).
    #[must_use]
    pub fn with_audprod(mut self, v: Option<Eac3AudioProduction>) -> Self {
        self.audprod = v;
        self
    }

    /// Builder: set [`sourcefscod`](Self::sourcefscod).
    #[must_use]
    pub fn with_sourcefscod(mut self, v: bool) -> Self {
        self.sourcefscod = v;
        self
    }
}

/// `audprodie` body for [`Eac3InfoMetadata`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Eac3AudioProduction {
    /// `mixlevel` (5 bits, 0..=31) — peak SPL is `80 + mixlevel`.
    pub mixlevel: u8,
    /// `roomtyp` (2 bits, 0..=2; 3 is reserved) — Table 5.12.
    pub roomtyp: u8,
    /// `adconvtyp` (1 bit) — HDCD-type A/D conversion.
    pub adconvtyp: bool,
}

impl Eac3AudioProduction {
    /// Build a [`Eac3AudioProduction`] from every field.
    pub fn new(mixlevel: u8, roomtyp: u8, adconvtyp: bool) -> Self {
        Self {
            mixlevel,
            roomtyp,
            adconvtyp,
        }
    }

    /// Builder: set [`mixlevel`](Self::mixlevel).
    #[must_use]
    pub fn with_mixlevel(mut self, v: u8) -> Self {
        self.mixlevel = v;
        self
    }

    /// Builder: set [`roomtyp`](Self::roomtyp).
    #[must_use]
    pub fn with_roomtyp(mut self, v: u8) -> Self {
        self.roomtyp = v;
        self
    }

    /// Builder: set [`adconvtyp`](Self::adconvtyp).
    #[must_use]
    pub fn with_adconvtyp(mut self, v: bool) -> Self {
        self.adconvtyp = v;
        self
    }
}

impl Default for Eac3InfoMetadata {
    fn default() -> Self {
        Self {
            bsmod: 0,
            copyrightb: false,
            origbs: true,
            dsurmod: 0,
            dheadphonmod: 0,
            dsurexmod: 0,
            audprod: None,
            sourcefscod: false,
        }
    }
}

/// Encoder-side E-AC-3 bitstream-metadata surface: the fixed-BSI
/// `dialnorm` + `compr` words, the per-block §5.4.3.3-4 `dynrng` word
/// (emitted in every block of every substream), and the optional
/// Table E1.2 mixing / informational metadata blocks (emitted on the
/// independent substream; a 7.1 pair's dependent substream keeps
/// `mixmdate = infomdate = 0`).
///
/// Registry option keys: `dialnorm` (1..=31), `compr` / `dynrng`
/// (8-bit gain words, decimal or `0x`-hex); mixing block — `dmixmod`,
/// `ltrtcmixlev`, `lorocmixlev`, `ltrtsurmixlev`, `lorosurmixlev`,
/// `lfemixlevcod`, `pgmscl`, `extpgmscl`; informational block —
/// `bsmod`, `copyright`, `origbs`, `dsurmod`, `dheadphonmod`,
/// `dsurexmod`, `mixlevel` + `roomtyp` + `adconvtyp`, `sourcefscod`.
/// Setting any key of a block emits that block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Eac3Metadata {
    /// `dialnorm` (5 bits, §5.4.2.8 semantics) — valid 1..=31.
    pub dialnorm: u8,
    /// §5.4.2.9-10 heavy-compression word (Table 7.30), per substream.
    pub compr: Option<u8>,
    /// §5.4.3.3-4 dynamic-range word (§7.7.1.2), emitted in every
    /// audio block of every substream.
    pub dynrng: Option<u8>,
    /// Table E1.2 mixing-metadata block (`mixmdate = 1`).
    pub mixmd: Option<Eac3MixMetadata>,
    /// §E.2.3.1.62+ informational-metadata block (`infomdate = 1`).
    pub infomd: Option<Eac3InfoMetadata>,
}

impl Eac3Metadata {
    /// Builder: set [`dialnorm`](Self::dialnorm).
    #[must_use]
    pub fn with_dialnorm(mut self, v: u8) -> Self {
        self.dialnorm = v;
        self
    }

    /// Builder: set [`compr`](Self::compr).
    #[must_use]
    pub fn with_compr(mut self, v: Option<u8>) -> Self {
        self.compr = v;
        self
    }

    /// Builder: set [`dynrng`](Self::dynrng).
    #[must_use]
    pub fn with_dynrng(mut self, v: Option<u8>) -> Self {
        self.dynrng = v;
        self
    }

    /// Builder: set [`mixmd`](Self::mixmd).
    #[must_use]
    pub fn with_mixmd(mut self, v: Option<Eac3MixMetadata>) -> Self {
        self.mixmd = v;
        self
    }

    /// Builder: set [`infomd`](Self::infomd).
    #[must_use]
    pub fn with_infomd(mut self, v: Option<Eac3InfoMetadata>) -> Self {
        self.infomd = v;
        self
    }
}

impl Default for Eac3Metadata {
    /// The encoder's historical fixed words: dialnorm −27 dB, nothing
    /// else emitted.
    fn default() -> Self {
        Self {
            dialnorm: 27,
            compr: None,
            dynrng: None,
            mixmd: None,
            infomd: None,
        }
    }
}

impl Eac3Metadata {
    /// Range-check every codepoint against its syntax slot.
    fn validate(&self) -> Result<()> {
        if self.dialnorm == 0 || self.dialnorm > 31 {
            return Err(Error::invalid(format!(
                "eac3 encoder: dialnorm {} out of range (1..=31; 0 is reserved)",
                self.dialnorm
            )));
        }
        if let Some(m) = &self.mixmd {
            if m.dmixmod > 2 {
                return Err(Error::invalid(format!(
                    "eac3 encoder: dmixmod {} out of range (0..=2; 3 is reserved)",
                    m.dmixmod
                )));
            }
            for (name, v) in [
                ("ltrtcmixlev", m.ltrtcmixlev),
                ("lorocmixlev", m.lorocmixlev),
            ] {
                if v > 7 {
                    return Err(Error::invalid(format!(
                        "eac3 encoder: {name} {v} out of range (0..=7)"
                    )));
                }
            }
            for (name, v) in [
                ("ltrtsurmixlev", m.ltrtsurmixlev),
                ("lorosurmixlev", m.lorosurmixlev),
            ] {
                if !(3..=7).contains(&v) {
                    return Err(Error::invalid(format!(
                        "eac3 encoder: {name} {v} out of range (3..=7; 0..=2 are reserved)"
                    )));
                }
            }
            if let Some(l) = m.lfemixlevcod
                && l > 31
            {
                return Err(Error::invalid(format!(
                    "eac3 encoder: lfemixlevcod {l} out of range (0..=31)"
                )));
            }
            for (name, v) in [("pgmscl", m.pgmscl), ("extpgmscl", m.extpgmscl)] {
                if let Some(v) = v
                    && v > 63
                {
                    return Err(Error::invalid(format!(
                        "eac3 encoder: {name} {v} out of range (0..=63)"
                    )));
                }
            }
        }
        if let Some(i) = &self.infomd {
            if i.bsmod > 7 {
                return Err(Error::invalid(format!(
                    "eac3 encoder: bsmod {} out of range (0..=7)",
                    i.bsmod
                )));
            }
            for (name, v) in [
                ("dsurmod", i.dsurmod),
                ("dheadphonmod", i.dheadphonmod),
                ("dsurexmod", i.dsurexmod),
            ] {
                if v > 2 {
                    return Err(Error::invalid(format!(
                        "eac3 encoder: {name} {v} out of range (0..=2; 3 is reserved)"
                    )));
                }
            }
            if let Some(a) = i.audprod {
                if a.mixlevel > 31 {
                    return Err(Error::invalid(format!(
                        "eac3 encoder: mixlevel {} out of range (0..=31)",
                        a.mixlevel
                    )));
                }
                if a.roomtyp > 2 {
                    return Err(Error::invalid(format!(
                        "eac3 encoder: roomtyp {} out of range (0..=2; 3 is reserved)",
                        a.roomtyp
                    )));
                }
            }
        }
        Ok(())
    }

    /// Bits this metadata adds to one substream's syncframe on top of
    /// the fixed layout (which already spends the 1-bit `compre` /
    /// `mixmdate` / `infomdate` / per-block `dynrnge` flags).
    fn reserve_bits(&self, acmod: u8, lfeon: bool, indep: bool) -> u32 {
        let mut bits = 0u32;
        if self.compr.is_some() {
            bits += 8;
        }
        if self.dynrng.is_some() {
            bits += 8 * BLOCKS_PER_FRAME as u32;
        }
        if !indep {
            return bits;
        }
        if let Some(m) = &self.mixmd {
            if acmod > 0x2 {
                bits += 2; // dmixmod
            }
            if (acmod & 0x1) != 0 && acmod > 0x2 {
                bits += 6; // ltrtcmixlev + lorocmixlev
            }
            if (acmod & 0x4) != 0 {
                bits += 6; // ltrtsurmixlev + lorosurmixlev
            }
            if lfeon {
                bits += 1 + if m.lfemixlevcod.is_some() { 5 } else { 0 };
            }
            bits += 1 + if m.pgmscl.is_some() { 6 } else { 0 };
            bits += 1 + if m.extpgmscl.is_some() { 6 } else { 0 };
            bits += 2; // mixdef = 0
            if acmod < 0x2 {
                bits += 1; // paninfoe = 0
            }
            bits += 1; // frmmixcfginfoe = 0
        }
        if let Some(i) = &self.infomd {
            bits += 3 + 1 + 1; // bsmod + copyrightb + origbs
            if acmod == 0x2 {
                bits += 4; // dsurmod + dheadphonmod
            }
            if acmod >= 0x6 {
                bits += 2; // dsurexmod
            }
            bits += 1 + if i.audprod.is_some() { 8 } else { 0 };
            bits += 1; // sourcefscod (fscod < 0x3 always here)
        }
        bits
    }
}

/// Parse the metadata codec options (see [`Eac3Metadata`]) from
/// `CodecParameters::options`; absent keys keep the defaults.
fn eac3_metadata_from_options(params: &CodecParameters) -> Result<Eac3Metadata> {
    let opts = &params.options;
    let get_u8 = |key: &str| -> Result<Option<u8>> {
        match opts.get(key) {
            None => Ok(None),
            Some(v) => {
                let parsed =
                    if let Some(hex) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
                        u8::from_str_radix(hex, 16).ok()
                    } else {
                        v.parse::<u8>().ok()
                    };
                parsed.map(Some).ok_or_else(|| {
                    Error::invalid(format!(
                        "eac3 encoder: option {key}={v} (expected 0..=255, decimal or 0x-hex)"
                    ))
                })
            }
        }
    };
    let get_bool = |key: &str| -> Result<Option<bool>> {
        match opts.get(key) {
            None => Ok(None),
            Some("1") | Some("true") => Ok(Some(true)),
            Some("0") | Some("false") => Ok(Some(false)),
            Some(v) => Err(Error::invalid(format!(
                "eac3 encoder: option {key}={v} (expected 1/true/0/false)"
            ))),
        }
    };
    let mut meta = Eac3Metadata::default();
    if let Some(v) = get_u8("dialnorm")? {
        meta.dialnorm = v;
    }
    meta.compr = get_u8("compr")?;
    meta.dynrng = get_u8("dynrng")?;
    // Mixing block — any key present emits the block.
    let dmixmod = get_u8("dmixmod")?;
    let ltrtc = get_u8("ltrtcmixlev")?;
    let loroc = get_u8("lorocmixlev")?;
    let ltrts = get_u8("ltrtsurmixlev")?;
    let loros = get_u8("lorosurmixlev")?;
    let lfemix = get_u8("lfemixlevcod")?;
    let pgmscl = get_u8("pgmscl")?;
    let extpgmscl = get_u8("extpgmscl")?;
    if dmixmod.is_some()
        || ltrtc.is_some()
        || loroc.is_some()
        || ltrts.is_some()
        || loros.is_some()
        || lfemix.is_some()
        || pgmscl.is_some()
        || extpgmscl.is_some()
    {
        let d = Eac3MixMetadata::default();
        meta.mixmd = Some(Eac3MixMetadata {
            dmixmod: dmixmod.unwrap_or(d.dmixmod),
            ltrtcmixlev: ltrtc.unwrap_or(d.ltrtcmixlev),
            lorocmixlev: loroc.unwrap_or(d.lorocmixlev),
            ltrtsurmixlev: ltrts.unwrap_or(d.ltrtsurmixlev),
            lorosurmixlev: loros.unwrap_or(d.lorosurmixlev),
            lfemixlevcod: lfemix,
            pgmscl,
            extpgmscl,
        });
    }
    // Informational block — any key present emits the block.
    let bsmod = get_u8("bsmod")?;
    let copyright = get_bool("copyright")?;
    let origbs = get_bool("origbs")?;
    let dsurmod = get_u8("dsurmod")?;
    let dheadphonmod = get_u8("dheadphonmod")?;
    let dsurexmod = get_u8("dsurexmod")?;
    let mixlevel = get_u8("mixlevel")?;
    let roomtyp = get_u8("roomtyp")?;
    let adconvtyp = get_bool("adconvtyp")?;
    let sourcefscod = get_bool("sourcefscod")?;
    if bsmod.is_some()
        || copyright.is_some()
        || origbs.is_some()
        || dsurmod.is_some()
        || dheadphonmod.is_some()
        || dsurexmod.is_some()
        || mixlevel.is_some()
        || roomtyp.is_some()
        || adconvtyp.is_some()
        || sourcefscod.is_some()
    {
        let d = Eac3InfoMetadata::default();
        let audprod = if mixlevel.is_some() || roomtyp.is_some() || adconvtyp.is_some() {
            Some(Eac3AudioProduction {
                mixlevel: mixlevel.unwrap_or(0),
                roomtyp: roomtyp.unwrap_or(0),
                adconvtyp: adconvtyp.unwrap_or(false),
            })
        } else {
            None
        };
        meta.infomd = Some(Eac3InfoMetadata {
            bsmod: bsmod.unwrap_or(d.bsmod),
            copyrightb: copyright.unwrap_or(d.copyrightb),
            origbs: origbs.unwrap_or(d.origbs),
            dsurmod: dsurmod.unwrap_or(d.dsurmod),
            dheadphonmod: dheadphonmod.unwrap_or(d.dheadphonmod),
            dsurexmod: dsurexmod.unwrap_or(d.dsurexmod),
            audprod,
            sourcefscod: sourcefscod.unwrap_or(d.sourcefscod),
        });
    }
    meta.validate()?;
    Ok(meta)
}

/// [`make_encoder`] with a typed [`Eac3Metadata`] — the encoder-side
/// bitstream-metadata surface (fixed-BSI `dialnorm`/`compr`, per-block
/// `dynrng`, and the Table E1.2 mixing / informational blocks).
pub fn make_encoder_with_metadata(
    params: &CodecParameters,
    meta: Eac3Metadata,
) -> Result<Box<dyn Encoder>> {
    meta.validate()?;
    let mut concrete = build_concrete_encoder(params)?;
    concrete.meta = meta;
    validate_num_blocks_tools(&concrete)?;
    validate_frame_fit(&mut concrete)?;
    Ok(Box::new(concrete))
}

/// Enhanced coupling needs at least two coupled fbw channels in the
/// independent substream — mono has nothing to couple.
fn validate_ecpl(_ecpl: &EcplParams, channels: u16) -> Result<()> {
    match channels {
        2 | 6 | 8 => Ok(()),
        n => Err(Error::Unsupported(format!(
            "eac3 encoder: ecpl requires 2, 6, or 8 input channels (got {n}) — \
             the independent substream must carry at least two fbw channels"
        ))),
    }
}

/// Build an E-AC-3 encoder with the Adaptive Hybrid Transform enabled
/// (§3.4 / Table E1.3 `ahte`). Every full-bandwidth channel of every
/// substream is coded through the 6-block DCT-II + VQ/GAQ quantiser
/// stack: exponents are transmitted once per frame (block-0 anchor,
/// blocks 1..5 REUSE, so `nchregs[ch] == 1`), the per-bin `hebap[]`
/// pointers come from the §3.4.3.1 high-efficiency allocation, and
/// the block-0 mantissa slot carries the front-loaded `chgaqmod` +
/// gain words + 6×nmant codeword stream instead of six per-block
/// mantissa payloads. Best suited to stationary content (the encoder
/// forces long transforms — no block switching — while AHT is on).
///
/// Also reachable through the registry path via
/// `CodecParameters::options` key `aht` = `1`/`true`.
pub fn make_encoder_with_aht(params: &CodecParameters) -> Result<Box<dyn Encoder>> {
    let mut concrete = build_concrete_encoder(params)?;
    concrete.aht = true;
    validate_num_blocks_tools(&concrete)?;
    validate_frame_fit(&mut concrete)?;
    Ok(Box::new(concrete))
}

/// Build an E-AC-3 encoder with **transient pre-noise processing**
/// analysis enabled (§3.7 / §2.3.2.21-23 / Table E1.3 `transproce`).
///
/// Per frame and per full-bandwidth channel the encoder locates the
/// first sharp onset in the PCM and transmits the §2.3.2.22 transient
/// location (4-sample resolution, relative to the frame's first
/// sample) plus the §2.3.2.23 time-scaling-synthesis length; the
/// decoder's §3.7.2 synthesis then overwrites the coding pre-noise
/// ahead of the transient with time-scaled clean audio from before
/// it. Long transforms are forced while TPNP is on — the tool exists
/// to fix the pre-noise of long-transform coding, which
/// block-switching would otherwise avoid. Frames without a detected
/// transient emit `transproce = 0` and are byte-identical to the
/// default encoder's long-transform output.
///
/// Also reachable through the registry path via the
/// `CodecParameters::options` key `tpnp` = `1`/`true`.
pub fn make_encoder_with_tpnp(params: &CodecParameters) -> Result<Box<dyn Encoder>> {
    let mut concrete = build_concrete_encoder(params)?;
    concrete.tpnp = true;
    validate_num_blocks_tools(&concrete)?;
    validate_frame_fit(&mut concrete)?;
    Ok(Box::new(concrete))
}

/// Parse the `spx*` codec options (see [`make_encoder`]) into an
/// [`SpxParams`], or `None` when SPX is not requested.
fn spx_params_from_options(params: &CodecParameters) -> Result<Option<SpxParams>> {
    let opts = &params.options;
    let parse_bool = |key: &str| -> Result<Option<bool>> {
        match opts.get(key) {
            None => Ok(None),
            Some("1") | Some("true") => Ok(Some(true)),
            Some("0") | Some("false") => Ok(Some(false)),
            Some(v) => Err(Error::invalid(format!(
                "eac3 encoder: option {key}={v} (expected 1/true/0/false)"
            ))),
        }
    };
    let parse_u8 = |key: &str, max: u8| -> Result<Option<u8>> {
        match opts.get(key) {
            None => Ok(None),
            Some(v) => match v.parse::<u8>() {
                Ok(n) if n <= max => Ok(Some(n)),
                _ => Err(Error::invalid(format!(
                    "eac3 encoder: option {key}={v} (expected 0..={max})"
                ))),
            },
        }
    };
    let enabled = parse_bool("spx")?;
    let begf = parse_u8("spx_begf", 7)?;
    let endf = parse_u8("spx_endf", 7)?;
    let strtf = parse_u8("spx_strtf", 3)?;
    let blnd = parse_u8("spx_blnd", 31)?;
    let atten = parse_u8("spx_atten", 31)?;
    let adaptive = parse_bool("spx_adaptive_copy_start")?;
    let explicit = parse_bool("spx_explicit_band_structure")?;
    // §E.2.3.3.3 mixed per-channel chinspx: a bitmask of fbw channels
    // in SPX (bit ch → channel ch). 0 / absent → all channels.
    let chmask = parse_u8("spx_chmask", 255)?;
    let any_subkey = begf.is_some()
        || endf.is_some()
        || strtf.is_some()
        || blnd.is_some()
        || atten.is_some()
        || adaptive.is_some()
        || explicit.is_some()
        || chmask.is_some();
    // `spx=0` wins over sub-keys; sub-keys alone imply enablement.
    let on = match enabled {
        Some(v) => v,
        None => any_subkey,
    };
    if !on {
        return Ok(None);
    }
    let d = SpxParams::default();
    Ok(Some(SpxParams {
        spxbegf: begf.unwrap_or(d.spxbegf),
        spxendf: endf.unwrap_or(d.spxendf),
        spxstrtf: strtf.unwrap_or(d.spxstrtf),
        spxblnd: blnd.unwrap_or(d.spxblnd),
        adaptive_copy_start: adaptive.unwrap_or(d.adaptive_copy_start),
        atten_code: atten,
        explicit_band_structure: explicit.unwrap_or(d.explicit_band_structure),
        channel_mask: match chmask {
            None | Some(0) => None,
            m => m,
        },
    }))
}

/// Standard-coupling configuration (Annex E `ecplinu = 0`,
/// §E.2.3.3.14-15). `None` fields take the defaults chosen by
/// [`StdCplGeometry::derive`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct StdCplParams {
    /// `cplbegf` (4 bits). Default 8 (bin 133), lowered to the derived
    /// end when SPX bounds the region below that.
    begf: Option<u8>,
    /// `cplendf` (4 bits) — transmitted only when SPX is off (with SPX
    /// it is derived from `spxbegf`, so setting it is rejected).
    /// Default 15 (bin 253).
    endf: Option<u8>,
}

/// Resolved standard-coupling region (§E.2.3.3.14-15 / §7.4.2).
#[derive(Clone, Copy, Debug)]
struct StdCplGeometry {
    /// `cplbegf` code.
    begf: u8,
    /// Transmitted `cplendf` (SPX off only).
    endf: u8,
    /// SPX co-active: `cplendf` is derived and not transmitted.
    spx_bounded: bool,
    /// `ncplsubnd = 3 + cplendf - cplbegf`.
    nsubbnd: usize,
    /// Merge flags by offset from `cplbegf` (Table E2.12 default
    /// structure — `cplbndstrce = 0` on the frame's first coupling
    /// block).
    bndstrc: [bool; 18],
    /// Number of coupling bands after merging.
    nbnd: usize,
    /// Sub-band offset → band index.
    sbnd_to_bnd: [usize; 18],
    /// First coupled bin (`37 + 12·cplbegf`).
    begin_bin: usize,
    /// One past the last coupled bin (`37 + 12·(cplendf + 3)`).
    end_bin: usize,
}

impl StdCplGeometry {
    fn derive(p: &StdCplParams, spx: Option<&SpxParams>) -> Result<Self> {
        // Signed cplendf: with SPX in use, Table E1.4 derives it from
        // spxbegf and notes it may be negative (spxbegf < 2).
        let (endf_signed, spx_bounded): (i32, bool) = match spx {
            Some(sp) => {
                if p.endf.is_some() {
                    return Err(Error::invalid(
                        "eac3 encoder: cpl_endf is derived from spx_begf when SPX is on \
                         (Table E1.4) and cannot be set",
                    ));
                }
                let b = sp.spxbegf as i32;
                (if b < 6 { b - 2 } else { b * 2 - 7 }, true)
            }
            None => (p.endf.unwrap_or(15) as i32, false),
        };
        let max_begf = endf_signed + 2; // ncplsubnd >= 1
        if max_begf < 0 {
            return Err(Error::invalid("eac3 encoder: empty coupling region"));
        }
        let begf = match p.begf {
            Some(b) => b as i32,
            None => 8.min(max_begf),
        };
        if !(0..=15).contains(&begf) || !(-2..=15).contains(&endf_signed) {
            return Err(Error::invalid(format!(
                "eac3 encoder: coupling codes out of range (cplbegf={begf}, cplendf={endf_signed})"
            )));
        }
        let nsubbnd_signed = 3 + endf_signed - begf;
        if nsubbnd_signed < 1 {
            return Err(Error::invalid(format!(
                "eac3 encoder: cpl_begf={begf} leaves no coupling sub-band below cplendf+2={max_begf}"
            )));
        }
        let nsubbnd = nsubbnd_signed as usize;
        let mut bndstrc = [false; 18];
        for (off, slot) in bndstrc.iter_mut().enumerate().take(nsubbnd).skip(1) {
            *slot = super::dsp::DEFCPLBNDSTRC[begf as usize + off];
        }
        let mut sbnd_to_bnd = [0usize; 18];
        let mut bnd = 0usize;
        for (off, slot) in sbnd_to_bnd.iter_mut().enumerate().take(nsubbnd) {
            if off > 0 && !bndstrc[off] {
                bnd += 1;
            }
            *slot = bnd;
        }
        Ok(Self {
            begf: begf as u8,
            endf: endf_signed.max(0) as u8,
            spx_bounded,
            nsubbnd,
            bndstrc,
            nbnd: bnd + 1,
            sbnd_to_bnd,
            begin_bin: 37 + 12 * begf as usize,
            end_bin: (37 + 12 * (endf_signed + 3)) as usize,
        })
    }
}

fn std_cpl_params_from_options(params: &CodecParameters) -> Result<Option<StdCplParams>> {
    let opts = &params.options;
    let parse_u8 = |key: &str| -> Result<Option<u8>> {
        match opts.get(key) {
            None => Ok(None),
            Some(v) => match v.parse::<u8>() {
                Ok(n) if n <= 15 => Ok(Some(n)),
                _ => Err(Error::invalid(format!(
                    "eac3 encoder: option {key}={v} (expected 0..=15)"
                ))),
            },
        }
    };
    let enabled = match opts.get("cpl") {
        None => None,
        Some("1") | Some("true") => Some(true),
        Some("0") | Some("false") => Some(false),
        Some(v) => {
            return Err(Error::invalid(format!(
                "eac3 encoder: option cpl={v} (expected 1/true/0/false)"
            )));
        }
    };
    let begf = parse_u8("cpl_begf")?;
    let endf = parse_u8("cpl_endf")?;
    // `cpl=0` wins over sub-keys; sub-keys alone imply enablement.
    if !enabled.unwrap_or(begf.is_some() || endf.is_some()) {
        return Ok(None);
    }
    Ok(Some(StdCplParams { begf, endf }))
}

/// Construction-time checks for standard coupling: two or more fbw
/// channels, a valid region (with SPX: below the SPX begin), and no
/// other coupling-domain tool alongside.
fn validate_std_cpl(enc: &Eac3Encoder, cpl: &StdCplParams, channels: u16) -> Result<()> {
    if channels < 2 {
        return Err(Error::Unsupported(
            "eac3 encoder: cpl needs at least two fbw channels".into(),
        ));
    }
    if enc.aht || enc.ecpl.is_some() {
        return Err(Error::Unsupported(
            "eac3 encoder: cpl cannot be combined with aht / ecpl (single-subsystem scope)".into(),
        ));
    }
    if let Some(spx) = &enc.spx
        && spx.channel_mask.is_some()
    {
        return Err(Error::Unsupported(
            "eac3 encoder: spx channel_mask cannot be combined with cpl — every coupled \
             channel must also be in SPX"
                .into(),
        ));
    }
    StdCplGeometry::derive(cpl, enc.spx.as_ref())?;
    Ok(())
}

/// Build the concrete [`Eac3Encoder`] (with `snroffststr == 0`). The
/// public [`make_encoder`] boxes this behind `dyn Encoder`; the test-only
/// [`make_encoder_with_snroffststr`] reuses it and overrides the strategy.
fn build_concrete_encoder(params: &CodecParameters) -> Result<Eac3Encoder> {
    let sample_rate = params.sample_rate.ok_or_else(|| {
        Error::invalid("eac3 encoder: sample_rate is required (48000/44100/32000)")
    })?;
    let channels = params
        .channels
        .ok_or_else(|| Error::invalid("eac3 encoder: channels is required (1, 2, 6, or 8)"))?;
    let fscod: u8 = match sample_rate {
        48_000 => 0,
        44_100 => 1,
        32_000 => 2,
        _ => {
            return Err(Error::Unsupported(format!(
                "eac3 encoder: unsupported sample rate {sample_rate} (48000/44100/32000 only)"
            )));
        }
    };
    let target_kbps: Option<u32> = params.bit_rate.map(|b| (b / 1000) as u32);
    let num_blocks = num_blocks_from_options(params)?;

    // Build per-substream layout descriptors. For 1/2/6 channel input
    // we emit one independent substream; for 8 we emit indep+dep.
    let layout = match channels {
        1 => Layout::Indep(SubstreamLayout {
            strmtyp: 0,
            substreamid: 0,
            acmod: 1,
            lfeon: false,
            nfchans: 1,
            chanmap: None,
            // Map each substream channel slot to the input PCM
            // interleaved-channel index. Mono = passthrough.
            src_indices: vec![0],
            // Bytes per syncframe; chosen from `bit_rate` or the per-
            // layout default below.
            kbps: target_kbps.unwrap_or(96),
            frame_bytes: 0,
        }),
        2 => Layout::Indep(SubstreamLayout {
            strmtyp: 0,
            substreamid: 0,
            acmod: 2,
            lfeon: false,
            nfchans: 2,
            chanmap: None,
            src_indices: vec![0, 1],
            kbps: target_kbps.unwrap_or(192),
            frame_bytes: 0,
        }),
        6 => Layout::Indep(SubstreamLayout {
            strmtyp: 0,
            substreamid: 0,
            acmod: 7, // 3/2 — L,C,R,Ls,Rs
            lfeon: true,
            nfchans: 5,
            chanmap: None,
            // Input layout L,C,R,Ls,Rs,LFE → substream order:
            // [L, C, R, Ls, Rs] then LFE pseudo-channel last.
            src_indices: vec![0, 1, 2, 3, 4, 5],
            kbps: target_kbps.unwrap_or(384),
            frame_bytes: 0,
        }),
        8 => {
            // 7.1 input: L,C,R,Ls,Rs,LFE,Lb,Rb.
            // Indep substream = 5.1 downmix where Ls/Rs come straight
            // from the 7.1 source's surround pair (the spec's
            // §E.3.8.2 figure shows the 5.1 downmix as the
            // independently-decodable program).
            let total_kbps = target_kbps.unwrap_or(576);
            // Split: most of the budget goes to the 5.1 indep stream,
            // a quarter goes to the dep Lb/Rb pair (matches stereo).
            let indep_kbps = match total_kbps {
                k if k >= 384 + 192 => 384,
                k => (k * 2) / 3, // generous fallback
            };
            let dep_kbps = total_kbps.saturating_sub(indep_kbps).max(64);
            Layout::Pair {
                indep: SubstreamLayout {
                    strmtyp: 0,
                    substreamid: 0,
                    acmod: 7,
                    lfeon: true,
                    nfchans: 5,
                    chanmap: None,
                    src_indices: vec![0, 1, 2, 3, 4, 5],
                    kbps: indep_kbps,
                    frame_bytes: 0,
                },
                dep: SubstreamLayout {
                    strmtyp: 1,
                    substreamid: 0,
                    acmod: 2, // 2/0 — two coded channels (Lb, Rb)
                    lfeon: false,
                    nfchans: 2,
                    // chanmap bit 6 = Lrs/Rrs pair (Table E2.5).
                    // Per §E.2.3.1.8: bit 0 → MSB of the 16-bit field.
                    // bit 6 → MSB-6 → mask = 1 << (15 - 6) = 0x0200.
                    chanmap: Some(1u16 << (15 - 6)),
                    // Source PCM offsets 6 (Lb) and 7 (Rb).
                    src_indices: vec![6, 7],
                    kbps: dep_kbps,
                    frame_bytes: 0,
                },
            }
        }
        n => {
            return Err(Error::Unsupported(format!(
                "eac3 encoder: unsupported channel count {n} (must be 1, 2, 6, or 8)"
            )));
        }
    };

    // Resolve frame_bytes for each substream + total per-syncframe size
    // (used in `out_params.bit_rate` reporting).
    let total_kbps_reported: u32;
    let layout_resolved = match layout {
        Layout::Indep(mut s) => {
            let bytes = ac3_frame_bytes_blocks(fscod, s.kbps, num_blocks).ok_or_else(|| {
                Error::Unsupported(format!(
                    "eac3 encoder: bit rate {} kbps has no frame-size mapping",
                    s.kbps
                ))
            })? as usize;
            s.frame_bytes = bytes;
            total_kbps_reported = s.kbps;
            Layout::Indep(s)
        }
        Layout::Pair { mut indep, mut dep } => {
            let i_bytes =
                ac3_frame_bytes_blocks(fscod, indep.kbps, num_blocks).ok_or_else(|| {
                    Error::Unsupported(format!(
                        "eac3 encoder: indep bit rate {} kbps has no frame-size mapping",
                        indep.kbps
                    ))
                })? as usize;
            let d_bytes = ac3_frame_bytes_blocks(fscod, dep.kbps, num_blocks).ok_or_else(|| {
                Error::Unsupported(format!(
                    "eac3 encoder: dep bit rate {} kbps has no frame-size mapping",
                    dep.kbps
                ))
            })? as usize;
            indep.frame_bytes = i_bytes;
            dep.frame_bytes = d_bytes;
            total_kbps_reported = indep.kbps + dep.kbps;
            Layout::Pair { indep, dep }
        }
    };

    match &layout_resolved {
        Layout::Indep(s) => validate_min_rate(s, num_blocks, 0)?,
        Layout::Pair { indep, dep } => {
            validate_min_rate(indep, num_blocks, 0)?;
            validate_min_rate(dep, num_blocks, 0)?;
        }
    }

    let total_pcm_chans = channels as usize;
    let out_params = {
        let mut p = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        p.sample_rate = Some(sample_rate);
        p.channels = Some(channels);
        p.sample_format = Some(SampleFormat::S16);
        p.bit_rate = Some(total_kbps_reported as u64 * 1000);
        p
    };

    let input_sample_format = params.sample_format.unwrap_or(SampleFormat::S16);
    Ok(Eac3Encoder {
        codec_id: CodecId::new(CODEC_ID_STR),
        out_params,
        sample_rate,
        total_pcm_chans,
        input_sample_format,
        fscod,
        layout: layout_resolved,
        // One delay-line / transient slot per *input* channel. Every
        // distinct PCM channel that ends up in any substream needs its
        // own MDCT-priming history; channels never appear in two
        // substreams under the 7.1 layout, so total_pcm_chans is the
        // upper bound.
        delay_line: vec![vec![0.0f32; SAMPLES_PER_BLOCK]; total_pcm_chans],
        pending_samples: vec![Vec::<f32>::new(); total_pcm_chans],
        transient_state: (0..total_pcm_chans)
            .map(|_| TransientDetector::default())
            .collect(),
        packet_queue: Vec::new(),
        pts: 0,
        snroffststr: 0,
        spx: None,
        aht: false,
        ecpl: None,
        stdcpl: None,
        dither: true,
        meta: Eac3Metadata::default(),
        num_blocks,
        frames_emitted: 0,
        tpnp: false,
        ecpl_carry: vec![None, None],
        cadence_csnr: vec![CADENCE_CSNR_INIT; 2],
        last_blksw: vec![false; total_pcm_chans],
    })
}

/// Validate a mixed-membership `channel_mask` against the encoder's
/// channel count (§E.2.3.3.3): the mask must keep at least one coded
/// channel in SPX, and mono cannot exclude its only channel (the
/// spec makes `chinspx[0]` implicit for `acmod == 0x1`).
fn validate_spx_channel_mask(spx: &SpxParams, channels: u16) -> Result<()> {
    if let Some(m) = spx.channel_mask {
        if m == 0 {
            return Err(Error::invalid(
                "eac3 encoder: spx channel_mask must keep at least one channel in SPX",
            ));
        }
        if channels == 1 && m & 1 == 0 {
            return Err(Error::invalid(
                "eac3 encoder: mono SPX cannot exclude channel 0 (chinspx[0] is implicit)",
            ));
        }
    }
    Ok(())
}

/// Build an E-AC-3 encoder with Spectral Extension (SPX, §E.2.3.3 /
/// §E.3.6) enabled.
///
/// SPX is E-AC-3's parametric high-frequency reconstruction: the coded
/// bandwidth of every full-bandwidth channel stops at the SPX begin
/// frequency (`25 + 12·spx_begin_subbnd` transform coefficients; tc#
/// 109 ≈ 10.2 kHz at 48 kHz for the default [`SpxParams`]), and the
/// decoder regenerates the extension region from a translated copy of
/// the channel's own low-frequency spectrum, noise-blended and scaled
/// by per-band coordinates the encoder derives from the §3.6.4.3
/// energy-matching rule. The bits saved on high-frequency exponents /
/// mantissas are re-spent on the coded low band by the SNR-offset
/// tuner, so SPX trades exact HF waveforms for a better LF floor —
/// the intended low-bit-rate operating mode.
///
/// Accepts the same `params` as [`make_encoder`]. The geometry is
/// validated up front; an invalid combination (inverted sub-band
/// range, empty copy region, out-of-range field) fails here rather
/// than at the first frame.
pub fn make_encoder_with_spx(params: &CodecParameters, spx: SpxParams) -> Result<Box<dyn Encoder>> {
    // Fail fast on invalid geometry (same validation the per-frame
    // emitter performs).
    SpxGeometry::derive(&spx, &DEFAULT_SPX_BNDSTRC)?;
    validate_spx_channel_mask(&spx, params.channels.unwrap_or(0))?;
    let mut concrete = build_concrete_encoder(params)?;
    concrete.spx = Some(spx);
    validate_num_blocks_tools(&concrete)?;
    validate_frame_fit(&mut concrete)?;
    Ok(Box::new(concrete))
}

/// Test-only constructor that selects a non-zero `snroffststr` (per-block
/// SNR-offset strategy). Builds the same encoder as [`make_encoder`] then
/// overrides the strategy. Used by the round-trip tests to drive the
/// decoder's §2.3.3.27 per-block SNR-offset parse.
#[cfg(test)]
pub(crate) fn make_encoder_with_snroffststr(
    params: &CodecParameters,
    snroffststr: u8,
) -> Result<Box<dyn Encoder>> {
    let mut concrete = build_concrete_encoder(params)?;
    concrete.snroffststr = snroffststr;
    Ok(Box::new(concrete))
}

/// Pick a syncframe size in bytes for a given (fscod, target kbps)
/// and block count.
///
/// For the 6-block shape the formula matches AC-3's Table 5.18 row
/// size: `bytes = round((kbps * 1000 * 1536) / (sample_rate * 8))` —
/// at 48 kHz this collapses to `kbps * 4` exactly. Fractional frames
/// (§E.2.3.1.5 — 256 samples per block) scale the byte budget by
/// `nblks / 6`. Returns `None` for unsupported bit rates.
fn ac3_frame_bytes_blocks(fscod: u8, kbps: u32, nblks: usize) -> Option<u32> {
    let sample_rate: u32 = match fscod {
        0 => 48_000,
        1 => 44_100,
        2 => 32_000,
        _ => return None,
    };
    // 256·nblks samples per syncframe; 8 bits per byte.
    let numerator = kbps as u64 * 1000 * (SAMPLES_PER_BLOCK as u64 * nblks as u64);
    let denom = sample_rate as u64 * 8;
    // AC-3 spec rounds toward the table value; we round down so the
    // bit-rate is a hard upper bound. Even byte counts only: round to
    // the nearest 2-byte boundary so the frame size is a whole word
    // (E-AC-3 frmsiz is in 16-bit words, value = words - 1).
    let raw = numerator / denom;
    let rounded = raw & !1;
    if (32..=4096).contains(&rounded) {
        Some(rounded as u32)
    } else {
        None
    }
}

/// Parse the `blocks` codec option (§E.2.3.1.5 `numblkscod`): audio
/// blocks per syncframe — 1, 2, 3 (fractional frames) or 6 (default).
fn num_blocks_from_options(params: &CodecParameters) -> Result<usize> {
    match params.options.get("blocks") {
        None => Ok(BLOCKS_PER_FRAME),
        Some(v) => match v.parse::<usize>() {
            Ok(n @ (1 | 2 | 3 | 6)) => Ok(n),
            _ => Err(Error::invalid(format!(
                "eac3 encoder: option blocks={v} (expected 1, 2, 3, or 6 — Table E2.4)"
            ))),
        },
    }
}

/// Configuration for one E-AC-3 substream within a syncframe pair.
#[derive(Clone, Debug)]
struct SubstreamLayout {
    /// `strmtyp` field (§E.2.3.1.1). 0 = independent, 1 = dependent.
    strmtyp: u8,
    /// `substreamid` (§E.2.3.1.2). 0 for the primary indep / first dep.
    substreamid: u8,
    /// AC-3 audio coding mode (Table 5.8) of the coded substream.
    acmod: u8,
    /// Whether an LFE pseudo-channel is coded inside the substream.
    lfeon: bool,
    /// Number of full-bandwidth channels coded by this substream
    /// (`acmod_nfchans(acmod)`).
    nfchans: usize,
    /// Custom chanmap (§E.2.3.1.7-8). When `Some`, `chanmape=1` is
    /// emitted in the bsi and the 16-bit `chanmap` field follows. Only
    /// valid when `strmtyp == 1`.
    chanmap: Option<u16>,
    /// Source-PCM channel index for each coded slot. Length =
    /// `nfchans + lfeon as usize`.
    src_indices: Vec<usize>,
    /// Target bit rate in kbps (recorded for reporting / debug).
    #[allow(dead_code)]
    kbps: u32,
    /// Computed syncframe size in bytes (filled in by `make_encoder`).
    frame_bytes: usize,
}

#[derive(Clone, Debug)]
enum Layout {
    /// Single independent substream.
    Indep(SubstreamLayout),
    /// 7.1: independent (5.1 channel program) + dependent (Lb/Rb pair).
    Pair {
        indep: SubstreamLayout,
        dep: SubstreamLayout,
    },
}

struct Eac3Encoder {
    codec_id: CodecId,
    out_params: CodecParameters,
    sample_rate: u32,
    /// Total number of input PCM channels (1, 2, 6, or 8).
    total_pcm_chans: usize,
    input_sample_format: SampleFormat,
    fscod: u8,
    layout: Layout,
    /// Per-input-channel left-half MDCT context (256 samples each).
    delay_line: Vec<Vec<f32>>,
    /// Per-input-channel pending PCM samples, drained 1536 at a time.
    pending_samples: Vec<Vec<f32>>,
    /// Per-input-channel transient-detector state.
    transient_state: Vec<TransientDetector>,
    packet_queue: Vec<Packet>,
    pts: i64,
    /// SNR-offset strategy emitted in audfrm (§2.3.2.3 / Table E1.3).
    /// `0` = single frame-level pair (the default the public
    /// [`make_encoder`] always selects); `1` = one shared per-block fine
    /// offset; `2` = independent per-channel per-block fine offsets. The
    /// non-zero modes are exercised by the round-trip tests to validate
    /// the decoder's per-block SNR-offset parse; the chosen offsets equal
    /// the frame-level values so the decoded PCM matches the `0` baseline.
    snroffststr: u8,
    /// Spectral extension configuration (§E.2.3.3 / §E.3.6). `None`
    /// (the [`make_encoder`] default) codes the full bandwidth; `Some`
    /// (via [`make_encoder_with_spx`]) puts every fbw channel of every
    /// substream in SPX.
    spx: Option<SpxParams>,
    /// Adaptive Hybrid Transform (§3.4). When `true` (via
    /// [`make_encoder_with_aht`] or the `aht` option) every fbw
    /// channel of every substream is AHT-coded: single block-0
    /// exponent anchor, hebap-driven VQ/GAQ mantissas front-loaded in
    /// block 0, long transforms only. Mutually exclusive with `spx`.
    aht: bool,
    /// Enhanced coupling (§E.2.3.3.16-26 / §E.3.5.5). When `Some` (via
    /// [`make_encoder_with_ecpl`] or the `ecpl*` options) every fbw
    /// channel of the independent substream is coupled: the region
    /// above the enhanced-coupling begin frequency is carried by one
    /// shared carrier channel plus per-band amplitude/angle
    /// coordinates. Long transforms are forced while enhanced coupling
    /// is on. Mutually exclusive with `spx` and `aht`
    /// (single-subsystem scope).
    ecpl: Option<EcplParams>,
    /// Standard coupling (§E.2.3.3.14-15 with `ecplinu = 0`, §7.4).
    /// When `Some` (via the `cpl*` options) every fbw channel of the
    /// independent substream is coupled above `cplbegf`: one shared
    /// coupling channel plus per-band `cplco` coordinates refreshed on
    /// blocks 0 and 3. Co-active with SPX (the coupling region then
    /// ends at the SPX begin frequency, `cplendf` derived and possibly
    /// negative — Table E1.4 note); exclusive with `aht` / `ecpl`.
    stdcpl: Option<StdCplParams>,
    /// `dithflag[ch]` policy (§5.4.3.2 / §7.3.4). `true` (default):
    /// dither on except around block switches (§8.2.9). `false` (the
    /// `dither = 0` option): every block signals `dithflag = 0`, so
    /// zero-bit mantissas decode as true zeros — a deterministic decode,
    /// since the §7.3.4 dither sequence is decoder-private.
    dither: bool,
    /// Encoder-side bitstream metadata (fixed-BSI dialnorm/compr,
    /// per-block dynrng, Table E1.2 mixing / informational blocks).
    /// Validated at construction.
    meta: Eac3Metadata,
    /// Audio blocks per syncframe (§E.2.3.1.5 `numblkscod`, Table
    /// E2.4): 1, 2, 3 (fractional frames — `numblkscod` 0/1/2) or 6
    /// (the default, `numblkscod = 0x3`). Fractional frames use the
    /// standard coding path only: AHT is spec-implicit-off
    /// (`ahte = 0` when `numblkscod != 0x3`, Table E1.3) and SPX /
    /// enhanced coupling are 6-block-scope in this encoder (their
    /// coordinate anchors ride blocks 0/3).
    num_blocks: usize,
    /// Syncframes emitted so far — drives the §E.2.3.1.64 `convsync`
    /// cadence on fractional frames (the flag marks the syncframe
    /// that starts a 6-block AC-3 conversion group).
    frames_emitted: u64,
    /// Transient pre-noise processing (§3.7 / §2.3.2.21-23). When
    /// `true` (via [`make_encoder_with_tpnp`] or the `tpnp` option)
    /// the encoder performs the §3.7.1 transient-location analysis
    /// per fbw channel per frame and emits `transproce` +
    /// `chintransproc`/`transprocloc`/`transproclen` in audfrm; long
    /// transforms are forced (TPNP is the pre-noise tool for
    /// long-transform coding — with block switching active the
    /// pre-noise TPNP corrects is largely absent).
    tpnp: bool,
    /// Per-substream cross-frame enhanced-coupling analysis carry: the
    /// previous frame's last-block carrier + per-channel
    /// region-restricted MDCT buffers, mirroring the decoder's
    /// `EcplState::prev_frame_last_mant` threading (§E.3.5.5.1 "previous
    /// block" of frame block 0). Index = substream position in the
    /// layout (0 = indep, 1 = dep).
    ecpl_carry: Vec<Option<EcplCarry>>,
    /// Per-substream previous tuned `csnroffst` — the operating point
    /// the exponent-cadence election costs its candidates at.
    cadence_csnr: Vec<u8>,
    /// Per source PCM channel: was the previous frame's last block
    /// switched? (§8.2.9 dither defeat spans the following block.)
    last_blksw: Vec<bool>,
}

/// Cross-frame enhanced-coupling carry (see [`Eac3Encoder::ecpl_carry`]).
#[derive(Clone)]
struct EcplCarry {
    /// Previous frame's last-block carrier MDCT (region-restricted).
    carrier: [f32; 256],
    /// Previous frame's last-block per-fbw-channel region-restricted
    /// MDCT buffers (the per-channel §E.3.5.5.1 analysis neighbours).
    channels: Vec<[f32; 256]>,
}

impl Encoder for Eac3Encoder {
    fn codec_id(&self) -> &CodecId {
        &self.codec_id
    }
    fn output_params(&self) -> &CodecParameters {
        &self.out_params
    }
    fn send_frame(&mut self, frame: &Frame) -> Result<()> {
        let audio = match frame {
            Frame::Audio(a) => a,
            _ => {
                return Err(Error::invalid(
                    "eac3 encoder: send_frame requires an audio frame",
                ));
            }
        };
        let per_chan = decode_input_samples(audio, self.total_pcm_chans, self.input_sample_format)?;
        for ch in 0..self.total_pcm_chans {
            self.pending_samples[ch].extend_from_slice(&per_chan[ch]);
        }
        while self.pending_samples[0].len() >= self.frame_samples() {
            self.emit_syncframe()?;
        }
        Ok(())
    }
    fn receive_packet(&mut self) -> Result<Packet> {
        if self.packet_queue.is_empty() {
            return Err(Error::NeedMore);
        }
        Ok(self.packet_queue.remove(0))
    }
    fn flush(&mut self) -> Result<()> {
        if self.pending_samples[0].is_empty() {
            return Ok(());
        }
        let missing = self.frame_samples() - self.pending_samples[0].len();
        if missing > 0 {
            for ch in 0..self.total_pcm_chans {
                self.pending_samples[ch].extend(std::iter::repeat_n(0.0, missing));
            }
        }
        self.emit_syncframe()?;
        Ok(())
    }
}

impl Eac3Encoder {
    /// Drain one syncframe worth of PCM (1536 samples per input channel),
    /// emit one packet whose payload is the indep substream — or the
    /// indep+dep concatenation for the 7.1 pair layout.
    fn emit_syncframe(&mut self) -> Result<()> {
        let n_per = self.frame_samples();

        // Drain `n_per` samples per input channel into a per-input
        // PCM matrix, *advance* the per-input delay-line + transient
        // state once across all blocks/channels. We do the windowing /
        // MDCT inside `emit_substream` per-substream so that blksw and
        // exponents are computed against the substream's actual coded
        // PCM (which for indep substream of 7.1 is just a select of
        // the source channels).
        let mut frame_pcm: Vec<Vec<f32>> = Vec::with_capacity(self.total_pcm_chans);
        for ch in 0..self.total_pcm_chans {
            let drain: Vec<f32> = self.pending_samples[ch].drain(0..n_per).collect();
            frame_pcm.push(drain);
        }

        let mut payload = Vec::<u8>::with_capacity(self.layout_total_frame_bytes());
        // Snapshot the layout once so we don't borrow self while
        // calling emit_substream.
        let substreams: Vec<SubstreamLayout> = match &self.layout {
            Layout::Indep(s) => vec![s.clone()],
            Layout::Pair { indep, dep } => vec![indep.clone(), dep.clone()],
        };
        for (sub_idx, sub) in substreams.iter().enumerate() {
            let bytes = self.emit_substream(sub_idx, sub, &frame_pcm)?;
            payload.extend_from_slice(&bytes);
        }

        self.packet_queue.push(
            Packet::new(0, TimeBase::new(1, self.sample_rate as i64), payload).with_pts(self.pts),
        );
        self.pts += n_per as i64;
        self.frames_emitted += 1;
        Ok(())
    }

    /// PCM samples per input channel per syncframe (256 per block).
    fn frame_samples(&self) -> usize {
        SAMPLES_PER_BLOCK * self.num_blocks
    }

    /// Total syncframe-pair size in bytes.
    fn layout_total_frame_bytes(&self) -> usize {
        match &self.layout {
            Layout::Indep(s) => s.frame_bytes,
            Layout::Pair { indep, dep } => indep.frame_bytes + dep.frame_bytes,
        }
    }

    /// Emit one E-AC-3 syncframe (indep or dep substream) of size
    /// `sub.frame_bytes` bytes for the channels named by `sub.src_indices`.
    fn emit_substream(
        &mut self,
        sub_idx: usize,
        sub: &SubstreamLayout,
        frame_pcm: &[Vec<f32>],
    ) -> Result<Vec<u8>> {
        let nfchans = sub.nfchans;
        let total_chans = nfchans + usize::from(sub.lfeon);
        // Audio blocks this syncframe (§E.2.3.1.5). 6 by default;
        // 1/2/3 for fractional frames (standard coding path only —
        // AHT/SPX/ecpl are rejected at construction for nblks != 6).
        let nblks = self.num_blocks;
        // Enhanced coupling applies to the independent substream only
        // (the 7.1 pair's dependent Lb/Rb stream stays plain) and needs
        // at least two fbw channels to couple.
        let ecpl_on = self.ecpl.is_some() && sub.strmtyp == 0 && nfchans >= 2;
        let ecpl_geom: Option<EcplGeometry> = match (&self.ecpl, ecpl_on) {
            (Some(p), true) => Some(match &self.spx {
                // SPX co-active: the coupling region is bounded by the
                // SPX begin frequency and `ecplendf` is not transmitted
                // (§E.2.3.3.17).
                Some(spx) => EcplGeometry::derive_with_spx(p, spx.spxbegf, &DEFAULT_ECPL_BNDSTRC)?,
                None => EcplGeometry::derive(p, &DEFAULT_ECPL_BNDSTRC)?,
            }),
            _ => None,
        };
        // Standard coupling (independent substream, >= 2 fbw channels).
        let stdcpl_geom: Option<StdCplGeometry> = match &self.stdcpl {
            Some(p) if sub.strmtyp == 0 && nfchans >= 2 => {
                Some(StdCplGeometry::derive(p, self.spx.as_ref())?)
            }
            _ => None,
        };
        let stdcpl_on = stdcpl_geom.is_some();
        // Either coupling flavour puts a coupling channel in the frame
        // (`cplinu = 1`); `cpl_region` is its coded bin range.
        let cpl_on = ecpl_on || stdcpl_on;
        let cpl_region: Option<(usize, usize)> = match (&ecpl_geom, &stdcpl_geom) {
            (Some(g), _) => Some((g.start_bin, g.end_bin)),
            (None, Some(g)) => Some((g.begin_bin, g.end_bin)),
            (None, None) => None,
        };

        // -------- DSP: window + MDCT per substream channel per block --------
        let mut coeffs: Vec<Vec<[f32; N_COEFFS]>> = vec![vec![[0.0; N_COEFFS]; nblks]; total_chans];
        // blksw exists only for fbw channels (LFE never short-blocks).
        let mut blksw: Vec<[bool; BLOCKS_PER_FRAME]> = vec![[false; BLOCKS_PER_FRAME]; nfchans];
        // Pass 1 — §8.2.2 transient detection per fbw channel per
        // block. AHT / enhanced-coupling / TPNP frames force long
        // transforms: the 6-block DCT-II (§3.4.1) targets stationary
        // content and a short block would break the cross-block bin
        // alignment.
        let blksw_allowed =
            !self.aht && !ecpl_on && !self.tpnp && std::env::var("EAC3_DISABLE_BLKSW").is_err();
        for ch in 0..nfchans {
            let src_idx = sub.src_indices[ch];
            let drain = &frame_pcm[src_idx];
            for blk in 0..nblks {
                let fresh = &drain[blk * SAMPLES_PER_BLOCK..(blk + 1) * SAMPLES_PER_BLOCK];
                let fired = self.transient_state[src_idx].process(fresh);
                blksw[ch][blk] = blksw_allowed && fired;
            }
        }
        // Joint block-switch policy (see the AC-3 encoder): all fbw
        // channels of the substream switch together whenever any
        // channel's detector fires. `AC3_BLKSW_PER_CHANNEL=1` keeps
        // the per-channel flags for A/B measurement.
        if std::env::var("AC3_BLKSW_PER_CHANNEL").is_err() {
            for blk in 0..nblks {
                let any = (0..nfchans).any(|c| blksw[c][blk]);
                for c in 0..nfchans {
                    blksw[c][blk] = any;
                }
            }
        }
        // §8.2.9 dither defeat on a switched block and the one after it.
        let mut dithflag: Vec<[bool; BLOCKS_PER_FRAME]> = vec![[true; BLOCKS_PER_FRAME]; nfchans];
        for ch in 0..nfchans {
            let src_idx = sub.src_indices[ch];
            for blk in 0..nblks {
                let prev = if blk == 0 {
                    self.last_blksw[src_idx]
                } else {
                    blksw[ch][blk - 1]
                };
                dithflag[ch][blk] = !(blksw[ch][blk] || prev);
            }
            self.last_blksw[src_idx] = blksw[ch][nblks - 1];
        }
        // Pass 2 — window + forward MDCT per channel per block.
        for ch in 0..total_chans {
            let src_idx = sub.src_indices[ch];
            let drain = &frame_pcm[src_idx];
            let is_lfe_chan = sub.lfeon && ch == nfchans;
            for blk in 0..nblks {
                let mut in_buf = [0.0f32; 512];
                in_buf[..256].copy_from_slice(&self.delay_line[src_idx]);
                in_buf[256..].copy_from_slice(
                    &drain[blk * SAMPLES_PER_BLOCK..(blk + 1) * SAMPLES_PER_BLOCK],
                );
                let is_short = !is_lfe_chan && blksw[ch][blk];
                let mut win_buf = [0.0f32; 512];
                for n in 0..256 {
                    win_buf[n] = in_buf[n] * WINDOW[n];
                    win_buf[511 - n] = in_buf[511 - n] * WINDOW[n];
                }
                self.delay_line[src_idx].copy_from_slice(
                    &drain[blk * SAMPLES_PER_BLOCK..(blk + 1) * SAMPLES_PER_BLOCK],
                );
                if is_short {
                    mdct_256_pair(&win_buf, &mut coeffs[ch][blk]);
                } else {
                    mdct_512(&win_buf, &mut coeffs[ch][blk]);
                }
            }
        }

        // ---- Standard coupling (§7.4.1 / §E.2.3.3.14-15) ----
        //
        // Build the coupling channel + per-span coordinates and replace
        // every coupled channel's region bins with the decoder's
        // reconstruction, so SPX analysis (whose copy region may reach
        // into the coupled bins) sees what the decoder will translate.
        let stdcpl_plan: Option<StdCplPlan> = stdcpl_geom
            .as_ref()
            .map(|g| plan_std_cpl(g, &mut coeffs, nfchans, nblks));

        // ---- Transient pre-noise processing analysis (§3.7.1) ----
        //
        // Per fbw channel of this substream, locate the first sharp
        // onset in the frame's PCM. `transprocloc` is the transient
        // location relative to the frame's first PCM sample in
        // 4-sample units (§2.3.2.22); `transproclen` is the
        // time-scaling-synthesis length in samples (§2.3.2.23 — the
        // exact analysis is encoder tuning, only sketched by Figure
        // E3.2; see `detect_tpnp_transient` for the value chosen
        // here). A transient on a block boundary is skipped.
        let tpnp_plan: Option<Vec<Option<(u16, u16)>>> = if self.tpnp {
            Some(
                (0..nfchans)
                    .map(|ch| detect_tpnp_transient(&frame_pcm[sub.src_indices[ch]]))
                    .collect(),
            )
        } else {
            None
        };
        let mut transproce = tpnp_plan
            .as_ref()
            .is_some_and(|p| p.iter().any(Option::is_some));

        // -------- Exponents --------
        // Layout (matches the AC-3 helpers' expectation):
        //   0..nfchans  → fbw channels
        //   nfchans     → coupling pseudo-channel (unused — cplinu=0)
        //   nfchans + 1 → LFE pseudo-channel (when lfeon)
        let lfe_idx_in_exps = nfchans + 1;
        let mut exps: Vec<Vec<[u8; N_COEFFS]>> = vec![vec![[24u8; N_COEFFS]; nblks]; nfchans + 2];
        let chbwcod: u8 = 60;
        // §E.2.3.3 / §E.3.3.3 — with SPX in use, every fbw channel's
        // coded bandwidth ends at the SPX begin frequency
        // (`endmant = spxbandtable[spx_begin_subbnd]`) and no chbwcod
        // is emitted; without SPX the bandwidth code drives it.
        // The emitted `spxstrtf` — either the configured code or, with
        // `adaptive_copy_start`, the per-frame least-saturated pick.
        let mut spx_strtf_emit: u8 = self.spx.as_ref().map_or(0, |p| p.spxstrtf);
        let spx_geom = match &self.spx {
            Some(p) => {
                let mut geom = SpxGeometry::derive(p, &DEFAULT_SPX_BNDSTRC)?;
                if p.adaptive_copy_start {
                    // Score each valid candidate by the frame's total
                    // coordinate saturation (log-excess above the 0.875
                    // representable ceiling, §E.2.3.3.11-13), summed
                    // over channels and bands. Strict `<` keeps the
                    // lowest candidate on ties — the largest copy
                    // region carries the most source correlation.
                    let mut best_score = f64::INFINITY;
                    for cand in 0..=3u8 {
                        let cand_params = SpxParams {
                            spxstrtf: cand,
                            ..*p
                        };
                        let Ok(g) = SpxGeometry::derive(&cand_params, &DEFAULT_SPX_BNDSTRC) else {
                            continue; // empty copy region — invalid
                        };
                        let mut score = 0.0f64;
                        for ch_coeffs in coeffs.iter().take(nfchans) {
                            let span: Vec<&[f32; N_COEFFS]> = ch_coeffs.iter().collect();
                            let targets = band_coord_targets_span_atten(&span, &g, p.atten_code);
                            for &t in targets.iter().take(g.nbnds) {
                                if t as f64 > 0.875 {
                                    score += (t as f64 / 0.875).log2();
                                }
                            }
                        }
                        if score < best_score {
                            best_score = score;
                            spx_strtf_emit = cand;
                            geom = g;
                        }
                    }
                }
                Some(geom)
            }
            None => None,
        };
        let end_full: usize = 37 + 3 * (chbwcod as usize + 12);
        // §E.2.3.3.3 chinspx[ch] — mixed per-channel SPX membership.
        // With no mask (or SPX off) the vector is uniform.
        let in_spx: Vec<bool> = (0..nfchans)
            .map(|ch| {
                spx_geom.is_some()
                    && (
                        // Mono substreams carry no chinspx bits — the
                        // decoder assumes chinspx[0] == 1 (§E.2.3.3.3),
                        // so the mask cannot exclude the only channel.
                        (sub.acmod == 0x1 && ch == 0)
                            || self
                                .spx
                                .as_ref()
                                .and_then(|p| p.channel_mask)
                                .is_none_or(|m| m & (1 << ch) != 0)
                    )
            })
            .collect();
        let end_mant: usize = match (&spx_geom, cpl_region) {
            (_, Some((start, _))) if stdcpl_on => start,
            (Some(g), _) => g.begin_tc,
            (None, Some((start, _))) => start,
            (None, None) => end_full,
        };
        // Per-channel coded bandwidth: SPX channels stop at the SPX
        // begin frequency (§E.3.3.3), enhanced-coupling channels stop
        // at the region start (§E.3.3.3: endmant[ch] =
        // ecplsubbndtab[ecpl_begin_subbnd]), the rest run to the
        // chbwcod-derived end.
        let end_mant_ch: Vec<usize> = (0..nfchans)
            .map(|ch| {
                if let Some((start, _)) = cpl_region {
                    start
                } else if in_spx[ch] {
                    spx_geom.as_ref().map_or(end_full, |g| g.begin_tc)
                } else {
                    end_full
                }
            })
            .collect();
        let ch_end_mant = end_mant;
        for ch in 0..nfchans {
            for blk in 0..nblks {
                for k in 0..end_mant_ch[ch] {
                    exps[ch][blk][k] = extract_exponent(coeffs[ch][blk][k]);
                }
            }
        }
        if self.aht {
            // §3.4.2: AHT channels transmit exponents ONCE per frame
            // (nchregs[ch] == 1). The single set must bound all six
            // blocks' coefficients, so extract from the per-bin frame
            // maximum — every block's mantissa then stays sub-unity.
            for ch in 0..nfchans {
                for k in 0..ch_end_mant {
                    let mut mx = 0.0f32;
                    for blk in 0..nblks {
                        mx = mx.max(coeffs[ch][blk][k].abs());
                    }
                    exps[ch][0][k] = extract_exponent(mx);
                }
            }
        }
        if sub.lfeon {
            // §7.1.3 — the LFE carries `lfeendmant = 7` coefficients
            // (bins 0..6); all seven are coded and the §7.2 allocator
            // drops what is masked (see the AC-3 encoder's note).
            for blk in 0..nblks {
                for k in 0..LFE_END_MANT {
                    exps[lfe_idx_in_exps][blk][k] = extract_exponent(coeffs[nfchans][blk][k]);
                }
            }
        }
        // When `snroffststr != 0`, the audblk carries per-block SNR
        // offsets instead of the single frame-level pair in audfrm. Those
        // extra header bits must be reserved out of the mantissa budget,
        // or the bit-allocation tuner would fill the frame and the
        // per-block offsets would push the syncframe past `frame_bytes`.
        // Net extra bits vs the `snroffststr == 0` baseline:
        //   * audfrm drops `frmcsnroffst`(6) + `frmfsnroffst`(4) = 10.
        //   * each block adds an explicit `snroffste`(1) except block 0,
        //     plus `csnroffst`(6) plus the fine offsets.
        //   * snroffststr==1: one shared `blkfsnroffst`(4) per block.
        //   * snroffststr==2: `fsnroffst[ch]`(4·nfchans) + LFE(4) per
        //     block (no coupling slot — cplinu==0 throughout).
        // When `snroffststr != 0` the audblk carries per-block SNR
        // offsets instead of the single frame-level pair in audfrm. Those
        // extra header bits must be reserved out of the mantissa budget,
        // or the bit-allocation tuner would fill the frame and the
        // per-block offsets would push the syncframe past `frame_bytes`.
        // The `snroffststr == 0` default path reserves nothing, so its
        // output is byte-identical to before this change. Net extra bits
        // vs the baseline:
        //   * audfrm drops `frmcsnroffst`(6) + `frmfsnroffst`(4) = 10.
        //   * each block adds an explicit `snroffste`(1) except block 0,
        //     plus `csnroffst`(6) plus the fine offsets.
        //   * snroffststr==1: one shared `blkfsnroffst`(4) per block.
        //   * snroffststr==2: `fsnroffst[ch]`(4·nfchans) + LFE(4) per
        //     block (no coupling slot — cplinu==0 throughout).
        // Both non-zero strategies reserve the SAME (worst-case, i.e.
        // `snroffststr == 2`) overhead so that `1` and `2` tune against an
        // identical mantissa budget — making their decoded PCM bit-exact
        // to each other, which the round-trip test asserts. The `1` mode
        // leaves a few reserved bits unused as extra padding; that does
        // not change the bit allocation.
        let snr_reserve_bits: u32 = if self.snroffststr == 0 {
            0
        } else {
            let per_block_fine = 4 * nfchans as u32 + if sub.lfeon { 4 } else { 0 };
            let explicit_snroffste = (nblks as u32) - 1; // blk 0 implicit
            let per_block = nblks as u32 * (6 + per_block_fine);
            (per_block + explicit_snroffste).saturating_sub(10)
        };
        // SPX header bits over the SPX-off baseline (which spends one
        // bit per block on the disabled `spxinu`/`spxstre` slot). The
        // reserve always assumes the worst case — an explicit
        // `spxbndstrce == 1` band structure — so the default-banding
        // and explicit-banding emissions tune against an identical
        // mantissa budget and decode bit-identically (pinned by test).
        let spx_reserve_bits: u32 = match &spx_geom {
            None => 0,
            Some(g) => {
                // Block-0 strategy: chinspx (explicit unless mono) +
                // spxstrtf(2) + spxbegf(3) + spxendf(3) + spxbndstrce(1)
                // + explicit structure flags. spxinu itself replaces the
                // baseline's 1-bit slot (no delta).
                let chinspx = if sub.acmod == 0x1 { 0 } else { nfchans as u32 };
                let strat = chinspx + 2 + 3 + 3 + 1 + (g.end_subbnd - g.begin_subbnd - 1) as u32;
                // Coordinates: spxblnd(5) + mstrspxco(2) + 6 bits/band.
                let payload = 5 + 2 + 6 * g.nbnds as u32;
                // Attenuation (audfrm): chinspxatten(1) + spxattencod(5)
                // per fbw channel when signalled.
                let atten = if self.spx.as_ref().is_some_and(|p| p.atten_code.is_some()) {
                    6 * nfchans as u32
                } else {
                    0
                };
                // blk 0: payload (spxcoe implicit); blk 3: 1 + payload;
                // blks 1/2/4/5: 1-bit spxcoe each. Only channels in
                // SPX carry coordinates (mixed chinspx).
                let n_spx = in_spx.iter().filter(|&&v| v).count() as u32;
                strat + atten + n_spx * (payload + (1 + payload) + 4)
            }
        };
        // AHT header bits over the AHT-off baseline: one chahtinu bit
        // per fbw channel in audfrm (the 2-bit chgaqmod fields are
        // accounted inside the AHT mantissa cost).
        let aht_reserve_bits: u32 = if self.aht {
            nfchans as u32 + u32::from(sub.lfeon) + 8
        } else {
            0
        };
        // Enhanced-coupling header bits over what the shared (base-AC-3
        // shaped) overhead model already counts once `cpl.in_use` is set.
        // The model prices AC-3 standard-coupling syntax: strategy fields
        // on BOTH anchor blocks, per-block `cplcoe` bits and one
        // block-0 `mstrcplco + 8·nbnd` coordinate payload per coupled
        // channel. True Annex E enhanced coupling emits its strategy on
        // block 0 only but carries a much larger coordinate block —
        // `ecplangleintrp` every block plus per-channel exist bits,
        // 5-bit amplitudes, 6+3-bit angle/chaos pairs and a transient
        // flag. Reserve the (worst-case: block-3 full refresh) true cost
        // minus the modelled cost, with a small slop; over-reservation
        // only pads the frame, under-reservation would overflow the
        // hard bit-budget check below.
        let ecpl_reserve_bits: u32 = match &ecpl_geom {
            None => 0,
            Some(g) => {
                let nb = g.necplbnd as u32;
                let nch = nfchans as u32;
                let chincpl_bits = if sub.acmod == 0x2 { 0 } else { nch };
                let strategy = 1 + chincpl_bits + 4 + if g.spx_bounded { 0 } else { 4 } + 1;
                // Coordinates: block 0 (implicit exist bits) + block 3
                // (explicit, worst-case refresh) + 4 reuse blocks, plus
                // one ecplangleintrp bit per block.
                let blk0 = 5 * nb + (nch - 1) * (5 * nb + 9 * nb + 1);
                let blk3 = (1 + 5 * nb) + (nch - 1) * (2 + 5 * nb + 9 * nb + 1);
                let reuse = 4 * (1 + 3 * (nch - 1));
                let true_coords = 6 + blk0 + blk3 + reuse;
                // Modelled by `overhead_bits_for_ends` with cpl.in_use:
                let modelled_strategy = 2
                    * (nch
                        + 8
                        + u32::from(sub.acmod == 0x2)
                        + (g.end_subbnd - g.begin_subbnd - 1) as u32);
                let modelled_coords = 6 * nch + nch * (2 + 8 * nb);
                (strategy + true_coords + 16).saturating_sub(modelled_strategy + modelled_coords)
            }
        };
        // Standard-coupling header bits over the shared overhead model
        // (which prices the strategy on BOTH anchor blocks, one `cplcoe`
        // bit per coupled channel per block and ONE block-0 coordinate
        // payload). This encoder emits the strategy once (block 0, the
        // audfrm `cplstre` reuse covers the rest) but refreshes the
        // coordinates on block 3 too: reserve the true cost minus the
        // modelled cost, with a small slop.
        let stdcpl_reserve_bits: u32 = match &stdcpl_geom {
            None => 0,
            Some(g) => {
                let nch = nfchans as u32;
                let nb = g.nbnd as u32;
                let chincpl_bits = if sub.acmod == 0x2 { 0 } else { nch };
                let phs = u32::from(sub.acmod == 0x2);
                let strategy = 1 + chincpl_bits + phs + 4 + if g.spx_bounded { 0 } else { 4 } + 1;
                let payload = 2 + 8 * nb;
                let true_coords = nch * (2 * payload + (nblks as u32 - 1));
                let modelled_strategy = 2 * (nch + 8 + phs + g.nsubbnd.saturating_sub(1) as u32);
                let modelled_coords = nblks as u32 * nch + nch * payload;
                (strategy + true_coords + 16).saturating_sub(modelled_strategy + modelled_coords)
            }
        };
        // Optional metadata words (compr / mixing / informational
        // blocks / per-block dynrng) on top of the fixed layout.
        let meta_reserve_bits: u32 = self
            .meta
            .reserve_bits(sub.acmod, sub.lfeon, sub.strmtyp == 0);
        // §2.3.2.21-23 transient pre-noise data on top of the
        // baseline (which spends one transproce bit either way):
        // chintransproc per fbw channel + loc/len words per carrier.
        let mut tpnp_reserve_bits: u32 = if transproce {
            nfchans as u32 * (1 + 10 + 8)
        } else {
            0
        };
        // Transient pre-noise words are optional per frame: when the
        // cheapest legal exponent layout (one block-0 D45 anchor per
        // channel) cannot carry them inside this syncframe, the frame
        // goes out with `transproce = 0` instead of overflowing.
        if transproce {
            let cheapest = {
                let mut a = [0u8; BLOCKS_PER_FRAME];
                a[0] = 3;
                vec![a; nfchans]
            };
            let single_anchor: [u8; BLOCKS_PER_FRAME] = [1, 0, 0, 0, 0, 0];
            let floor_bits = overhead_bits_for_ends(
                &single_anchor[..nblks],
                Some(&cheapest),
                ch_end_mant,
                Some(&end_mant_ch),
                nfchans,
                &coupling_plan_for(ecpl_geom.as_ref(), stdcpl_geom.as_ref(), nfchans),
                &DbaPlan::default(),
                sub.acmod,
                sub.lfeon,
            ) + 64
                + snr_reserve_bits
                + spx_reserve_bits
                + aht_reserve_bits
                + ecpl_reserve_bits
                + stdcpl_reserve_bits
                + meta_reserve_bits;
            if floor_bits + tpnp_reserve_bits >= (sub.frame_bytes * 8) as u32 {
                transproce = false;
                tpnp_reserve_bits = 0;
            }
        }
        // Every tool / metadata reserve above depends on the frame's
        // geometry only, so it is fixed before the exponent plan is
        // elected: the plan guard below and the SNR tuner budget against
        // the same figure.
        let tool_reserve_bits: u32 = snr_reserve_bits
            + spx_reserve_bits
            + aht_reserve_bits
            + ecpl_reserve_bits
            + stdcpl_reserve_bits
            + meta_reserve_bits
            + tpnp_reserve_bits;
        // Exponent-strategy selection (§7.1.3 / §5.4.3.22 / §E.2.3.2).
        // Every fbw channel elects its own refresh cadence + D15/D25/D45
        // per anchor by measured bit cost (`elect_exponent_cadence`);
        // the frame-wide `exp_strategies` cadence drives the coupling
        // carrier / LFE exponent emission and the side-info
        // reservation. AHT frames keep their single block-0 anchor
        // (§3.4.2 `nchregs == 1`); enhanced-coupling frames keep the
        // legacy D15-on-blocks-0/3 carrier cadence.
        // EAC3_DISABLE_EXPSTR_SEL=1 pins every channel to the legacy
        // pattern for A/B testing.
        let legacy: [u8; BLOCKS_PER_FRAME] = [1, 0, 0, 1, 0, 0];
        let fixed_plan = std::env::var("EAC3_DISABLE_EXPSTR_SEL").is_ok();
        let ref_ba = cadence_reference_ba(
            &BitAllocParams {
                sdcycod: 2,
                fdcycod: 1,
                sgaincod: 1,
                dbpbcod: 2,
                floorcod: 4,
                csnroffst: 0,
                fsnroffst: 0,
                fsnroffst_ch: [0u8; crate::audblk::MAX_FBW],
                cplfsnroffst: 0,
                lfefsnroffst: 0,
                fgaincod: 4,
                cplfgaincod: 4,
                lfefgaincod: 4,
            },
            self.cadence_csnr[sub_idx.min(1)],
        );
        let fscod = self.fscod;
        let bpcb = (sub.frame_bytes * 8 / (nfchans.max(1) * nblks)) as u32;
        let mut exp_strategies: [u8; BLOCKS_PER_FRAME] = if fixed_plan || self.aht || cpl_on {
            legacy
        } else if sub.lfeon {
            let lfe_bap = |x: &[u8; N_COEFFS]| -> [u8; N_COEFFS] {
                let mut bap = [0u8; N_COEFFS];
                compute_bap(x, LFE_END_MANT, fscod, &ref_ba, &mut bap, None);
                bap
            };
            elect_exponent_cadence(
                &exps[lfe_idx_in_exps],
                0,
                LFE_END_MANT,
                0,
                true,
                bpcb,
                &lfe_bap,
            )
        } else {
            [1, 0, 0, 0, 0, 0]
        };
        // AHT frames: D15 on every channel is the dearest single-anchor
        // layout; when even the syntax + D15 exponents cannot leave a
        // mantissa byte, fall to D45 anchors.
        let aht_tight = self.aht && {
            let d15 = {
                let mut a = [0u8; BLOCKS_PER_FRAME];
                a[0] = 1;
                vec![a; nfchans]
            };
            overhead_bits_for_ends(
                &exp_strategies[..nblks],
                Some(&d15),
                ch_end_mant,
                Some(&end_mant_ch),
                nfchans,
                &CouplingPlan::default(),
                &DbaPlan::default(),
                sub.acmod,
                sub.lfeon,
            ) + 64
                + tool_reserve_bits
                >= (sub.frame_bytes * 8) as u32
        };
        let chexpstr_plan: Vec<[u8; BLOCKS_PER_FRAME]> = if self.aht {
            // §3.4.2 AHT eligibility: nchregs[ch] == 1 — a single
            // block-0 anchor with blocks 1..5 all REUSE. Pick the
            // smoothest legal strategy for the frame-wide exponent set
            // and propagate it to every block (the decoder reuses the
            // block-0 exponents for the whole frame, and the AHT
            // mantissa cache is scaled against them once).
            let mut out = vec![[0u8; BLOCKS_PER_FRAME]; nfchans];
            for ch in 0..nfchans {
                preprocess_d15(&mut exps[ch][0][..ch_end_mant]);
                // A frame whose syntax leaves no room for the fine
                // strategies takes D45 on every channel (the cheapest
                // legal anchor) regardless of spectral smoothness.
                let strat = if aht_tight {
                    3
                } else {
                    pick_strategy_for_block(&exps[ch][0], ch_end_mant)
                };
                out[ch][0] = strat;
                if strat >= 2 {
                    let grpsize = if strat == 2 { 2 } else { 4 };
                    quantise_exponents_to_grpsize(&mut exps[ch][0][..ch_end_mant], grpsize);
                }
                for blk in 1..nblks {
                    let src: [u8; N_COEFFS] = exps[ch][0];
                    exps[ch][blk][..ch_end_mant].copy_from_slice(&src[..ch_end_mant]);
                }
            }
            out
        } else {
            let plan: Vec<[u8; BLOCKS_PER_FRAME]> = (0..nfchans)
                .map(|ch| {
                    if fixed_plan {
                        let mut p = [0u8; BLOCKS_PER_FRAME];
                        p[..nblks].copy_from_slice(&legacy[..nblks]);
                        p
                    } else {
                        let chbw_bits = if !cpl_on && !in_spx[ch] { 6 } else { 0 };
                        let end = end_mant_ch[ch];
                        let fbw_bap = |e: &[u8; N_COEFFS]| -> [u8; N_COEFFS] {
                            let mut bap = [0u8; N_COEFFS];
                            compute_bap(e, end, fscod, &ref_ba, &mut bap, None);
                            bap
                        };
                        elect_exponent_cadence(&exps[ch], 0, end, chbw_bits, false, bpcb, &fbw_bap)
                    }
                })
                .collect();
            // Fold every reuse block into its anchor (per-bin minimum
            // exponent) before legalisation — `bound_shared_exponents`
            // — then D15-legalise every anchor.
            for ch in 0..nfchans {
                bound_shared_exponents(&mut exps[ch], &plan[ch][..nblks], 0, end_mant_ch[ch]);
                for blk in 0..nblks {
                    if plan[ch][blk] != 0 {
                        preprocess_d15(&mut exps[ch][blk][..end_mant_ch[ch]]);
                    }
                }
            }
            // Budget-floor guard: the elected plan can exceed a tight
            // frame on its own — the tuner then bails and the packer
            // overflows. Fractional frames hit this at ordinary rates
            // (they re-anchor exponents every frame while the byte
            // budget scales by nblks/6); 6-block frames hit it at rate
            // floors, especially with tool/metadata overhead riding
            // along. When the chosen plan's overhead cannot fit,
            // collapse every channel to a single block-0 D45 anchor
            // (the cheapest legal frame) before committing exponent
            // quantisation.
            let plan = {
                let mut plan = plan;
                let budget = (sub.frame_bytes * 8) as u32;
                let probe_cpl =
                    coupling_plan_for(ecpl_geom.as_ref(), stdcpl_geom.as_ref(), nfchans);
                let probe = overhead_bits_for_ends(
                    &exp_strategies[..nblks],
                    Some(&plan),
                    ch_end_mant,
                    Some(&end_mant_ch),
                    nfchans,
                    &probe_cpl,
                    &DbaPlan::default(),
                    sub.acmod,
                    sub.lfeon,
                ) + 64
                    + tool_reserve_bits;
                if probe >= budget {
                    // One block-0 D45 anchor per channel — the
                    // cheapest legal frame (an elected second anchor
                    // demoted to D45 still overflowed a 1360-bit 5.1
                    // 3-block frame: r457 fuzz finding).
                    for p in plan.iter_mut() {
                        *p = [0u8; BLOCKS_PER_FRAME];
                        p[0] = 3; // D45
                    }
                    // The elected LFE cadence shrinks to its single
                    // block-0 anchor too. With a coupling channel the
                    // frame-wide cadence also times the carrier's
                    // exponent refresh and stays as it is.
                    if !fixed_plan && !cpl_on {
                        exp_strategies = [1, 0, 0, 0, 0, 0];
                    }
                }
                plan
            };
            // Apply grpsize quantisation for D25/D45 anchor blocks.
            for ch in 0..nfchans {
                let ch_end = end_mant_ch[ch];
                for blk in 0..nblks {
                    let strat = plan[ch][blk];
                    if strat >= 2 {
                        let grpsize = if strat == 2 { 2 } else { 4 };
                        quantise_exponents_to_grpsize(&mut exps[ch][blk][..ch_end], grpsize);
                    }
                }
                // REUSE blocks get the most-recent anchor's exponents.
                let mut last = 0usize;
                for blk in 0..nblks {
                    if plan[ch][blk] != 0 {
                        last = blk;
                    } else {
                        let src: [u8; N_COEFFS] = exps[ch][last];
                        exps[ch][blk][..ch_end].copy_from_slice(&src[..ch_end]);
                    }
                }
            }
            plan
        };
        let exp_strategies = exp_strategies;
        // §3.4.2: LFE AHT eligibility needs nlferegs == 1 — a single
        // D15 anchor at block 0.
        let lfe_exp_strategies: [u8; BLOCKS_PER_FRAME] = if self.aht {
            [1, 0, 0, 0, 0, 0]
        } else {
            exp_strategies
        };
        if sub.lfeon {
            // AHT LFE: one exponent set per frame from the per-bin
            // 6-block maximum (mirrors the fbw AHT handling above).
            if self.aht {
                for k in 0..LFE_END_MANT {
                    let mut mx = 0.0f32;
                    for blk in 0..nblks {
                        mx = mx.max(coeffs[nfchans][blk][k].abs());
                    }
                    exps[lfe_idx_in_exps][0][k] = extract_exponent(mx);
                }
            }
            if !self.aht {
                bound_shared_exponents(
                    &mut exps[lfe_idx_in_exps],
                    &lfe_exp_strategies[..nblks],
                    0,
                    LFE_END_MANT,
                );
            }
            // LFE strategy: D15 on anchor blocks, REUSE elsewhere. The
            // 1-bit lfeexpstr field only supports D15 or REUSE (§5.4.3.23
            // / §E.1.2.3).
            for blk in 0..nblks {
                if lfe_exp_strategies[blk] == 1 {
                    preprocess_d15(&mut exps[lfe_idx_in_exps][blk][..LFE_END_MANT]);
                }
            }
            let mut last = 0usize;
            for blk in 0..nblks {
                if lfe_exp_strategies[blk] == 1 {
                    last = blk;
                } else {
                    let src: [u8; N_COEFFS] = exps[lfe_idx_in_exps][last];
                    exps[lfe_idx_in_exps][blk][..LFE_END_MANT]
                        .copy_from_slice(&src[..LFE_END_MANT]);
                }
            }
        }

        // -------- Enhanced coupling: carrier + coordinates (§E.3.5.5) ------
        //
        // Built before the SNR tuner so the carrier's exponents ride the
        // shared coupling-channel accounting. The coordinate measurement
        // (pass 2) analyses the decoder-faithful carrier — including the
        // zero next-block spectrum at the frame edge and the previous
        // frame's carried last block — so every analysis imperfection
        // folds into the transmitted amplitudes/angles.
        let ecpl_carrier_plan: Option<EcplCarrierPlan> = ecpl_geom
            .as_ref()
            .map(|g| build_ecpl_carrier(g, &coeffs, nfchans));
        // The coupling channel's MDCT per block — the enhanced-coupling
        // carrier or the standard-coupling channel average.
        let cpl_carrier: Option<&[[f32; N_COEFFS]]> = match (&ecpl_carrier_plan, &stdcpl_plan) {
            (Some(p), _) => Some(&p.carrier),
            (None, Some(p)) => Some(&p.carrier),
            (None, None) => None,
        };
        if let (Some((start_bin, end_bin)), Some(carrier)) = (cpl_region, cpl_carrier) {
            // Carrier exponents into the cpl pseudo-channel slot: D15 on
            // the anchor blocks (0 and 3, matching `exp_strategies` and
            // the audfrm `cplexpstr[blk]` codes), REUSE elsewhere —
            // mirroring the fbw handling so the decoder's exponent state
            // matches the encoder's `exps[]` bin-for-bin.
            let cpl_idx = nfchans;
            for blk in 0..nblks {
                for k in start_bin..end_bin {
                    exps[cpl_idx][blk][k] = extract_exponent(carrier[blk][k]);
                }
            }
            bound_shared_exponents(
                &mut exps[cpl_idx],
                &exp_strategies[..nblks],
                start_bin,
                end_bin,
            );
            for blk in 0..nblks {
                if exp_strategies[blk] == 1 {
                    preprocess_d15(&mut exps[cpl_idx][blk][start_bin..end_bin]);
                }
            }
            let mut last = 0usize;
            for blk in 0..nblks {
                if exp_strategies[blk] == 1 {
                    last = blk;
                } else {
                    let src: [u8; N_COEFFS] = exps[cpl_idx][last];
                    exps[cpl_idx][blk][start_bin..end_bin]
                        .copy_from_slice(&src[start_bin..end_bin]);
                }
            }
        }

        // -------- SPX coordinate planning (§E.2.3.3.9-13 / §E.3.6.3) --------
        //
        // Coordinates are refreshed twice per frame — block 0 (where
        // `spxcoe` is implicit for a channel's first SPX block,
        // §E.2.3.3.9) and block 3, matching the exponent anchor
        // pattern; blocks 1/2/4/5 emit `spxcoe = 0` and reuse. Each
        // refresh's coordinate set energy-matches its whole 3-block
        // span (§3.6.4.3), computed from the encoder's own unquantised
        // MDCT spectra via the shared decoder translation plan.
        const SPX_REFRESH_BLOCKS: [usize; 2] = [0, 3];
        const SPX_SPAN: usize = 3;
        // Per channel, per refresh span: (mstrspxco, per-band (exp, mant)).
        type SpxCoordSet = (u8, [(u8, u8); 18]);
        let spx_coords: Option<Vec<[SpxCoordSet; 2]>> = spx_geom.as_ref().map(|g| {
            (0..nfchans)
                .map(|ch| {
                    SPX_REFRESH_BLOCKS.map(|start| {
                        let span: Vec<&[f32; N_COEFFS]> =
                            (start..start + SPX_SPAN).map(|b| &coeffs[ch][b]).collect();
                        let targets = band_coord_targets_span_atten(
                            &span,
                            g,
                            self.spx.as_ref().and_then(|p| p.atten_code),
                        );
                        let mstr = choose_mstrspxco(&targets[..g.nbnds]);
                        let mut codes = [(0u8, 0u8); 18];
                        for bnd in 0..g.nbnds {
                            codes[bnd] = quantise_coord(targets[bnd], mstr);
                        }
                        (mstr, codes)
                    })
                })
                .collect()
        });

        // -------- AHT-domain coefficients (§3.4.5, forward direction) ----
        //
        // Per bin, the 6 per-block mantissas (coeff · 2^exp, using the
        // frame's single exponent set) map through the forward DCT-II
        // into the AHT coefficients X(k, j) that the VQ/GAQ quantiser
        // stack codes once per frame.
        let aht_x: Option<Vec<Vec<[f32; 6]>>> = if self.aht {
            Some(
                (0..nfchans)
                    .map(|ch| {
                        (0..ch_end_mant)
                            .map(|bin| {
                                let e = exps[ch][0][bin] as i32;
                                let scale = 2f32.powi(e);
                                let mut c = [0.0f32; 6];
                                for (blk, cm) in c.iter_mut().enumerate() {
                                    *cm = coeffs[ch][blk][bin] * scale;
                                }
                                dct_ii_6(&c)
                            })
                            .collect()
                    })
                    .collect(),
            )
        } else {
            None
        };
        let aht_lfe_x: Option<Vec<[f32; 6]>> = if self.aht && sub.lfeon {
            Some(
                (0..LFE_END_MANT)
                    .map(|bin| {
                        let e = exps[lfe_idx_in_exps][0][bin] as i32;
                        let scale = 2f32.powi(e);
                        let mut c = [0.0f32; 6];
                        for (blk, cm) in c.iter_mut().enumerate() {
                            *cm = coeffs[nfchans][blk][bin] * scale;
                        }
                        dct_ii_6(&c)
                    })
                    .collect(),
            )
        } else {
            None
        };

        // -------- Bit allocation --------
        let ba = BitAllocParams {
            sdcycod: 2,
            fdcycod: 1,
            sgaincod: 1,
            dbpbcod: 2,
            floorcod: 4,
            csnroffst: 15,
            fsnroffst: 0,
            fsnroffst_ch: [0u8; crate::audblk::MAX_FBW],
            cplfsnroffst: 0,
            lfefsnroffst: 0,
            fgaincod: 4,
            cplfgaincod: 4,
            lfefgaincod: 4,
        };
        // Coupling plan: the enhanced-coupling carrier region for the
        // shared tuner / bap machinery, or the no-coupling default.
        let cpl = coupling_plan_for(ecpl_geom.as_ref(), stdcpl_geom.as_ref(), nfchans);
        let dba_plan = if std::env::var("EAC3_DISABLE_DBA").is_ok() {
            crate::encoder::DbaPlan::default()
        } else {
            build_dba_plan(
                &exps,
                nfchans,
                end_mant_ch.iter().copied().min().unwrap_or(ch_end_mant),
                &cpl,
            )
        };
        // Optional side-info: drop the dba segment lists when the fixed
        // syntax plus dba would leave the mantissas no room (see the
        // AC-3 encoder's note — the cadence guard probes without dba).
        let dba_plan = {
            let probe = overhead_bits_for_ends(
                &exp_strategies[..nblks],
                Some(&chexpstr_plan),
                ch_end_mant,
                Some(&end_mant_ch),
                nfchans,
                &cpl,
                &dba_plan,
                sub.acmod,
                sub.lfeon,
            ) + 64
                + tool_reserve_bits;
            if probe >= (sub.frame_bytes * 8) as u32 {
                crate::encoder::DbaPlan::default()
            } else {
                dba_plan
            }
        };
        let tuner_frame_bytes = sub
            .frame_bytes
            .saturating_sub(tool_reserve_bits.div_ceil(8) as usize);
        // Pass chexpstr_plan so the overhead calculator accounts for
        // D25/D45 exponent savings when sizing the mantissa budget.
        let tuned_ba = if let Some(x) = &aht_x {
            tune_snroffst_aht(
                &ba,
                &exps,
                x,
                aht_lfe_x.as_deref(),
                ch_end_mant,
                nfchans,
                self.fscod,
                tuner_frame_bytes,
                &lfe_exp_strategies,
                &chexpstr_plan,
                &dba_plan,
                sub.acmod,
                sub.lfeon,
            )
        } else {
            tune_snroffst_with_plan_ends(
                &ba,
                &exps,
                ch_end_mant,
                Some(&end_mant_ch),
                nfchans,
                self.fscod,
                tuner_frame_bytes,
                &exp_strategies[..nblks],
                Some(&chexpstr_plan),
                &cpl,
                &dba_plan,
                sub.acmod,
                sub.lfeon,
            )
        };
        self.cadence_csnr[sub_idx.min(1)] = tuned_ba.csnroffst;
        let mut baps: Vec<Vec<[u8; N_COEFFS]>> = vec![vec![[0u8; N_COEFFS]; nblks]; nfchans + 2];
        let frame_ba = tuned_ba;
        if !self.aht {
            // AHT channels use hebap[] (computed at emission) instead
            // of bap[]; only non-AHT frames need the fbw bap arrays.
            for ch in 0..nfchans {
                for blk in 0..nblks {
                    compute_bap(
                        &exps[ch][blk],
                        end_mant_ch[ch],
                        self.fscod,
                        &frame_ba,
                        &mut baps[ch][blk],
                        Some((&dba_plan, ch)),
                    );
                }
            }
        }
        if sub.lfeon && !self.aht {
            // LFE bit allocation. The shared `compute_bap` is generic
            // — pass the LFE exponents and the LFE-specific budget
            // through a dedicated `BitAllocParams` snapshot whose
            // fsnroffst_ch is irrelevant (LFE uses the lfefsnroffst).
            // AHT frames use hebap[] at emission instead.
            let lfe_ba = BitAllocParams {
                fsnroffst: tuned_ba.lfefsnroffst,
                fgaincod: tuned_ba.lfefgaincod,
                ..frame_ba
            };
            for blk in 0..nblks {
                compute_bap(
                    &exps[lfe_idx_in_exps][blk],
                    LFE_END_MANT,
                    self.fscod,
                    &lfe_ba,
                    &mut baps[lfe_idx_in_exps][blk],
                    None,
                );
            }
        }
        if let Some((start_bin, end_bin)) = cpl_region {
            // Coupling-channel (enhanced carrier / standard average) bap — the §7.2.2.4 coupling
            // excitation path with the tuner's coupling fine offset
            // (tied to the fbw fine offset) and the default fast gain,
            // exactly what the decoder derives from `frmfsnroffst` +
            // `fgaincode == 0` + `cplfleak = cplsleak = 0`.
            let cpl_ba = BitAllocParams {
                fsnroffst: frame_ba.cplfsnroffst,
                fgaincod: frame_ba.cplfgaincod,
                ..frame_ba
            };
            let cpl_idx = nfchans;
            for blk in 0..nblks {
                compute_bap_cpl(
                    &exps[cpl_idx][blk],
                    start_bin,
                    end_bin,
                    self.fscod,
                    &cpl_ba,
                    &mut baps[cpl_idx][blk],
                    Some((&dba_plan, crate::audblk::MAX_FBW)),
                );
            }
        }
        // Enhanced-coupling coordinates (pass 2) — measured against the
        // carrier the decoder will actually reconstruct: this frame's
        // carrier run through the exponent + bap + mantissa quantiser
        // (bap = 0 bins are true zeros — the coupling channel is never
        // dithered), with the previous frame's carried QUANTISED last
        // block at the frame head. Band-level carrier coding loss then
        // folds into the transmitted amplitudes.
        let ecpl_plan: Option<EcplCoordPlan> = match (&ecpl_geom, &ecpl_carrier_plan) {
            (Some(g), Some(cp)) => Some(plan_ecpl_coords(
                g,
                cp,
                &exps[nfchans],
                &baps[nfchans],
                nfchans,
                self.ecpl.as_ref().is_some_and(|p| p.chaos),
                self.ecpl_carry.get(sub_idx).and_then(|c| c.as_ref()),
            )),
            _ => None,
        };

        // -------- Pack syncframe --------
        let mut bw = BitWriter::with_capacity(sub.frame_bytes);

        // syncinfo (§E.2.2.1) — just the 16-bit syncword.
        bw.write_u32(0x0B77, 16);

        // bsi (§E.2.2.2)
        bw.write_u32(sub.strmtyp as u32, 2);
        bw.write_u32(sub.substreamid as u32, 3);
        let frmsiz = (sub.frame_bytes / 2 - 1) as u32;
        bw.write_u32(frmsiz, 11);
        bw.write_u32(self.fscod as u32, 2);
        // fscod != 0x3, so emit numblkscod (2 bits; Table E2.4 —
        // 0/1/2 = 1/2/3 blocks per syncframe, 0x3 = 6 blocks).
        let numblkscod: u32 = if nblks == BLOCKS_PER_FRAME {
            0x3
        } else {
            nblks as u32 - 1
        };
        bw.write_u32(numblkscod, 2);
        bw.write_u32(sub.acmod as u32, 3);
        bw.write_u32(u32::from(sub.lfeon), 1);
        bw.write_u32(EAC3_BSID as u32, 5);
        bw.write_u32(self.meta.dialnorm as u32, 5);
        match self.meta.compr {
            // §5.4.2.9-10 semantics — Table 7.30 heavy compression.
            Some(c) => {
                bw.write_u32(1, 1); // compre = 1
                bw.write_u32(c as u32, 8);
            }
            None => bw.write_u32(0, 1), // compre = 0
        }
        // acmod != 0 → no dialnorm2/compr2e (we never produce 1+1).
        // §E.2.2.2 / E.2.3.1.7-8: dependent substreams emit chanmape
        // (and chanmap when chanmape=1) immediately after compre.
        if sub.strmtyp == 1 {
            if let Some(map) = sub.chanmap {
                bw.write_u32(1, 1); // chanmape = 1
                bw.write_u32(map as u32, 16); // chanmap
            } else {
                bw.write_u32(0, 1); // chanmape = 0
            }
        }
        // §E.2.3.1.9-61 mixing metadata (independent substream only;
        // a paired dependent substream keeps mixmdate = 0).
        match (&self.meta.mixmd, sub.strmtyp) {
            (Some(m), 0) => {
                bw.write_u32(1, 1); // mixmdate = 1
                if sub.acmod > 0x2 {
                    bw.write_u32(m.dmixmod as u32, 2);
                }
                if (sub.acmod & 0x1) != 0 && sub.acmod > 0x2 {
                    bw.write_u32(m.ltrtcmixlev as u32, 3);
                    bw.write_u32(m.lorocmixlev as u32, 3);
                }
                if (sub.acmod & 0x4) != 0 {
                    bw.write_u32(m.ltrtsurmixlev as u32, 3);
                    bw.write_u32(m.lorosurmixlev as u32, 3);
                }
                if sub.lfeon {
                    match m.lfemixlevcod {
                        Some(l) => {
                            bw.write_u32(1, 1); // lfemixlevcode = 1
                            bw.write_u32(l as u32, 5);
                        }
                        None => bw.write_u32(0, 1),
                    }
                }
                // strmtyp == 0 arm: pgmscle / extpgmscle / mixdef /
                // (acmod < 2: paninfoe) / frmmixcfginfoe.
                match m.pgmscl {
                    Some(p) => {
                        bw.write_u32(1, 1);
                        bw.write_u32(p as u32, 6);
                    }
                    None => bw.write_u32(0, 1),
                }
                match m.extpgmscl {
                    Some(p) => {
                        bw.write_u32(1, 1);
                        bw.write_u32(p as u32, 6);
                    }
                    None => bw.write_u32(0, 1),
                }
                bw.write_u32(0, 2); // mixdef = 0 (no further mixdata)
                if sub.acmod < 0x2 {
                    bw.write_u32(0, 1); // paninfoe = 0
                }
                bw.write_u32(0, 1); // frmmixcfginfoe = 0
            }
            _ => bw.write_u32(0, 1), // mixmdate = 0
        }
        // §E.2.3.1.62+ informational metadata (independent substream
        // only).
        match (&self.meta.infomd, sub.strmtyp) {
            (Some(i), 0) => {
                bw.write_u32(1, 1); // infomdate = 1
                bw.write_u32(i.bsmod as u32, 3);
                bw.write_u32(u32::from(i.copyrightb), 1);
                bw.write_u32(u32::from(i.origbs), 1);
                if sub.acmod == 0x2 {
                    bw.write_u32(i.dsurmod as u32, 2);
                    bw.write_u32(i.dheadphonmod as u32, 2);
                }
                if sub.acmod >= 0x6 {
                    bw.write_u32(i.dsurexmod as u32, 2);
                }
                match i.audprod {
                    Some(a) => {
                        bw.write_u32(1, 1); // audprodie = 1
                        bw.write_u32(a.mixlevel as u32, 5);
                        bw.write_u32(a.roomtyp as u32, 2);
                        bw.write_u32(u32::from(a.adconvtyp), 1);
                    }
                    None => bw.write_u32(0, 1),
                }
                // fscod < 0x3 always holds here (48/44.1/32 kHz).
                bw.write_u32(u32::from(i.sourcefscod), 1);
            }
            _ => bw.write_u32(0, 1), // infomdate = 0
        }
        // §E.2.3.1.64 convsync — present when strmtyp == 0 and
        // numblkscod != 0x3: set on the syncframe that starts a
        // 6-block AC-3 conversion group (every (6/nblks)-th frame).
        // With numblkscod == 0x3 the field is absent; strmtyp == 2
        // (blkid / frmsizecod) is never emitted by this encoder.
        if sub.strmtyp == 0 && nblks != BLOCKS_PER_FRAME {
            let group = (BLOCKS_PER_FRAME / nblks) as u64;
            let convsync = u32::from(self.frames_emitted.is_multiple_of(group));
            bw.write_u32(convsync, 1);
        }
        bw.write_u32(0, 1); // addbsie = 0

        // audfrm (§E.2.2.3 / §E.2.3.2). Table E1.3: expstre and ahte
        // are transmitted only on 6-block frames; fractional frames
        // have expstre = 1 and ahte = 0 implicit.
        if nblks == BLOCKS_PER_FRAME {
            bw.write_u32(1, 1); // expstre = 1
            bw.write_u32(u32::from(self.aht), 1); // ahte (§2.3.2.2)
        }
        bw.write_u32(self.snroffststr as u32, 2); // snroffststr
        bw.write_u32(u32::from(transproce), 1); // transproce (§2.3.2.4)
        bw.write_u32(1, 1); // blkswe = 1
        bw.write_u32(1, 1); // dithflage = 1
        bw.write_u32(1, 1); // bamode = 1
        bw.write_u32(1, 1); // frmfgaincode = 1
        bw.write_u32(1, 1); // dbaflde = 1
        bw.write_u32(1, 1); // skipflde = 1
        let spx_atten: Option<u8> = self.spx.as_ref().and_then(|p| p.atten_code);
        bw.write_u32(u32::from(spx_atten.is_some()), 1); // spxattene (§2.3.2.23)
        if sub.acmod > 1 {
            // cplinu[0] — 1 when enhanced coupling is on (the coupling
            // strategy then rides block 0's implicit cplstre = 1 and is
            // reused by every later block via cplstre[blk] = 0).
            bw.write_u32(u32::from(cpl_on), 1);
            for _blk in 1..nblks {
                bw.write_u32(0, 1); // cplstre[blk] = 0 (strategy reuse)
            }
        }
        // §E.1.2.3 / Table E1.3 — exponent strategy data.
        //
        // When `expstre == 1`, the per-block per-channel `chexpstr`
        // codes (2 bits each) AND any per-block `cplexpstr` (when
        // coupling is in use) live HERE in audfrm — NOT in audblk.
        // The previous "round 2 fix" inverted this: it moved the bits
        // into audblk because the spec text for §E.2.3.2.1 (`expstre`)
        // says "the fields for the full exponent strategy shall be
        // present in each audio block" — but Table E1.3 makes clear
        // those fields are still emitted in audfrm, just indexed by
        // block. The validator binary's parser consumes them in audfrm
        // and rejects any frame that doesn't supply them, surfacing
        // as the "new bit allocation info must be present in block 0"
        // / "delta bit allocation strategy reserved" / "error in bit
        // allocation" cascade once the parser misaligns.
        //
        // Coupling: cplinu==0 for every block of every substream we
        // emit, so the `if (cplinu[blk] == 1) {cplexpstr[blk]}` branch
        // never fires. We emit per-channel chexpstr from the adaptive
        // D15/D25/D45 plan selected above (chexpstr_plan[ch][blk]).
        for blk in 0..nblks {
            // §E.1.2.3: `cplexpstr[blk]` (2 bits) precedes the
            // per-channel codes on every block where cplinu[blk] == 1.
            // The carrier follows the frame-wide anchor cadence
            // (D15 on blocks 0/3, REUSE elsewhere).
            if cpl_on {
                bw.write_u32(exp_strategies[blk] as u32, 2);
            }
            for ch in 0..nfchans {
                bw.write_u32(chexpstr_plan[ch][blk] as u32, 2);
            }
        }
        // §E.1.2.3 — `lfeexpstr[blk]` (1 bit) per block when lfeon,
        // OUTSIDE the `if (expstre)` gate (always present when LFE is
        // on, regardless of expstre). LFE always D15 or REUSE.
        if sub.lfeon {
            for blk in 0..nblks {
                bw.write_u32(lfe_exp_strategies[blk] as u32, 1);
            }
        }
        // §E.1.2.3 — converter exponent strategy data. strmtyp == 0x0
        // (independent substream) and numblkscod == 0x3 ⇒ convexpstre
        // implicit = 1, followed by per-channel convexpstr (5 bits each).
        if sub.strmtyp == 0 {
            if nblks == BLOCKS_PER_FRAME {
                for _ in 0..nfchans {
                    bw.write_u32(0, 5); // convexpstr = 0 (REUSE codeword)
                }
            } else {
                // numblkscod != 0x3 → explicit convexpstre flag; 0
                // skips the per-channel convexpstr codes.
                bw.write_u32(0, 1); // convexpstre = 0
            }
        }
        // §3.4.2 / Table E1.3 AHT in-use flags. Presence is gated by
        // the decoder-derived regs counts: no cplahtinu (coupling is
        // never in use, so ncplblks == 0), one chahtinu[ch] bit per
        // fbw channel (every channel's plan is a single block-0
        // anchor → nchregs[ch] == 1), and one lfeahtinu bit (the AHT
        // LFE plan is a single D15 anchor → nlferegs == 1).
        if self.aht {
            for _ in 0..nfchans {
                bw.write_u32(1, 1); // chahtinu[ch] = 1
            }
            if sub.lfeon {
                bw.write_u32(1, 1); // lfeahtinu = 1 (nlferegs == 1)
            }
        }
        // snroffststr == 0 ⇒ frame-level (frmcsnroffst, frmfsnroffst) in
        // audfrm; snroffststr ∈ {1, 2} ⇒ no frame-level fields here (the
        // offsets are carried per-block in each audblk instead).
        if self.snroffststr == 0 {
            bw.write_u32(tuned_ba.csnroffst as u32, 6); // frmcsnroffst
            bw.write_u32(tuned_ba.fsnroffst as u32, 4); // frmfsnroffst
        }
        // §2.3.2.21-23 — transient pre-noise processing data, between
        // the SNR-offset tail and the SPX-attenuation data per Table
        // E1.3: one chintransproc[ch] bit per fbw channel, plus
        // transprocloc[ch] (10 bits) + transproclen[ch] (8 bits) for
        // each channel carrying a transient this frame.
        if transproce {
            let plan = tpnp_plan.as_ref().expect("tpnp plan when transproce");
            for entry in plan.iter().take(nfchans) {
                match entry {
                    Some((loc, len)) => {
                        bw.write_u32(1, 1); // chintransproc[ch] = 1
                        bw.write_u32(*loc as u32, 10); // transprocloc[ch]
                        bw.write_u32(*len as u32, 8); // transproclen[ch]
                    }
                    None => bw.write_u32(0, 1), // chintransproc[ch] = 0
                }
            }
        }
        // §2.3.2.24-25 — spectral extension attenuation data: one
        // chinspxatten[ch] bit per fbw channel, + spxattencod[ch]
        // (5 bits) when set. Emitted between the SNR-offset tail and
        // blkstrtinfoe per Table E1.3. Channels in SPX carry the
        // (shared) code; non-SPX channels signal chinspxatten = 0.
        if let Some(code) = spx_atten {
            for ch in 0..nfchans {
                if in_spx[ch] {
                    bw.write_u32(1, 1); // chinspxatten[ch] = 1
                    bw.write_u32(code as u32, 5); // spxattencod[ch]
                } else {
                    bw.write_u32(0, 1); // chinspxatten[ch] = 0
                }
            }
        }
        if nblks != 1 {
            bw.write_u32(0, 1); // blkstrtinfoe = 0 (absent on 1-block frames)
        }

        // -------- audio blocks --------
        for blk in 0..nblks {
            for ch in 0..nfchans {
                bw.write_u32(blksw[ch][blk] as u32, 1);
            }
            for ch in 0..nfchans {
                // AHT channels reconstruct every bin from the front-
                // loaded coefficient cache — hebap==0 bins are true
                // zeros, so signal dithflag=0 to keep spec-strict
                // decoders from substituting dither there. Otherwise
                // dither is on except around block switches (§8.2.9).
                bw.write_u32(u32::from(self.dither && !self.aht && dithflag[ch][blk]), 1); // dithflag
            }
            // §5.4.3.3-4 dynrnge + dynrng: when metadata configures a
            // dynamic-range word it is transmitted in EVERY block.
            match self.meta.dynrng {
                Some(w) => {
                    bw.write_u32(1, 1);
                    bw.write_u32(w as u32, 8);
                }
                None => bw.write_u32(0, 1), // dynrnge = 0
            }

            // SPX strategy (§E.2.3.3.1-8). Block 0 has implicit
            // `spxstre = 1` and emits `spxinu` directly; later blocks
            // emit `spxstre = 0` (strategy reuse) — the geometry is
            // frame-constant.
            match (&spx_geom, blk) {
                (Some(g), 0) => {
                    let p = self.spx.as_ref().expect("spx params when geom is set");
                    bw.write_u32(1, 1); // spxinu = 1
                    // §E.2.3.3.3 chinspx[ch] — implicit for mono
                    // (acmod == 0x1); explicit per fbw channel
                    // otherwise (mixed membership allowed).
                    if sub.acmod != 0x1 {
                        for &member in in_spx.iter().take(nfchans) {
                            bw.write_u32(u32::from(member), 1);
                        }
                    }
                    bw.write_u32(spx_strtf_emit as u32, 2); // §E.2.3.3.4
                    bw.write_u32(p.spxbegf as u32, 3); // §E.2.3.3.5
                    bw.write_u32(p.spxendf as u32, 3); // §E.2.3.3.6
                    if p.explicit_band_structure {
                        // §E.2.3.3.7-8 — explicit structure carrying the
                        // same content as the Table E2.11 default.
                        bw.write_u32(1, 1);
                        for bnd in (g.begin_subbnd + 1)..g.end_subbnd {
                            bw.write_u32(u32::from(g.bndstrc[bnd]), 1);
                        }
                    } else {
                        bw.write_u32(0, 1); // spxbndstrce = 0 → default banding
                    }
                }
                (Some(_), _) => bw.write_u32(0, 1), // spxstre = 0 (reuse)
                (None, 0) => bw.write_u32(0, 1),    // spxinu = 0
                (None, _) => bw.write_u32(0, 1),    // spxstre = 0
            }
            // SPX coordinates (§E.2.3.3.9-13). `spxcoe` is implicit-1
            // on a channel's first SPX block (block 0 here); explicit
            // thereafter. Refresh on the anchor blocks, reuse between —
            // and when a channel's block-3 refresh quantises to exactly
            // the block-0 codes (stationary spectrum), thrift the
            // payload with `spxcoe = 0` instead: the decoder's reuse
            // path reconstructs identical coordinates, so the decode is
            // unchanged and the ~7 + 6·nbnds bits return to padding.
            if let (Some(g), Some(coords)) = (&spx_geom, &spx_coords) {
                let refresh = SPX_REFRESH_BLOCKS.contains(&blk);
                let span_idx = usize::from(blk >= SPX_REFRESH_BLOCKS[1]);
                let spxblnd = self.spx.as_ref().expect("spx params").spxblnd;
                for ch in 0..nfchans {
                    if !in_spx[ch] {
                        // §E.2.3.3.9: spxcoe exists only for channels
                        // with chinspx[ch] == 1.
                        continue;
                    }
                    let reuse_prior = !refresh || (span_idx == 1 && coords[ch][1] == coords[ch][0]);
                    if blk != 0 {
                        bw.write_u32(u32::from(!reuse_prior), 1); // spxcoe[ch]
                    }
                    if blk == 0 || !reuse_prior {
                        let (mstr, codes) = &coords[ch][span_idx];
                        bw.write_u32(spxblnd as u32, 5); // §E.2.3.3.10
                        bw.write_u32(*mstr as u32, 2); // §E.2.3.3.11
                        for bnd in 0..g.nbnds {
                            bw.write_u32(codes[bnd].0 as u32, 4); // spxcoexp
                            bw.write_u32(codes[bnd].1 as u32, 2); // spxcomant
                        }
                    }
                }
            }

            // Coupling strategy (§E.2.3.3.14-19). Only with enhanced
            // coupling: block 0 (implicit cplstre = 1, cplinu = 1 from
            // audfrm) carries `ecplinu` + `chincpl[ch]` (implicit for
            // 2/0) + the begin/end frequency codes + `ecplbndstrce = 0`
            // (Table E2.14 default banding). Later blocks have
            // cplstre = 0 in audfrm, so no strategy bits at all.
            // Without coupling: cplinu[0] = 0 in audfrm — no audblk
            // bits either way.
            if let (Some(g), 0) = (&ecpl_geom, blk) {
                bw.write_u32(1, 1); // ecplinu = 1
                if sub.acmod != 0x2 {
                    for _ch in 0..nfchans {
                        bw.write_u32(1, 1); // chincpl[ch] = 1
                    }
                }
                bw.write_u32(g.ecplbegf as u32, 4); // §E.2.3.3.16
                if !g.spx_bounded {
                    // §E.2.3.3.17 — transmitted only when SPX is off;
                    // with SPX co-active the end sub-band is derived
                    // from spxbegf and the field is absent.
                    bw.write_u32(g.ecplendf as u32, 4);
                }
                bw.write_u32(0, 1); // ecplbndstrce = 0 → default banding
            }

            // Standard-coupling strategy (§E.2.3.3.14-15, Table E1.4
            // `ecplinu == 0` arm) on block 0 only — later blocks carry
            // audfrm `cplstre = 0` (reuse). `cplbndstrce = 0` selects the
            // Table E2.12 default banding on this first coupling block.
            if let (Some(g), 0) = (&stdcpl_geom, blk) {
                bw.write_u32(0, 1); // ecplinu = 0
                if sub.acmod != 0x2 {
                    for _ch in 0..nfchans {
                        bw.write_u32(1, 1); // chincpl[ch] = 1
                    }
                } else {
                    bw.write_u32(0, 1); // phsflginu = 0
                }
                bw.write_u32(g.begf as u32, 4); // cplbegf
                if !g.spx_bounded {
                    bw.write_u32(g.endf as u32, 4); // cplendf
                }
                bw.write_u32(0, 1); // cplbndstrce = 0
            }
            // Standard-coupling coordinates (Table E1.4): `cplcoe[ch]`
            // implicit 1 on block 0 (firstcplcos), explicit after —
            // refreshed on block 3 unless its codes equal block 0's.
            if let (Some(g), Some(plan)) = (&stdcpl_geom, &stdcpl_plan) {
                for ch in 0..nfchans {
                    let span = usize::from(blk >= 3);
                    let send = match blk {
                        0 => true,
                        3 => {
                            plan.mstr[ch][1] != plan.mstr[ch][0]
                                || plan.codes[ch][1] != plan.codes[ch][0]
                        }
                        _ => false,
                    };
                    if blk != 0 {
                        bw.write_u32(u32::from(send), 1); // cplcoe[ch]
                    }
                    if send {
                        bw.write_u32(plan.mstr[ch][span] as u32, 2); // mstrcplco
                        for bnd in 0..g.nbnd {
                            let (e, m) = plan.codes[ch][span][bnd];
                            bw.write_u32(e as u32, 4); // cplcoexp
                            bw.write_u32(m as u32, 4); // cplcomant
                        }
                    }
                }
                // phsflginu = 0 → no phsflg.
            }

            // Enhanced-coupling coordinates (§E.2.3.3.20-26) — present
            // on every block where cplinu[blk] == 1. Coordinates
            // refresh on the exponent anchor blocks: block 0 has
            // implicit exist bits (`firstcplcos`), block 3 re-emits
            // them explicitly unless the span-1 codes quantised
            // identically to span 0 (thrift: `ecplparam1e = 0` — the
            // decoder's §2.3.3.21-22 reuse path reconstructs the same
            // coordinates), and blocks 1/2/4/5 always reuse. Chaos is
            // 0 (no random de-correlation) and `ecpltrans` is 0.
            if let Some(plan) = &ecpl_plan {
                bw.write_u32(0, 1); // ecplangleintrp = 0
                for ch in 0..nfchans {
                    let is_first = ch == 0;
                    let span = usize::from(blk >= 3);
                    let (p1, p2) = match blk {
                        0 => (true, !is_first),
                        3 => (!plan.reuse1_amp[ch], !is_first && !plan.reuse1_ang[ch]),
                        _ => (false, false),
                    };
                    if blk != 0 {
                        bw.write_u32(u32::from(p1), 1); // ecplparam1e[ch]
                        if !is_first {
                            bw.write_u32(u32::from(p2), 1); // ecplparam2e[ch]
                        }
                    }
                    if p1 {
                        for &code in &plan.amp[ch][span] {
                            bw.write_u32(code as u32, 5); // ecplamp
                        }
                    }
                    if p2 {
                        for (bnd, &code) in plan.angle[ch][span].iter().enumerate() {
                            bw.write_u32(code as u32, 6); // ecplangle
                            let cha = plan.chaos[ch][span].get(bnd).copied().unwrap_or(0);
                            bw.write_u32(cha as u32, 3); // ecplchaos
                        }
                    }
                    if !is_first {
                        bw.write_u32(0, 1); // ecpltrans[ch] = 0
                    }
                }
            }

            // Rematrixing — only acmod==2. The flag-field size folds in
            // SPX per §E.3.3.2 (spxbegf < 2 → 3 bands, else 4; 4 when
            // SPX is off since coupling is never in use here). Round-1
            // keeps rematrixing disabled (all flags 0).
            if sub.acmod == 2 {
                let nrematbd = remat_band_count_spx(
                    cpl_on,
                    stdcpl_geom.as_ref().map_or(0, |g| g.begf),
                    ecpl_on,
                    ecpl_geom.as_ref().map_or(0, |g| g.ecplbegf),
                    spx_geom.is_some(),
                    spx_geom.as_ref().map_or(0, |g| g.begin_subbnd),
                );
                if blk == 0 {
                    for _bnd in 0..nrematbd {
                        bw.write_u32(0, 1);
                    }
                } else {
                    bw.write_u32(0, 1); // rematstr = 0
                }
            }

            let lfe_exp_strategy = lfe_exp_strategies[blk];
            // §E.1.2.4 / Table E1.4 — `chexpstr[blk][ch]` and
            // `lfeexpstr[blk]` were already emitted in audfrm above.
            // audblk only carries the **bandwidth code + exponent
            // payload** that follows from those strategies, gated by
            // the `chexpstr[blk][ch] != reuse` test.
            //
            // §E.1.2.4 chbwcod[ch] — 6 bits per fbw channel whose
            // strategy this block is non-REUSE AND that channel is
            // neither in coupling nor in spectral extension
            // (`if((!chincpl[ch]) && (!chinspx[ch])) {chbwcod[ch]}`).
            // SPX channels derive their bandwidth from the SPX begin
            // frequency instead (§E.3.3.3); with mixed chinspx the
            // full-bandwidth channels still carry their chbwcod.
            for ch in 0..nfchans {
                if !cpl_on && !in_spx[ch] && chexpstr_plan[ch][blk] != 0 {
                    bw.write_u32(chbwcod as u32, 6);
                }
            }
            // §E.1.3.4.4 coupling-channel exponents — cplabsexp (4 bits)
            // + D15 groups over [ecplstartmant, ecplendmant), emitted on
            // the blocks whose audfrm `cplexpstr[blk]` is non-REUSE
            // (anchors 0 and 3). No gainrng for the coupling channel.
            if let Some((start_bin, end_bin)) = cpl_region
                && exp_strategies[blk] == 1
            {
                write_exponents_cpl(&mut bw, &exps[nfchans][blk], start_bin, end_bin);
            }
            // §E.1.2.4 fbw exponents. After each channel's grouped
            // exponents, the spec emits `gainrng[ch]` (2 bits) — same as
            // base AC-3 §5.4.2.20. Emit exponents using the per-channel
            // strategy (D15/D25/D45 from chexpstr_plan).
            for ch in 0..nfchans {
                let strat = chexpstr_plan[ch][blk];
                if strat != 0 {
                    let grpsize = match strat {
                        1 => 1usize,
                        2 => 2usize,
                        _ => 4usize,
                    };
                    write_exponents_grouped(&mut bw, &exps[ch][blk], end_mant_ch[ch], grpsize);
                    bw.write_u32(0, 2); // gainrng[ch] = 0
                }
            }
            // LFE exponents — D15 only, no chbwcod (LFE has fixed bw),
            // and no gainrng (only fbw channels carry gainrng).
            if sub.lfeon && lfe_exp_strategy == 1 {
                write_exponents_grouped(&mut bw, &exps[lfe_idx_in_exps][blk], LFE_END_MANT, 1);
            }

            let baie = blk == 0;
            bw.write_u32(baie as u32, 1);
            if baie {
                bw.write_u32(tuned_ba.sdcycod as u32, 2);
                bw.write_u32(tuned_ba.fdcycod as u32, 2);
                bw.write_u32(tuned_ba.sgaincod as u32, 2);
                bw.write_u32(tuned_ba.dbpbcod as u32, 2);
                bw.write_u32(tuned_ba.floorcod as u32, 3);
            }

            // §2.3.3.27 — per-block SNR offsets when snroffststr ∈ {1,2}.
            // (The snroffststr == 0 path carries them once in audfrm.)
            // Block 0 emits implicitly (snroffste == 1); later blocks emit
            // an explicit `snroffste = 1` so every block carries the same
            // value (the decode then matches the frame-level reference).
            // cplinu == 0 for every block, so the coupling slot is skipped.
            if self.snroffststr != 0 {
                if blk != 0 {
                    bw.write_u32(1, 1); // snroffste = 1
                }
                bw.write_u32(tuned_ba.csnroffst as u32, 6); // csnroffst
                if self.snroffststr == 1 {
                    bw.write_u32(tuned_ba.fsnroffst as u32, 4); // blkfsnroffst
                } else {
                    // snroffststr == 2 — per-channel fine offsets (no cpl
                    // slot: cplinu == 0). LFE slot when present.
                    for _ in 0..nfchans {
                        bw.write_u32(tuned_ba.fsnroffst as u32, 4); // fsnroffst[ch]
                    }
                    if sub.lfeon {
                        bw.write_u32(tuned_ba.lfefsnroffst as u32, 4); // lfefsnroffst
                    }
                }
            }

            // §E.1.3.5.4 frmfgaincode==1 ⇒ per-block `fgaincode` flag.
            bw.write_u32(0, 1); // fgaincode = 0 (no per-channel override)

            // §E.1.2.4 convsnroffste — UNCONDITIONAL when strmtyp == 0
            // (independent substream). Per Table E1.4 line 7965, this
            // sits between the fgaincode block and the cplleak block,
            // gated only by `if (strmtyp == 0x0)` — NOT inside the
            // `if (snroffste)` branch as the previous comment claimed.
            // We never emit `convsnroffst`, so write the gating bit at
            // 0.
            if sub.strmtyp == 0 {
                bw.write_u32(0, 1); // convsnroffste = 0
            }
            // §E.1.3.5.4 cplleak — only when cplinu[blk]. The first
            // coupling block of the frame has `cplleake = 1` implicit
            // (no gate bit): emit cplfleak = cplsleak = 0 directly,
            // matching the 768 + 0 leak init `compute_bap_cpl` assumes.
            // Later blocks emit an explicit `cplleake = 0` (reuse).
            if cpl_on {
                if blk == 0 {
                    bw.write_u32(0, 3); // cplfleak = 0
                    bw.write_u32(0, 3); // cplsleak = 0
                } else {
                    bw.write_u32(0, 1); // cplleake = 0
                }
            }

            let any_fbw_dba = (0..nfchans).any(|c| dba_plan.nseg[c] > 0);
            if blk == 0 && any_fbw_dba {
                bw.write_u32(1, 1); // deltbaie = 1
                if cpl_on {
                    // cpldeltbae precedes the per-channel codes when
                    // cplinu[blk] == 1; 2 = "perform no delta bit
                    // allocation" for the coupling channel.
                    bw.write_u32(2, 2);
                }
                for ch in 0..nfchans {
                    let code = if dba_plan.nseg[ch] > 0 { 1 } else { 2 };
                    bw.write_u32(code as u32, 2);
                }
                for ch in 0..nfchans {
                    if dba_plan.nseg[ch] > 0 {
                        let nseg = dba_plan.nseg[ch] as u32;
                        bw.write_u32(nseg - 1, 3);
                        for seg in 0..nseg as usize {
                            bw.write_u32(dba_plan.offst[ch][seg] as u32, 5);
                            bw.write_u32(dba_plan.len[ch][seg] as u32, 4);
                            bw.write_u32(dba_plan.ba[ch][seg] as u32, 3);
                        }
                    }
                }
            } else {
                bw.write_u32(0, 1); // deltbaie = 0
            }

            bw.write_u32(0, 1); // skiple = 0

            // -------- Mantissas --------
            // Order per AC-3 §5.4.3.49: fbw channels in ascending index,
            // then (when present) the LFE pseudo-channel.
            //
            // AHT frames front-load every fbw channel's mantissas in
            // block 0 as a self-contained chgaqmod + gain-word +
            // VQ/GAQ codeword stream (§3.4.4); blocks 1..5 carry no
            // fbw mantissa bits at all (the decoder replays its
            // 6-block coefficient cache). The LFE stays on the
            // standard per-block path either way, and its bap-1/2/4
            // grouping is self-contained because AHT channels
            // contribute nothing to the shared grouping buffers.
            let mut codes: Vec<(u8, u32)> = Vec::with_capacity(nfchans * ch_end_mant + 4);
            if let Some(x) = &aht_x {
                if blk == 0 {
                    for ch in 0..nfchans {
                        // §3.4.3.1 hebap[] with the same psd/mask/
                        // snroffset/dba state the decoder derives for
                        // this channel's block-0 bit allocation.
                        let mut hebap = [0u8; N_COEFFS];
                        compute_hebap(
                            &exps[ch][0],
                            ch_end_mant,
                            self.fscod,
                            &tuned_ba,
                            &mut hebap,
                            Some((&dba_plan, ch)),
                        );
                        let plan = plan_aht_channel(&hebap[..ch_end_mant], 0, ch_end_mant, &x[ch]);
                        write_aht_channel(
                            &mut bw,
                            &plan,
                            &hebap[..ch_end_mant],
                            0,
                            ch_end_mant,
                            &x[ch],
                        );
                    }
                    if let Some(lx) = &aht_lfe_x {
                        // LFE-AHT front-loaded block (§3.4.2 lfeahtinu):
                        // hebap masking uses the LFE fine-SNR offset and
                        // fast gain in place of the per-channel values.
                        let lfe_ba = BitAllocParams {
                            fsnroffst: tuned_ba.lfefsnroffst,
                            fgaincod: tuned_ba.lfefgaincod,
                            ..tuned_ba
                        };
                        let mut hebap = [0u8; N_COEFFS];
                        compute_hebap(
                            &exps[lfe_idx_in_exps][0],
                            LFE_END_MANT,
                            self.fscod,
                            &lfe_ba,
                            &mut hebap,
                            None,
                        );
                        let plan = plan_aht_channel(&hebap[..LFE_END_MANT], 0, LFE_END_MANT, lx);
                        write_aht_channel(
                            &mut bw,
                            &plan,
                            &hebap[..LFE_END_MANT],
                            0,
                            LFE_END_MANT,
                            lx,
                        );
                    }
                }
            } else {
                for ch in 0..nfchans {
                    for bin in 0..end_mant_ch[ch] {
                        let bap = baps[ch][blk][bin];
                        if bap == 0 {
                            continue;
                        }
                        let e = exps[ch][blk][bin] as i32;
                        let mant = quantise_mantissa(coeffs[ch][blk][bin], e, bap);
                        codes.push((bap, mant));
                    }
                    // §5.4.3.49 order: the coupling-channel mantissas
                    // follow the FIRST coupled channel's own mantissas
                    // (ch 0 — every fbw channel is coupled here), then
                    // the remaining channels and the LFE. The grouped
                    // bap-1/2/4 buffers are shared across the whole
                    // walk, which the single `codes` stream preserves.
                    if let (Some((start_bin, end_bin)), Some(carrier), 0) =
                        (cpl_region, cpl_carrier, ch)
                    {
                        for bin in start_bin..end_bin {
                            let bap = baps[nfchans][blk][bin];
                            if bap == 0 {
                                continue;
                            }
                            let e = exps[nfchans][blk][bin] as i32;
                            let mant = quantise_mantissa(carrier[blk][bin], e, bap);
                            codes.push((bap, mant));
                        }
                    }
                }
            }
            if sub.lfeon && !self.aht {
                for bin in 0..LFE_END_MANT {
                    let bap = baps[lfe_idx_in_exps][blk][bin];
                    if bap == 0 {
                        continue;
                    }
                    let e = exps[lfe_idx_in_exps][blk][bin] as i32;
                    let mant = quantise_mantissa(coeffs[nfchans][blk][bin], e, bap);
                    codes.push((bap, mant));
                }
            }
            write_mantissa_stream(&mut bw, &codes);
        }

        // Thread the cross-frame enhanced-coupling analysis carry: the
        // next frame's block-0 carrier (and per-channel analysis) uses
        // this frame's last block as its "previous block" (§E.3.5.5.1),
        // exactly as the decoder's `EcplState::prev_frame_last_mant`
        // does on its side.
        if sub_idx < self.ecpl_carry.len() {
            self.ecpl_carry[sub_idx] = match (&ecpl_carrier_plan, &ecpl_plan) {
                (Some(cp), Some(plan)) => Some(EcplCarry {
                    carrier: plan.carrier_hat_last,
                    channels: (0..nfchans)
                        .map(|ch| cp.restricted[ch][nblks - 1])
                        .collect(),
                }),
                _ => None,
            };
        }

        // auxdata + errorcheck.
        let target_bits = (sub.frame_bytes * 8) as u64;
        let used_bits = bw.bit_position();
        let errorcheck_bits = 17u64;
        if used_bits + errorcheck_bits > target_bits {
            return Err(Error::other(format!(
                "eac3 encoder: bit budget overflow ({} bits used, frame {} bits, strmtyp={}, acmod={}, lfeon={})",
                used_bits, target_bits, sub.strmtyp, sub.acmod, sub.lfeon
            )));
        }
        let pad_bits = target_bits - used_bits - errorcheck_bits;
        let mut left = pad_bits;
        while left >= 32 {
            bw.write_u32(0, 32);
            left -= 32;
        }
        if left > 0 {
            bw.write_u32(0, left as u32);
        }
        bw.write_u32(0, 1); // encinfo = 0
        bw.write_u32(0, 16); // crc2 placeholder
        let mut frame = bw.into_bytes();
        debug_assert_eq!(frame.len(), sub.frame_bytes);

        // crc2 in augmented form per §7.10.1 (Annex E §E.1.2 inherits
        // the same residue check). Shift the body + 16 trailing zero
        // bits through the LFSR; the resulting register value goes in
        // the crc2 field so a spec-strict decoder's residue check
        // `ac3_crc_update(0, post_syncword) == 0` succeeds. E-AC-3
        // syncframes have no crc1 (§E.1.2 elides it), so the running
        // CRC starts at byte 2 (post-syncword).
        let body_residue = ac3_crc_update(0, &frame[2..(sub.frame_bytes - 2)]);
        let crc2_val = ac3_crc_update(body_residue, &[0u8, 0u8]);
        let n = sub.frame_bytes;
        frame[n - 2] = (crc2_val >> 8) as u8;
        frame[n - 1] = (crc2_val & 0xFF) as u8;
        debug_assert_eq!(
            ac3_crc_update(0, &frame[2..sub.frame_bytes]),
            0,
            "E-AC-3 crc2 emit produced a non-zero post-syncword residue"
        );
        Ok(frame)
    }
}

/// The coupling-plan view of the frame's coupling region for the
/// shared tuner / bit-accounting machinery.
fn coupling_plan_for(
    ecpl: Option<&EcplGeometry>,
    stdcpl: Option<&StdCplGeometry>,
    nfchans: usize,
) -> CouplingPlan {
    match (ecpl, stdcpl) {
        (Some(g), _) => CouplingPlan::for_ecpl(g.begin_subbnd, g.end_subbnd, nfchans, g.necplbnd),
        (None, Some(g)) => CouplingPlan::for_eac3_standard(g.begf, g.nsubbnd, &g.bndstrc, nfchans),
        (None, None) => CouplingPlan::default(),
    }
}

/// Standard-coupling encode plan: the coupling channel and the
/// quantised per-span coordinates (span 0 = blocks 0..3, span 1 =
/// blocks 3..6).
struct StdCplPlan {
    /// Coupling-channel MDCT per block (§7.4.1 channel average over
    /// the coupled region; zero elsewhere).
    carrier: Vec<[f32; N_COEFFS]>,
    /// `mstrcplco[ch][span]`.
    mstr: Vec<[u8; 2]>,
    /// `(cplcoexp, cplcomant)` per `[ch][span][bnd]`.
    codes: Vec<[[(u8, u8); 18]; 2]>,
}

/// Standard coupling, encode direction (§7.4.1 / §7.4.3): the coupling
/// channel is the average of the coupled channels' coefficients; each
/// channel's per-band coordinate is `sqrt(E_ch / E_cpl) / 8` over the
/// span (the decoder reconstructs `cplmant · cplco · 8`). Every coupled
/// channel's region bins are then replaced by that reconstruction.
fn plan_std_cpl(
    g: &StdCplGeometry,
    coeffs: &mut [Vec<[f32; N_COEFFS]>],
    nfchans: usize,
    nblks: usize,
) -> StdCplPlan {
    let inv_n = 1.0f32 / nfchans as f32;
    let mut carrier = vec![[0.0f32; N_COEFFS]; nblks];
    for (blk, cb) in carrier.iter_mut().enumerate() {
        for bin in g.begin_bin..g.end_bin {
            let sum: f32 = (0..nfchans).map(|ch| coeffs[ch][blk][bin]).sum();
            cb[bin] = sum * inv_n;
        }
    }
    let spans: [(usize, usize); 2] = [(0, 3.min(nblks)), (3.min(nblks), nblks)];
    let mut mstr = vec![[0u8; 2]; nfchans];
    let mut codes = vec![[[(15u8, 0u8); 18]; 2]; nfchans];
    let mut recon = vec![[[0.0f32; 18]; 2]; nfchans];
    for (span, &(b0, b1)) in spans.iter().enumerate() {
        let mut e_cpl = [0.0f64; 18];
        let mut e_ch = vec![[0.0f64; 18]; nfchans];
        for blk in b0..b1 {
            for bin in g.begin_bin..g.end_bin {
                let bnd = g.sbnd_to_bnd[(bin - g.begin_bin) / 12];
                let c = carrier[blk][bin] as f64;
                e_cpl[bnd] += c * c;
                for ch in 0..nfchans {
                    let v = coeffs[ch][blk][bin] as f64;
                    e_ch[ch][bnd] += v * v;
                }
            }
        }
        for ch in 0..nfchans {
            let mut raw = [0.0f32; 18];
            let mut max_co = 0.0f32;
            for bnd in 0..g.nbnd {
                raw[bnd] = if e_cpl[bnd] > 1e-20 {
                    ((e_ch[ch][bnd] / e_cpl[bnd]).sqrt() as f32) / 8.0
                } else {
                    0.0
                };
                max_co = max_co.max(raw[bnd]);
            }
            let m = if span == 1 && b0 == b1 {
                mstr[ch][0]
            } else {
                pick_mstrcplco(max_co)
            };
            mstr[ch][span] = m;
            for bnd in 0..g.nbnd {
                codes[ch][span][bnd] = if span == 1 && b0 == b1 {
                    codes[ch][0][bnd]
                } else {
                    quantise_cplco(raw[bnd], m)
                };
                let (e, mm) = codes[ch][span][bnd];
                recon[ch][span][bnd] = reconstruct_cplco(e, mm, m);
            }
        }
    }
    for (span, &(b0, b1)) in spans.iter().enumerate() {
        for blk in b0..b1 {
            for bin in g.begin_bin..g.end_bin {
                let bnd = g.sbnd_to_bnd[(bin - g.begin_bin) / 12];
                for ch in 0..nfchans {
                    coeffs[ch][blk][bin] = carrier[blk][bin] * recon[ch][span][bnd] * 8.0;
                }
            }
        }
    }
    StdCplPlan {
        carrier,
        mstr,
        codes,
    }
}

/// Phase A of the enhanced-coupling encode (§E.3.5.5, encode
/// direction): the carrier MDCT buffers + the per-channel
/// region-restricted analysis inputs. Built before the SNR tuner so
/// the carrier's exponents ride the shared coupling-channel
/// accounting; the coordinates ([`plan_ecpl_coords`]) wait until the
/// bit allocation is final.
struct EcplCarrierPlan {
    /// Region-restricted carrier MDCT per block (what the coupling
    /// pseudo-channel codes).
    carrier: Vec<[f32; 256]>,
    /// Region-restricted per-channel MDCT buffers
    /// (`restricted[ch][blk]`) — the §E.3.5.5.1 analysis inputs for the
    /// coordinate targets.
    restricted: Vec<Vec<[f32; 256]>>,
}

/// Phase B: the quantised per-span coordinate codes.
struct EcplCoordPlan {
    /// Quantised Table E3.10 amplitude codes: `amp[ch][span][bnd]`
    /// (span 0 = blocks 0..3, span 1 = blocks 3..6).
    amp: Vec<[Vec<u8>; 2]>,
    /// Quantised Table E3.11 angle codes: `angle[ch][span][bnd]`.
    /// Empty for the first coupled channel (spec-fixed to 0, never
    /// transmitted).
    angle: Vec<[Vec<u8>; 2]>,
    /// Table E3.12 chaos codes: `chaos[ch][span][bnd]` (all zero when
    /// the coherence-driven chaos signalling is off). Empty for the
    /// first coupled channel.
    chaos: Vec<[Vec<u8>; 2]>,
    /// Per channel: span 1's amplitude codes quantised identically to
    /// span 0's — block 3 emits `ecplparam1e = 0` (reuse thrift).
    reuse1_amp: Vec<bool>,
    /// Per channel: span 1's angle codes match span 0's — block 3
    /// emits `ecplparam2e = 0`.
    reuse1_ang: Vec<bool>,
    /// Last block's QUANTISED carrier (the decoder-faithful buffer the
    /// next frame's block-0 analysis uses as its "previous block").
    carrier_hat_last: [f32; 256],
}

/// Carrier-headroom margin: the carrier is scaled ~3 dB above the
/// loudest coupled channel per band so the Table E3.10 amplitude
/// ceiling of 1.0 can absorb band-level carrier *coding* loss (bap = 0
/// bins reconstruct as true zeros; the amplitude coordinate re-scales
/// what survives back to the channel's band energy, but only downward
/// from 1.0).
const ECPL_CARRIER_MARGIN: f32 = std::f32::consts::SQRT_2;

/// Build the enhanced-coupling carrier for one frame: the first
/// coupled channel's MDCT restricted to the active region, scaled per
/// band (per coordinate span) so the loudest coupled channel's
/// amplitude coordinate lands ~3 dB under the Table E3.10 ceiling
/// ([`carrier_band_gains`] × [`ECPL_CARRIER_MARGIN`]). Using channel
/// 0's own coefficients keeps the carrier's phase locked to the first
/// coupled channel, whose angle is spec-fixed to 0 and never
/// transmitted.
fn build_ecpl_carrier(
    geom: &EcplGeometry,
    coeffs: &[Vec<[f32; N_COEFFS]>],
    nfchans: usize,
) -> EcplCarrierPlan {
    const SPAN: usize = 3;
    let mut gains: [Vec<f32>; 2] = [Vec::new(), Vec::new()];
    for (span, g) in gains.iter_mut().enumerate() {
        let energies: Vec<Vec<f64>> = (0..nfchans)
            .map(|ch| {
                let blocks: Vec<&[f32; N_COEFFS]> = (span * SPAN..(span + 1) * SPAN)
                    .map(|blk| &coeffs[ch][blk])
                    .collect();
                band_mdct_energies(&blocks, geom)
            })
            .collect();
        *g = carrier_band_gains(&energies, geom.necplbnd);
        for v in g.iter_mut() {
            *v = (*v * ECPL_CARRIER_MARGIN).min(32.0);
        }
    }
    let carrier: Vec<[f32; 256]> = (0..BLOCKS_PER_FRAME)
        .map(|blk| build_carrier_block(&coeffs[0][blk], &gains[blk / SPAN], geom))
        .collect();
    let restricted: Vec<Vec<[f32; 256]>> = (0..nfchans)
        .map(|ch| {
            (0..BLOCKS_PER_FRAME)
                .map(|blk| region_restrict(&coeffs[ch][blk], geom))
                .collect()
        })
        .collect();
    EcplCarrierPlan {
        carrier,
        restricted,
    }
}

/// Measure + quantise the enhanced-coupling coordinates against the
/// decoder-faithful carrier.
///
/// Per block, `Z = reconstruct_carrier(prev, curr, next)` over the
/// QUANTISED carrier (each bin run through the final exponent + bap
/// mantissa quantiser via `quantise_reconstruct`; bap = 0 bins are true
/// zeros — the coupling channel is never dithered), with the previous
/// frame's carried quantised last block at the frame head and a zero
/// spectrum after the frame tail (the decoder's streaming edge). The
/// same analysis applied to each coupled channel's region-restricted
/// (unquantised — they are the *targets*) MDCT gives `X`; band
/// statistics accumulate over each 3-block coordinate span and
/// quantise through [`quantise_amp`] / [`quantise_angle`]. Measuring
/// against the quantised carrier folds band-level carrier coding loss
/// into the transmitted amplitudes.
fn plan_ecpl_coords(
    geom: &EcplGeometry,
    cp: &EcplCarrierPlan,
    cpl_exps: &[[u8; N_COEFFS]],
    cpl_baps: &[[u8; N_COEFFS]],
    nfchans: usize,
    chaos_enabled: bool,
    carry: Option<&EcplCarry>,
) -> EcplCoordPlan {
    const SPAN: usize = 3;
    let zeros = [0.0f32; 256];

    // Decoder-faithful (quantised) carrier per block.
    let carrier_hat: Vec<[f32; 256]> = (0..BLOCKS_PER_FRAME)
        .map(|blk| {
            let mut out = [0.0f32; 256];
            for bin in geom.start_bin..geom.end_bin.min(256) {
                out[bin] = crate::encoder::quantise_reconstruct(
                    cp.carrier[blk][bin],
                    cpl_exps[blk][bin] as i32,
                    cpl_baps[blk][bin],
                );
            }
            out
        })
        .collect();

    let carry_carrier = carry.map_or(&zeros, |c| &c.carrier);
    let z: Vec<_> = (0..BLOCKS_PER_FRAME)
        .map(|blk| {
            let prev = if blk == 0 {
                carry_carrier
            } else {
                &carrier_hat[blk - 1]
            };
            let next = if blk + 1 < BLOCKS_PER_FRAME {
                &carrier_hat[blk + 1]
            } else {
                &zeros
            };
            reconstruct_carrier(prev, &carrier_hat[blk], next)
        })
        .collect();

    let mut amp: Vec<[Vec<u8>; 2]> = Vec::with_capacity(nfchans);
    let mut angle: Vec<[Vec<u8>; 2]> = Vec::with_capacity(nfchans);
    let mut chaos: Vec<[Vec<u8>; 2]> = Vec::with_capacity(nfchans);
    let mut reuse1_amp = vec![false; nfchans];
    let mut reuse1_ang = vec![false; nfchans];
    for ch in 0..nfchans {
        let carry_ch = carry.and_then(|c| c.channels.get(ch)).unwrap_or(&zeros);
        let mut amp_spans: [Vec<u8>; 2] = [Vec::new(), Vec::new()];
        let mut ang_spans: [Vec<u8>; 2] = [Vec::new(), Vec::new()];
        let mut cha_spans: [Vec<u8>; 2] = [Vec::new(), Vec::new()];
        for span in 0..2 {
            let mut stats = vec![BandStats::default(); geom.necplbnd];
            for blk in span * SPAN..(span + 1) * SPAN {
                let prev = if blk == 0 {
                    carry_ch
                } else {
                    &cp.restricted[ch][blk - 1]
                };
                let next = if blk + 1 < BLOCKS_PER_FRAME {
                    &cp.restricted[ch][blk + 1]
                } else {
                    &zeros
                };
                let x = reconstruct_carrier(prev, &cp.restricted[ch][blk], next);
                band_cross_stats(&x, &z[blk], geom, &mut stats);
            }
            // Per band: chaos from the measured coherence (non-first
            // channels only — the first coupled channel's chaos is
            // spec-fixed to 0), then the amplitude pre-divided by the
            // decoder's §E.3.5.5.2 modification factor. If the
            // pre-compensated amplitude would exceed the Table E3.10
            // ceiling of 1.0, back the chaos off until it fits —
            // preserving band ENERGY takes precedence over restoring
            // de-correlation.
            let mut amps = Vec::with_capacity(geom.necplbnd);
            let mut chas = Vec::with_capacity(geom.necplbnd);
            for st in &stats {
                let mut code = if chaos_enabled && ch != 0 {
                    chaos_code_for(st.coherence())
                } else {
                    0
                };
                let measured = st.amp();
                while code > 0 && measured / chaos_amp_factor(code) > 1.0 {
                    code -= 1;
                }
                amps.push(quantise_amp(measured / chaos_amp_factor(code)));
                chas.push(code);
            }
            amp_spans[span] = amps;
            if ch != 0 {
                ang_spans[span] = stats
                    .iter()
                    .map(|st| quantise_angle(st.angle_units()))
                    .collect();
                cha_spans[span] = chas;
            }
        }
        reuse1_amp[ch] = amp_spans[1] == amp_spans[0];
        reuse1_ang[ch] = ch == 0 || (ang_spans[1] == ang_spans[0] && cha_spans[1] == cha_spans[0]);
        amp.push(amp_spans);
        angle.push(ang_spans);
        chaos.push(cha_spans);
    }

    EcplCoordPlan {
        amp,
        angle,
        chaos,
        reuse1_amp,
        reuse1_ang,
        carrier_hat_last: carrier_hat[BLOCKS_PER_FRAME - 1],
    }
}

/// SNR-offset tuner for AHT frames. The fbw mantissa cost is the
/// exact §3.4.4 payload (hebap-driven VQ/GAQ codewords + gain words +
/// chgaqmod, front-loaded once per frame) instead of six per-block bap
/// payloads; the LFE keeps the standard bap cost. `hebap[]` — and
/// therefore the cost — is monotone non-decreasing in the combined
/// SNR offset, so the largest offset that fits the budget is found by
/// binary search over `csnroffst·16 + fsnroffst`.
#[allow(clippy::too_many_arguments)]
fn tune_snroffst_aht(
    ba: &BitAllocParams,
    exps: &[Vec<[u8; N_COEFFS]>],
    aht_x: &[Vec<[f32; 6]>],
    aht_lfe_x: Option<&[[f32; 6]]>,
    end: usize,
    nchan: usize,
    fscod: u8,
    frame_bytes: usize,
    exp_strategies: &[u8; BLOCKS_PER_FRAME],
    chexpstr_plan: &[[u8; BLOCKS_PER_FRAME]],
    dba: &DbaPlan,
    acmod: u8,
    lfeon: bool,
) -> BitAllocParams {
    let cpl = CouplingPlan::default();
    let overhead = overhead_bits_for(
        exp_strategies,
        Some(chexpstr_plan),
        end,
        nchan,
        &cpl,
        dba,
        acmod,
        lfeon,
    ) + 32;
    let total_bits = (frame_bytes * 8) as u32;
    let mut best = *ba;
    best.csnroffst = 0;
    best.fsnroffst = 0;
    best.lfefsnroffst = 0;
    if overhead >= total_bits {
        return best;
    }
    let budget = total_bits - overhead;

    let used_at = |combined: i32| -> u32 {
        let mut cand = *ba;
        cand.csnroffst = (combined / 16) as u8;
        cand.fsnroffst = (combined % 16) as u8;
        cand.lfefsnroffst = cand.fsnroffst;
        let mut used = 0u32;
        for ch in 0..nchan {
            let mut hebap = [0u8; N_COEFFS];
            compute_hebap(&exps[ch][0], end, fscod, &cand, &mut hebap, Some((dba, ch)));
            used += plan_aht_channel(&hebap[..end], 0, end, &aht_x[ch]).total_bits();
        }
        if lfeon {
            let lfe_idx = nchan + 1;
            let mut lfe_ba = cand;
            lfe_ba.fsnroffst = cand.lfefsnroffst;
            lfe_ba.fgaincod = cand.lfefgaincod;
            if let Some(lx) = aht_lfe_x {
                // LFE-AHT: exact front-loaded payload (§3.4.4).
                let mut hebap = [0u8; N_COEFFS];
                compute_hebap(
                    &exps[lfe_idx][0],
                    LFE_END_MANT,
                    fscod,
                    &lfe_ba,
                    &mut hebap,
                    None,
                );
                used += plan_aht_channel(&hebap[..LFE_END_MANT], 0, LFE_END_MANT, lx).total_bits();
            } else {
                // Layout for mantissa_bits_total with nchan == 0: slot 0
                // is the (unused) coupling pseudo-channel, slot 1 the LFE.
                let mut lfe_baps: Vec<Vec<[u8; N_COEFFS]>> =
                    vec![vec![[0u8; N_COEFFS]; BLOCKS_PER_FRAME]; 2];
                for blk in 0..BLOCKS_PER_FRAME {
                    compute_bap(
                        &exps[lfe_idx][blk],
                        LFE_END_MANT,
                        fscod,
                        &lfe_ba,
                        &mut lfe_baps[1][blk],
                        None,
                    );
                }
                used += mantissa_bits_total(&lfe_baps, 0, 0, &cpl, true);
            }
        }
        used
    };

    if used_at(0) > budget {
        // Even the most negative SNR offset overflows — degrade to the
        // floor allocation (near-universal hebap 0) so the packer
        // emits near-silence instead of overflowing with the default
        // offsets.
        return crate::encoder::floor_allocation(&best);
    }
    let (mut lo, mut hi) = (0i32, 63 * 16 + 15);
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if used_at(mid) <= budget {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    best.csnroffst = (lo / 16) as u8;
    best.fsnroffst = (lo % 16) as u8;
    best.lfefsnroffst = best.fsnroffst;
    for ch in 0..crate::audblk::MAX_FBW {
        best.fsnroffst_ch[ch] = best.fsnroffst;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_bytes_lookup_48k() {
        // 192 kbps @ 48 kHz ⇒ 768 bytes (matches AC-3's frmsizecod=20).
        assert_eq!(ac3_frame_bytes_blocks(0, 192, 6), Some(768));
        // 96 kbps @ 48 kHz ⇒ 384 bytes.
        assert_eq!(ac3_frame_bytes_blocks(0, 96, 6), Some(384));
        // 32 kbps lower bound — even, in range.
        assert_eq!(ac3_frame_bytes_blocks(0, 32, 6), Some(128));
        // 640 kbps @ 48 kHz ⇒ 2560 bytes (largest A/52 row).
        assert_eq!(ac3_frame_bytes_blocks(0, 640, 6), Some(2560));
        // Fractional frames scale the byte budget by nblks/6 (even
        // byte counts — frmsiz is in 16-bit words).
        assert_eq!(ac3_frame_bytes_blocks(0, 192, 3), Some(384));
        assert_eq!(ac3_frame_bytes_blocks(0, 192, 2), Some(256));
        assert_eq!(ac3_frame_bytes_blocks(0, 192, 1), Some(128));
        // 1-block frames at very low rates fall below the 32-byte
        // frmsiz floor.
        assert_eq!(ac3_frame_bytes_blocks(0, 32, 1), None);
    }

    // ---- fractional syncframes (§E.2.3.1.5 numblkscod 0/1/2) ----
    //
    // A syncframe may carry 1, 2, or 3 audio blocks instead of 6
    // (Table E2.4); the byte budget scales by nblks/6 so the bit rate
    // is unchanged. The round-trips below gate three invariants:
    // syntax (numblkscod / frame size / §E.2.3.1.64 convsync cadence
    // through the typed BSI parse), decode alignment (PSNR vs the
    // 6-block encode of the same PCM at the same rate — a single-bit
    // audfrm/bsi misalignment collapses this to ~0 dB), and the
    // options-vs-typed construction parity pin.

    fn encode_blocks(
        pcm: &[f32],
        channels: usize,
        bit_rate: u64,
        blocks: usize,
        via_option: bool,
    ) -> Vec<u8> {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(channels as u16);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(bit_rate);
        let mut enc: Box<dyn Encoder> = if via_option {
            params.options = oxideav_core::CodecOptions::new().set("blocks", blocks.to_string());
            make_encoder(&params).expect("options-driven blocks encoder")
        } else {
            make_encoder_with_blocks(&params, blocks).expect("typed blocks encoder")
        };
        let n_samp = pcm.len() / channels;
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n_samp as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut out = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => out.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("blocks encode error: {e:?}"),
            }
        }
        out
    }

    /// Walk a single-substream stream and assert the per-frame BSI
    /// syntax: frame size, numblkscod/num_blocks, and the convsync
    /// cadence (set on every (6/nblks)-th frame, starting at frame 0).
    fn assert_fractional_syntax(stream: &[u8], frame_bytes: usize, blocks: usize) {
        assert!(!stream.is_empty(), "encoder produced no bytes");
        assert_eq!(
            stream.len() % frame_bytes,
            0,
            "stream should be whole {frame_bytes}-byte syncframes"
        );
        let group = BLOCKS_PER_FRAME / blocks;
        for (idx, frame) in stream.chunks_exact(frame_bytes).enumerate() {
            assert_eq!(&frame[..2], &[0x0B, 0x77], "syncword at frame {idx}");
            let bsi = super::super::bsi::parse(&frame[2..]).expect("fractional BSI parse");
            assert_eq!(bsi.num_blocks as usize, blocks, "num_blocks at frame {idx}");
            assert_eq!(
                bsi.numblkscod,
                if blocks == 6 { 3 } else { blocks as u8 - 1 },
                "numblkscod at frame {idx}"
            );
            assert_eq!(
                bsi.frame_bytes as usize, frame_bytes,
                "frmsiz at frame {idx}"
            );
            if blocks == 6 {
                assert_eq!(bsi.convsync, None, "convsync absent on 6-block frames");
            } else {
                assert_eq!(
                    bsi.convsync,
                    Some(idx % group == 0),
                    "convsync cadence at frame {idx} (group = {group})"
                );
            }
        }
    }

    fn assert_fractional_roundtrip(channels: usize, bit_rate: u64, blocks: usize, floor_db: f64) {
        let pcm = build_sine_pcm(channels, 4);
        let frame_bytes =
            ac3_frame_bytes_blocks(0, (bit_rate / 1000) as u32, blocks).unwrap() as usize;
        let stream = encode_blocks(&pcm, channels, bit_rate, blocks, false);
        assert_fractional_syntax(&stream, frame_bytes, blocks);

        // Decode alignment gate: the fractional decode must track the
        // 6-block decode of the same PCM at the same rate. The two are
        // different lossy codings (fractional frames amortise their
        // header overhead over fewer samples and re-anchor exponents
        // every frame), so this is a gross-desync floor, not a
        // bit-exactness claim — a cursor misalignment anywhere in the
        // fractional bsi/audfrm/audblk walk collapses it to ~0 dB.
        let dec = decode_all(&stream, frame_bytes);
        let base_frame_bytes = ac3_frame_bytes_blocks(0, (bit_rate / 1000) as u32, 6).unwrap();
        let base = decode_all(
            &encode_with(&pcm, channels, bit_rate, 0),
            base_frame_bytes as usize,
        );
        assert_eq!(
            dec.len(),
            base.len(),
            "fractional decode must cover the same sample count"
        );
        let psnr = psnr_vs(&dec, &base);
        eprintln!(
            "fractional {blocks}-block {}ch @ {} kbps: PSNR vs 6-block decode = {psnr:.2} dB",
            channels,
            bit_rate / 1000
        );
        assert!(
            psnr >= floor_db,
            "{blocks}-block decode diverged from the 6-block reference: \
             {psnr:.2} dB < {floor_db} dB"
        );
    }

    // Floors are gross-desync guards, not fidelity claims: a cursor
    // misalignment collapses the mutual PSNR to ~0 dB, while the
    // honest efficiency cost of the shape (per-frame BSI/audfrm
    // overhead + a full exponent re-anchor every 256·nblks samples at
    // an unchanged bit rate) measures ~24-28 dB for 3-block, ~24 dB
    // for 2-block, and ~13 dB for the 1-block extreme at these rates.
    #[test]
    fn fractional_frames_roundtrip_stereo_3blocks() {
        assert_fractional_roundtrip(2, 192_000, 3, 22.0);
    }

    #[test]
    fn fractional_frames_roundtrip_stereo_2blocks() {
        assert_fractional_roundtrip(2, 192_000, 2, 18.0);
    }

    #[test]
    fn fractional_frames_roundtrip_mono_1block() {
        assert_fractional_roundtrip(1, 96_000, 1, 10.0);
    }

    #[test]
    fn fractional_frames_roundtrip_51_2blocks() {
        assert_fractional_roundtrip(6, 448_000, 2, 16.0);
    }

    /// r457 fuzz finding (`encode_decode_roundtrip` input `7b 01 4f 00`):
    /// a 7.1 pair at 128 kbps with 3-block syncframes — the smallest
    /// 5.1 independent-substream budget the constructor accepts
    /// (1360 bits) — overflowed the packer on a sawtooth payload once
    /// the exponent cadence was elected per channel. Every
    /// construction-accepted configuration must encode arbitrary PCM.
    #[test]
    fn seven_one_pair_three_block_128k_never_overflows() {
        use crate::eac3::decoder::{Eac3DecoderState, decode_eac3_packet};
        use oxideav_core::{AudioFrame, Error, Frame};
        let channels = 8u16;
        let blocks = 3usize;
        let mut params = CodecParameters::audio(CodecId::new("eac3"));
        params.sample_rate = Some(48_000);
        params.channels = Some(channels);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(128_000);
        params.options = oxideav_core::CodecOptions::new().set("blocks", blocks.to_string());
        let mut enc = make_encoder(&params).expect("contract-valid 7.1 pair config");
        let spf = 256 * blocks;
        let n = spf * 2 * channels as usize;
        let mut s16 = Vec::with_capacity(n * 2);
        for i in 0..n {
            s16.push(0u8);
            s16.push((i & 0x3f) as u8);
        }
        enc.send_frame(&Frame::Audio(AudioFrame {
            samples: (n / channels as usize) as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .expect("send_frame on a contract-valid config");
        enc.flush().expect("flush");
        let mut st = Eac3DecoderState::default();
        let mut samples = 0usize;
        loop {
            match enc.receive_packet() {
                Ok(p) => {
                    let f = decode_eac3_packet(&mut st, &p.data).expect("own decode");
                    samples += f.pcm_s16le.len() / 2 / channels as usize;
                }
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("receive_packet: {e:?}"),
            }
        }
        assert_eq!(samples, 2 * spf);
    }

    #[test]
    fn fractional_frames_options_vs_typed_byte_identical() {
        let pcm = build_sine_pcm(2, 2);
        let typed = encode_blocks(&pcm, 2, 192_000, 3, false);
        let via_option = encode_blocks(&pcm, 2, 192_000, 3, true);
        assert_eq!(
            typed, via_option,
            "registry `blocks` option and make_encoder_with_blocks must build the same encoder"
        );
    }

    #[test]
    fn fractional_frames_71_pair_2blocks() {
        // 7.1 emits an indep(5.1) + dep(Lb/Rb) pair per syncframe;
        // both substreams share the block count. 576 kbps splits
        // 384 + 192, so a 2-block pair is 512 + 256 bytes.
        let pcm = build_sine_pcm(8, 3);
        let stream = encode_blocks(&pcm, 8, 576_000, 2, false);
        let pair_bytes = 512 + 256;
        assert_eq!(stream.len() % pair_bytes, 0, "whole 2-block pairs");
        for (idx, pair) in stream.chunks_exact(pair_bytes).enumerate() {
            let indep = super::super::bsi::parse(&pair[2..]).expect("indep BSI");
            assert_eq!(indep.num_blocks, 2, "indep num_blocks at pair {idx}");
            assert_eq!(indep.convsync, Some(idx % 3 == 0), "indep convsync cadence");
            assert_eq!(&pair[512..514], &[0x0B, 0x77], "dep syncword");
            let dep = super::super::bsi::parse(&pair[514..]).expect("dep BSI");
            assert_eq!(dep.num_blocks, 2, "dep num_blocks at pair {idx}");
            // Dependent substreams never carry convsync (§E.2.3.1.64).
            assert_eq!(dep.convsync, None, "dep convsync absent");
        }
        let dec = decode_all(&stream, pair_bytes);
        assert_eq!(
            dec.len(),
            pcm.len(),
            "7.1 fractional pair decode must cover the full sample count"
        );
    }

    #[test]
    fn fractional_frames_flush_pads_to_frame_multiple() {
        // 300 samples at blocks=1 → two 256-sample frames (the tail
        // frame zero-padded by flush).
        let pcm: Vec<f32> = (0..300)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin())
            .collect();
        let stream = encode_blocks(&pcm, 1, 96_000, 1, false);
        let frame_bytes = ac3_frame_bytes_blocks(0, 96, 1).unwrap() as usize;
        assert_fractional_syntax(&stream, frame_bytes, 1);
        assert_eq!(stream.len() / frame_bytes, 2, "two 1-block frames");
        let dec = decode_all(&stream, frame_bytes);
        assert_eq!(dec.len(), 512, "2 × 256 decoded samples");
    }

    #[test]
    fn fractional_frames_reject_six_block_tools() {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(2);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(192_000);
        // AHT is implicit-off on the wire for numblkscod != 0x3
        // (Table E1.3) — construction must fail, not silently emit.
        params.options = oxideav_core::CodecOptions::new()
            .set("blocks", "3")
            .set("aht", "1");
        assert!(make_encoder(&params).is_err(), "blocks=3 + aht must fail");
        // SPX / ecpl are 6-block scope in this encoder.
        params.options = oxideav_core::CodecOptions::new()
            .set("blocks", "2")
            .set("spx", "1");
        assert!(make_encoder(&params).is_err(), "blocks=2 + spx must fail");
        params.options = oxideav_core::CodecOptions::new()
            .set("blocks", "2")
            .set("ecpl", "1");
        assert!(make_encoder(&params).is_err(), "blocks=2 + ecpl must fail");
        // Out-of-range block counts are rejected in both APIs.
        params.options = oxideav_core::CodecOptions::new().set("blocks", "4");
        assert!(make_encoder(&params).is_err(), "blocks=4 must fail");
        // Minimum-rate floor: a 1-block stereo frame at 96 kbps is 64
        // bytes — the fixed syntax + D45 exponent payload alone
        // exceeds it, so construction must reject rather than
        // overflow on the first frame.
        params.options = oxideav_core::CodecOptions::new().set("blocks", "1");
        params.bit_rate = Some(96_000);
        assert!(
            make_encoder(&params).is_err(),
            "blocks=1 stereo @ 96 kbps must fail the minimum-rate floor"
        );
        params.bit_rate = Some(192_000);
        params.options = oxideav_core::CodecOptions::new();
        assert!(
            make_encoder_with_blocks(&params, 5).is_err(),
            "typed blocks=5 must fail"
        );
        // Metadata co-exists with fractional frames (dynrng is
        // per-block regardless of the block count).
        params.options = oxideav_core::CodecOptions::new()
            .set("blocks", "3")
            .set("dynrng", "0xC0")
            .set("dialnorm", "24");
        let pcm = build_sine_pcm(2, 1);
        let stream = {
            let mut enc = make_encoder(&params).expect("blocks=3 + metadata");
            let mut s16 = Vec::with_capacity(pcm.len() * 2);
            for &v in &pcm {
                let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
                s16.extend_from_slice(&q.to_le_bytes());
            }
            enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
                samples: (pcm.len() / 2) as u32,
                pts: Some(0),
                data: vec![s16],
            }))
            .unwrap();
            enc.flush().unwrap();
            let mut out = Vec::new();
            while let Ok(p) = enc.receive_packet() {
                out.extend_from_slice(&p.data);
            }
            out
        };
        let frame_bytes = ac3_frame_bytes_blocks(0, 192, 3).unwrap() as usize;
        assert_fractional_syntax(&stream, frame_bytes, 3);
        let bsi = super::super::bsi::parse(&stream[2..]).expect("metadata BSI");
        assert_eq!(bsi.dialnorm, 24, "dialnorm rides fractional frames");
        let dec = decode_all(&stream, frame_bytes);
        assert!(!dec.is_empty(), "metadata-bearing fractional decode");
    }

    // ---- transient pre-noise processing emission (§3.7) ----

    /// Quiet stereo tone with a hard impulse on channel 0 at
    /// `impulse_at` (absolute sample index). Channel 1 stays smooth.
    fn build_impulse_pcm(frames: usize, impulse_at: usize) -> Vec<f32> {
        let n = frames * SAMPLES_PER_FRAME as usize;
        let mut pcm = vec![0.0f32; n * 2];
        for i in 0..n {
            let t = i as f32 / 48_000.0;
            let s = 0.05 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
            pcm[i * 2] = s;
            pcm[i * 2 + 1] = s;
        }
        // A 4-sample burst so the whole 4-sample analysis group jumps.
        for k in 0..4 {
            pcm[(impulse_at + k) * 2] = if k % 2 == 0 { 0.9 } else { -0.9 };
        }
        pcm
    }

    fn encode_tpnp(pcm: &[f32], via_option: bool) -> Vec<u8> {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(2);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(192_000);
        let mut enc: Box<dyn Encoder> = if via_option {
            params.options = oxideav_core::CodecOptions::new().set("tpnp", "1");
            make_encoder(&params).expect("options-driven tpnp encoder")
        } else {
            make_encoder_with_tpnp(&params).expect("typed tpnp encoder")
        };
        let n_samp = pcm.len() / 2;
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n_samp as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut out = Vec::new();
        while let Ok(p) = enc.receive_packet() {
            out.extend_from_slice(&p.data);
        }
        out
    }

    /// Parse frame 0's audfrm and return it.
    fn parse_first_audfrm(stream: &[u8]) -> (super::super::bsi::Bsi, crate::eac3::audfrm::AudFrm) {
        assert_eq!(&stream[..2], &[0x0B, 0x77]);
        let mut br = oxideav_core::bits::BitReader::new(&stream[2..]);
        let bsi = super::super::bsi::parse_with(&mut br).expect("tpnp BSI parse");
        let af = crate::eac3::audfrm::parse_with(&mut br, &bsi).expect("tpnp audfrm parse");
        (bsi, af)
    }

    #[test]
    fn tpnp_impulse_detected_and_parsed() {
        // Impulse at sample 612 = block 2, offset 100: transprocloc =
        // 612 / 4 = 153 (§2.3.2.22 4-sample units), pre-noise gap =
        // 612 % 256 = 100 samples (the emitted time-scaling length).
        let pcm = build_impulse_pcm(2, 612);
        let stream = encode_tpnp(&pcm, false);
        let (_bsi, af) = parse_first_audfrm(&stream);
        assert!(af.transproce, "frame 0 must signal transproce");
        assert!(af.chintransproc[0], "impulse channel carries TPNP data");
        assert!(
            !af.chintransproc[1],
            "smooth channel must not carry TPNP data"
        );
        assert!(
            (af.transprocloc[0] as i32 - 153).abs() <= 4,
            "transprocloc {} not within 4 groups of 153",
            af.transprocloc[0]
        );
        assert_eq!(
            af.transproclen[0], 100,
            "time-scaling length = pre-noise gap"
        );
        // Frame 1 carries no transient.
        let frame1 = &stream[768..];
        let (_b1, af1) = parse_first_audfrm(&{
            let mut v = frame1.to_vec();
            v.truncate(768);
            v
        });
        assert!(
            !af1.transproce,
            "transient-free frame signals transproce = 0"
        );
    }

    #[test]
    fn tpnp_stationary_matches_long_transform_baseline() {
        // With no transient anywhere, the TPNP encoder emits
        // transproce = 0 in every frame and its output is
        // byte-identical to the plain encoder's (long transforms are
        // forced either way on this smooth signal).
        let pcm = build_sine_pcm(2, 2);
        let tpnp = encode_tpnp(&pcm, false);
        let base = encode_with(&pcm, 2, 192_000, 0);
        assert_eq!(
            tpnp, base,
            "stationary TPNP stream must match the baseline byte-for-byte"
        );
    }

    #[test]
    fn tpnp_options_vs_typed_byte_identical() {
        let pcm = build_impulse_pcm(2, 612);
        assert_eq!(
            encode_tpnp(&pcm, false),
            encode_tpnp(&pcm, true),
            "registry `tpnp` option and make_encoder_with_tpnp must build the same encoder"
        );
    }

    #[test]
    fn tpnp_roundtrip_decodes_with_synthesis() {
        // End-to-end through our own decoder: the §3.7.2 synthesis
        // path runs (transproce = 1 with a real pre-noise gap), the
        // sample count is exact, and the decode stays comparable to
        // the TPNP-less decode of the same PCM — the synthesis only
        // rewrites the pre-noise region ahead of the transient.
        let pcm = build_impulse_pcm(3, 612 + 1536);
        let stream = encode_tpnp(&pcm, false);
        let dec = decode_all(&stream, 768);
        assert_eq!(dec.len(), pcm.len(), "sample count through TPNP decode");
        let base = decode_all(&encode_with(&pcm, 2, 192_000, 0), 768);
        let psnr = psnr_vs(&dec, &base);
        eprintln!("tpnp decode vs baseline decode: {psnr:.2} dB");
        assert!(
            psnr >= 15.0,
            "TPNP decode diverged wholesale from the baseline: {psnr:.2} dB"
        );
    }

    #[test]
    fn tpnp_rides_fractional_frames() {
        // TPNP is block-count-agnostic: a 2-block frame (512 samples)
        // with an impulse at sample 300 (block 1, offset 44) signals
        // transprocloc = 75 and a 44-sample gap.
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(2);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(192_000);
        params.options = oxideav_core::CodecOptions::new()
            .set("tpnp", "1")
            .set("blocks", "2");
        let mut enc = make_encoder(&params).expect("tpnp + blocks encoder");
        let n = 1024usize;
        let mut pcm = vec![0.0f32; n * 2];
        for i in 0..n {
            let t = i as f32 / 48_000.0;
            let s = 0.05 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
            pcm[i * 2] = s;
            pcm[i * 2 + 1] = s;
        }
        for k in 0..4 {
            pcm[(300 + k) * 2] = if k % 2 == 0 { 0.9 } else { -0.9 };
        }
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in &pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut stream = Vec::new();
        while let Ok(p) = enc.receive_packet() {
            stream.extend_from_slice(&p.data);
        }
        let (bsi, af) = parse_first_audfrm(&stream);
        assert_eq!(bsi.num_blocks, 2);
        assert!(af.transproce);
        assert!(af.chintransproc[0]);
        assert!((af.transprocloc[0] as i32 - 75).abs() <= 4);
        assert_eq!(af.transproclen[0], 44);
        let frame_bytes = ac3_frame_bytes_blocks(0, 192, 2).unwrap() as usize;
        let dec = decode_all(&stream, frame_bytes);
        assert_eq!(dec.len(), pcm.len());
    }

    #[test]
    fn make_encoder_stereo_48k() {
        let mut p = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        p.sample_rate = Some(48_000);
        p.channels = Some(2);
        p.bit_rate = Some(192_000);
        let _enc = make_encoder(&p).expect("eac3 stereo 192k");
    }

    // ---- per-block SNR-offset (snroffststr ∈ {1, 2}) round-trip ----
    //
    // The decoder's §2.3.3.27 per-block SNR-offset parse is validated by
    // a self-encode → self-decode round-trip: the same PCM is encoded
    // three times — once with the default frame-level `snroffststr == 0`
    // and once each with `snroffststr == 1` and `== 2`. The `1` / `2`
    // encodes carry the SAME numeric (csnroffst, fsnroffst) the bit
    // allocator chose, just spread per-block across the audblks instead of
    // once in audfrm; their mantissa budget is fractionally smaller (the
    // per-block headers are reserved), so the decoded PCM is near-identical
    // to the `0` baseline rather than bit-exact. The discriminating signal
    // is that a ONE-BIT cursor misalignment in the new parse would corrupt
    // every downstream mantissa and collapse the PSNR — so we gate on a
    // high PSNR-vs-baseline floor (≥ 70 dB), which only a correctly
    // bit-aligned parse can reach.

    pub(crate) fn psnr_vs(a: &[i16], b: &[i16]) -> f64 {
        assert_eq!(a.len(), b.len(), "length mismatch in psnr_vs");
        let mut se = 0.0f64;
        for (x, y) in a.iter().zip(b.iter()) {
            let d = *x as f64 - *y as f64;
            se += d * d;
        }
        let mse = se / a.len().max(1) as f64;
        if mse == 0.0 {
            return f64::INFINITY;
        }
        let peak = 32768.0f64;
        10.0 * (peak * peak / mse).log10()
    }

    pub(crate) fn build_sine_pcm(channels: usize, frames: usize) -> Vec<f32> {
        let n = frames * SAMPLES_PER_FRAME as usize;
        let mut pcm = vec![0.0f32; n * channels];
        for i in 0..n {
            let t = i as f32 / 48_000.0;
            // Two tones so several bands carry energy (so the SNR offset
            // actually changes how many mantissa bits each band gets).
            let s = 0.4 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                + 0.25 * (2.0 * std::f32::consts::PI * 3500.0 * t).sin();
            for ch in 0..channels {
                pcm[i * channels + ch] = s * (1.0 - 0.1 * ch as f32);
            }
        }
        pcm
    }

    pub(crate) fn encode_with(
        pcm: &[f32],
        channels: usize,
        bit_rate: u64,
        snroffststr: u8,
    ) -> Vec<u8> {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(channels as u16);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(bit_rate);
        let mut enc: Box<dyn Encoder> = if snroffststr == 0 {
            make_encoder(&params).expect("eac3 make_encoder")
        } else {
            make_encoder_with_snroffststr(&params, snroffststr).expect("eac3 make_encoder snr")
        };
        let n_samp = pcm.len() / channels;
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n_samp as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut out = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => out.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("eac3 encode error: {e:?}"),
            }
        }
        out
    }

    pub(crate) fn decode_all(bytes: &[u8], frame_bytes: usize) -> Vec<i16> {
        use crate::eac3::decoder::{Eac3DecoderState, decode_eac3_packet};
        let mut st = Eac3DecoderState::default();
        let mut out = Vec::new();
        let mut off = 0usize;
        while off + frame_bytes <= bytes.len() {
            let frame = decode_eac3_packet(&mut st, &bytes[off..off + frame_bytes])
                .expect("decode eac3 packet");
            for chunk in frame.pcm_s16le.as_chunks::<2>().0 {
                out.push(i16::from_le_bytes([chunk[0], chunk[1]]));
            }
            off += frame_bytes;
        }
        out
    }

    fn assert_snroffststr_roundtrip(channels: usize, bit_rate: u64, frame_bytes: usize) {
        let pcm = build_sine_pcm(channels, 4);
        let base = decode_all(&encode_with(&pcm, channels, bit_rate, 0), frame_bytes);
        let dec1 = decode_all(&encode_with(&pcm, channels, bit_rate, 1), frame_bytes);
        let dec2 = decode_all(&encode_with(&pcm, channels, bit_rate, 2), frame_bytes);
        assert!(!base.is_empty(), "baseline decode produced no PCM");
        assert_eq!(
            dec1.len(),
            base.len(),
            "snroffststr=1 sample-count mismatch"
        );
        assert_eq!(
            dec2.len(),
            base.len(),
            "snroffststr=2 sample-count mismatch"
        );

        // (a) The two per-block strategies share an identical mantissa
        // budget and SNR offsets, so they MUST decode bit-for-bit equal.
        // This is the strict cursor-alignment invariant: if either parse
        // mis-consumes a single bit, the mantissas diverge and the two
        // decodes differ.
        assert_eq!(
            dec1, dec2,
            "snroffststr=1 and =2 must decode bit-identically (same budget + offsets); \
             a mismatch means one of the per-block SNR-offset parses is misaligned"
        );

        // (b) Sanity: each per-block strategy decodes to near-identical
        // audio as the frame-level baseline (the only delta is the few
        // reserved header bits, which can move the tuner's SNR offset
        // and the elected exponent cadence by one notch). A misaligned
        // parse would collapse this PSNR toward 0 dB; the small budget
        // delta keeps it ≥ 40 dB.
        for (strat, dec) in [(1u8, &dec1), (2u8, &dec2)] {
            let psnr = psnr_vs(dec, &base);
            assert!(
                psnr >= 40.0,
                "snroffststr={strat}: PSNR vs frame-level baseline {psnr:.2} dB < 40 dB — \
                 the per-block SNR-offset parse is bit-misaligned"
            );
        }
    }

    #[test]
    fn snroffststr_per_block_roundtrip_mono() {
        // 128 kbps @ 48 kHz ⇒ 512-byte frames, single indep substream.
        // The slightly higher rate (vs the 96k default) leaves padding
        // headroom for the extra per-block SNR-offset header bits the
        // §2.3.3.27 strategies carry over the frame-level baseline.
        assert_snroffststr_roundtrip(1, 128_000, 512);
    }

    #[test]
    fn snroffststr_per_block_roundtrip_stereo() {
        // 256 kbps @ 48 kHz ⇒ 1024-byte frames.
        assert_snroffststr_roundtrip(2, 256_000, 1024);
    }

    #[test]
    fn snroffststr_per_block_roundtrip_51() {
        // 5.1 @ 448 kbps ⇒ 1792-byte frames, lfeon=1 so the §2.3.3.27
        // `snroffststr == 2` path also exercises the per-block
        // `lfefsnroffst` slot.
        assert_snroffststr_roundtrip(6, 448_000, 1792);
    }

    // ---- spectral extension (§E.2.3.3 / §E.3.6) round-trips ----
    //
    // The SPX encode is validated with the in-tree decoder on two
    // axes:
    //
    // 1. **Cursor alignment** — the SPX strategy + coordinate fields
    //    thread through the audblk between the dynrng and coupling
    //    slots and re-gate chbwcod / nrematbd; one mis-sized field
    //    corrupts every downstream exponent + mantissa and collapses
    //    the decode to noise. The banded-energy gates below can only
    //    pass on a bit-aligned parse.
    // 2. **§3.6.4.3 energy matching** — the whole point of the tool:
    //    per SPX band, the synthesized HF energy must match the
    //    original signal's banded HF energy (the encoder's coordinate
    //    computation + quantiser + the decoder's translation / blend /
    //    scale chain, end to end).
    //
    // The analysis measures banded energy in the encoder's own MDCT
    // domain (same window + transform), averaged over the steady-state
    // interior. Pure-tone content is stationary, so the metric is
    // insensitive to coder delay and needs no lag alignment.

    fn encode_spx(pcm: &[f32], channels: usize, bit_rate: u64, spx: SpxParams) -> Vec<u8> {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(channels as u16);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(bit_rate);
        let mut enc = make_encoder_with_spx(&params, spx).expect("eac3 make_encoder_with_spx");
        let n_samp = pcm.len() / channels;
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n_samp as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut out = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => out.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("eac3 spx encode error: {e:?}"),
            }
        }
        out
    }

    /// Multitone with content below AND above the default SPX begin
    /// frequency (tc# 109 ≈ 10.2 kHz at 48 kHz): LF tones at 700 Hz /
    /// 3.1 kHz / 6 kHz feed the coded band + translation copy region,
    /// HF tones at 12.2 / 15.4 / 19 kHz land in SPX bands 0 / 2 / 3.
    ///
    /// A deterministic broadband noise bed (~-38 dBFS) rides under the
    /// tones. SPX synthesizes each HF band by *scaling a translated
    /// copy* of the low band — a band whose translation source is
    /// spectrally EMPTY can only be filled up to the coordinate
    /// ceiling (`0.875 · 32 ×` the source RMS), so an all-tones signal
    /// with silent gaps between tones is unencodable by construction
    /// (the coordinate saturates and the band comes out low). Real
    /// content carries a broadband floor; the bed models that and
    /// keeps every band's coordinate inside the representable range.
    fn build_spx_multitone(channels: usize, frames: usize) -> Vec<f32> {
        let n = frames * SAMPLES_PER_FRAME as usize;
        let mut pcm = vec![0.0f32; n * channels];
        let tones: [(f32, f32); 6] = [
            (700.0, 0.22),
            (3_100.0, 0.18),
            (6_000.0, 0.14),
            (12_200.0, 0.055),
            (15_400.0, 0.035),
            (19_000.0, 0.030),
        ];
        let mut lfsr: u32 = 0x2545_F491;
        for i in 0..n {
            let t = i as f32 / 48_000.0;
            let mut s = 0.0f32;
            for (f, a) in tones {
                s += a * (2.0 * std::f32::consts::PI * f * t).sin();
            }
            // Deterministic xorshift noise bed, uniform in ±0.012.
            lfsr ^= lfsr << 13;
            lfsr ^= lfsr >> 17;
            lfsr ^= lfsr << 5;
            s += ((lfsr as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.012;
            for ch in 0..channels {
                pcm[i * channels + ch] = s * (1.0 - 0.08 * ch as f32);
            }
        }
        pcm
    }

    /// Mean per-bin MDCT energy of one channel of interleaved PCM,
    /// using the encoder's own window + long transform, skipping the
    /// first / last few blocks (codec priming + flush edges).
    pub(crate) fn mdct_energy_profile(pcm: &[f32], channels: usize, ch: usize) -> [f64; N_COEFFS] {
        use crate::mdct::mdct_512;
        use crate::tables::WINDOW;
        let n = pcm.len() / channels;
        let mut acc = [0.0f64; N_COEFFS];
        let mut blocks = 0usize;
        let mut start = 4 * SAMPLES_PER_BLOCK; // skip priming edge
        while start + 512 + 4 * SAMPLES_PER_BLOCK <= n {
            let mut win = [0.0f32; 512];
            for k in 0..256 {
                win[k] = pcm[(start + k) * channels + ch] * WINDOW[k];
                win[511 - k] = pcm[(start + 511 - k) * channels + ch] * WINDOW[k];
            }
            let mut coeffs = [0.0f32; N_COEFFS];
            mdct_512(&win, &mut coeffs);
            for (a, &c) in acc.iter_mut().zip(coeffs.iter()) {
                *a += (c as f64) * (c as f64);
            }
            blocks += 1;
            start += SAMPLES_PER_BLOCK;
        }
        assert!(blocks > 8, "analysis needs a steady-state interior");
        for a in acc.iter_mut() {
            *a /= blocks as f64;
        }
        acc
    }

    fn band_db_deltas(
        orig: &[f64; N_COEFFS],
        dec: &[f64; N_COEFFS],
        geom: &crate::eac3::spxenc::SpxGeometry,
    ) -> Vec<f64> {
        let mut out = Vec::new();
        let mut lo = geom.begin_tc;
        for bnd in 0..geom.nbnds {
            let hi = lo + geom.bndsztab[bnd];
            let eo: f64 = orig[lo..hi].iter().sum();
            let ed: f64 = dec[lo..hi].iter().sum();
            out.push(10.0 * (ed.max(1e-30) / eo.max(1e-30)).log10());
            lo = hi;
        }
        out
    }

    pub(crate) fn to_f32_interleaved(pcm: &[i16]) -> Vec<f32> {
        pcm.iter().map(|&v| v as f32 / 32767.0).collect()
    }

    fn assert_spx_roundtrip(channels: usize, bit_rate: u64, frame_bytes: usize, tol_db: f64) {
        let spx = SpxParams::default();
        let geom = SpxGeometry::derive(&spx, &DEFAULT_SPX_BNDSTRC).unwrap();
        let pcm = build_spx_multitone(channels, 8);
        let stream = encode_spx(&pcm, channels, bit_rate, spx);
        assert_eq!(
            stream.len() % frame_bytes,
            0,
            "stream not a whole number of {frame_bytes}-byte frames"
        );
        let dec = decode_all(&stream, frame_bytes);
        assert_eq!(
            dec.len() / channels,
            (stream.len() / frame_bytes) * SAMPLES_PER_FRAME as usize,
            "sample-count mismatch"
        );
        let dec_f = to_f32_interleaved(&dec);
        for ch in 0..channels.min(2) {
            let orig_prof = mdct_energy_profile(&pcm, channels, ch);
            let dec_prof = mdct_energy_profile(&dec_f, channels, ch);
            // (a) SPX-band energy match (§3.6.4.3).
            let deltas = band_db_deltas(&orig_prof, &dec_prof, &geom);
            for (bnd, d) in deltas.iter().enumerate() {
                assert!(
                    d.abs() <= tol_db,
                    "ch{ch} SPX band {bnd}: energy delta {d:+.2} dB exceeds ±{tol_db} dB \
                     (all deltas: {deltas:?})"
                );
            }
            // (b) Coded-band fidelity: total LF energy within ±1.5 dB.
            let eo: f64 = orig_prof[..geom.begin_tc].iter().sum();
            let ed: f64 = dec_prof[..geom.begin_tc].iter().sum();
            let lf_delta = 10.0 * (ed / eo).log10();
            assert!(
                lf_delta.abs() <= 1.5,
                "ch{ch}: coded-band energy delta {lf_delta:+.2} dB"
            );
        }
    }

    #[test]
    fn spx_roundtrip_stereo_band_energy() {
        // 192 kbps @ 48 kHz ⇒ 768-byte frames.
        assert_spx_roundtrip(2, 192_000, 768, 3.0);
    }

    #[test]
    fn spx_roundtrip_mono_implicit_chinspx() {
        // Mono exercises the §E.2.3.3.3 implicit `chinspx[0] = 1` arm
        // (no per-channel bits). 96 kbps ⇒ 384-byte frames.
        assert_spx_roundtrip(1, 96_000, 384, 3.0);
    }

    #[test]
    fn spx_roundtrip_51_with_lfe() {
        // 5.1 exercises SPX alongside the LFE pseudo-channel (LFE is
        // never in SPX; its exponents/mantissas follow the fbw SPX
        // fields, so a mis-sized SPX field would corrupt it too).
        // 384 kbps ⇒ 1536-byte frames.
        assert_spx_roundtrip(6, 384_000, 1536, 3.5);
    }

    #[test]
    fn spx_explicit_band_structure_decodes_bit_identically() {
        // `spxbndstrce = 1` with the Table E2.11 content vs
        // `spxbndstrce = 0` (decoder default): identical geometry and
        // — because the encoder reserves the worst-case (explicit)
        // header bits in both modes — identical mantissa budgets, so
        // the two decodes must match bit-for-bit. A one-bit misparse
        // of the explicit structure field would break this instantly.
        let pcm = build_spx_multitone(2, 4);
        let dec_default = decode_all(&encode_spx(&pcm, 2, 192_000, SpxParams::default()), 768);
        let dec_explicit = decode_all(
            &encode_spx(
                &pcm,
                2,
                192_000,
                SpxParams {
                    explicit_band_structure: true,
                    ..SpxParams::default()
                },
            ),
            768,
        );
        assert!(!dec_default.is_empty());
        assert_eq!(
            dec_default, dec_explicit,
            "default-banding and explicit-banding SPX encodes must decode bit-identically"
        );
    }

    #[test]
    fn spx_narrow_region_nondefault_codes() {
        // Non-default geometry: begin sub-band 4 (spxbegf=2 → tc 73),
        // end sub-band 9 (spxendf=3 → tc 133), copy start sub-band 1
        // (tc 37 — non-zero spxstrtf arm), low-noise blend. Exercises
        // remat_band_count_spx's spxbegf ≥ 2 arm with a narrower coded
        // band and a copy region smaller than the SPX span (wraps).
        let spx = SpxParams {
            spxbegf: 2,
            spxendf: 3,
            spxstrtf: 1,
            spxblnd: 31,
            ..SpxParams::default()
        };
        let geom = SpxGeometry::derive(&spx, &DEFAULT_SPX_BNDSTRC).unwrap();
        assert_eq!((geom.begin_tc, geom.end_tc), (73, 133));
        let pcm = build_spx_multitone(2, 6);
        let stream = encode_spx(&pcm, 2, 192_000, spx);
        let dec = decode_all(&stream, 768);
        let dec_f = to_f32_interleaved(&dec);
        let orig_prof = mdct_energy_profile(&pcm, 2, 0);
        let dec_prof = mdct_energy_profile(&dec_f, 2, 0);
        let deltas = band_db_deltas(&orig_prof, &dec_prof, &geom);
        for (bnd, d) in deltas.iter().enumerate() {
            assert!(
                d.abs() <= 3.5,
                "SPX band {bnd}: energy delta {d:+.2} dB (all: {deltas:?})"
            );
        }
    }

    /// Fixture for the adaptive copy-start test: a spectral HOLE in
    /// tc# 25..49 (no content between ~2.3 and ~4.6 kHz), LF tones only
    /// above it, HF tones in SPX bands 0/2/3, and a bed too weak to
    /// carry a band on its own. With `spxstrtf = 0` the copy region
    /// starts at tc# 25, so SPX band 0 (tc 109..133) translates the
    /// near-silent hole and its coordinate pins at the 0.875 ceiling;
    /// `spxstrtf = 2` starts the copy at tc# 49 where the tones live.
    fn build_gap_multitone(channels: usize, frames: usize) -> Vec<f32> {
        let n = frames * SAMPLES_PER_FRAME as usize;
        let mut pcm = vec![0.0f32; n * channels];
        let tones: [(f32, f32); 6] = [
            (700.0, 0.20),     // below the copy region entirely
            (5_500.0, 0.16),   // tc ≈ 58 — inside 49..109
            (8_600.0, 0.12),   // tc ≈ 91 — inside 49..109
            (12_200.0, 0.045), // SPX band 0
            (15_400.0, 0.030), // SPX band 2
            (19_000.0, 0.025), // SPX band 3
        ];
        let mut lfsr: u32 = 0x0BAD_5EED;
        for i in 0..n {
            let t = i as f32 / 48_000.0;
            let mut s = 0.0f32;
            for (f, a) in tones {
                s += a * (2.0 * std::f32::consts::PI * f * t).sin();
            }
            lfsr ^= lfsr << 13;
            lfsr ^= lfsr >> 17;
            lfsr ^= lfsr << 5;
            s += ((lfsr as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.001;
            for ch in 0..channels {
                pcm[i * channels + ch] = s * (1.0 - 0.08 * ch as f32);
            }
        }
        pcm
    }

    #[test]
    fn spx_adaptive_copy_start_avoids_saturation() {
        let geom = SpxGeometry::derive(&SpxParams::default(), &DEFAULT_SPX_BNDSTRC).unwrap();
        let pcm = build_gap_multitone(2, 8);
        // Tone-bearing SPX bands (12.2 / 15.4 / 19 kHz → bands 0/2/3).
        let tone_bands = [0usize, 2, 3];

        // Fixed spxstrtf = 0: band 0's translation source is the
        // spectral hole → the coordinate saturates and the band decodes
        // far below the original energy.
        let fixed = SpxParams::default(); // spxstrtf = 0
        let dec_fixed = to_f32_interleaved(&decode_all(&encode_spx(&pcm, 2, 192_000, fixed), 768));
        let orig_prof = mdct_energy_profile(&pcm, 2, 0);
        let fixed_prof = mdct_energy_profile(&dec_fixed, 2, 0);
        let fixed_deltas = band_db_deltas(&orig_prof, &fixed_prof, &geom);
        assert!(
            fixed_deltas[0] < -6.0,
            "premise: fixed copy-start must saturate band 0 (delta {:+.2} dB)",
            fixed_deltas[0]
        );

        // Adaptive: the per-frame scorer must move the copy start past
        // the hole and recover every tone band.
        let adaptive = SpxParams {
            adaptive_copy_start: true,
            ..SpxParams::default()
        };
        let dec_ad = to_f32_interleaved(&decode_all(&encode_spx(&pcm, 2, 192_000, adaptive), 768));
        let ad_prof = mdct_energy_profile(&dec_ad, 2, 0);
        let ad_deltas = band_db_deltas(&orig_prof, &ad_prof, &geom);
        for &bnd in &tone_bands {
            assert!(
                ad_deltas[bnd].abs() <= 3.5,
                "adaptive: SPX band {bnd} delta {:+.2} dB (all: {ad_deltas:?}; fixed: {fixed_deltas:?})",
                ad_deltas[bnd]
            );
        }
        // Coded band unaffected by the choice.
        let eo: f64 = orig_prof[..geom.begin_tc].iter().sum();
        let ed: f64 = ad_prof[..geom.begin_tc].iter().sum();
        assert!((10.0 * (ed / eo).log10()).abs() <= 1.5);
    }

    #[test]
    fn spx_atten_roundtrip_stereo_band_energy() {
        // §3.6.4.2.3 attenuation: spxattene=1 in audfrm with
        // chinspxatten/spxattencod per channel; the decoder notches the
        // translated coefficients at the border + wrap sites and the
        // encoder folds the notch into its coordinate computation, so
        // the banded-energy contract must STILL hold. This gates (a)
        // the audfrm attenuation-field alignment (a mis-sized field
        // shifts blkstrtinfoe and every audblk after it) and (b) the
        // energy compensation.
        let spx = SpxParams {
            atten_code: Some(14), // taps [0.5, 0.25, 0.125] — a strong notch
            ..SpxParams::default()
        };
        let geom = SpxGeometry::derive(&spx, &DEFAULT_SPX_BNDSTRC).unwrap();
        let pcm = build_spx_multitone(2, 8);
        let stream = encode_spx(&pcm, 2, 192_000, spx);
        let dec = decode_all(&stream, 768);
        let dec_f = to_f32_interleaved(&dec);
        for ch in 0..2 {
            let orig_prof = mdct_energy_profile(&pcm, 2, ch);
            let dec_prof = mdct_energy_profile(&dec_f, 2, ch);
            let deltas = band_db_deltas(&orig_prof, &dec_prof, &geom);
            for (bnd, d) in deltas.iter().enumerate() {
                assert!(
                    d.abs() <= 3.0,
                    "ch{ch} SPX band {bnd} (atten): energy delta {d:+.2} dB (all: {deltas:?})"
                );
            }
        }
        // The attenuated stream must actually differ from the plain
        // one (the notch + the audfrm fields are real bit-level
        // effects, not a silent no-op).
        let plain = encode_spx(&pcm, 2, 192_000, SpxParams::default());
        assert_ne!(stream, plain, "attenuation must change the bitstream");
    }

    #[test]
    fn spx_coordinate_refresh_tracks_mid_frame_level_step() {
        // The frame carries TWO coordinate refreshes (blocks 0 and 3,
        // each energy-matching its own 3-block span) with a thrifted
        // `spxcoe = 0` when block 3 quantises identically. This test
        // proves the per-span refresh actually fires when the spectrum
        // moves: a 14 kHz tone (SPX band 1) is gated ON only in the
        // second half of every frame. A correct encoder+decoder tracks
        // the step — the decoded second-half band energy must sit well
        // above the first half's. A broken span refresh (or a thrift
        // that wrongly reuses across the step) would flatten the two
        // halves.
        let channels = 2usize;
        let frames = 8usize;
        let n = frames * SAMPLES_PER_FRAME as usize;
        let mut pcm = vec![0.0f32; n * channels];
        let mut lfsr: u32 = 0x1234_5677;
        for i in 0..n {
            let t = i as f32 / 48_000.0;
            let in_second_half = (i % SAMPLES_PER_FRAME as usize) >= 768;
            let mut s = 0.20 * (2.0 * std::f32::consts::PI * 700.0 * t).sin()
                + 0.16 * (2.0 * std::f32::consts::PI * 3_100.0 * t).sin()
                + 0.12 * (2.0 * std::f32::consts::PI * 6_000.0 * t).sin();
            if in_second_half {
                s += 0.10 * (2.0 * std::f32::consts::PI * 14_000.0 * t).sin();
            }
            lfsr ^= lfsr << 13;
            lfsr ^= lfsr >> 17;
            lfsr ^= lfsr << 5;
            s += ((lfsr as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.008;
            for ch in 0..channels {
                pcm[i * channels + ch] = s * (1.0 - 0.08 * ch as f32);
            }
        }
        let stream = encode_spx(&pcm, channels, 192_000, SpxParams::default());
        let dec_f = to_f32_interleaved(&decode_all(&stream, 768));

        // 14 kHz ≈ tc 149 → SPX band 1 of the default geometry
        // (tc 133..157). Measure that band's energy in analysis windows
        // confined to each half of each frame (self round-trip output
        // is sample-aligned with the input).
        use crate::mdct::mdct_512;
        use crate::tables::WINDOW;
        let band = 133usize..157;
        let half_energy = |pcm: &[f32], second: bool| -> f64 {
            let mut acc = 0.0f64;
            let mut cnt = 0usize;
            for f in 1..frames - 1 {
                let base = f * SAMPLES_PER_FRAME as usize + if second { 768 } else { 0 };
                for w0 in [0usize, 256] {
                    let start = base + w0;
                    let mut win = [0.0f32; 512];
                    for k in 0..256 {
                        win[k] = pcm[(start + k) * 2] * WINDOW[k];
                        win[511 - k] = pcm[(start + 511 - k) * 2] * WINDOW[k];
                    }
                    let mut coeffs = [0.0f32; N_COEFFS];
                    mdct_512(&win, &mut coeffs);
                    for tc in band.clone() {
                        acc += (coeffs[tc] as f64).powi(2);
                    }
                    cnt += 1;
                }
            }
            acc / cnt as f64
        };
        let orig_ratio = 10.0 * (half_energy(&pcm, true) / half_energy(&pcm, false)).log10();
        let dec_ratio = 10.0 * (half_energy(&dec_f, true) / half_energy(&dec_f, false)).log10();
        assert!(
            orig_ratio >= 12.0,
            "premise: source step should be >= 12 dB (got {orig_ratio:+.1})"
        );
        assert!(
            dec_ratio >= 6.0,
            "decoded SPX band 1 must track the mid-frame level step              (orig {orig_ratio:+.1} dB, decoded {dec_ratio:+.1} dB)"
        );
    }

    /// Mixed per-channel chinspx (§E.2.3.3.3): stereo with ch0 in SPX
    /// and ch1 waveform-coded to its full chbwcod bandwidth. Gates:
    ///
    /// * ch0 keeps the §3.6.4.3 SPX band-energy contract;
    /// * ch1's high-frequency region (above the SPX begin frequency,
    ///   where ch0 is synthesized) is waveform-coded and must hold its
    ///   total energy;
    /// * both coded bands stay faithful.
    ///
    /// This exercises the per-channel end_mant plumbing end to end:
    /// exponent sets of different lengths in one frame, chbwcod
    /// emitted only for the non-SPX channel, spxcoe/coordinates only
    /// for the SPX channel, and the SNR tuner budgeting each channel
    /// at its own bandwidth. A mis-sized field desyncs everything.
    #[test]
    fn spx_mixed_membership_stereo() {
        let spx = SpxParams {
            channel_mask: Some(0b01), // ch0 in SPX, ch1 full-bandwidth
            ..SpxParams::default()
        };
        let geom = SpxGeometry::derive(&spx, &DEFAULT_SPX_BNDSTRC).unwrap();
        let pcm = build_spx_multitone(2, 8);
        // 256 kbps ⇒ 1024-byte frames — headroom for the full-bw ch1.
        let stream = encode_spx(&pcm, 2, 256_000, spx);
        assert_eq!(stream.len() % 1024, 0);
        // Mixed membership must actually change the bitstream vs the
        // uniform all-SPX encode at the same rate.
        assert_ne!(
            stream,
            encode_spx(&pcm, 2, 256_000, SpxParams::default()),
            "channel_mask must change the emission"
        );
        let dec_f = to_f32_interleaved(&decode_all(&stream, 1024));

        // ch0 — SPX band-energy contract (§3.6.4.3).
        let orig0 = mdct_energy_profile(&pcm, 2, 0);
        let dec0 = mdct_energy_profile(&dec_f, 2, 0);
        let deltas = band_db_deltas(&orig0, &dec0, &geom);
        for (bnd, d) in deltas.iter().enumerate() {
            assert!(
                d.abs() <= 3.5,
                "ch0 (SPX) band {bnd}: energy delta {d:+.2} dB (all: {deltas:?})"
            );
        }
        let eo: f64 = orig0[..geom.begin_tc].iter().sum();
        let ed: f64 = dec0[..geom.begin_tc].iter().sum();
        assert!(
            (10.0 * (ed / eo).log10()).abs() <= 1.5,
            "ch0 coded-band energy"
        );

        // ch1 — full-bandwidth waveform coding: the HF region that ch0
        // synthesizes must be carried by real mantissas here.
        let orig1 = mdct_energy_profile(&pcm, 2, 1);
        let dec1 = mdct_energy_profile(&dec_f, 2, 1);
        let hf_o: f64 = orig1[geom.begin_tc..geom.end_tc].iter().sum();
        let hf_d: f64 = dec1[geom.begin_tc..geom.end_tc].iter().sum();
        let hf_delta = 10.0 * (hf_d.max(1e-30) / hf_o.max(1e-30)).log10();
        assert!(
            hf_delta.abs() <= 3.0,
            "ch1 (full-bw) HF energy delta {hf_delta:+.2} dB"
        );
        let lo_o: f64 = orig1[..geom.begin_tc].iter().sum();
        let lo_d: f64 = dec1[..geom.begin_tc].iter().sum();
        assert!(
            (10.0 * (lo_d / lo_o).log10()).abs() <= 1.5,
            "ch1 coded-band energy"
        );
    }

    /// The `spx_chmask` option key must build the same encoder as the
    /// typed channel_mask, and the §E.2.3.3.3 constraints are enforced
    /// at construction.
    #[test]
    fn spx_chmask_option_and_validation() {
        let pcm = build_spx_multitone(2, 3);
        let typed = encode_spx(
            &pcm,
            2,
            192_000,
            SpxParams {
                channel_mask: Some(0b10),
                ..SpxParams::default()
            },
        );
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(2);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(192_000);
        params.options = oxideav_core::CodecOptions::new().set("spx_chmask", "2");
        let mut enc = make_encoder(&params).expect("options-driven mixed SPX encoder");
        let n_samp = pcm.len() / 2;
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in &pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n_samp as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut from_options = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => from_options.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("options mixed-spx encode error: {e:?}"),
            }
        }
        assert_eq!(
            typed, from_options,
            "spx_chmask option and typed channel_mask must emit identical bytes"
        );

        // Validation: an all-zero typed mask is rejected.
        let mut p = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        p.sample_rate = Some(48_000);
        p.channels = Some(2);
        assert!(
            make_encoder_with_spx(
                &p,
                SpxParams {
                    channel_mask: Some(0),
                    ..SpxParams::default()
                }
            )
            .is_err()
        );
        // Mono cannot exclude its only channel (chinspx[0] implicit).
        p.channels = Some(1);
        assert!(
            make_encoder_with_spx(
                &p,
                SpxParams {
                    channel_mask: Some(0b10),
                    ..SpxParams::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn spx_options_path_matches_typed_constructor_bit_for_bit() {
        // The registry-facing options surface (`spx*` keys on
        // CodecParameters::options) must build the SAME encoder as the
        // typed make_encoder_with_spx — proven by byte-identical
        // streams for a non-default configuration.
        let pcm = build_spx_multitone(2, 3);
        let spx = SpxParams {
            spxbegf: 4,
            spxendf: 7,
            spxstrtf: 1,
            spxblnd: 20,
            adaptive_copy_start: false,
            atten_code: Some(9),
            explicit_band_structure: true,
            channel_mask: None,
        };
        let typed = encode_spx(&pcm, 2, 192_000, spx);

        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(2);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(192_000);
        params.options = oxideav_core::CodecOptions::new()
            .set("spx_begf", "4")
            .set("spx_endf", "7")
            .set("spx_strtf", "1")
            .set("spx_blnd", "20")
            .set("spx_atten", "9")
            .set("spx_explicit_band_structure", "true");
        let mut enc = make_encoder(&params).expect("options-driven SPX encoder");
        let n_samp = pcm.len() / 2;
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in &pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n_samp as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut from_options = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => from_options.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("options encode error: {e:?}"),
            }
        }
        assert_eq!(
            typed, from_options,
            "options-driven and typed SPX constructors must emit identical bytes"
        );
    }

    #[test]
    fn spx_options_validation_and_gating() {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(2);
        // Bad value → rejected at construction.
        params.options = oxideav_core::CodecOptions::new().set("spx_begf", "9");
        assert!(make_encoder(&params).is_err());
        params.options = oxideav_core::CodecOptions::new().set("spx", "maybe");
        assert!(make_encoder(&params).is_err());
        // spx=0 disables even when sub-keys are present.
        params.options = oxideav_core::CodecOptions::new()
            .set("spx", "0")
            .set("spx_begf", "4");
        assert!(make_encoder(&params).is_ok());
        // Geometry validation still applies through the options path
        // (inverted range).
        params.options = oxideav_core::CodecOptions::new()
            .set("spx_begf", "7")
            .set("spx_endf", "0");
        assert!(make_encoder(&params).is_err());
        // Plain enable works.
        params.options = oxideav_core::CodecOptions::new().set("spx", "1");
        assert!(make_encoder(&params).is_ok());
    }

    fn cpl_params(channels: u16, bit_rate: u64, opts: &[(&str, &str)]) -> CodecParameters {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(channels);
        params.bit_rate = Some(bit_rate);
        let mut o = oxideav_core::CodecOptions::new();
        for (k, v) in opts {
            o = o.set(*k, *v);
        }
        params.options = o;
        params
    }

    #[test]
    fn std_cpl_options_validation_and_gating() {
        let ok = |opts: &[(&str, &str)]| make_encoder(&cpl_params(2, 192_000, opts)).is_ok();
        assert!(ok(&[("cpl", "1")]));
        assert!(ok(&[("cpl_begf", "3")])); // sub-key implies enable
        assert!(ok(&[("cpl", "0"), ("cpl_begf", "15"), ("cpl_endf", "0")])); // cpl=0 wins
        assert!(!ok(&[("cpl", "maybe")]));
        assert!(!ok(&[("cpl_begf", "16")]));
        // ncplsubnd = 3 + cplendf - cplbegf must be >= 1.
        assert!(ok(&[("cpl_begf", "12"), ("cpl_endf", "10")]));
        assert!(!ok(&[("cpl_begf", "13"), ("cpl_endf", "10")]));
        // With SPX, cplendf is derived (Table E1.4) — setting it is an
        // error, and the region must stay below the SPX begin.
        assert!(!ok(&[("cpl", "1"), ("spx", "1"), ("cpl_endf", "7")]));
        assert!(ok(&[
            ("cpl", "1"),
            ("spx", "1"),
            ("spx_begf", "0"),
            ("cpl_begf", "0")
        ]));
        assert!(!ok(&[
            ("cpl", "1"),
            ("spx", "1"),
            ("spx_begf", "0"),
            ("cpl_begf", "1")
        ]));
        assert!(!ok(&[("cpl", "1"), ("spx", "1"), ("spx_chmask", "1")]));
        // Single-subsystem scope and 6-block scope.
        assert!(!ok(&[("cpl", "1"), ("aht", "1")]));
        assert!(!ok(&[("cpl", "1"), ("ecpl", "1")]));
        assert!(!ok(&[("cpl", "1"), ("blocks", "3")]));
        assert!(make_encoder(&cpl_params(1, 96_000, &[("cpl", "1")])).is_err());
        // dither knob.
        assert!(ok(&[("dither", "0")]));
        assert!(!ok(&[("dither", "maybe")]));
    }

    #[test]
    fn std_cpl_geometry_negative_cplendf_meets_spx_begin() {
        // Table E1.4: with SPX in use cplendf = spxbegf - 2 (spxbegf <
        // 6) or 2·spxbegf - 7 — negative for spxbegf 0/1 — and the
        // coupling region ends exactly where SPX begins.
        for spxbegf in 0..=7u8 {
            let spx = SpxParams {
                spxbegf,
                ..SpxParams::default()
            };
            let g = StdCplGeometry::derive(&StdCplParams::default(), Some(&spx)).unwrap();
            let spx_g = SpxGeometry::derive(&spx, &DEFAULT_SPX_BNDSTRC).unwrap();
            assert_eq!(g.end_bin, spx_g.begin_tc, "spxbegf={spxbegf}");
            assert!(g.spx_bounded);
            let endf_signed = if spxbegf < 6 {
                spxbegf as i32 - 2
            } else {
                2 * spxbegf as i32 - 7
            };
            assert_eq!(g.nsubbnd as i32, 3 + endf_signed - g.begf as i32);
        }
        let spx0 = SpxParams {
            spxbegf: 0,
            ..SpxParams::default()
        };
        let g = StdCplGeometry::derive(&StdCplParams::default(), Some(&spx0)).unwrap();
        assert_eq!((g.begf, g.nsubbnd, g.begin_bin, g.end_bin), (0, 1, 37, 49));
        // Default region (cplbegf 8, cplendf 15): the merge flags are
        // the Table E2.12 entries for absolute sub-bands 9..17.
        let g = StdCplGeometry::derive(&StdCplParams::default(), None).unwrap();
        assert_eq!((g.begf, g.nsubbnd), (8, 10));
        let merged: Vec<bool> = (1..g.nsubbnd).map(|o| g.bndstrc[o]).collect();
        let table: Vec<bool> = (9..18)
            .map(|s| super::super::dsp::DEFCPLBNDSTRC[s])
            .collect();
        assert_eq!(merged, table);
    }

    fn assert_std_cpl_roundtrip(opts: &[(&str, &str)], bit_rate: u64, frame_bytes: usize) {
        let channels = 2usize;
        let params = cpl_params(channels as u16, bit_rate, opts);
        let spx = spx_params_from_options(&params).unwrap();
        let cpl = std_cpl_params_from_options(&params).unwrap().unwrap();
        let g = StdCplGeometry::derive(&cpl, spx.as_ref()).unwrap();
        let mut enc = make_encoder(&params).expect("make_encoder cpl");
        let pcm = build_spx_multitone(channels, 8);
        let s16: Vec<u8> = pcm
            .iter()
            .flat_map(|&v| ((v * 32767.0).clamp(-32768.0, 32767.0) as i16).to_le_bytes())
            .collect();
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: (pcm.len() / channels) as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut stream = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => stream.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("eac3 cpl encode error: {e:?}"),
            }
        }
        assert_eq!(stream.len() % frame_bytes, 0);
        let dec = to_f32_interleaved(&decode_all(&stream, frame_bytes));
        for ch in 0..channels {
            let orig = mdct_energy_profile(&pcm, channels, ch);
            let got = mdct_energy_profile(&dec, channels, ch);
            // Per coupling band: the coordinates carry each channel's
            // band energy (§7.4.3).
            for bnd in 0..g.nbnd {
                let bins: Vec<usize> = (g.begin_bin..g.end_bin)
                    .filter(|&b| g.sbnd_to_bnd[(b - g.begin_bin) / 12] == bnd)
                    .collect();
                let eo: f64 = bins.iter().map(|&b| orig[b]).sum();
                let ed: f64 = bins.iter().map(|&b| got[b]).sum();
                let d = 10.0 * (ed.max(1e-30) / eo.max(1e-30)).log10();
                assert!(d.abs() <= 2.0, "{opts:?} ch{ch} cpl band {bnd}: {d:+.2} dB");
            }
            let eo: f64 = orig[..g.begin_bin].iter().sum();
            let ed: f64 = got[..g.begin_bin].iter().sum();
            let d = 10.0 * (ed / eo).log10();
            assert!(
                d.abs() <= 1.5,
                "{opts:?} ch{ch}: coded-band delta {d:+.2} dB"
            );
        }
    }

    #[test]
    fn std_cpl_roundtrip_band_energy() {
        assert_std_cpl_roundtrip(&[("cpl", "1"), ("cpl_begf", "5")], 192_000, 768);
        assert_std_cpl_roundtrip(&[("cpl", "1")], 128_000, 512);
    }

    #[test]
    fn std_cpl_with_spx_roundtrip_band_energy() {
        // Coupling between cplbegf and the SPX begin (derived cplendf).
        assert_std_cpl_roundtrip(
            &[
                ("cpl", "1"),
                ("cpl_begf", "2"),
                ("spx", "1"),
                ("spx_begf", "4"),
            ],
            128_000,
            512,
        );
    }

    #[test]
    fn make_encoder_with_spx_rejects_invalid_geometry() {
        let mut p = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        p.sample_rate = Some(48_000);
        p.channels = Some(2);
        // Inverted sub-band range (begf=7 → sub-band 11, endf=0 → 5).
        let bad = SpxParams {
            spxbegf: 7,
            spxendf: 0,
            ..SpxParams::default()
        };
        assert!(make_encoder_with_spx(&p, bad).is_err());
        // Empty copy region (strtf sub-band 3 ≥ begin sub-band 2).
        let bad = SpxParams {
            spxbegf: 0,
            spxendf: 7,
            spxstrtf: 3,
            ..SpxParams::default()
        };
        assert!(make_encoder_with_spx(&p, bad).is_err());
    }

    #[test]
    fn make_encoder_rejects_unsupported_channels() {
        let mut p = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        p.sample_rate = Some(48_000);
        // 4 ch is between stereo (2) and 5.1 (6) — not yet covered by
        // the encoder's allow-list (1, 2, 6, 8). Likewise 7 isn't in
        // the list (it would map to 3/2 + 1 ambiguous extra channel
        // and we don't speculate).
        p.channels = Some(4);
        match make_encoder(&p) {
            Ok(_) => panic!("must reject 4ch (not in 1/2/6/8 allow-list)"),
            Err(e) => assert!(matches!(e, Error::Unsupported(_))),
        }
    }

    #[test]
    fn make_encoder_rejects_bad_sample_rate() {
        let mut p = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        p.sample_rate = Some(16_000);
        p.channels = Some(2);
        match make_encoder(&p) {
            Ok(_) => panic!("must reject 16 kHz"),
            Err(e) => assert!(matches!(e, Error::Unsupported(_))),
        }
    }

    #[test]
    fn make_encoder_71_builds_pair_layout() {
        let mut p = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        p.sample_rate = Some(48_000);
        p.channels = Some(8);
        let _enc = make_encoder(&p).expect("eac3 7.1");
        // Reach into the layout via a probe encode would require an
        // accessor; since the layout struct is private, we cover the
        // chanmap math directly: bit 6 (Lrs/Rrs pair, Table E2.5) is
        // stored in MSB-6 = 1 << 9 = 0x0200.
        assert_eq!(1u16 << (15 - 6), 0x0200);
    }

    #[test]
    fn make_encoder_51_5fbw_plus_lfe() {
        let mut p = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        p.sample_rate = Some(48_000);
        p.channels = Some(6);
        p.bit_rate = Some(384_000);
        let _enc = make_encoder(&p).expect("eac3 5.1");
    }
}

#[cfg(test)]
mod aht_tests {
    use super::tests::{build_sine_pcm, decode_all, encode_with, psnr_vs};
    use super::*;

    pub(crate) fn encode_aht(pcm: &[f32], channels: usize, bit_rate: u64) -> Vec<u8> {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(channels as u16);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(bit_rate);
        let mut enc = make_encoder_with_aht(&params).expect("eac3 make_encoder_with_aht");
        let n_samp = pcm.len() / channels;
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n_samp as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut out = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => out.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("eac3 aht encode error: {e:?}"),
            }
        }
        out
    }

    /// AHT round-trip: encode stationary two-tone PCM with AHT, decode
    /// with the in-tree decoder, and require a healthy absolute PSNR
    /// plus a bounded distance from the non-AHT baseline at the same
    /// bit rate. (Until r457 the standard path clipped every reuse
    /// block's mantissas — see `bound_shared_exponents` — and sat at
    /// ~24 dB, so AHT "won" by 20 dB; with the standard path fixed it
    /// codes this fixture ~2-23 dB *above* the AHT path, whose
    /// rate-distortion tuning is a recorded follow-up. The gate keeps
    /// AHT within 25 dB of the baseline so a regression in either
    /// path still fails.)
    ///
    /// A single mis-sized field anywhere in the new audfrm chahtinu /
    /// chgaqmod / gain-word / VQ / GAQ chain would corrupt every
    /// downstream bit and collapse the PSNR to single digits, so this
    /// is also the cursor-alignment proof.
    fn assert_aht_roundtrip(channels: usize, bit_rate: u64, frame_bytes: usize) {
        let pcm = build_sine_pcm(channels, 4);
        let base_stream = encode_with(&pcm, channels, bit_rate, 0);
        let base = decode_all(&base_stream, frame_bytes);
        let aht_stream = encode_aht(&pcm, channels, bit_rate);
        assert_eq!(
            aht_stream.len() % frame_bytes,
            0,
            "AHT stream not a whole number of {frame_bytes}-byte frames"
        );
        assert_eq!(aht_stream.len(), base_stream.len(), "same rate → same size");
        assert_ne!(aht_stream, base_stream, "AHT must change the bitstream");
        let dec = decode_all(&aht_stream, frame_bytes);
        assert_eq!(dec.len(), base.len(), "sample-count mismatch");

        // Align out the 256-sample MDCT priming delay, and measure the
        // fbw channels only: the LFE keeps the identical standard path
        // in both encoders, and its 0-120 Hz band-limit makes it drop
        // the fixture's tonal content by design — including it would
        // just mask the fbw comparison behind a shared floor.
        let delay = 256 * channels;
        let fbw = if channels == 6 { 5 } else { channels };
        let input_i16: Vec<i16> = pcm
            .iter()
            .map(|&v| (v * 32767.0).clamp(-32768.0, 32767.0) as i16)
            .collect();
        let n = base.len() - delay;
        let strip_lfe = |data: &[i16]| -> Vec<i16> {
            data.iter()
                .enumerate()
                .filter(|(i, _)| i % channels < fbw)
                .map(|(_, &v)| v)
                .collect()
        };
        let p_base = psnr_vs(&strip_lfe(&base[delay..]), &strip_lfe(&input_i16[..n]));
        let p_aht = psnr_vs(&strip_lfe(&dec[delay..]), &strip_lfe(&input_i16[..n]));
        assert!(
            p_aht >= 35.0,
            "AHT decode PSNR {p_aht:.2} dB < 35 dB (baseline {p_base:.2} dB)"
        );
        assert!(
            p_aht >= p_base - 25.0,
            "AHT ({p_aht:.2} dB) fell more than 25 dB behind the non-AHT baseline ({p_base:.2} dB) on stationary content"
        );
    }

    #[test]
    fn aht_roundtrip_mono() {
        assert_aht_roundtrip(1, 96_000, 384);
    }

    #[test]
    fn aht_roundtrip_stereo() {
        assert_aht_roundtrip(2, 192_000, 768);
    }

    #[test]
    fn aht_roundtrip_51_with_lfe() {
        // 5.1 exercises AHT alongside the (non-AHT) LFE pseudo-channel:
        // the LFE's standard per-block mantissas follow the fbw AHT
        // blocks in block 0 and stand alone in blocks 1..5.
        assert_aht_roundtrip(6, 384_000, 1536);
    }

    /// LFE-AHT (§3.4.2 lfeahtinu): a 5.1 fixture whose LFE carries a
    /// 60 Hz tone (inside the 0-120 Hz coded band). The decoded LFE
    /// channel must round-trip through the front-loaded LFE-AHT block
    /// at a healthy PSNR and stay within 15 dB of the standard
    /// per-block LFE path at the same rate (the standard path codes
    /// all seven LFE bins from r457 on and sits ~13 dB above the
    /// AHT path here — LFE-AHT rate-distortion is a recorded
    /// follow-up). A mis-sized lfeahtinu / lfegaqmod / LFE codeword
    /// field would corrupt the whole frame tail and collapse both.
    #[test]
    fn aht_lfe_channel_roundtrip() {
        let channels = 6usize;
        let n = 4 * SAMPLES_PER_FRAME as usize;
        let mut pcm = vec![0.0f32; n * channels];
        for i in 0..n {
            let t = i as f32 / 48_000.0;
            let s = 0.4 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                + 0.25 * (2.0 * std::f32::consts::PI * 3500.0 * t).sin();
            let lfe = 0.4 * (2.0 * std::f32::consts::PI * 60.0 * t).sin();
            for ch in 0..5 {
                pcm[i * channels + ch] = s * (1.0 - 0.1 * ch as f32);
            }
            pcm[i * channels + 5] = lfe;
        }
        let base = decode_all(&encode_with(&pcm, channels, 384_000, 0), 1536);
        let dec = decode_all(&encode_aht(&pcm, channels, 384_000), 1536);
        assert_eq!(dec.len(), base.len());
        let delay = 256 * channels;
        let input_i16: Vec<i16> = pcm
            .iter()
            .map(|&v| (v * 32767.0).clamp(-32768.0, 32767.0) as i16)
            .collect();
        let n_cmp = base.len() - delay;
        let lfe_only = |data: &[i16]| -> Vec<i16> {
            data.iter()
                .enumerate()
                .filter(|(i, _)| i % channels == 5)
                .map(|(_, &v)| v)
                .collect()
        };
        let p_base = psnr_vs(&lfe_only(&base[delay..]), &lfe_only(&input_i16[..n_cmp]));
        let p_aht = psnr_vs(&lfe_only(&dec[delay..]), &lfe_only(&input_i16[..n_cmp]));
        assert!(
            p_aht >= 30.0,
            "LFE-AHT PSNR {p_aht:.2} dB < 30 dB (standard-path LFE {p_base:.2} dB)"
        );
        assert!(
            p_aht >= p_base - 15.0,
            "LFE-AHT ({p_aht:.2} dB) fell more than 15 dB behind the standard LFE path ({p_base:.2} dB)"
        );
    }

    /// The per-frame SNR-offset search must convert additional rate
    /// into quality: across a 96 → 192 → 384 kbps ladder the AHT
    /// decode PSNR must be non-decreasing and gain substantially end
    /// to end (the measured stereo curve runs ~42 dB at 96 kbps to
    /// ~73 dB at 384 kbps; the ±0.5 dB slack and the ≥ 15 dB
    /// end-to-end floor leave wide margins). A tuner regression that
    /// stopped spending the larger budget — or a cost model that
    /// overflowed a frame — would flatten or break the curve.
    #[test]
    fn aht_quality_scales_with_rate() {
        let pcm = build_sine_pcm(2, 4);
        let input: Vec<i16> = pcm
            .iter()
            .map(|&v| (v * 32767.0).clamp(-32768.0, 32767.0) as i16)
            .collect();
        let delay = 256 * 2;
        let mut curve = Vec::new();
        for (rate, frame_bytes) in [(96_000u64, 384usize), (192_000, 768), (384_000, 1536)] {
            let dec = decode_all(&encode_aht(&pcm, 2, rate), frame_bytes);
            let n = dec.len() - delay;
            curve.push(psnr_vs(&dec[delay..], &input[..n]));
        }
        for w in curve.windows(2) {
            assert!(
                w[1] >= w[0] - 0.5,
                "AHT PSNR must be non-decreasing in rate: {curve:?}"
            );
        }
        assert!(
            curve[curve.len() - 1] - curve[0] >= 15.0,
            "AHT PSNR must gain >= 15 dB from 96k to 384k: {curve:?}"
        );
    }

    #[test]
    fn aht_options_path_matches_typed_constructor_bit_for_bit() {
        let pcm = build_sine_pcm(2, 3);
        let typed = encode_aht(&pcm, 2, 192_000);

        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(2);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(192_000);
        params.options = oxideav_core::CodecOptions::new().set("aht", "1");
        let mut enc = make_encoder(&params).expect("options-driven AHT encoder");
        let n_samp = pcm.len() / 2;
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in &pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n_samp as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut from_options = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => from_options.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("options aht encode error: {e:?}"),
            }
        }
        assert_eq!(
            typed, from_options,
            "options-driven and typed AHT constructors must emit identical bytes"
        );
    }

    #[test]
    fn aht_and_spx_cannot_combine() {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(2);
        params.options = oxideav_core::CodecOptions::new()
            .set("aht", "1")
            .set("spx", "1");
        assert!(make_encoder(&params).is_err());
        // Bad value rejected too.
        params.options = oxideav_core::CodecOptions::new().set("aht", "maybe");
        assert!(make_encoder(&params).is_err());
    }
}

#[cfg(test)]
mod ecpl_tests {
    use super::tests::{decode_all, mdct_energy_profile, to_f32_interleaved};
    use super::*;

    // ---- enhanced coupling (§E.2.3.3.16-26 / §E.3.5.5) round-trip ----
    //
    // Enhanced coupling is parametric above the begin frequency: the
    // decoder rebuilds each coupled channel from one shared carrier via
    // per-band amplitude + angle coordinates, so the quality contract
    // is (a) per-ecpl-band decoded ENERGY within a small tolerance of
    // the original (the §E.3.5.5.2 amplitude semantics), (b) unchanged
    // fidelity of the independently-coded low band, and (c) waveform-
    // level accuracy for phase-locked content (the carrier is phase-
    // locked to the first coupled channel; other channels' phases ride
    // the Table E3.11 angle coordinates — a broken angle path turns a
    // quadrature-shifted tone into 100 % error energy, which the PSNR
    // floor catches).

    fn encode_ecpl(pcm: &[f32], channels: usize, bit_rate: u64, ecpl: EcplParams) -> Vec<u8> {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(channels as u16);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(bit_rate);
        let mut enc = make_encoder_with_ecpl(&params, ecpl).expect("eac3 make_encoder_with_ecpl");
        let n_samp = pcm.len() / channels;
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n_samp as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut out = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => out.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("eac3 ecpl encode error: {e:?}"),
            }
        }
        out
    }

    /// Per-enhanced-coupling-band decoded-vs-original energy deltas in
    /// dB, walking the geometry's band structure.
    fn ecpl_band_db_deltas(
        orig: &[f64; N_COEFFS],
        dec: &[f64; N_COEFFS],
        geom: &EcplGeometry,
    ) -> Vec<f64> {
        let mut out = Vec::new();
        let mut lo = geom.start_bin;
        for &nb in &geom.band_bins {
            let hi = (lo + nb).min(N_COEFFS);
            let eo: f64 = orig[lo..hi].iter().sum();
            let ed: f64 = dec[lo..hi].iter().sum();
            out.push(10.0 * (ed.max(1e-30) / eo.max(1e-30)).log10());
            lo = hi;
        }
        out
    }

    /// Correlated multitone spanning the coupled region (bins 37..253 ≈
    /// 3.5-23.7 kHz at 48 kHz) plus a shared LF tone below it. Each
    /// channel carries the SAME tone complex, level-panned, with an
    /// optional per-channel phase offset on the in-region tones (to
    /// exercise the §E.2.3.3.24 angle coordinates).
    fn build_ecpl_multitone(channels: usize, frames: usize, phase_step: f32) -> Vec<f32> {
        let n = frames * SAMPLES_PER_FRAME as usize;
        let mut pcm = vec![0.0f32; n * channels];
        // In-region tones (all above tc 37 ≈ 3.5 kHz).
        let hf_tones: [(f32, f32); 4] = [
            (4_300.0, 0.16),
            (6_700.0, 0.13),
            (9_800.0, 0.10),
            (14_500.0, 0.07),
        ];
        for i in 0..n {
            let t = i as f32 / 48_000.0;
            let lf = 0.20 * (2.0 * std::f32::consts::PI * 700.0 * t).sin();
            for ch in 0..channels {
                let phase = phase_step * ch as f32;
                let mut s = lf;
                for (f, a) in hf_tones {
                    s += a * (2.0 * std::f32::consts::PI * f * t + phase).sin();
                }
                pcm[i * channels + ch] = s * (1.0 - 0.12 * ch as f32);
            }
        }
        pcm
    }

    /// Interior PSNR between the original PCM and the decoded stream
    /// for one channel, searching over the codec's fixed small latency
    /// (the MDCT overlap delay) and skipping the priming / flush edges.
    fn interior_psnr_ch(orig: &[f32], dec: &[i16], channels: usize, ch: usize) -> f64 {
        let n = (orig.len() / channels).min(dec.len() / channels);
        let skip = 4 * SAMPLES_PER_BLOCK; // priming edge
        let tail = 2 * SAMPLES_PER_BLOCK; // flush edge
        let mut best = f64::MIN;
        for lag in 0..=512usize {
            let mut se = 0.0f64;
            let mut count = 0usize;
            let mut i = skip;
            while i + lag < n - tail {
                let o = (orig[i * channels + ch] * 32767.0) as f64;
                let d = dec[(i + lag) * channels + ch] as f64;
                se += (o - d) * (o - d);
                count += 1;
                i += 1;
            }
            if count == 0 {
                continue;
            }
            let mse = se / count as f64;
            let psnr = 10.0 * (32768.0f64 * 32768.0 / mse.max(1e-12)).log10();
            if psnr > best {
                best = psnr;
            }
        }
        best
    }

    #[allow(clippy::too_many_arguments)]
    fn assert_ecpl_roundtrip(
        channels: usize,
        bit_rate: u64,
        frame_bytes: usize,
        phase_step: f32,
        band_tol_db: f64,
        lf_tol_db: f64,
        psnr_floor_db: f64,
    ) {
        let ecpl = EcplParams::default();
        let geom = EcplGeometry::derive(&ecpl, &DEFAULT_ECPL_BNDSTRC).unwrap();
        let pcm = build_ecpl_multitone(channels, 8, phase_step);
        let stream = encode_ecpl(&pcm, channels, bit_rate, ecpl);
        assert_eq!(
            stream.len() % frame_bytes,
            0,
            "stream not a whole number of {frame_bytes}-byte frames"
        );
        let dec = decode_all(&stream, frame_bytes);
        assert_eq!(
            dec.len() / channels,
            (stream.len() / frame_bytes) * SAMPLES_PER_FRAME as usize,
            "sample-count mismatch"
        );
        let dec_f = to_f32_interleaved(&dec);
        let nfchans = if channels == 6 { 5 } else { channels };
        for ch in 0..nfchans {
            let orig_prof = mdct_energy_profile(&pcm, channels, ch);
            let dec_prof = mdct_energy_profile(&dec_f, channels, ch);
            // (a) per-band energy match across the coupled region.
            // Bands more than 40 dB below the loudest coupled band
            // carry no signal (fixture noise floor) — their ratio is
            // quantiser-noise-vs-noise and meaningless, so skip them.
            let deltas = ecpl_band_db_deltas(&orig_prof, &dec_prof, &geom);
            let band_energies: Vec<f64> = {
                let mut out = Vec::new();
                let mut lo = geom.start_bin;
                for &nb in &geom.band_bins {
                    let hi = (lo + nb).min(N_COEFFS);
                    out.push(orig_prof[lo..hi].iter().sum());
                    lo = hi;
                }
                out
            };
            let peak = band_energies.iter().cloned().fold(0.0f64, f64::max);
            for (bnd, d) in deltas.iter().enumerate() {
                if band_energies[bnd] < peak * 1e-3 {
                    continue; // > 30 dB below the loudest band: the
                    // pure-tone fixtures leave only MDCT
                    // leakage there — reconstruction noise
                    // vs leakage is not a meaningful ratio.
                }
                assert!(
                    d.abs() <= band_tol_db,
                    "ch{ch} ecpl band {bnd}: energy delta {d:+.2} dB exceeds ±{band_tol_db} dB \
                     (all deltas: {deltas:?})"
                );
            }
            // (b) coded low-band fidelity.
            let eo: f64 = orig_prof[..geom.start_bin].iter().sum();
            let ed: f64 = dec_prof[..geom.start_bin].iter().sum();
            let lf_delta: f64 = 10.0 * (ed / eo).log10();
            assert!(
                lf_delta.abs() <= lf_tol_db,
                "ch{ch}: coded-band energy delta {lf_delta:+.2} dB exceeds ±{lf_tol_db} dB"
            );
            // (c) waveform-level accuracy (amplitude AND phase).
            let psnr = interior_psnr_ch(&pcm, &dec, channels, ch);
            assert!(
                psnr >= psnr_floor_db,
                "ch{ch}: interior PSNR {psnr:.1} dB below the {psnr_floor_db} dB floor"
            );
        }
    }

    #[test]
    fn ecpl_roundtrip_stereo_band_energy() {
        // 2/0 exercises the implicit-chincpl arm. 192 kbps ⇒ 768-byte
        // frames. In-phase panned content: the carrier is (a scaled)
        // channel 0, so both channels reconstruct with angle ≈ 0.
        assert_ecpl_roundtrip(2, 192_000, 768, 0.0, 3.0, 1.5, 20.0);
    }

    #[test]
    fn ecpl_roundtrip_stereo_quadrature_angles() {
        // Channel 1's in-region tones are 90° out of phase with the
        // carrier source: without the §E.2.3.3.24 angle coordinates the
        // reconstruction would be in quadrature (≈ 3 dB PSNR against
        // the original); with them the waveform floor holds.
        assert_ecpl_roundtrip(2, 192_000, 768, std::f32::consts::FRAC_PI_2, 3.0, 1.5, 18.0);
    }

    /// `docs/audio/ac3/ac3-errata.md` entry E3, encode→decode
    /// direction: the full bitstream round-trip through the corrected
    /// §E.3.5.5.1 overlap-add reproduces the enhanced-coupling region
    /// at unity level. The erratum's as-printed reading (step 3 without
    /// the §7.9.4.1 step-6 factor of 2) reconstructs the carrier at
    /// half scale; the loudest coupled channel would then need the
    /// unrepresentable amplitude coordinate 2.0 (Table E3.10 ceiling is
    /// exactly 1.0, §E.3.5.4 code 0 = 0 dB), so the region decodes at a
    /// systematic −6 dB. Gate the aggregate in-region energy delta well
    /// inside that discriminator, per channel.
    #[test]
    fn ecpl_roundtrip_unity_level_pins_factor2_erratum() {
        let ecpl = EcplParams::default();
        let geom = EcplGeometry::derive(&ecpl, &DEFAULT_ECPL_BNDSTRC).unwrap();
        let pcm = build_ecpl_multitone(2, 8, 0.0);
        let stream = encode_ecpl(&pcm, 2, 192_000, ecpl);
        let dec = decode_all(&stream, 768);
        let dec_f = to_f32_interleaved(&dec);
        let (lo, hi) = (geom.start_bin, geom.end_bin.min(N_COEFFS));
        for ch in 0..2 {
            let orig = mdct_energy_profile(&pcm, 2, ch);
            let decp = mdct_energy_profile(&dec_f, 2, ch);
            let eo: f64 = orig[lo..hi].iter().sum();
            let ed: f64 = decp[lo..hi].iter().sum();
            let delta = 10.0 * (ed / eo.max(1e-30)).log10();
            assert!(
                delta.abs() <= 1.5,
                "ch{ch}: aggregate ecpl-region energy delta {delta:+.2} dB is not unity \
                 (the as-printed §E.3.5.5.1 overlap-add sits at −6.02 dB)"
            );
        }
    }

    #[test]
    fn ecpl_roundtrip_51_explicit_chincpl() {
        // acmod = 7 (5 fbw channels) transmits explicit chincpl[ch]
        // bits — this pins the §E.1.3.3.7 field ORDER (chincpl before
        // the enhanced-coupling strategy fields; the pre-r406 decoder
        // read them swapped, which desyncs every multichannel ecpl
        // frame) and the LFE integrity behind the coupling fields.
        // 384 kbps ⇒ 1536-byte frames.
        assert_ecpl_roundtrip(6, 384_000, 1536, 0.35, 3.5, 2.0, 15.0);
    }

    #[test]
    fn ecpl_registry_options_build_identical_encoder() {
        let pcm = build_ecpl_multitone(2, 3, 0.0);
        let typed = encode_ecpl(&pcm, 2, 192_000, EcplParams::default());

        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(2);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(192_000);
        params.options = oxideav_core::CodecOptions::new().set("ecpl", "1");
        let mut enc = make_encoder(&params).expect("options ecpl encoder");
        let n_samp = pcm.len() / 2;
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in &pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n_samp as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut from_options = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => from_options.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("options encode error: {e:?}"),
            }
        }
        assert_eq!(
            typed, from_options,
            "options-driven and typed ecpl constructors must emit identical bytes"
        );
    }

    #[test]
    fn ecpl_options_validation_and_gating() {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(2);
        // Bad values → rejected at construction.
        params.options = oxideav_core::CodecOptions::new().set("ecpl_begf", "16");
        assert!(make_encoder(&params).is_err());
        params.options = oxideav_core::CodecOptions::new().set("ecpl", "maybe");
        assert!(make_encoder(&params).is_err());
        // Below the supported begin grid (begf 0/1 → begin sub-band < 4).
        params.options = oxideav_core::CodecOptions::new().set("ecpl_begf", "1");
        assert!(make_encoder(&params).is_err());
        // Inverted range.
        params.options = oxideav_core::CodecOptions::new()
            .set("ecpl_begf", "13")
            .set("ecpl_endf", "0");
        assert!(make_encoder(&params).is_err());
        // ecpl=0 disables even with sub-keys present.
        params.options = oxideav_core::CodecOptions::new()
            .set("ecpl", "0")
            .set("ecpl_begf", "3");
        assert!(make_encoder(&params).is_ok());
        // Plain enable works; mutual exclusions fire.
        params.options = oxideav_core::CodecOptions::new().set("ecpl", "1");
        assert!(make_encoder(&params).is_ok());
        params.options = oxideav_core::CodecOptions::new()
            .set("ecpl", "1")
            .set("aht", "1");
        assert!(make_encoder(&params).is_err());
        // spx + ecpl is the §3.6.1 co-active configuration — allowed
        // (see `spx_ecpl_coactive_stereo_band_energy`).
        params.options = oxideav_core::CodecOptions::new()
            .set("ecpl", "1")
            .set("spx", "1");
        assert!(make_encoder(&params).is_ok());
        // Mono cannot couple.
        params.channels = Some(1);
        params.options = oxideav_core::CodecOptions::new().set("ecpl", "1");
        assert!(make_encoder(&params).is_err());
    }
    #[test]
    fn ecpl_71_pair_couples_indep_only() {
        // 7.1 emits an indep 5.1 substream (enhanced-coupled) + a
        // plain dependent Lb/Rb substream in every packet — the two
        // syntaxes must coexist: the dep substream carries no coupling
        // bits (cplinu[0] = 0) while the indep one carries the full
        // strategy/coordinate/carrier stack. A field-width error in
        // either would desync the pair walk.
        let pcm = build_ecpl_multitone(8, 6, 0.3);
        let stream = encode_ecpl(&pcm, 8, 576_000, EcplParams::default());
        // 384k indep (1536 B) + 192k dep (768 B) per packet.
        let pair_bytes = 1536 + 768;
        assert_eq!(stream.len() % pair_bytes, 0, "not a whole pair stream");

        use crate::eac3::decoder::{Eac3DecoderState, decode_eac3_packet};
        let mut st = Eac3DecoderState::default();
        let mut out: Vec<i16> = Vec::new();
        let mut channels = 0usize;
        let mut off = 0usize;
        while off + pair_bytes <= stream.len() {
            let frame = decode_eac3_packet(&mut st, &stream[off..off + pair_bytes])
                .expect("decode ecpl 7.1 indep+dep packet");
            assert_eq!(frame.channels, 8, "expected 8 output channels");
            channels = frame.channels as usize;
            for c in frame.pcm_s16le.as_chunks::<2>().0 {
                out.push(i16::from_le_bytes([c[0], c[1]]));
            }
            off += pair_bytes;
        }
        assert!(!out.is_empty(), "no PCM decoded");
        // Every output channel is live (the fixture feeds all 8 source
        // channels; LFE gets the sub-120 Hz residue of the 700 Hz tone
        // suppressed, so exempt slot 5's level check).
        let n = out.len() / channels;
        for ch in 0..channels {
            if ch == 5 {
                continue; // LFE — fixture has no sub-120 Hz content
            }
            let mut e = 0.0f64;
            for i in n / 4..(3 * n / 4) {
                let v = out[i * channels + ch] as f64;
                e += v * v;
            }
            let rms = (e / (n / 2) as f64).sqrt();
            assert!(
                rms > 100.0,
                "output channel {ch} is near-silent (rms {rms:.1})"
            );
        }
    }
    // ---- SPX + enhanced coupling co-active (§3.6.1) ----

    fn encode_spx_ecpl(pcm: &[f32], channels: usize, bit_rate: u64) -> Vec<u8> {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(channels as u16);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(bit_rate);
        let mut enc =
            make_encoder_with_spx_ecpl(&params, SpxParams::default(), EcplParams::default())
                .expect("eac3 make_encoder_with_spx_ecpl");
        let n_samp = pcm.len() / channels;
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n_samp as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut out = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => out.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("eac3 spx+ecpl encode error: {e:?}"),
            }
        }
        out
    }

    /// Three-region fixture: an LF tone below the coupling begin
    /// (bins < 37), phase-offset tones inside the coupling region
    /// (37..109 — 4.3 / 6.7 / 9.2 kHz), HF tones + a noise bed inside
    /// the SPX region (109..229 — 12.2 / 15.4 kHz).
    fn build_spx_ecpl_multitone(channels: usize, frames: usize) -> Vec<f32> {
        let n = frames * SAMPLES_PER_FRAME as usize;
        let mut pcm = vec![0.0f32; n * channels];
        let cpl_tones: [(f32, f32); 3] = [(4_300.0, 0.16), (6_700.0, 0.13), (9_200.0, 0.10)];
        let spx_tones: [(f32, f32); 2] = [(12_200.0, 0.05), (15_400.0, 0.035)];
        let mut lfsr: u32 = 0x2545_F491;
        for i in 0..n {
            let t = i as f32 / 48_000.0;
            let lf = 0.20 * (2.0 * std::f32::consts::PI * 700.0 * t).sin();
            lfsr ^= lfsr << 13;
            lfsr ^= lfsr >> 17;
            lfsr ^= lfsr << 5;
            let noise = ((lfsr as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.012;
            for ch in 0..channels {
                let phase = 0.6 * ch as f32;
                let mut s = lf + noise;
                for (f, a) in cpl_tones {
                    s += a * (2.0 * std::f32::consts::PI * f * t + phase).sin();
                }
                for (f, a) in spx_tones {
                    s += a * (2.0 * std::f32::consts::PI * f * t).sin();
                }
                pcm[i * channels + ch] = s * (1.0 - 0.1 * ch as f32);
            }
        }
        pcm
    }

    #[test]
    fn spx_ecpl_coactive_stereo_band_energy() {
        let spx = SpxParams::default();
        let spx_geom = SpxGeometry::derive(&spx, &DEFAULT_SPX_BNDSTRC).unwrap();
        let ecpl_geom = EcplGeometry::derive_with_spx(
            &EcplParams::default(),
            spx.spxbegf,
            &DEFAULT_ECPL_BNDSTRC,
        )
        .unwrap();
        // The two regions abut exactly: ecplsubbndtab[end] == SPX begin tc.
        assert_eq!(ecpl_geom.end_bin, spx_geom.begin_tc);
        assert!(ecpl_geom.spx_bounded);

        let pcm = build_spx_ecpl_multitone(2, 8);
        let stream = encode_spx_ecpl(&pcm, 2, 192_000);
        assert_eq!(stream.len() % 768, 0);
        let dec = decode_all(&stream, 768);
        assert_eq!(
            dec.len() / 2,
            (stream.len() / 768) * SAMPLES_PER_FRAME as usize
        );
        let dec_f = to_f32_interleaved(&dec);
        for ch in 0..2 {
            let orig_prof = mdct_energy_profile(&pcm, 2, ch);
            let dec_prof = mdct_energy_profile(&dec_f, 2, ch);
            // (a) coupling-region band energies (37..109).
            let deltas = ecpl_band_db_deltas(&orig_prof, &dec_prof, &ecpl_geom);
            let mut lo = ecpl_geom.start_bin;
            let mut energies = Vec::new();
            for &nb in &ecpl_geom.band_bins {
                let hi = (lo + nb).min(N_COEFFS);
                energies.push(orig_prof[lo..hi].iter().sum::<f64>());
                lo = hi;
            }
            let peak = energies.iter().cloned().fold(0.0f64, f64::max);
            for (bnd, d) in deltas.iter().enumerate() {
                if energies[bnd] < peak * 1e-4 {
                    continue;
                }
                assert!(
                    d.abs() <= 3.0,
                    "ch{ch} coupling band {bnd}: energy delta {d:+.2} dB (all: {deltas:?})"
                );
            }
            // (b) SPX-region band energies (109..229, §3.6.4.3).
            let mut lo = spx_geom.begin_tc;
            for bnd in 0..spx_geom.nbnds {
                let hi = lo + spx_geom.bndsztab[bnd];
                let eo: f64 = orig_prof[lo..hi].iter().sum();
                let ed: f64 = dec_prof[lo..hi].iter().sum();
                let d = 10.0 * (ed.max(1e-30) / eo.max(1e-30)).log10();
                assert!(
                    d.abs() <= 3.5,
                    "ch{ch} SPX band {bnd}: energy delta {d:+.2} dB"
                );
                lo = hi;
            }
            // (c) coded low band (< 37) fidelity.
            let eo: f64 = orig_prof[..ecpl_geom.start_bin].iter().sum();
            let ed: f64 = dec_prof[..ecpl_geom.start_bin].iter().sum();
            let lf_delta: f64 = 10.0 * (ed / eo).log10();
            assert!(
                lf_delta.abs() <= 1.5,
                "ch{ch}: coded-band energy delta {lf_delta:+.2} dB"
            );
        }
    }

    #[test]
    fn spx_ecpl_constructor_rules() {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(2);
        params.bit_rate = Some(192_000);
        // chmask + ecpl rejected (a coupled channel outside SPX would
        // have no content above the coupling region).
        let masked = SpxParams {
            channel_mask: Some(1),
            ..SpxParams::default()
        };
        assert!(make_encoder_with_spx_ecpl(&params, masked, EcplParams::default()).is_err());
        // aht + ecpl still rejected via options.
        params.options = oxideav_core::CodecOptions::new()
            .set("ecpl", "1")
            .set("aht", "1");
        assert!(make_encoder(&params).is_err());
        // spx + ecpl now allowed via options.
        params.options = oxideav_core::CodecOptions::new()
            .set("ecpl", "1")
            .set("spx", "1");
        assert!(make_encoder(&params).is_ok());
        // An SPX begin low enough to empty the coupling region is
        // rejected: spxbegf = 0 → ecpl end sub-band 5 > begin 4 is
        // still valid, so use a begin code above it (ecplbegf 4 →
        // begin sub-band 6 >= end 5).
        let spx_low = SpxParams {
            spxbegf: 0,
            ..SpxParams::default()
        };
        params.options = oxideav_core::CodecOptions::new();
        let ecpl_high = EcplParams {
            ecplbegf: 4,
            ecplendf: 15,
            ..EcplParams::default()
        };
        assert!(make_encoder_with_spx_ecpl(&params, spx_low, ecpl_high).is_err());
    }

    #[test]
    fn spx_ecpl_registry_options_build_identical_encoder() {
        let pcm = build_spx_ecpl_multitone(2, 3);
        let typed = encode_spx_ecpl(&pcm, 2, 192_000);

        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(2);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(192_000);
        params.options = oxideav_core::CodecOptions::new()
            .set("spx", "1")
            .set("ecpl", "1");
        let mut enc = make_encoder(&params).expect("options spx+ecpl encoder");
        let n_samp = pcm.len() / 2;
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in &pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n_samp as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut from_options = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => from_options.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("options encode error: {e:?}"),
            }
        }
        assert_eq!(
            typed, from_options,
            "options-driven and typed spx+ecpl constructors must emit identical bytes"
        );
    }
    /// MDCT-domain inter-channel coherence of interleaved stereo PCM
    /// over the enhanced-coupling region: `|Σ L·R| / sqrt(ΣL²·ΣR²)`
    /// accumulated per block across the steady interior. ≈1 for
    /// phase-locked copies (what a chaos-less parametric reconstruction
    /// produces), ≈0 for independent content.
    fn region_coherence(pcm: &[f32], geom: &EcplGeometry) -> f64 {
        use crate::mdct::mdct_512;
        use crate::tables::WINDOW;
        let channels = 2usize;
        let n = pcm.len() / channels;
        let mut cross = 0.0f64;
        let mut el = 0.0f64;
        let mut er = 0.0f64;
        let mut start = 4 * SAMPLES_PER_BLOCK;
        while start + 512 + 4 * SAMPLES_PER_BLOCK <= n {
            let mut coeffs = [[0.0f32; N_COEFFS]; 2];
            for (ch, out) in coeffs.iter_mut().enumerate() {
                let mut win = [0.0f32; 512];
                for k in 0..256 {
                    win[k] = pcm[(start + k) * channels + ch] * WINDOW[k];
                    win[511 - k] = pcm[(start + 511 - k) * channels + ch] * WINDOW[k];
                }
                mdct_512(&win, out);
            }
            for bin in geom.start_bin..geom.end_bin.min(N_COEFFS) {
                let l = coeffs[0][bin] as f64;
                let r = coeffs[1][bin] as f64;
                cross += l * r;
                el += l * l;
                er += r * r;
            }
            start += SAMPLES_PER_BLOCK;
        }
        cross.abs() / (el * er).sqrt().max(1e-30)
    }

    /// Stereo fixture with PARTIAL in-region coherence: the right
    /// channel carries the left channel's tones in phase (the coherent
    /// part — its band angle vs the carrier is ≈ 0) plus equal-level
    /// independent tones 200 Hz away, landing in the SAME coupling
    /// bands (the incoherent part). Per-band coherence ≈ 0.7, so the
    /// coherence-driven chaos codes fire while the measured band angle
    /// stays pinned near zero — isolating the chaos path from the
    /// angle path.
    fn build_width_fixture(frames: usize) -> Vec<f32> {
        let n = frames * SAMPLES_PER_FRAME as usize;
        let mut pcm = vec![0.0f32; n * 2];
        let tones: [(f32, f32); 3] = [(4_300.0, 0.16), (6_700.0, 0.13), (9_800.0, 0.10)];
        for i in 0..n {
            let t = i as f32 / 48_000.0;
            let lf = 0.20 * (2.0 * std::f32::consts::PI * 700.0 * t).sin();
            let mut l = lf;
            let mut r = lf;
            for (f, a) in tones {
                let common = a * (2.0 * std::f32::consts::PI * f * t).sin();
                l += common;
                r += 0.7 * common
                    + 0.7 * a * (2.0 * std::f32::consts::PI * (f + 200.0) * t + 1.1).sin();
            }
            pcm[i * 2] = l;
            pcm[i * 2 + 1] = r;
        }
        pcm
    }

    #[test]
    fn ecpl_chaos_restores_stereo_width() {
        // A chaos-less parametric reconstruction turns the right
        // channel into a phase-rotated copy of the carrier (≈ left):
        // decoded inter-channel coherence over the coupled region
        // collapses toward 1 even though the source channels are
        // independent there. With coherence-driven chaos coordinates
        // the decoder's §E.3.5.5.3 random de-correlation restores the
        // width: decoded coherence drops far below the chaos-less
        // decode. Band energies must stay matched either way (the
        // §E.3.5.5.2 amplitude modification is pre-compensated).
        let geom = EcplGeometry::derive(&EcplParams::default(), &DEFAULT_ECPL_BNDSTRC).unwrap();
        let pcm = build_width_fixture(8);
        let orig_coh = region_coherence(&pcm, &geom);
        assert!(
            orig_coh < 0.75,
            "fixture is not partially incoherent in-region (coherence {orig_coh:.3})"
        );

        let with_chaos = EcplParams::default();
        let without_chaos = EcplParams {
            chaos: false,
            ..EcplParams::default()
        };
        let dec_on =
            to_f32_interleaved(&decode_all(&encode_ecpl(&pcm, 2, 192_000, with_chaos), 768));
        let dec_off = to_f32_interleaved(&decode_all(
            &encode_ecpl(&pcm, 2, 192_000, without_chaos),
            768,
        ));
        let coh_on = region_coherence(&dec_on, &geom);
        let coh_off = region_coherence(&dec_off, &geom);
        eprintln!("ECPL-WIDTH orig={orig_coh:.3} off={coh_off:.3} on={coh_on:.3}");
        // Measured (deterministic decode): orig ≈ 0.71, chaos-less
        // 0.914 (the coherent part is reconstructed phase-locked; the
        // incoherent part is REPLACED by more phase-locked carrier
        // content), chaos-on 0.796 (the per-bin ±2/7·π jitter of the
        // code-2 bands knocks the excess coherence back down —
        // sinc-of-jitter ≈ 0.87 multiplier, matching theory). Gate the
        // regression envelope rather than exact values.
        assert!(
            coh_off > 0.87,
            "chaos-less decode should be near-coherent (got {coh_off:.3})"
        );
        assert!(
            coh_on < 0.83,
            "chaos decode should restore width (got {coh_on:.3} vs chaos-less {coh_off:.3})"
        );
        assert!(
            coh_off - coh_on > 0.06,
            "chaos should reduce coherence by a clear margin \
             (off {coh_off:.3} vs on {coh_on:.3})"
        );
        // Band energies stay matched with chaos on (amplitude
        // pre-compensation): reuse the per-band check on both channels.
        for ch in 0..2 {
            let orig_prof = mdct_energy_profile(&pcm, 2, ch);
            let dec_prof = mdct_energy_profile(&dec_on, 2, ch);
            let deltas = ecpl_band_db_deltas(&orig_prof, &dec_prof, &geom);
            let mut lo = geom.start_bin;
            let mut energies = Vec::new();
            for &nb in &geom.band_bins {
                let hi = (lo + nb).min(N_COEFFS);
                energies.push(orig_prof[lo..hi].iter().sum::<f64>());
                lo = hi;
            }
            let peak = energies.iter().cloned().fold(0.0f64, f64::max);
            for (bnd, d) in deltas.iter().enumerate() {
                // This fixture is pure tones (no noise bed): bands
                // without a tone hold only MDCT leakage, 25-35 dB
                // down — their "energy delta" is reconstruction noise
                // vs leakage and meaningless. Gate the tone bands.
                if energies[bnd] < peak * 1e-3 {
                    continue;
                }
                assert!(
                    d.abs() <= 3.5,
                    "ch{ch} band {bnd}: chaos-on energy delta {d:+.2} dB (all: {deltas:?})"
                );
            }
        }
    }
    #[test]
    fn ecpl_roundtrip_narrow_geometry() {
        // Non-default begin/end codes: ecplbegf = 7 exercises the
        // Table E3.8 middle arm (begin = ecplbegf + 2 = 9 → tc 97);
        // ecplendf = 8 → end sub-band 15 → tc 169. The region spans
        // 9.1-15.8 kHz and the default Table E2.14 banding merges its
        // 6 sub-bands into 3 bands (merges at 9, 11, 13).
        let params = EcplParams {
            ecplbegf: 7,
            ecplendf: 8,
            ..EcplParams::default()
        };
        let geom = EcplGeometry::derive(&params, &DEFAULT_ECPL_BNDSTRC).unwrap();
        assert_eq!((geom.start_bin, geom.end_bin), (97, 169));
        // Span 9..15 under the default banding: sub-band 9 starts band
        // 0 (its Table E2.14 merge bit is masked per §E.2.3.3.19),
        // merges at 11 and 13 → 4 bands.
        assert_eq!(geom.necplbnd, 4);
        assert_eq!(geom.band_bins.len(), 4);

        // Tones inside the narrow region (10.1 / 12.4 / 14.6 kHz →
        // bins 108 / 132 / 156) + LF anchor below it.
        let n = 8 * SAMPLES_PER_FRAME as usize;
        let mut pcm = vec![0.0f32; n * 2];
        for i in 0..n {
            let t = i as f32 / 48_000.0;
            let lf = 0.20 * (2.0 * std::f32::consts::PI * 700.0 * t).sin();
            for ch in 0..2 {
                let phase = 0.5 * ch as f32;
                let s = lf
                    + 0.15 * (2.0 * std::f32::consts::PI * 10_100.0 * t + phase).sin()
                    + 0.12 * (2.0 * std::f32::consts::PI * 12_400.0 * t + phase).sin()
                    + 0.08 * (2.0 * std::f32::consts::PI * 14_600.0 * t + phase).sin();
                pcm[i * 2 + ch] = s * (1.0 - 0.1 * ch as f32);
            }
        }
        let stream = encode_ecpl(&pcm, 2, 192_000, params);
        assert_eq!(stream.len() % 768, 0);
        let dec_f = to_f32_interleaved(&decode_all(&stream, 768));
        for ch in 0..2 {
            let orig_prof = mdct_energy_profile(&pcm, 2, ch);
            let dec_prof = mdct_energy_profile(&dec_f, 2, ch);
            let deltas = ecpl_band_db_deltas(&orig_prof, &dec_prof, &geom);
            for (bnd, d) in deltas.iter().enumerate() {
                assert!(
                    d.abs() <= 3.0,
                    "ch{ch} narrow band {bnd}: energy delta {d:+.2} dB (all: {deltas:?})"
                );
            }
            // Content ABOVE the narrow region is waveform-coded
            // normally (chbwcod suppressed but end_mant for coupled
            // channels is the region start... the region end < full
            // bandwidth means bins above 169 are NOT coded — the
            // decoder zeroes them). The fixture keeps everything
            // below tc 169, so total energy still matches.
            let eo: f64 = orig_prof[..geom.start_bin].iter().sum();
            let ed: f64 = dec_prof[..geom.start_bin].iter().sum();
            let lf_delta: f64 = 10.0 * (ed / eo).log10();
            assert!(
                lf_delta.abs() <= 1.5,
                "ch{ch}: below-region energy delta {lf_delta:+.2} dB"
            );
        }
    }

    #[test]
    fn ecpl_roundtrip_44100() {
        // fscod = 1 interplay: the coupling grid is a transform-bin
        // grid, so nothing rate-specific should change — this guards
        // the frame-size lookup + bit-allocation table plumbing at
        // 44.1 kHz. 192 kbps @ 44.1 kHz frames.
        let params = {
            let mut p = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
            p.sample_rate = Some(44_100);
            p.channels = Some(2);
            p.sample_format = Some(SampleFormat::S16);
            p.bit_rate = Some(192_000);
            p
        };
        let mut enc =
            make_encoder_with_ecpl(&params, EcplParams::default()).expect("44.1k ecpl encoder");
        let geom = EcplGeometry::derive(&EcplParams::default(), &DEFAULT_ECPL_BNDSTRC).unwrap();
        let frames = 8usize;
        let n = frames * SAMPLES_PER_FRAME as usize;
        let mut pcm = vec![0.0f32; n * 2];
        for i in 0..n {
            let t = i as f32 / 44_100.0;
            let lf = 0.20 * (2.0 * std::f32::consts::PI * 650.0 * t).sin();
            for ch in 0..2 {
                let phase = 0.5 * ch as f32;
                let s = lf
                    + 0.16 * (2.0 * std::f32::consts::PI * 4_000.0 * t + phase).sin()
                    + 0.12 * (2.0 * std::f32::consts::PI * 8_200.0 * t + phase).sin()
                    + 0.08 * (2.0 * std::f32::consts::PI * 13_000.0 * t + phase).sin();
                pcm[i * 2 + ch] = s * (1.0 - 0.1 * ch as f32);
            }
        }
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in &pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut stream = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => stream.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("44.1k ecpl encode error: {e:?}"),
            }
        }
        assert_eq!(stream.len() % frames, 0, "uneven frame sizes");
        let frame_bytes = stream.len() / frames;
        let dec = decode_all(&stream, frame_bytes);
        assert_eq!(dec.len() / 2, frames * SAMPLES_PER_FRAME as usize);
        let dec_f = to_f32_interleaved(&dec);
        for ch in 0..2 {
            let orig_prof = mdct_energy_profile(&pcm, 2, ch);
            let dec_prof = mdct_energy_profile(&dec_f, 2, ch);
            let deltas = ecpl_band_db_deltas(&orig_prof, &dec_prof, &geom);
            let mut lo = geom.start_bin;
            let mut energies = Vec::new();
            for &nb in &geom.band_bins {
                let hi = (lo + nb).min(N_COEFFS);
                energies.push(orig_prof[lo..hi].iter().sum::<f64>());
                lo = hi;
            }
            let peak = energies.iter().cloned().fold(0.0f64, f64::max);
            for (bnd, d) in deltas.iter().enumerate() {
                if energies[bnd] < peak * 1e-3 {
                    continue;
                }
                assert!(
                    d.abs() <= 3.0,
                    "ch{ch} 44.1k band {bnd}: energy delta {d:+.2} dB (all: {deltas:?})"
                );
            }
        }
    }
}

#[cfg(test)]
mod meta_tests {
    use super::tests::{build_sine_pcm, decode_all};
    use super::*;

    fn encode_meta(
        pcm: &[f32],
        channels: usize,
        bit_rate: u64,
        meta: Option<Eac3Metadata>,
        options: &[(&str, &str)],
    ) -> Vec<u8> {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(channels as u16);
        params.sample_format = Some(SampleFormat::S16);
        params.bit_rate = Some(bit_rate);
        let mut opts = oxideav_core::CodecOptions::new();
        for (k, v) in options {
            opts = opts.set(*k, *v);
        }
        params.options = opts;
        let mut enc: Box<dyn Encoder> = match meta {
            Some(m) => make_encoder_with_metadata(&params, m).expect("typed metadata encoder"),
            None => make_encoder(&params).expect("options encoder"),
        };
        let n_samp = pcm.len() / channels;
        let mut s16 = Vec::with_capacity(pcm.len() * 2);
        for &v in pcm {
            let q = (v * 32767.0).clamp(-32768.0, 32767.0) as i16;
            s16.extend_from_slice(&q.to_le_bytes());
        }
        enc.send_frame(&Frame::Audio(oxideav_core::AudioFrame {
            samples: n_samp as u32,
            pts: Some(0),
            data: vec![s16],
        }))
        .unwrap();
        enc.flush().unwrap();
        let mut out = Vec::new();
        loop {
            match enc.receive_packet() {
                Ok(p) => out.extend_from_slice(&p.data),
                Err(Error::NeedMore) | Err(Error::Eof) => break,
                Err(e) => panic!("eac3 encode error: {e:?}"),
            }
        }
        assert!(!out.is_empty(), "no packets produced");
        out
    }

    fn full_meta() -> Eac3Metadata {
        Eac3Metadata {
            dialnorm: 22,
            compr: Some(0xB3),
            dynrng: None,
            mixmd: Some(Eac3MixMetadata {
                dmixmod: 2, // LoRo preferred
                ltrtcmixlev: 5,
                lorocmixlev: 4,
                ltrtsurmixlev: 6,
                lorosurmixlev: 5,
                lfemixlevcod: Some(15),
                pgmscl: Some(51), // 0 dB
                extpgmscl: Some(45),
            }),
            infomd: Some(Eac3InfoMetadata {
                bsmod: 2,
                copyrightb: true,
                origbs: false,
                dsurmod: 0,
                dheadphonmod: 0,
                dsurexmod: 2,
                audprod: Some(Eac3AudioProduction {
                    mixlevel: 18,
                    roomtyp: 1,
                    adconvtyp: true,
                }),
                sourcefscod: false,
            }),
        }
    }

    /// Every Table E1.2 mixing + informational word configured through
    /// [`Eac3Metadata`] must read back through the typed Annex E BSI
    /// surface on every syncframe — 5.1 exercises the dmixmod,
    /// centre/surround mix-level, LFE-mix-level and dsurexmod arms.
    #[test]
    fn metadata_bsi_words_roundtrip_51() {
        let pcm = build_sine_pcm(6, 3);
        let stream = encode_meta(&pcm, 6, 384_000, Some(full_meta()), &[]);
        let fb = 1536usize; // 384 kbps @ 48 kHz
        assert_eq!(stream.len() % fb, 0);
        for off in (0..stream.len()).step_by(fb) {
            let bsi = crate::eac3::bsi::parse(&stream[off + 2..]).expect("eac3 bsi");
            assert_eq!(bsi.dialnorm, 22);
            assert_eq!(bsi.compr.expect("compr present").raw(), 0xB3);
            assert_eq!(bsi.dmixmod, 2);
            let ml = bsi.annex_e_mix_levels.expect("mix levels present");
            assert_eq!(ml.ltrtcmixlev, 5);
            assert_eq!(ml.lorocmixlev, 4);
            assert_eq!(ml.ltrtsurmixlev, 6);
            assert_eq!(ml.lorosurmixlev, 5);
            assert_eq!(bsi.lfemixlevcod, Some(15));
            let pg = bsi.pgmscl.expect("pgmscl present");
            assert_eq!(pg.raw(), 51);
            assert_eq!(pg.decibels(), Some(0));
            assert_eq!(bsi.extpgmscl.expect("extpgmscl present").raw(), 45);
            assert_eq!(bsi.bsmod, Some(2));
            assert!(bsi.dsurexmod.is_some(), "dsurexmod absent (acmod 7)");
            let ap = bsi.audio_production.expect("audprod present");
            assert_eq!(ap.mixlevel, 18);
            assert_eq!(ap.roomtyp.raw(), 1);
            assert!(bsi.adconvtyp.is_some());
            let ci = bsi.copyright_info.expect("copyright info present");
            assert!(ci.is_copyright_protected());
            assert!(!ci.is_original_bitstream());
        }
        // The metadata-bearing stream still decodes.
        let dec = decode_all(&stream, fb);
        assert!(!dec.is_empty());
    }

    /// 2/0 exercises the dsurmod + dheadphonmod informational arm and
    /// the front/surround-less mixing block (pgmscl only).
    #[test]
    fn metadata_stereo_dsurmod_dheadphon_roundtrip() {
        let meta = Eac3Metadata {
            mixmd: Some(Eac3MixMetadata {
                pgmscl: Some(40), // −11 dB
                ..Eac3MixMetadata::default()
            }),
            infomd: Some(Eac3InfoMetadata {
                dsurmod: 2,
                dheadphonmod: 2,
                ..Eac3InfoMetadata::default()
            }),
            ..Eac3Metadata::default()
        };
        let pcm = build_sine_pcm(2, 2);
        let stream = encode_meta(&pcm, 2, 192_000, Some(meta), &[]);
        let fb = 768usize;
        assert_eq!(stream.len() % fb, 0);
        for off in (0..stream.len()).step_by(fb) {
            let bsi = crate::eac3::bsi::parse(&stream[off + 2..]).expect("eac3 bsi");
            // 2/0: no dmixmod / mix-level arms.
            assert_eq!(bsi.dmixmod, 0xFF);
            assert!(bsi.annex_e_mix_levels.is_none());
            assert_eq!(bsi.pgmscl.expect("pgmscl").decibels(), Some(-11));
            assert!(bsi.dolby_surround_mode.is_some(), "dsurmod absent");
            assert!(bsi.dheadphonmod.is_some(), "dheadphonmod absent");
            assert_eq!(bsi.bsmod, Some(0));
        }
    }

    /// A 7.1 indep+dep pair carries the metadata blocks on the
    /// INDEPENDENT substream only; the dependent substream keeps
    /// `mixmdate = infomdate = 0` but shares dialnorm/compr.
    #[test]
    fn metadata_71_pair_blocks_on_indep_only() {
        let meta = Eac3Metadata {
            compr: Some(0x77),
            ..full_meta()
        };
        let pcm = build_sine_pcm(8, 3);
        let stream = encode_meta(&pcm, 8, 576_000, Some(meta), &[]);
        let pair = 1536 + 768;
        assert_eq!(stream.len() % pair, 0, "not a whole pair stream");
        for off in (0..stream.len()).step_by(pair) {
            let indep = crate::eac3::bsi::parse(&stream[off + 2..]).expect("indep bsi");
            assert_eq!(indep.frame_bytes, 1536);
            assert_eq!(indep.dialnorm, 22);
            assert_eq!(indep.compr.expect("indep compr").raw(), 0x77);
            assert_eq!(indep.dmixmod, 2);
            assert_eq!(indep.bsmod, Some(2));
            let dep = crate::eac3::bsi::parse(&stream[off + 1536 + 2..]).expect("dep bsi");
            assert_eq!(dep.frame_bytes, 768);
            assert_eq!(dep.dialnorm, 22);
            assert_eq!(dep.compr.expect("dep compr").raw(), 0x77);
            // Blocks absent on the dependent substream.
            assert_eq!(dep.dmixmod, 0xFF);
            assert!(dep.annex_e_mix_levels.is_none());
            assert!(dep.pgmscl.is_none());
            assert_eq!(dep.bsmod, None);
            assert!(dep.copyright_info.is_none());
        }
    }

    /// The per-block §5.4.3.4 dynrng word is applied by the E-AC-3
    /// decode path's mandatory §7.7.1 line-out gain: −12.04 dB word ⇒
    /// 0.25× output.
    #[test]
    fn metadata_dynrng_scales_eac3_decode() {
        let word = 0xC0u8;
        let expected = crate::drc::dynrng_to_linear(word) as f64; // 0.25
        let pcm = build_sine_pcm(2, 4);
        let fb = 768usize;
        let plain = decode_all(&encode_meta(&pcm, 2, 192_000, None, &[]), fb);
        let cut = decode_all(
            &encode_meta(
                &pcm,
                2,
                192_000,
                Some(Eac3Metadata {
                    dynrng: Some(word),
                    ..Eac3Metadata::default()
                }),
                &[],
            ),
            fb,
        );
        assert_eq!(plain.len(), cut.len());
        let rms = |v: &[i16]| -> f64 {
            let n = v.len().max(1);
            (v.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / n as f64).sqrt()
        };
        let skip = 2 * SAMPLES_PER_BLOCK * 2;
        let ratio = rms(&cut[skip..]) / rms(&plain[skip..]).max(1e-9);
        let delta_db = 20.0 * (ratio / expected).log10().abs();
        assert!(
            delta_db < 0.5,
            "eac3 decoded dynrng gain {ratio:.4} vs authored {expected:.4} \
             (off by {delta_db:.2} dB)"
        );
    }

    /// Registry `options` and the typed constructor must build the
    /// same encoder — pinned byte-identical.
    #[test]
    fn metadata_options_build_identical_encoder() {
        let pcm = build_sine_pcm(6, 2);
        let typed = encode_meta(&pcm, 6, 384_000, Some(full_meta()), &[]);
        let from_options = encode_meta(
            &pcm,
            6,
            384_000,
            None,
            &[
                ("dialnorm", "22"),
                ("compr", "0xB3"),
                ("dmixmod", "2"),
                ("ltrtcmixlev", "5"),
                ("lorocmixlev", "4"),
                ("ltrtsurmixlev", "6"),
                ("lorosurmixlev", "5"),
                ("lfemixlevcod", "15"),
                ("pgmscl", "51"),
                ("extpgmscl", "45"),
                ("bsmod", "2"),
                ("copyright", "1"),
                ("origbs", "false"),
                ("dsurexmod", "2"),
                ("mixlevel", "18"),
                ("roomtyp", "1"),
                ("adconvtyp", "true"),
            ],
        );
        assert_eq!(
            typed, from_options,
            "options-driven and typed metadata constructors must emit identical bytes"
        );
    }

    /// Out-of-range codepoints are rejected at construction.
    #[test]
    fn metadata_validation_rejects_out_of_range() {
        let mut params = CodecParameters::audio(CodecId::new(CODEC_ID_STR));
        params.sample_rate = Some(48_000);
        params.channels = Some(2);
        let bad_metas = [
            Eac3Metadata {
                dialnorm: 0,
                ..Eac3Metadata::default()
            },
            Eac3Metadata {
                mixmd: Some(Eac3MixMetadata {
                    dmixmod: 3,
                    ..Eac3MixMetadata::default()
                }),
                ..Eac3Metadata::default()
            },
            Eac3Metadata {
                mixmd: Some(Eac3MixMetadata {
                    ltrtsurmixlev: 2, // 0..=2 reserved
                    ..Eac3MixMetadata::default()
                }),
                ..Eac3Metadata::default()
            },
            Eac3Metadata {
                mixmd: Some(Eac3MixMetadata {
                    pgmscl: Some(64),
                    ..Eac3MixMetadata::default()
                }),
                ..Eac3Metadata::default()
            },
            Eac3Metadata {
                infomd: Some(Eac3InfoMetadata {
                    dsurmod: 3,
                    ..Eac3InfoMetadata::default()
                }),
                ..Eac3Metadata::default()
            },
            Eac3Metadata {
                infomd: Some(Eac3InfoMetadata {
                    audprod: Some(Eac3AudioProduction {
                        mixlevel: 0,
                        roomtyp: 3,
                        adconvtyp: false,
                    }),
                    ..Eac3InfoMetadata::default()
                }),
                ..Eac3Metadata::default()
            },
        ];
        for bad in bad_metas {
            assert!(
                make_encoder_with_metadata(&params, bad).is_err(),
                "expected rejection: {bad:?}"
            );
        }
        // Options-path rejection.
        params.options = oxideav_core::CodecOptions::new().set("dmixmod", "3");
        assert!(make_encoder(&params).is_err());
    }

    /// Decode an E-AC-3 elementary stream through the external decoder
    /// binary, returning the s16le samples. `None` when the binary is
    /// unavailable.
    fn ffmpeg_decode(stream: &[u8], tag: &str, dec_args: &[&str]) -> Option<Vec<i16>> {
        use std::process::Command;
        let in_path = std::env::temp_dir().join(format!("oxideav_eac3_meta_{tag}.ec3"));
        let out_path = std::env::temp_dir().join(format!("oxideav_eac3_meta_{tag}.pcm"));
        std::fs::write(&in_path, stream).expect("write ec3");
        let _ = std::fs::remove_file(&out_path);
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-f", "eac3"]);
        cmd.args(dec_args);
        cmd.arg("-i").arg(&in_path);
        cmd.args(["-f", "s16le", "-acodec", "pcm_s16le"]);
        cmd.arg(&out_path);
        let status = cmd.status();
        let _ = std::fs::remove_file(&in_path);
        let Ok(status) = status else {
            eprintln!("ffmpeg unavailable — skipping eac3 metadata black-box gate");
            return None;
        };
        assert!(status.success(), "ffmpeg failed to decode ({tag})");
        let bytes = std::fs::read(&out_path).expect("ffmpeg output");
        let _ = std::fs::remove_file(&out_path);
        Some(
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect(),
        )
    }

    fn interior_rms(samples: &[i16], channels: usize) -> f64 {
        let skip = (SAMPLES_PER_FRAME as usize * channels).min(samples.len() / 2);
        let tail = &samples[skip..];
        (tail.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / tail.len().max(1) as f64)
            .sqrt()
    }

    /// Black-box cross-validation of the metadata emission syntax and
    /// the eac3-path dynrng semantics:
    ///
    /// 1. The external decoder binary accepts a 5.1 stream carrying the
    ///    full mixing + informational blocks and decodes it at the same
    ///    level as the block-less encode of the same PCM — a single
    ///    misaligned bit anywhere in the Table E1.2 walk would corrupt
    ///    every downstream field and collapse the decode.
    /// 2. The per-block dynrng word decodes at exactly its Table 7.29
    ///    gain (DRC on vs off).
    #[test]
    fn metadata_black_box_syntax_and_dynrng() {
        let pcm = build_sine_pcm(6, 4);
        let plain = encode_meta(&pcm, 6, 384_000, Some(Eac3Metadata::default()), &[]);
        let meta = encode_meta(&pcm, 6, 384_000, Some(full_meta()), &[]);
        let Some(dec_plain) = ffmpeg_decode(&plain, "plain", &["-drc_scale", "0"]) else {
            return;
        };
        let dec_meta = ffmpeg_decode(&meta, "blocks", &["-drc_scale", "0"]).unwrap();
        assert_eq!(
            dec_plain.len(),
            dec_meta.len(),
            "metadata blocks changed the decoded sample count"
        );
        let rms_plain = interior_rms(&dec_plain, 6);
        let rms_meta = interior_rms(&dec_meta, 6);
        assert!(rms_plain > 100.0, "plain decode suspiciously quiet");
        let level_db = 20.0 * (rms_meta / rms_plain).log10().abs();
        assert!(
            level_db < 0.5,
            "metadata-bearing stream decodes {level_db:.2} dB off the plain encode"
        );

        // dynrng word gain, eac3 path.
        let word = 0xC0u8;
        let dyn_stream = encode_meta(
            &pcm,
            6,
            384_000,
            Some(Eac3Metadata {
                dynrng: Some(word),
                ..Eac3Metadata::default()
            }),
            &[],
        );
        let off = ffmpeg_decode(&dyn_stream, "dyn0", &["-drc_scale", "0"]).unwrap();
        let on = ffmpeg_decode(&dyn_stream, "dyn1", &["-drc_scale", "1"]).unwrap();
        let gain = interior_rms(&on, 6) / interior_rms(&off, 6).max(1e-9);
        let expected = crate::drc::dynrng_to_linear(word) as f64;
        let err_db = 20.0 * (gain / expected).log10().abs();
        assert!(
            err_db < 0.5,
            "black-box eac3 dynrng gain {gain:.4} vs authored {expected:.4} \
             (off by {err_db:.2} dB)"
        );
        eprintln!(
            "black-box eac3 metadata: block-bearing level delta {level_db:.3} dB, \
             dynrng gain {gain:.4} (authored {expected:.4}, delta {err_db:.3} dB)"
        );
    }
}
