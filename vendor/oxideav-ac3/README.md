# oxideav-ac3

[![CI](https://github.com/OxideAV/oxideav-ac3/actions/workflows/ci.yml/badge.svg)](https://github.com/OxideAV/oxideav-ac3/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/oxideav-ac3.svg)](https://crates.io/crates/oxideav-ac3) [![docs.rs](https://docs.rs/oxideav-ac3/badge.svg)](https://docs.rs/oxideav-ac3) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Pure-Rust **AC-3 (Dolby Digital)** + **E-AC-3 (Enhanced AC-3 / Dolby
Digital Plus)** audio decoder + encoder — elementary streams per
ATSC A/52:2018 (= ETSI TS 102 366). Zero C dependencies.

Part of the [oxideav](https://github.com/OxideAV/oxideav-workspace)
framework but usable standalone.

## Architecture

The pipeline follows the spec's natural ordering; each module owns one
slice of §5..§7 (base AC-3) or §E (E-AC-3):

1. [`syncinfo`] — sync word 0x0B77, crc1, fscod, frmsizecod,
   frame-length lookup (§5.3.1 / §5.4.1 / Table 5.18).
2. [`bsi`] — Bit Stream Information: bsid, bsmod, acmod → channel layout
   + lfeon + dialnorm + the optional timecode / Annex D alternate-syntax
   metadata blocks (§5.4.2).
3. [`audblk`] — per-block exponent decode (§7.1), parametric bit
   allocation (§7.2 with §7.2.2.6 delta-bit-allocation), mantissa decode
   (§7.3), channel coupling (§7.4), rematrixing (§7.5), dynamic-range
   compression (§7.7).
4. [`imdct`] + [`mdct`] — §7.9.4 FFT-backed 512-point IMDCT and
   256-point short-block pair, plus the forward transforms the encoder
   uses.
5. [`downmix`] — §7.8 LoRo + §7.8.2 LtRt downmix matrices for every
   source acmod, with Annex D / E-AC-3 mix-level extension routing.
6. [`wave_order`] — channel reorder for front-centre-bearing layouts
   (`acmod ∈ {3, 5, 7}`).
7. [`encoder`] — base AC-3 encoder.
8. [`eac3`] — Annex E decoder + encoder, including opt-in JOC/OAMD
   object reconstruction and rendering to a selectable speaker layout
   (stereo … 9.1.6).
9. [`crc`] — §7.10.1 CRC-16 over poly 0x8005, shared between the encoder
   and the opt-in `decoder::verify_packet_crc` residue check.
10. [`drc`] — §6.1.9 / §7.6 / §7.7 dynamic-range-control + dialogue-
    normalisation control surface (`DrcSettings`: partial-compression
    cut/boost, heavy-compression "RF mode", dialnorm playback target).

## Capabilities

### AC-3 decoder

- Sync frame + BSI parse (§5.3 / §5.4). All §5.4.2 metadata words —
  bit-stream mode, compression gain, dialogue normalisation, mix
  levels, Dolby Surround mode, timecodes, copyright/original flags,
  language code, audio-production info, the Annex D alternate-syntax
  informational blocks, and the `addbsi` trailer — are parsed and
  surfaced as typed accessors (advisory metadata; the PCM path is
  unchanged).
- Audio-block parse (§5.4.3), exponent decode (§7.1) + parametric bit
  allocation (§7.2), mantissa decode (§7.3) with bap=0 dither (§7.3.4),
  delta bit allocation (§7.2.2.6).
- IMDCT synthesis (§7.9) — both 512-point long-block and 256-point
  short-block paths.
- Channel coupling (§7.4) + rematrix (§7.5) + dynrng (§7.7).
- **Dynamic-range-control + dialogue-normalisation control surface**
  (§6.1.9 / §7.6 / §7.7, [`drc`]). The mandatory §7.7.1 full-`dynrng`
  decode is the default ("line out"); a listener-facing
  [`DrcSettings`] steers the §7.7.1.2 *partial-compression* cut/boost
  factors (apply a fraction of each gain reduction / increase, with
  independent directions), §7.7.2 *heavy compression* ("RF mode" —
  substitutes the BSI `compr` word, ±48 dB, falling back to `dynrng`
  when a frame carries no `compr` per §7.7.2.1), and §7.6 dialogue
  normalisation (an opt-in playback scalar `10^((target − dialnorm)/20)`
  toward a chosen headroom target). Build a configured decoder with
  `decoder::make_decoder_with_drc(params, DrcSettings)`. Applies to both
  the AC-3 and E-AC-3 paths. The resolved block gain scales every
  reproduced channel including the LFE (§5.4.3.4 / §6.1.9; only 1+1
  mode splits the program into `dynrng` / `dynrng2`), pinned by
  `tests/dynrng_lfe.rs` against Table 7.29.
- Downmix (§7.8) — LoRo and LtRt 2-channel.
- Bitstream → WAV channel reorder for multichannel layouts.
- Output format: interleaved S16LE by default; a decoder built with
  `CodecParameters::sample_format = Some(SampleFormat::F32)` emits the
  same channels as interleaved little-endian f32, unquantised (the S16
  output is exactly that f32 output truncated to 16 bits). Both the
  AC-3 and E-AC-3 paths (passthrough, downmix, JOC stereo) honour it.

### AC-3 encoder

- Multichannel encode — 1/0, 2/0, 2/0+LFE (2.1), 3/0, 2/2, 3/2, 3/2.1
  (5.1) and other acmod layouts, with a per-channel exponent refresh
  cadence + D15/D25/D45 strategy elected by measured bit cost
  (§7.1.3 — every anchor set bounds the blocks that reuse it), 5-fbw
  channel coupling within the §5.4.3.12 narrow-coupling validity
  envelope, a §8.2.2 transient detector (4th-order Butterworth 8 kHz
  split; all fbw channels switch together, dither defeated on the
  switched block and the next per §8.2.9), all seven LFE bins coded
  (§7.1.3 `lfeendmant = 7`), per-channel `fsnroffst[ch]` tuning
  (§5.4.3.40), per-block SNR-offset bit-pool redistribution, the
  §7.2.2.1.1 all-zero-offset special case mirrored in the allocator,
  and §7.10.1 dual-CRC emission.
- A/B knobs for the two encoder tools whose contribution the suite
  measures: options `cpl = 0` (no §7.4 coupling) and `perblock_snr = 0`
  (flat frame-level SNR offsets), read once at construction; the
  `AC3_DISABLE_CPL` / `AC3_DISABLE_PERBLOCK_SNR` environment variables
  remain as the legacy switch.
- **Bitstream-metadata surface** (`encoder::MetadataParams` /
  `make_encoder_with_metadata`, or the registry options `dialnorm`,
  `compr`, `dynrng`, `bsmod`, `cmixlev`, `surmixlev`, `dsurmod`,
  `langcod`, `mixlevel`+`roomtyp`, `copyright`, `origbs`): every
  §5.4.2 BSI advisory word plus the §5.4.3.3-4 per-block `dynrng`
  dynamic-range word. Round-tripped through the typed `bsi::parse`
  surface and black-box validated: the external decoder binary
  reproduces the authored `dynrng` / `compr` gains exactly
  (Δ0.000 dB at −12 dB words) and a 10 dB `dialnorm` delta measures
  −10.03 dB under target-level normalisation.

### E-AC-3 (Annex E)

- Decoder — BSI, audfrm (Tables E1.2 / E1.3), audblk DSP, the §3.4
  Adaptive Hybrid Transform on fbw / LFE / coupling channels, §3.6
  spectral extension with the §3.6.4.2.3 SPXATTEN border notch, and
  §3.7.2 transient pre-noise processing. All three §2.3.2.3 SNR-offset
  strategies decode: the frame-level `snroffststr == 0` pair plus the
  §2.3.3.27 per-block modes `0x1` (one shared `blkfsnroffst`) and `0x2`
  (independent per-coupling/channel/LFE fine offsets), each gated by the
  per-block `snroffste` reuse flag. Enhanced coupling
  (`ecplinu == 1`, §E.2.3.3.16-26 / §E.3.5.5) decodes end-to-end: the
  audblk parser reads the strategy + per-channel amplitude/angle/chaos
  coordinates, decodes the enhanced-coupling channel through the shared
  exponent / bit-allocation / mantissa path, and a deferred second pass
  reconstructs the non-aliased complex carrier `Z[k]` from the
  previous / current / next blocks (§E.3.5.5.1), processes the per-bin
  amplitudes + de-correlated angles, and emits each coupled channel's
  transform coefficients via the §E.3.5.5.4 complex product — replacing
  the standard §7.4 decouple. Block 0's "previous block" carrier source
  is threaded across the frame boundary from the prior frame's last
  enhanced-coupling block (carried on `EcplState`, §E.3.5.5.1), and the
  last block's "next block" is the first block of the following
  syncframe: the decoder parses that block ahead (on a scratch state,
  not rendered) so the coupling channel of every block is free of
  time-domain aliasing — see the look-ahead note under "Byte-stream
  input". Three
  enhanced-coupling conformance defects fixed in r406, pinned by the
  new encoder round-trips: `chincpl[ch]` is read directly after
  `ecplinu` (BEFORE the standard/enhanced strategy split — the prior
  order desynced every multichannel ecpl frame, invisibly in 2/0 where
  the flags are implicit); `ecplparam1e/2e == 0` now REUSE the
  previously transmitted amplitudes / angle+chaos values per
  §2.3.3.21-22 (previously each block's coordinate set was replaced
  wholesale, silencing every band of a reusing channel); and the
  resolved banding structure masks its entries up to and including
  `max(ecpl_begin_subbnd, 8)` per §E.2.3.3.19 — the Table E2.14
  default carries a merge bit at sub-band 9, so a default-banded
  region beginning there previously made `necplbnd` disagree with the
  §E.3.5.5.1 band walk by one band (a coordinate-count desync). The
  §E.3.3.2 `nrematbd` derivation now folds in enhanced coupling: a 2/0
  `ecplinu` block sizes its rematrix-flag field from the raw `ecplbegf`
  code (0/1/2/<5 → 0/1/2/3 bands, else 4) rather than `cplbegf`, keeping
  the bit cursor aligned on enhanced-coupling 2/0 frames. Standard coupling
  now applies the §E.2.3.3.15 **default coupling banding structure**
  (`defcplbndstrc[]`, Table E2.12, indexed by absolute sub-band) when
  `cplbndstrce == 0` in a frame's first coupling block, instead of leaving
  every sub-band un-merged — the prior all-zeros behaviour collapsed a
  7-subband region to 7 bands instead of 3, corrupting the §7.4
  coupling-coordinate scatter on every basic stereo-coupled frame. Three
  corpus stereo fixtures (`eac3-stereo-48000-192kbps`, `eac3-256-coeff-block`,
  `eac3-from-ac3-bitstream-recombination`) jumped from ~8-14 dB to ~91 dB
  PSNR and are now CI-gated at an 80 dB floor (`Tier::MinPsnr` in
  `tests/docs_corpus.rs`). Dependent-substream channel combination follows
  the §E.3.8.2 replace-or-extend rule: each dep coded channel is routed by
  its Table E2.5 location (or natural `acmod` order when `chanmape == 0`),
  *replacing* the matching independent-substream channel in place when the
  location is shared (e.g. a dep substream re-coding Center / LFE, or L/R
  via a custom `chanmap`) and *extending* the output only for genuinely new
  locations — so a real greater-than-5.1 broadcast program reassembles
  spatially correctly rather than duplicating and decorrelating the shared
  channels a blind append would have appended.
  Real-broadcast decode conformance (issue #13): three audblk/audfrm
  bit-accounting defects behind the reported "uncorrelated output +
  second-long dropouts" class are fixed — the Table E1.4 NEGATIVE
  derived `cplendf` when SPX and standard coupling are co-active with
  `spxbegf < 2` (formerly clamped at 0, over-reading up to two coupling
  sub-bands and desyncing the rest of the block), the §2.3.2.27
  `blkstrtinfo` width (16-bit words through ceiling-log2, formerly
  frame bits through floor-log2+1), and the per-block `fgaincod`
  default reset. The class is pinned by hand-written spec-conformant
  syncframes (SPX + coupling co-active) that ffmpeg accepts and both
  decoders agree on at ~92 dB, by our encoder now emitting that
  geometry (standard coupling, below), by a decode-side ffmpeg conformance harness
  (`tests/eac3_decode_ffmpeg.rs`: ffmpeg-generated streams across
  rates/modes/coupling/metadata + our-encoder broadcast BSI shapes,
  ≥ 50 dB per channel), and by an env-gated local-vector gate
  (`OXIDEAV_AC3_LOCAL_EAC3=/path` runs the same PSNR gate on captures
  that cannot be committed). Zero-fill fallbacks are now countable via
  `Eac3DecoderState::frames_zero_filled`, so callers can tell decoder
  dropouts from silent program audio. The harness also carries a
  **broadcast-shape matrix** (`crafted_broadcast_matrix_*`): every
  coding tool our encoder can switch on (plain / SPX / AHT / TPNP /
  SPX + TPNP + full metadata / AHT + mixing metadata) at 48 / 44.1 /
  32 kHz on stereo 192 kb/s and 5.1 448 kb/s six-block frames plus the
  1/2/3-block fractional shapes — 40 cells per rate, ffmpeg within
  57–93 dB of our decode, zero zero-filled frames. The matrix is drawn
  where the oracle is meaningful: SPX runs with `spxblnd = 31` because
  the §E.3.6.4.2 noise component is a decoder-private generator (blend 0
  → 20–30 dB between decoders with identical band spectra, blend 31 →
  68–77 dB), the per-channel budget keeps the §7.3.4 dither floor
  (bap = 0 bins, likewise decoder-private noise) out of the gate, and
  enhanced coupling / dependent substreams are excluded because the
  oracle does not render them (ecpl: −512-sample lag at 12 dB; the 7.1
  indep + dep pair: it drops the indep Ls/Rs and leaves one output
  channel silent). The matrix's first catch was the §7.7.1 `dynrng`
  LFE omission below. With the encoder's standard coupling (`cpl`)
  the matrix adds `cpl` and `cpl+spx` cells, and two sweeps walk the
  broadcast geometry exhaustively: every `spxbegf` code 0..7 with
  coupling co-active (derived `cplendf` −2..7, default / widest /
  single-sub-band regions, stereo 128 kb/s and 5.1 384 kb/s with
  `compr` + `dynrng`: 59–92 dB per channel vs ffmpeg) and seven
  transmitted `cplbegf`/`cplendf` regions × four rate/mode/sample-rate
  shapes. The second catch: zero-bit (bap = 0) coupling-channel
  mantissas were decoded as true zeros in every coupled channel.
  §7.3.4 applies dither *after* each channel is extracted from the
  coupling channel, per channel, gated by that channel's `dithflag` —
  so coupled high bands at low rates (the broadcast norm) came out
  as spectral holes, 27–39 dB below the reference decoder's band
  power on a near-threshold fixture; they now match it within 0.5 dB
  (`coupled_zero_bit_dither_level_matches_ffmpeg`, AC-3 and E-AC-3).
  Every PSNR gate in the harness is now **noise-ceiling aware**: the
  §7.3.4 dither and §E.3.6.4.2 SPX noise sequences are left to the
  decoder, so the stream is decoded twice by us under different noise
  seeds (`Eac3DecoderState::reseed_noise`) and the ours-vs-ours PSNR is
  the agreement any two conformant decoders can reach on it; the fixed
  floor is lowered only to 4 dB under that ceiling. Measured, the
  ours-vs-ffmpeg PSNR tracks the ceiling cell for cell (dense-noise
  192 kb/s: 44.28 vs 44.26 dB; SPX `spxbegf = 0`: 59.97 vs 60.05 dB),
  i.e. the residual against the reference decoder is decoder-private
  noise only; with `dither = 0` and no SPX blending every cell sits at
  ~90.7 dB (s16 rounding). Each gate also checks every 1536-sample
  segment for localised dropouts (power ≥ reference − 6 dB wherever the
  reference is above −50 dBFS). The env-gated local-vector gate runs
  the same checks, so a broadcast capture with SPX noise blending and
  dither — the norm — is judged against its own noise ceiling instead
  of an unreachable 50 dB.
  A third leg feeds the **foreign encoder programme-like material**
  (`generated_program_matrix_*`): harmonic music over a noise bed,
  syllable-gated speech-like noise and sparse click / tone-burst
  transients, every channel with its own pitch and noise generator, as
  mono / stereo / 5.1 at 32 / 44.1 / 48 kHz from starved to transparent
  rates (mono 32–128, stereo 48–384, 5.1 128–768 kb/s) plus the
  encoder's earliest and latest coupling start — 135 streams, 459
  channels. Every stream decodes with zero zero-filled frames and no
  segment dropout; 320 channels sit at ≥ 50 dB against the reference
  decoder (up to 93.0 dB), and the 139 below it are dither-limited:
  they track their own decoder-private noise ceiling at 0.11 dB mean,
  0.68 dB rms, 3.2 dB worst (lowest absolute figure 34.7 dB with a
  34.8 dB ceiling, on a 128 kb/s mono speech cell where most bins are
  zero-bit). No real broadcast capture is available to the repository
  (the staged corpus is synthetic and the reporter's stream is not
  redistributable), so this matrix and the env-gated local-vector gate
  stand in for one. The matrix's one finding was in the decoder-private
  noise itself: the §7.3.4 dither generator shifted a 16-bit register
  one bit per value and returned the whole register, so neighbouring
  transform coefficients received values correlated at 0.5 and a
  sparsely-set register produced runs of near-identical full-scale
  values (measured on a transient cell: eight adjacent zero-bit bins of
  one block all near −0.7, 4.7 dB above nominal dither power and 8 dB
  above the reference decoder in that block). The generator is now a
  full-step 32-bit xorshift with a multiplicative output mix — values
  uniform in ±0.707, uncorrelated at every lag, steady from block to
  block (pinned by `dither_values_are_uniform_and_uncorrelated`).
- **Opt-in JOC object presentation** parses EC-3 Extension Type A
  and EMDF metadata from declared E-AC-3 skip fields, reconstructs up to
  16 object signals with the TS 103 420 sparse/QMF pipeline, applies
  dynamic OAMD updates, and renders the objects to a **caller-selected
  speaker layout** — see "JOC object rendering" below. This is a
  standards-derived speaker renderer, not Dolby's proprietary
  binaural/headphone renderer; encoding JOC is outside its scope.
- Encoder — independent + dependent substream pairs for 1.0 / 2.0 / 5.1
  / 7.1 layouts, with adaptive / frame-based exponent strategies,
  fractional syncframes (§E.2.3.1.5 — see below) and the §3.7
  transient-pre-noise-processing emission (see below).
  **Bitstream-metadata surface** (`eac3::Eac3Metadata` /
  `eac3::make_encoder_with_metadata` + registry options): fixed-BSI
  `dialnorm` and `compr`, the per-block `dynrng` word (every block of
  every substream), the Table E1.2 **mixing-metadata block**
  (`dmixmod`, LtRt/LoRo centre + surround mix levels, `lfemixlevcod`,
  `pgmscl`, `extpgmscl`) and the §E.2.3.1.62+ **informational block**
  (`bsmod`, copyright/original, 2/0 `dsurmod`+`dheadphonmod`,
  ≥6-channel `dsurexmod`, audio-production info, `sourcefscod`) —
  emitted on the independent substream (a 7.1 pair's dependent
  substream keeps the blocks absent while sharing dialnorm/compr).
  Round-tripped through the typed Annex E `bsi::parse` surface (which
  now also surfaces `bsmod`); black-box validated: the external
  decoder binary accepts the block-bearing syntax (decode level
  within 0.004 dB of a block-less encode) and reproduces the authored
  `dynrng` gain exactly. Metadata-bearing AC-3 and E-AC-3 streams are
  swept through the corruption families in `tests/robustness.rs`.
  **Spectral extension is now available on the encoder side**
  (`eac3::make_encoder_with_spx(params, SpxParams)`, §E.2.3.3 / §E.3.6):
  every fbw channel is coded only up to the SPX begin frequency
  (default tc# 109 ≈ 10.2 kHz at 48 kHz) and the decoder regenerates
  the extension region (default up to tc# 229 ≈ 21.5 kHz) from a
  translated copy of the channel's own low band, noise-blended and
  scaled by per-band coordinates the encoder derives from the
  §3.6.4.3 energy-matching rule (`spxco = rms(original HF band) /
  (rms(translated band)·32)`, quantised through the §E.2.3.3.11-13
  exponent/mantissa/master-coordinate forms). Coordinates refresh on
  the exponent anchor blocks and are reused between (`spxcoe`);
  geometry (begin/end/copy-start codes, noise blend, default
  Table E2.11 vs explicit band structure) is configurable via
  `SpxParams`; the freed high-frequency bits are re-spent on the coded
  low band by the SNR-offset tuner. Optional extras: §3.6.4.2.3
  **attenuation** (`spxattene`/`chinspxatten`/`spxattencod` in audfrm,
  with the border/wrap notch folded into the encoder's coordinate
  computation so band energies still match) and **adaptive copy-start**
  (per-frame `spxstrtf` re-selection that scores every candidate by
  coordinate saturation — a spectrum with a hole above the first copy
  sub-band would otherwise pin a band's coordinate at the 0.875
  ceiling). Validated three ways: in-tree round-trips gate per-SPX-band
  decoded energy within ±3 dB of the original (mono / stereo / 5.1 /
  narrow non-default geometry), default-vs-explicit band structure
  decodes bit-identically, and an external decoder binary
  cross-validates the emitted syntax + banded energies (±4 dB) for the
  plain, mono, and attenuated variants. SPX-encoded streams are also
  swept through the truncation / bit-flip / garbage corruption
  families. Both entry points build the same encoder: the typed
  `make_encoder_with_spx` and the registry path via
  `CodecParameters::options` `spx*` keys (`spx`, `spx_begf`/`endf`/
  `strtf`/`blnd`, `spx_atten`, `spx_adaptive_copy_start`,
  `spx_explicit_band_structure`) — pinned byte-identical. Stationary
  coordinate refreshes are thrifted (`spxcoe = 0`) with a mid-frame
  level-step gate proving per-span refresh still tracks moving spectra.
  **Mixed per-channel membership** (§E.2.3.3.3) is supported via
  `SpxParams::channel_mask` / the `spx_chmask` option: excluded
  channels emit `chinspx[ch] = 0`, keep their `chbwcod`, and are
  waveform-coded to full bandwidth while member channels stop at the
  SPX begin frequency — the SNR tuner budgets every channel at its own
  coded bandwidth (per-channel `end_mant` plumbed through
  `tune_snroffst_with_plan_ends` / `overhead_bits_for_ends` /
  `mantissa_bits_total_ends`). The mixed split is validated in-tree
  (SPX band-energy contract on the member channel AND full-bandwidth
  HF fidelity on the excluded one) and through the external decoder.

  **The Adaptive Hybrid Transform is now on the encoder side too**
  (`eac3::make_encoder_with_aht(params)` / the `aht` option, §3.4):
  every fbw channel — and the LFE (`lfeahtinu`) — moves to a single
  block-0 exponent anchor (`nchregs[ch] == nlferegs == 1`, per-bin
  6-block-max exponents), long transforms are forced, and block 0
  front-loads the §3.4.4 mantissa stream: `chgaqmod` + gain words +
  per-bin codewords against the §3.4.3.1 `hebap[]` (the shared
  psd/excitation/mask pipeline with the Table E3.1 pointer table).
  Quantiser stack per Tables E3.2/E3.5/E3.6: minimum-Euclidean-
  distance VQ over Tables E4.1..E4.7 for `hebap` 1..7, and
  gain-adaptive quantisation for `hebap` >= 8 — the per-channel
  `gaqmod` (all four Table E3.3 modes, incl. the 5-bit composite gain
  triplets) is chosen by exact bit accounting, with per-bin Gk in
  {1, 2, 4} splitting short small-mantissa codewords from
  tag + dead-zone large escapes. The SNR-offset tuner costs AHT
  channels by their exact front-loaded payload and binary-searches
  the monotone `csnroffst·16 + fsnroffst` axis. Measured on a
  stationary stereo two-tone (in-tree decode): 42.0 dB @ 96 kbps
  rising to 75.6 dB @ 448 kbps. (Those r390 numbers were read against
  a standard path that clipped every reuse block's mantissas and sat
  at ~24 dB; with the r457 exponent-sharing bound the standard path
  codes the same fixture 2-23 dB *above* AHT — AHT rate-distortion
  tuning is a recorded follow-up, see "Equal-rate position".)
  Black-box: mono / stereo / 5.1 AHT streams decode through an
  external decoder binary at 28.1 / 28.1 / 33.4 dB.
  `examples/eac3_rate_curves.rs` prints the full
  standard/AHT/SPX/enhanced-coupling rate ladder;
  `aht_quality_scales_with_rate` gates the curve shape in CI.

  **Standard coupling on the encoder side** (the `cpl`, `cpl_begf`,
  `cpl_endf` options; Annex E `ecplinu = 0`, §E.2.3.3.14-15 / §7.4):
  every fbw channel of the independent substream is coupled above
  `cplbegf` (default 8, bin 133) through one shared coupling channel
  (§7.4.1 channel average) and per-band `cplco` coordinates refreshed
  on blocks 0 and 3 (block 3 thrifted to `cplcoe = 0` when its codes
  repeat), Table E2.12 default banding (`cplbndstrce = 0`). It runs
  co-active with SPX — `cplendf` is then derived from `spxbegf`
  (Table E1.4, negative for `spxbegf < 2`) and not transmitted — and
  is exclusive with AHT / enhanced coupling (6-block frames only).
  The `dither = 0` option signals `dithflag = 0` on every block, for
  deterministic decodes in conformance comparisons.

  **Enhanced coupling is now on the encoder side too**
  (`eac3::make_encoder_with_ecpl(params, EcplParams)` / the `ecpl`,
  `ecpl_begf`, `ecpl_endf` options; §E.2.3.3.16-26 / §E.3.5.5) — the
  last Annex E encoder tool. Every fbw channel of the independent
  substream is coupled: below the begin frequency (default tc# 37 ≈
  3.5 kHz) channels are waveform-coded as usual; above it a single
  shared **carrier** channel is coded through the standard coupling-
  channel exponent / bit-allocation / mantissa path (cplexpstr anchors
  on blocks 0/3, `cplabsexp` + D15 groups, implicit first-block
  `cplleake` with zero leak inits, mantissas interleaved after the
  first coupled channel) and each coupled channel is rebuilt from it
  via per-band Table E3.10 amplitude + Table E3.11 angle coordinates
  (chaos 0, `ecpltrans` 0 — deterministic decode). The carrier is the
  first coupled channel's MDCT scaled per band ~3 dB above the loudest
  coupled channel — phase-locked to channel 0, whose angle is
  spec-fixed to 0 and never transmitted, with the margin letting the
  1.0 amplitude ceiling absorb band-level carrier coding loss.
  Coordinates are measured in the §E.3.5.5.1 complex analysis domain
  against the carrier the decoder will actually reconstruct (each bin
  through the final exponent + bap quantiser; the previous frame's
  carried quantised last block at the frame head, zero after the
  tail — the decoder now uses the next frame's first block there, so
  the last block's coordinates are measured against a slightly
  different carrier; closing that needs one block of encoder
  look-ahead and is a recorded follow-up), refreshed on blocks 0/3 with §2.3.3.21-22 reuse thrift when
  the block-3 refresh quantises identically. **Chaos coordinates**
  (Table E3.12, on by default via `EcplParams::chaos` / the
  `ecpl_chaos` option) are derived from the measured per-band
  coherence — the incoherent fraction maps onto the 8-step grid,
  engaging the decoder's §E.3.5.5.3 per-bin random de-correlation for
  content the shared carrier cannot represent, with the transmitted
  amplitude pre-divided by the decoder's `1 + 0.38·chaosval`
  modification (chaos backs off when the pre-compensated amplitude
  would exceed the 1.0 ceiling — band energy wins over width). A
  partial-coherence stereo fixture gates the effect: decoded
  in-region inter-channel coherence 0.914 chaos-less → 0.796 with
  chaos (source 0.706), band energies still matched. Validated in-tree:
  stereo band-energy round-trip (±3 dB per signal band, ±1.5 dB coded
  low band, 20 dB waveform floor), a stereo **quadrature** fixture
  (channel 1's tones 90° off the carrier — pins the angle path at an
  18 dB waveform floor; a broken angle path collapses to ~3 dB),
  5.1 with explicit `chincpl` bits, a 7.1 indep(coupled)+dep(plain)
  pair walk, registry-vs-typed byte-identical construction, and the
  corruption families in `tests/robustness.rs`. Measured interior
  PSNR 25.4-30.4 dB across channels. Black-box cross-validation is
  **not possible for this tool**: the external validator binary
  reports enhanced coupling as not implemented and mutes (probed
  r406) — our decoder is ahead of the validator here, so validation
  is round-trip + spec-text only. **SPX and enhanced coupling can be
  co-active** (`make_encoder_with_spx_ecpl` / options `spx=1` +
  `ecpl=1`) per §3.6.1: channels are waveform-coded below the
  coupling begin, carried by the shared carrier + coordinates from
  there to the SPX begin frequency (the coupling region is
  SPX-bounded — `ecplendf` is not transmitted, §E.2.3.3.17), and
  SPX-synthesized above it; a stereo three-region round-trip gates
  all three regions' energies (coupling bands ±3 dB, SPX bands
  ±3.5 dB, coded low band ±1.5 dB). AHT remains mutually exclusive
  with both.

  Three spec-fidelity notes from this work: (1) GAQ dequantisation now
  uses the literal Table E3.5/E3.6 characteristics — the `Gk = 2`
  large mantissa is an `(m-1)`-bit codeword (2^(m-1) output points),
  and all scalar quantisers apply the exact Q15 `y = x + ax + b`
  remap. (2) The §3.4.5 IDCT's printed leading constant `2` measures
  as `√2` against an independent production decoder (a pure-DC and a
  40 Hz-modulated fixture both fit `external = ours(2·Σ)/√2` with
  ~89 dB residual); with `√2` the DC basis weight is exactly 1, and
  both transforms follow the deployed constant. (3) §E.3.5.5.1's
  step-3 overlap-add omits the §7.9.4.1 step-6 headroom-restoring
  factor of 2 (step 2 references only "steps 1 to 5") — taken
  literally the analysis→synthesis chain returns exactly half the
  original coefficients, so every enhanced-coupling channel would
  decode 6 dB low and the loudest coupled channel would need the
  unrepresentable amplitude 2.0; the Table E3.10 ceiling of exactly
  1.0 pins the intended identity at unity, and the factor of 2 is
  applied in the carrier reconstruction. All three notes are codified
  as clean-room errata entries (`docs/audio/ac3/ac3-errata.md` E2 /
  E1 / E3 respectively; E3 also records that the ETSI TS 102 366
  V1.4.1 copy omits the enhanced-coupling channel-processing clause
  entirely, so A/52:2018 is the operative text). The E3 correction is
  regression-pinned in both directions: a decode-side least-squares
  identity fit (corrected chain gain 1.0 vs exactly 0.5 as printed)
  and a full encode→decode bitstream round-trip gating the aggregate
  coupling-region energy within ±1.5 dB of unity (the as-printed
  reading sits at −6.02 dB). A real Dolby-encoded `ecplinu = 1`
  stream remains a recorded fixture GAP
  (`docs/audio/ac3/fixtures/eac3-ecpl-enhanced-coupling/GAP.md`), so
  ecpl validation stays in-tree round-trip + spec-text.

  **Fractional syncframes** (§E.2.3.1.5 `numblkscod` 0/1/2, Table
  E2.4 — `eac3::make_encoder_with_blocks` / the `blocks` option =
  `1`/`2`/`3`/`6`): a syncframe carries 1, 2, or 3 audio blocks with
  the byte budget scaled by nblks/6 (unchanged bit rate), the
  §E.2.3.1.64 `convsync` flag marking each 6-block AC-3
  conversion-group head (surfaced as the typed `Eac3Bsi::convsync`),
  Table E1.3's implicit `expstre = 1` / `ahte = 0` + explicit
  `convexpstre` audfrm arms, and `blkstrtinfoe` absent on 1-block
  frames. Works across every layout including the 7.1 indep+dep pair;
  AHT is spec-implicit-off and SPX / enhanced coupling stay 6-block
  scope. Guard rails: a construction-time minimum-rate floor (fixed
  syntax + D45 exponent payload + metadata reserve must fit the
  shrunken frame), budget-aware D45 anchor demotion, and a worst-case
  full-scale-noise dry-run through the real emission pipeline at
  construction. Round-trips walk every frame's BSI syntax (numblkscod
  / frmsiz / convsync cadence) and gate decode alignment against the
  6-block encode of the same PCM (r454 figures, taken before the r457
  exponent-sharing bound: ~24-28 dB for 3-block, ~24 dB 2-block,
  ~13 dB for the 1-block extreme — the honest overhead-amortisation
  cost of re-anchoring exponents every 256·nblks samples); the
  external decoder binary accepts all three shapes (r454: stereo
  3/2-block @ 192 kbps at 23.4 / 26.0 dB vs the same harness's
  22.3 dB 6-block baseline, 1-block @ 384 kbps at 62.0 dB). Implementing the round-trip flushed out a decoder
  conformance bug: the Annex E BSI parser read `convsync` / `blkid` /
  `frmsizecod` inside the informational-metadata block, but Table
  E1.2 places them OUTSIDE `if (infomdate)` — a fractional-frame
  stream without an infomd block desynced the BSI tail by 1 bit
  (fixed; `frmsizecod` is now also gated on `blkid`).

  **Transient pre-noise processing emission** (§3.7 / §2.3.2.21-23 —
  `eac3::make_encoder_with_tpnp` / the `tpnp` option): per frame and
  per fbw channel the encoder runs the §3.7.1 transient-location
  analysis (4-sample group-energy onset detector against a
  running-average reference) and emits `transproce` +
  `chintransproc[ch]` + `transprocloc[ch]` (frame-relative, 4-sample
  units) + `transproclen[ch]` — the time-scaling length, set to the
  transient's offset inside its coding block, which makes the
  decoder's §3.7.2 copy distance a constant two block hops (the
  precise length analysis is encoder tuning — Figure E3.2 is the
  spec's only sketch). Long transforms
  are forced while TPNP is on (the tool corrects long-transform
  pre-noise; block switching avoids it instead), and transient-free
  frames emit `transproce = 0`, byte-identical to the baseline
  encoder. Validated: audfrm parse-back pins loc (±4 groups) and len
  exactly on 6-block and fractional frames, options-vs-typed
  byte-identical, end-to-end decode through our own §3.7.2 synthesis,
  a TPNP corruption sweep, and the external decoder binary accepts
  the syntax (27.4 dB on impulse-train content).

### E-AC-3 decode: clause-by-clause checklist

Every Annex E syntax element and decode path, the clause that defines
it (ATSC A/52:2018 Annex E unless a main-body section is named), and
the test that exercises it. "Join" is `tests/eac3_frame_state.rs`:
the stream is decoded from its first frame and again from three later
frames on a fresh decoder, and the decodes must agree sample for
sample once the signal history is rebuilt. "Black box" names the
comparison against an external decoder binary (run when one is on
the machine, skipped otherwise); where there is none, the reason is
given.

| Element | Clause | Decoder behaviour | Tests | Black box |
| --- | --- | --- | --- | --- |
| Stream types 0 / 1 / 2, `substreamid` | §2.3.1.1-2, §3.8.1 | one program is decoded: the selected independent substream (default 0) and the dependent substreams that follow it; other programs are skipped | `tests/eac3_substreams.rs` (`default_selection_…`, `selected_program_…`, `dependent_substreams_stay_with_their_program`) | none — streams with a second program are built by re-labelling our encoder's frames |
| Dependent substreams, `chanmap` | §2.3.1.7-8, Table E2.5, §3.8.2 | up to eight per program, each with its own history; channels at a location the independent substream carries replace it, new locations extend the program (max 16) | `two_dependent_substreams_keep_separate_history`, `replacing_dependent_channels_reach_the_output`, `splice_*` unit tests | none — the external decoder does not combine dependent substreams |
| 5.1-or-fewer reproduction | §3.8.2 | a request for ≤ 6 channels decodes the independent substream only | `small_targets_decode_the_independent_substream_only` | — |
| Program dynamic range | §3.8.5 | with dependent substreams reproduced, the last one's `dynrng` / `compr` govern every substream | `program_dynamic_range_words_come_from_the_dependent_substream` | — |
| 1 / 2 / 3 / 6 blocks per syncframe | §2.3.1.5, Tables E2.4, E1.3 | `expstre = 1` / `ahte = 0` implicit below six blocks, `blkstrtinfoe` absent on one-block frames, `convsync` / `blkid` outside the informational block | Join (1, 2, 3 blocks); `fractional_frames_*` unit tests | `crafted_broadcast_matrix_*` (1/2/3-block cells, 57–93 dB) |
| Exponent strategy per block (`expstre = 1`) | §2.3.2.1, Table E1.3 | `chexpstr` / `cplexpstr` / `lfeexpstr` read in audfrm, indexed by block | every encoder-made stream | whole decode-vs-reference harness |
| Frame-based exponent strategy (`expstre = 0`) | §2.3.2.12-13, Table E2.10 | 5-bit row per channel (and for the coupling channel when any block couples) expanded to six strategies | `frame_based_exponent_strategy_expands_table_e2_10`; all seven staged E-AC-3 fixtures use it | `tests/docs_corpus.rs` (65–91 dB against the reference PCM) |
| Converter exponent strategy, converter SNR offset | §2.3.2.14-15, §2.3.3.29-30 | parsed and discarded (only an E-AC-3 → AC-3 converter uses them); present on Type 0 only, `convexpstre` explicit below six blocks | `converter_exponent_strategy_arms_on_fractional_frames` | fractional cells of the matrix above |
| Adaptive hybrid transform | §3.4, Tables E3.1-E3.6, E4.1-E4.7 | `nchregs` / `ncplregs` / `nlferegs` from the parsed strategies; full-bandwidth, coupling and LFE channels; VQ and all four GAQ modes | Join (stereo, 5.1); `aht_*` and `*_aht_*` unit tests | matrix `aht` cells, AHT + `compr` stereo cell |
| Standard coupling under Annex E | §2.3.3.15, Tables E2.12, E1.4, §3.3.1 | default banding in a frame's first coupling block, signed derived `cplendf` with SPX, first-coordinate and first-leak states | Join (stereo, 5.1, with SPX); `issue13_craft_tests` | geometry and region sweeps (59–92 dB) |
| Coupling across block switches | §7.4, §5.4.3.1 | a coupled channel's short-transform block takes the coupling channel's coefficients like any other; rematrix and dither follow per channel | Join ("standard coupling across block switches", "block switching 5.1" — the test checks the stream does switch) | programme matrix (transient material, both coupling extremes) |
| Enhanced coupling | §2.3.3.14-26, §3.5 | strategy, amplitude / angle / chaos with parameter reuse, §3.3.2 rematrix band count | Join; `ecpl` unit tests; encoder round-trips | none — the external decoder does not implement enhanced coupling |
| Enhanced coupling across the frame boundary | §3.5.5.1 | the carrier of a block is built from the previous, current and next block: the previous frame's last block is carried, and the next frame's first block is parsed ahead | `tests/eac3_ecpl_lookahead.rs` — the rotated channel's frame-boundary blocks measure 24.4 dB against the source with the next block (25.2 dB inside the frame), 15.1 dB without | none, as above |
| Spectral extension | §2.3.3.1-13, §3.6 | strategy and band structure (default in the first SPX block), coordinates with reuse, translation with wrap, noise blend, coordinate scaling | Join; `spx_*` unit tests; encoder round-trips | matrix `spx` cells (noise blend off: the noise sequence is decoder-private) |
| SPX attenuation | §2.3.2.24-25, §3.6.4.2.3, Table E3.14 | border filter at the region border and at wrap points, applied after the band RMS is measured | Join; `spx_noise_level_is_measured_before_the_border_filter`, `spx_border_filter_shapes_the_translated_signal` | attenuated-stream energy check (banded energies ± 4 dB) |
| SPX in the first block of a frame | Table E1.4 (`spxstre`, `firstspxcos`) | block 0 always carries the strategy and every channel's coordinates and blend: nothing is inherited from the previous frame | Join | matrix `spx` cells |
| Transient pre-noise processing | §2.3.2.21-23, §3.7 | pre-noise counted from the leading edge of the block before the transient's; synthesis buffer read across the frame boundary; transient beyond the frame corrects the tail | `tpnp_tests`; `transient_pre_noise_synthesis_copies_from_the_previous_frame`; Join (two frames to settle) | none — the external decoder ignores the data (its output matches ours with the tool disabled at 93 dB) |
| SNR offsets | §2.3.2.3, Table E2.9, §2.3.3.27 | strategies 1-3; `snroffste` reuse within the frame, block 0 always explicit | `snroffststr_per_block_roundtrip_*` | matrix (strategy 1) |
| Bit-allocation parameters | Table E1.4, §5.4.3.30-47, §8.2.12 | `bamode` / `baie`, fast-gain codes (reset to the default in every block without `fgaincode`), leak initialisation, delta bit allocation with reuse; the allocation of a channel is recomputed whenever its exponents or any of these change | Join; `allocation_is_reused_only_when_its_inputs_are_unchanged` | whole harness |
| State across syncframes | Table E1.3 tail, §5.4.3 | no parsed parameter crosses a frame boundary; signal history does — overlap tail, enhanced-coupling carrier, pre-noise synthesis source | Join (18 configurations × 3 join points) | — |
| Error detection and concealment | §3.2, §7.10 | CRC verified before decoding; a failing or undecodable frame is replaced by the last good block's overlap tail, then silence; the next frame fades in | `tests/stream_resilience.rs` | — |
| Reduced sample rates (`fscod2`) | §2.3.1.5, Table E2.3 | header parsed, six blocks, half-rate timing; **the bit-allocation hearing-threshold column for these rates is not stated in the staged text** — the 32 kHz column is used, unverified | header unit tests only | none — no encoder on hand emits these rates |

What a receiver sees on a broadcast stream follows from the last four
rows: tuning in mid-stream costs one block of fade-in (two frames
where transient pre-noise data refers back), a second audio service in
the same stream does not disturb the first, and a damaged frame costs
that frame.

### JOC object rendering (target speaker layout)

An E-AC-3 stream with joint object coding carries a 5.1 downmix plus
the side information to rebuild up to 16 audio objects and their
positions (ETSI TS 103 420). The decoder renders those objects to the
speaker layout you choose:

| `joc_layout` | [`JocLayout`] | channels, in output order |
| --- | --- | --- |
| `stereo` | `Stereo` | L, R |
| `5.1` | `Surround51` | L, R, C, LFE, Ls, Rs |
| `5.1.2` | `Surround512` | 5.1 + Tsl, Tsr |
| `5.1.4` | `Surround514` | 5.1 + Tfl, Tfr, Tbl, Tbr |
| `7.1` | `Surround71` | L, R, C, LFE, Ls, Rs, Lb, Rb |
| `7.1.2` | `Surround712` | 7.1 + Tsl, Tsr |
| `7.1.4` | `Surround714` | 7.1 + Tfl, Tfr, Tbl, Tbr |
| `9.1.6` | `Surround916` | 7.1 + Lw, Rw + Tfl, Tfr, Tsl, Tsr, Tbl, Tbr |

Select it on either construction path:

```rust
use oxideav_ac3::decoder::{Eac3DecoderOptions, make_eac3_decoder_with_options};
use oxideav_ac3::eac3::JocLayout;
use oxideav_core::{CodecId, CodecParameters};

// Direct API (typed options).
let params = CodecParameters::audio(CodecId::new("eac3"));
let options = Eac3DecoderOptions::new().with_joc_layout(JocLayout::Surround714);
let decoder = make_eac3_decoder_with_options(&params, &options)?;

// Registry path: the same option in the parameter bag.
let mut params = CodecParameters::audio(CodecId::new("eac3"));
params.options.insert("joc_layout", "7.1.4");
let decoder = registry.first_decoder(&params)?;
```

With a multichannel layout the decoder **always** outputs that layout
(`Decoder::output_params` reports the channel count and channel layout;
S16 or F32 as usual): frames with valid 5.X JOC/OAMD metadata are
rendered from the reconstructed objects, and every other frame — a
stream without object metadata, a malformed or unsupported
presentation, plain AC-3 — has its coded channels (dependent-substream
extensions included) routed to their own speakers, so the output shape
never changes mid-stream. `joc_layout = stereo` is the historical
`decoder::make_eac3_decoder_with_joc` behaviour: objects are rendered
when the caller asks for two channels and the §7.8 downmix is the
fallback. Stereo, 5.1 and 7.1 are described with the matching
`oxideav-core` `ChannelLayout`; core has no layout with height or wide
speakers yet, so the others report `ChannelLayout::DiscreteN(n)` and
[`JocLayout::speakers`] gives the per-slot speaker
([`JocSpeaker::channel_position`] where core has the position).

**What comes from the specification and what does not.** TS 103 420
defines the object properties and the geometry they live in — the
room-anchored coordinate system (clause 4.2.1), the speaker labels and
their room positions (clause 4.4, Table B.10), the speaker-to-zone
assignment (Table A.7), the zone-constraint codes (Tables 20 / 21) and
the bed-channel assignment (Tables 12 / 13) — and stops at the decoder
interface: how objects become speaker feeds is left to the renderer.
The panning law here is therefore **this crate's own design**, a
layered equal-power balance panner:

- speakers are grouped into the listener-plane layer and the top
  layer, each layer into rows of equal depth, each row ordered left to
  right (positions per Table B.10; without a back pair the surround
  pair takes the back wall, as Table A.7 note 2 counts it as the back
  zone there);
- a point object is panned with a sine/cosine law between the layers
  by its height, between the two rows bracketing its depth, and between
  the two speakers bracketing its left/right position — the squared
  gains always sum to one; a layout without a top layer keeps the
  object in the listener plane;
- object **size** averages the power of up to 3 × 3 × 3 point sources
  across the object's extent; **channel lock** sends the object to the
  single nearest speaker; **zone constraints** (and the elevation flag)
  remove speakers before panning (a speaker stays when at least one of
  its zones is included; a constraint that would silence the layout is
  ignored);
- **bed objects** (speaker-anchored) play from their own speaker when
  the layout has it and are panned at that speaker's position
  otherwise; the LFE is bypassed to the LFE channel;
- gains follow the OAMD update ramps, evaluated on a 32-sample grid and
  interpolated linearly in between, delayed with the QMF latency.

The stereo renderer keeps its original law (left/right pan by the X
coordinate, width spreading toward both speakers) and now also accepts
bed programs. Not rendered: intermediate-spatial-format objects (the
standard gives their ring geometry no coordinates — such a program
takes the channel route), screen-anchored scaling (treated as room
coordinates; no screen size is known to the decoder), object
divergence and trim, and the 7.X / 5.X + 2 JOC downmix configurations.
No JOC bitstream is available to this project, so the renderer is
validated on synthetic JOC/OAMD metadata (unit tests next to the
renderer: speaker-exact placement, unit power across the room, ramps
without discontinuities, bed identity, zone / lock / extent behaviour)
and the layout contract on encoder-made streams (`tests/joc_layout.rs`).

### Elementary-stream container (`.ac3` / `.eac3` / `.ec3`)

[`container`] registers the raw elementary stream with the runtime's
`ContainerRegistry` (name `ac3`, extensions `ac3` / `eac3` / `ec3`):
the probe wants the 0x0B77 syncword plus a valid header at offset 0
**and** at the frame boundary the first header announces, and the
demuxer emits one packet per presentation period — an AC-3 syncframe
(`frmsizecod` / `fscod`, Table 5.18, 1536 samples), or an E-AC-3
program-0 independent frame (`frmsiz`, `numblkscod` → 1/2/3/6 blocks
of 256 samples, reduced `fscod2` rates) together with the dependent /
additional-program frames that follow it, so a 7.1 indep + dep pair is
one packet. Timestamps count samples; duration and seeking use the
constant-rate estimate from the first packet's size. Packet counts for
the playback-corpus cells (AC-3 stereo / 5.1, E-AC-3 5.1) match
`ffprobe -count_frames` (`tests/elementary_stream.rs`).

### Byte-stream input, CRC check and concealment

The registry decoders treat what they are sent as a byte stream. A
packet may hold several syncframes, a fraction of one, or begin in the
middle of one (MPEG-TS PES payloads do all three):

- syncframes are reassembled across packet boundaries; one frame is
  returned per presentation period (an AC-3 syncframe, or an E-AC-3
  independent frame with the frames that follow it), through
  successive `receive_frame` calls;
- a packet's `pts` belongs to the first period that *commences* in it;
  later periods follow on by their sample counts, rescaled into the
  packet's time base;
- bytes that belong to no syncframe are skipped. A syncword found by
  scanning counts only when the next syncframe's header follows where
  it announces, or its CRC verifies (§6.1.2). 32 KiB of input without
  a syncframe is reported as an error;
- an access unit split between packets (independent and dependent
  frames in packets of their own) is held together once the stream
  has shown how many frames a period has.

One period per packet — what MP4, Matroska and the `.ac3` demuxer
deliver — decodes exactly as before, with no added latency, with one
exception:

**Enhanced-coupling look-ahead.** The last block of a syncframe that
uses enhanced coupling needs the first block of the next syncframe
(Annex E §3.5.5.1). For such a stream `receive_frame` answers
`NeedMore` until the following period has arrived, then returns the
frame — one packet of latency in the call pattern, no shift of the
audio or its timestamps; `flush` releases the last frame. Streams
without enhanced coupling are never held (the decoder learns which
kind a stream is from its first coupled frame). At the frame level,
`eac3::decode_eac3_packet_with_next` takes the following access unit;
`decode_eac3_packet` treats the frame as followed by nothing, which
leaves aliasing in the last block of every enhanced-coupling frame.

**Damaged frames** (§7.10, Annex E §3.2). The CRC of every syncframe
is verified before it is decoded (E-AC-3: the frame CRC; AC-3: a frame
is accepted when either `crc1` or `crc2` verifies, §6.1.2). A frame
that fails, or that cannot be decoded, is not an error: the decoder
returns a frame of the usual shape — taken from the substream's last
good frame, since a damaged header cannot be trusted — carrying the
overlap tail of the last good block followed by silence, and the next
frame fades in through its own window. Frame count and timestamps are
unaffected, so nothing downstream slips. The check condemns frames
only once the stream has shown a CRC word that verifies: a stream
whose CRC words are unusable from its first frame on is decoded as it
stands rather than muted.

| option (`CodecParameters::options`) | values | meaning |
| --- | --- | --- |
| `crc` | `true` (default) / `false` | verify syncframe CRCs and conceal failing frames; also `Eac3DecoderOptions::with_crc_check` |
| `program` | `0` (default) … `7` | independent substream to decode; also `Eac3DecoderOptions::with_program` |
| `joc_layout` | see below | object presentation |

At the frame level, `eac3::decode_eac3_packet` takes one access unit
and leaves the CRC check off unless
`Eac3DecoderState::set_crc_check(true)`;
`Eac3DecoderState::frames_zero_filled` / `frames_crc_failed` count
concealed frames.

### CRC

§7.10.1 CRC-16 (poly 0x8005), shared between the encoder (forward
generation, augmented form for crc2) and the opt-in decoder residue
check.

## Conformance corpus

`tests/docs_corpus.rs` decodes the AC-3 / E-AC-3 fixture set under
`docs/audio/ac3/fixtures/` (each a raw elementary stream paired with a
reference PCM decode) and scores per-channel PSNR. The decode is
floating-point in the IMDCT, so it is not bit-exact against the
reference, but it is **deterministic** (identical PSNR run-to-run).

Fixtures whose decode is known-good are gated at a `Tier::MinPsnr`
floor so a regression fails CI; the rest log deltas without gating:

| Tier | AC-3 | E-AC-3 |
| --- | --- | --- |
| `MinPsnr` (CI-gated) | 11 fixtures — mono / stereo / 2/1 / 3/0 / 3/2 (±LFE) at 48 / 44.1 / 32 kHz, 32-448 kbps, ~86-92 dB (80 dB floor, 96 kbps mono at 78), plus the torture-grade `ac3-low-bitrate-32kbps-mono` (~62 dB — the 32 kbps lossy / onset-overlap floor — at a loose 50 dB gross-regression floor) | 6 fixtures — stereo + 5.1 + 256-coeff at ~91 dB (80 dB floor), plus the low-rate `eac3-low-bitrate-32kbps` (~66 dB, 60 dB floor) and `eac3-low-rate-stereo-64kbps` (~72 dB, 65 dB floor) as gross-regression guards |

Every corpus fixture is now CI-gated; none remain `ReportOnly`. The
decoder is additionally fuzzed for panic-safety against
truncation / bit-flip / sync-prefixed-garbage corruption of every
fixture (`tests/robustness.rs`), and metadata- / SPX- / ecpl- /
TPNP-bearing encoder outputs join the same three corruption families.

## Fuzzing

`fuzz/` carries six coverage-guided libfuzzer harnesses (daily CI
runs via the `Fuzz` workflow; corpora seed from the fixture set):

- **`parse_headers`** — syncinfo + base-AC-3 BSI + Annex E BSI on raw
  bytes, incl. the metadata opt-in chains and the fractional-frame
  `convsync` tail.
- **`decode_frames`** — the registry AC-3 / E-AC-3 packet → PCM path,
  multiple carved packets on one stateful decoder, with the
  `joc_layout` option and the F32 output format picked by the input
  (a fixed layout must keep its frame shape on every hostile frame).
  Two decoders see the same packets: one with the CRC check off, so
  hostile bits reach the parser and the DSP, and one with the default
  policy (reassembly, CRC, concealment, enhanced-coupling hold-back).
- **`eac3_substream_walk`** — a persistent `Eac3DecoderState` over
  syncinfo-split syncframes: substream accumulation + §E.3.8.2
  chanmap channel combination, with the bytes after each frame
  offered as the look-ahead access unit.
- **`joc_oamd_parse`** — the TS 103 420 JOC/OAMD metadata surface
  (added with the #15 JOC contribution): EC-3 Extension Type A
  signalling, the EMDF container walk with its sparse / dense
  Huffman matrix decode, the standalone OAMD object-element decoder,
  and the QMF object reconstruction + object renderer (stereo and a
  selector-picked multichannel layout) on every container that parses
  (corpus seeded with synthetic valid EMDF), plus a structure-aware
  leg that builds OAMD programs from the input (dynamic / bed
  assignment, per-object position, size, zone, elevation, snap, ramp,
  optional bit flips) and renders them.
- **`encode_decode_roundtrip`** — structure-aware differential
  testing: a fuzz-picked contract-valid configuration (layout × rate
  × blocks × SPX/AHT/ecpl/TPNP × metadata) encodes arbitrary PCM and
  every emitted packet must decode through our own decoder with the
  exact sample count.
- **`ac3_encode_decode_roundtrip`** (r457) — the base-AC-3 twin:
  layout × Table 5.18 rate × sample rate × §5.4.2 metadata words
  through the registry path, encode → our AC-3 decoder with exact
  1536-sample frame counts.

The round-trip target found (and r454 fixed) three encoder
bit-budget-overflow classes on construction-accepted configs: a
metadata-blind minimum-rate floor, SNR tuners whose no-fit path
returned the default (overflowing) allocation, and tool syntax that
cannot fit its frame at all — now rejected by a construction-time
worst-case dry-run. r454 campaign (bounded local runs, ~20 min per
target, rss-limited): `parse_headers` 218.3M execs (182k/s, +167
corpus units), `decode_frames` 3.59M execs (+7,420 units),
`eac3_substream_walk` 2.85M execs (+2,041 units),
`encode_decode_roundtrip` 7.5K full encode→decode configs (+371
units) — zero outstanding findings. r457 (three bounded ≤ 300 s runs
over the new encoder paths): the new AC-3 target found two
minimum-rate-floor gaps (no floor at all; coupling side-info missing
from it) and a starved frame the optional dba segment lists pushed
one bit over, and the E-AC-3 target found the budget guard demoting
an elected second anchor instead of collapsing the frame — all fixed
and pinned, both targets then ran 300 s clean. r486 (bounded 90–150 s
runs of all six targets over the new decoder option paths):
`decode_frames`, now picking the `joc_layout` option and output format
from the input, found a concealed frame returning an empty payload
under a fixed layout and an out-of-bounds read in the stereo downmix
of a channel-less concealed frame — both fixed and pinned
(`tests/joc_layout.rs`, `tests/robustness.rs`), then 559k runs clean;
`joc_oamd_parse` gained a structure-aware leg that synthesizes OAMD
programs (dynamic and bed, every object property) and renders them
through each layout — 3.4M + 30k runs clean. r489: the scheduled
`encode_decode_roundtrip` run found a contract-valid configuration
(7.1 pair, 96 kb/s at 44.1 kHz, SPX, near-silent input) whose
independent substream overflowed its syncframe — the exponent-plan
guard did not count the SPX coordinate reserve; fixed (every budget
guard now shares one tool / metadata reserve) and replayed by
`tests/fuzz_regressions.rs`. After the decoder work of the round all
six targets ran 630 s each without a finding: `decode_frames` 604k
runs, `eac3_substream_walk` 2.09M, `parse_headers` 24.2M,
`joc_oamd_parse` 149k, `encode_decode_roundtrip` 27.7k,
`ac3_encode_decode_roundtrip` 38.1k; the two decode targets were then
run another 630 s each on the final code (712k and 2.56M runs, clean).

## Equal-rate position

`tests/equal_rate.rs` (+ `cargo run --release --example
equal_rate_report`) encodes a deterministic synthetic corpus — speech,
music, transients, a two-tone, pink noise and a 5.1 mix with LFE, all
band-limited to 16 kHz — with our encoder and with the black-box
reference encoder at the same nominal rate, decodes every stream with
**both** our decoder and the reference decoder, and scores worst-channel
SNR plus a mean noise-to-mask ratio (the §7.2.2 parametric mask
evaluated on the source's MDCT). The test pins our position and the
distance to the reference; the table is the r457 ladder (3 s clips,
worst-channel SNR in dB through the reference decoder; "r454" is the
same binary with the exponent-sharing bound disabled — the position
the previous round's two-tone table hid behind an aliased lag search).

| clip / kbps | AC-3 r454 | AC-3 r457 | AC-3 reference | E-AC-3 r457 | E-AC-3 reference |
| --- | --- | --- | --- | --- | --- |
| speech 64 / 96 / 192 | 9.3 / 10.4 / 11.1 | 11.1 / 16.8 / 29.5 | 13.9 / 19.3 / 32.8 | 11.2 / 16.8 / 29.4 | 13.9 / 18.1 / 30.3 |
| music 96 / 192 / 384 | 10.5 / 11.8 / 11.9 | 17.8 / 33.4 / 51.1 | 29.4 / 34.0 / 50.9 | 17.3 / 31.3 / 48.3 | 27.5 / 33.7 / 50.7 |
| transients 96 / 192 / 384 | 7.8 / 8.6 / 8.7 | 12.6 / 20.5 / 25.4 | 14.9 / 22.7 / 37.5 | 11.7 / 19.4 / 33.5 | 14.8 / 21.9 / 35.3 |
| two-tone 96 / 192 / 384 | 11.5 / 12.0 / 12.0 | 40.3 / 61.2 / 74.3 | 11.8 / 62.5 / 76.8 | 35.0 / 59.5 / 77.8 | 11.8 / 62.7 / 76.5 |
| pink 96 / 192 / 384 | 3.9 / 8.5 / 10.0 | 4.7 / 14.0 / 20.1 | 7.5 / 14.6 / 24.4 | 4.0 / 12.1 / 22.0 | 7.5 / 14.8 / 22.7 |
| 5.1 mix 256 / 448 / 640 | 9.2 / 10.2 / 10.4 | 12.9 / 21.9 / 24.4 | 15.5 / 23.3 / 29.5 | 12.6 / 20.7 / 27.1 | 14.5 / 22.2 / 28.0 |

Per-tool deltas measured while landing (ours → ours, 2 s clips):

- **Exponent-sharing bound** (the r454 defect): a REUSE block whose
  coefficient outgrew its anchor's exponent had its mantissa clamped
  at ±1 — every rate sat at ≈ 12 dB. Mono 440 Hz sine 11.2 → 76.4 dB;
  speech/192 11.1 → 29.5; music/192 11.8 → 33.0; two-tone/192 12.0 → 51.7.
- **Cadence + strategy election** (vs the fixed D15-on-blocks-0/3
  pattern): two-tone/96 20.9 → 40.4, two-tone/192 58.3 → 65.5,
  music/96 11.2 → 18.2, speech/192 28.4 → 29.4, transients/192 21.1 →
  21.7, 5.1/448 20.0 → 22.6; the tight cells (64-96 kbps, 5.1 at
  256 kbps) sit within 1 dB of a single-anchor frame.
- **Seven-bin LFE**: 5.1/448 LFE channel 20.1 → 29.9 dB (reference 34.6).
- **Joint block switching + §8.2.9 dither**: 5.1/448 through the
  reference decoder 21.0 → 21.2 dB and the two decoders' reading of
  our stream narrows from 1.9 to 1.1 dB mean.
- **§7.2.2.1.1** (all SNR offsets zero → `bap[] = 0`): a decode-side
  conformance fix in both decoders and the allocator — a frame the
  tuner floors no longer packs mantissas the reference decoder does
  not read.

What still separates us from the reference (recorded follow-ups, in
order of size): the low-rate coupling strategy (music at 96 kbps
−11.6 dB; the coupling begin/band structure/coordinate cadence are
fixed at `cplbegf = 8` / paired sub-bands / block-0 refresh), transient
coding at high rates (AC-3 transients/384 −12 dB — the short-block
path and per-block offset redistribution stop scaling above 192 kbps),
noise-like content (pink −1..−4 dB) and the 5.1 surround/centre balance
(−1.4..−5 dB). AHT codes a steady tone 3.5–19 dB *below* the standard
path (`eac3_rate_curves`, stereo 440 Hz: 96 kb/s 42.0 vs 45.6 dB,
192 kb/s 52.6 vs 71.3 dB, 448 kb/s 75.6 vs 88.6 dB). The r486 reading
of that gap: the AHT SNR-offset search is exact (it already bounds the
single exponent set by the frame maximum), and the loss is in the
transform itself. With the §3.4.5 constants as deployed the inverse
DCT is not orthonormal — independent quantisation noise of variance σ²
on the six AHT coefficients of a bin comes back as 6σ² on each block's
coefficient (+7.8 dB) — so AHT only pays where the energy collapses
into one or two AHT coefficients and the rest quantise to the short
GAQ codewords. A tone whose MDCT phase advances from block to block
(440 Hz moves 0.35 cycle per block) spreads over all six and keeps the
full 7.8 dB penalty at equal mantissa width. The fix is therefore an
election, not a retune: `chahtinu[ch]` per channel (and AHT-aware
`hebap` offsets) chosen by exact bit cost at equal noise against the
standard path — which needs AHT and non-AHT channels to coexist in one
frame, a structural encoder change recorded as the next step. The reference decoder accepts every stream shape
this encoder emits at these rates; the sole residual decoder
disagreement is a block in which the channels' `blksw` flags differ
(the reference corrupts the unswitched channel's overlap region — the
joint policy keeps our streams out of that case).

## Performance

`cargo run --release --example throughput [seconds]` encodes, decodes
and object-renders deterministic synthetic 5.1 programme material
(harmonic stacks with level motion, a noise bed, sparse transients) and
prints each stage as a multiple of real time together with an FNV-1a
hash of its output. Figures below: 20 s of programme, decode stages
repeated 20 times (`THROUGHPUT_REPEAT=20`), single thread, Apple M5 Max,
rustc 1.98 — the r486 pass, before → after. **Every stage's output
hash is unchanged**, and a 372-configuration encode / decode hash
matrix (AC-3 and E-AC-3, mono … 7.1, 32 / 44.1 / 48 kHz, every encoder
tool) is byte-identical before and after.

| stage (× real time) | before | after |
| --- | ---: | ---: |
| encode AC-3 5.1 448 kb/s | 0.7 | 25.6 |
| encode E-AC-3 5.1 384 kb/s | 0.6 | 19.7 |
| encode E-AC-3 5.1 384 kb/s, SPX + coupling | 1.2 | 36.3 |
| encode E-AC-3 5.1 384 kb/s, AHT | 2.1 | 42.6 |
| decode AC-3 5.1 448 kb/s | 253 | 469 |
| decode E-AC-3 5.1 384 kb/s | 228 | 402 |
| decode E-AC-3 5.1, SPX + coupling | 276 | 493 |
| decode E-AC-3 5.1, AHT | 284 | 519 |
| decode E-AC-3 5.1 → stereo downmix | 221 | 365 |
| decode E-AC-3 5.1 → 7.1.4 channel route | 202 | 308 |
| JOC render, 15 moving objects + LFE → stereo | 9.6 | 40.3 |
| JOC render → 5.1 | 9.1 | 43.8 |
| JOC render → 7.1.4 | 8.1 | 37.5 |
| JOC render → 9.1.6 | 7.9 | 39.3 |

Where the time went, and what replaced it (all exact — same values,
same summation order):

- **Forward MDCT** (encoders): the direct-form §8.2.3.2 sum evaluated a
  `cos` per term — 131 072 per block and channel. The kernels are
  tabulated once (`[n][k]`, each entry by the same expression) and the
  loops nested so every coefficient still accumulates over ascending
  `n` while the inner loop runs over independent sums.
- **SNR-offset search** (encoders): all 1024 `(csnroffst, fsnroffst)`
  pairs were tried, each re-running the whole §7.2.2 PSD / excitation /
  mask pipeline for every channel and block. The mask is now built once
  per channel-block (`MaskedPsd`); a candidate only redoes the final
  pointer lookup, counted as a histogram (`AllocEval`), and the largest
  fitting offset is found by bisecting a monotone lower bound of the
  bit count (grouped mantissas at their fractional cost) and walking
  down to the first exact fit — provably the same answer as the
  exhaustive scan. The per-channel and per-block refinements re-count
  only the cells whose offset changed.
- **Exponent cadence election** (encoders): the 32 anchor patterns of a
  frame only ever contain 21 distinct runs; each run's cost is computed
  once instead of once per pattern.
- **IMDCT** (decoders): the pre/post-twiddle tables were rebuilt (256
  `cos`/`sin`) on every call and the FFT derived its twiddles by a
  running product inside the butterfly loop; both are tables now (the
  FFT's holding the same running-product values).
- **Per-coefficient `2^-exp`** (decoders): `powi` per mantissa became a
  table of exact powers of two. Debug-trace switches, formerly ~12
  environment lookups per audio block, are read once per process.
- **JOC QMF bank and object mix**: analysis / synthesis modulation and
  the channel-to-object matrix run with the long dimension innermost
  over independent accumulators (same accumulation order per output);
  the per-timeslot scratch allocation and a 1280-sample silence scan
  per object and timeslot are gone; the stereo pan law is evaluated
  once per ramp position instead of once per sample; the layout panner
  evaluates each pan stage once per coordinate of the extent grid.

Remaining headroom is structural and would change output bits: the
forward MDCT and the JOC QMF bank are still direct-form `O(N²)`
(an FFT-based form rounds differently), and the cadence election still
runs the allocator on every candidate run.

**r489 decoder pass** — same example, 10 s of programme, decode stages
repeated 10 times, single thread, Apple M5 Max, rustc 1.98. The
machine was shared with other builds, so each figure is the **median
of seven runs** in which the build from before the pass and the current
build alternate; the ratio is the stable quantity. Output hashes are
identical, and the decoded output of 42 further streams (the staged
corpus, the in-tree fixtures and 19 encoder configurations) hashes the
same on both builds except one — the spectral-extension attenuation
stream, whose decode changed by the §3.6.4.2.3 fix. The "after"
column includes the syncframe CRC check, which the "before" build did
not perform.

| stage (× real time) | before | after | ratio |
| --- | ---: | ---: | ---: |
| decode AC-3 5.1 448 kb/s | 496 | 676 | 1.36 |
| decode E-AC-3 5.1 384 kb/s | 450 | 628 | 1.40 |
| decode E-AC-3 5.1, SPX + coupling | 566 | 668 | 1.18 |
| decode E-AC-3 5.1, AHT | 570 | 755 | 1.33 |
| decode E-AC-3 5.1 → stereo downmix | 391 | 561 | 1.43 |
| decode E-AC-3 5.1 → 7.1.4 channel route | 342 | 455 | 1.33 |

- **Bit allocation**: the §7.2.2 allocator ran for every channel of
  every block. A block that reuses its exponents and changes none of
  the allocation parameters has, by §7.2, the allocation of the block
  before; the allocator now returns at once when the channel's
  exponents over the allocated range and every parameter it reads
  (decay / gain / knee / floor codes, SNR offsets, fast gain, leak
  initialisation, delta segments, range, rate) equal those of the
  allocation already in the state.
- **Grouped mantissas** (bap 1, 2, 4): two or three divisions per
  group became one table row per code word.
- **CRC**: verification is new work on the decode path; the register
  advances four message bytes per step from four 256-entry tables
  (checked against the bit-serial register), and the AC-3 `crc1` /
  `crc2` check is a single pass over the frame.
- **Window + overlap-add** in one loop on the IMDCT output, without
  the coefficient and PCM scratch copies.
- **Per-frame allocation and passes removed**: exponent runs decode
  into a stack array; the 43 KiB AHT coefficient cache is allocated
  only for a frame that signals AHT; the registry decoder no longer
  packs a 16-bit copy of every E-AC-3 frame it then discards (float
  output, downmix, object rendering) nor copies the AC-3 program
  before packing it.

## Installation

```toml
[dependencies]
oxideav-core = "0.1"
oxideav-ac3 = "0.0"
```

## Codec ID

- Codecs: `"ac3"` (decoder + encoder) and `"eac3"` (decoder + encoder);
  output sample format `S16` interleaved.

## License

MIT — see [LICENSE](LICENSE).
