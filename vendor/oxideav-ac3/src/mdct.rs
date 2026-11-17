//! Forward MDCT (Modified Discrete Cosine Transform) for AC-3 encoding.
//!
//! Per §8.2.3.2 (A/52:2018), the AC-3 forward transform is
//!
//! ```text
//!   X_D[k] = (-2/N) * sum_{n=0..N-1} x[n] *
//!             cos( (2π/(4N)) * (2n+1) * (2k+1)
//!                  + (π/4) * (2k+1) * (1+α) )
//! ```
//!
//! with **N = 512** for the long block (α = 0) and **two N = 256
//! transforms** for a short-block pair, the first using α = −1 and the
//! second α = +1. Each half therefore carries a different `(π/4)·(2k+1)
//! ·(1+α)` phase offset — the analysis counterpart of the asymmetric
//! §7.9.4.2 step-5 de-interleave in `crate::imdct::imdct_256_pair_fft`.
//! The 128 coefficients from each short transform are interleaved into
//! a single 256-coeff buffer per §7.9.4.2 step 1: `X[2k] = X1[k]`,
//! `X[2k+1] = X2[k]`.
//!
//! The 256-coefficient output of this transform, when fed back through
//! our [`super::audblk::imdct_512`] reference, recovers the windowed
//! input (modulo the standard TDAC 50% overlap-add). Concretely, for a
//! block of 512 windowed input samples, this forward MDCT plus the
//! decoder's IMDCT+window+overlap-add chain reproduces the middle 256
//! samples exactly (to floating-point precision). The factor-of-`-2/N`
//! here pairs with the decoder's factor-of-`2` overlap-add so that the
//! combined round-trip gain is `1.0`.

use std::f32::consts::PI;
use std::sync::OnceLock;

/// Cosine kernels of the three forward transforms, laid out
/// `[n][k]` so the transform can walk the input once and update every
/// coefficient's running sum in a contiguous (vectorisable) inner loop.
///
/// Each entry is computed with exactly the expression the direct form
/// evaluates per term, so tabulating changes no output bit.
struct Kernels {
    /// 512 × 256, α = 0.
    long: Vec<f32>,
    /// 256 × 128, α = −1 (first short transform).
    short_first: Vec<f32>,
    /// 256 × 128, α = +1 (second short transform).
    short_second: Vec<f32>,
}

fn kernels() -> &'static Kernels {
    static KERNELS: OnceLock<Kernels> = OnceLock::new();
    KERNELS.get_or_init(|| {
        let n: usize = 512;
        let two_pi_over_4n = 2.0 * PI / (4.0 * n as f32);
        let pi_over_4 = PI / 4.0;
        let mut long = vec![0.0f32; 512 * 256];
        for nn in 0..n {
            for k in 0..256 {
                let two_k_plus_1 = (2 * k + 1) as f32;
                let phase_b = pi_over_4 * two_k_plus_1; // α = 0 → (1+α) = 1
                let phase = two_pi_over_4n * (2 * nn + 1) as f32 * two_k_plus_1 + phase_b;
                long[nn * 256 + k] = phase.cos();
            }
        }
        let short = |alpha: f32| {
            let n: usize = 256;
            // (2π/4N)·(2n+1)·(2k+1) = (π/2N)·(2n+1)·(2k+1).
            let pi_over_2n = PI / (2.0 * n as f32);
            let quarter_pi = PI / 4.0;
            let mut table = vec![0.0f32; 256 * 128];
            for nn in 0..n {
                for k in 0..128 {
                    let two_k_plus_1 = (2 * k + 1) as f32;
                    let phase_offset = quarter_pi * two_k_plus_1 * (1.0 + alpha);
                    let phase = pi_over_2n * (2 * nn + 1) as f32 * two_k_plus_1 + phase_offset;
                    table[nn * 128 + k] = phase.cos();
                }
            }
            table
        };
        Kernels {
            long,
            short_first: short(-1.0),
            short_second: short(1.0),
        }
    })
}

/// 512-point forward MDCT (§8.2.3.2, α=0 long transform).
///
/// `input`  : 512 windowed time-domain samples.
/// `output` : 256 MDCT coefficients — indices 0..N/2.
///
/// Direct form, `O(N^2)`: 256 × 512 multiply-adds per block against the
/// tabulated kernel. Every coefficient accumulates its terms in input
/// order (`n` ascending), so the result is bit-identical to evaluating
/// the §8.2.3.2 sum coefficient by coefficient.
pub fn mdct_512(input: &[f32; 512], output: &mut [f32; 256]) {
    let n: usize = 512;
    // §8.2.3.2 mandates a `-2/N` normalisation. Combined with the
    // decoder's `2/N` IMDCT scale and the ×2 overlap-add, the full
    // analysis-synthesis round-trip lands on unity gain (to within
    // window-table rounding).
    let scale: f32 = -2.0 / n as f32;
    let mut sums = [0.0f32; 256];
    for (&x, row) in input.iter().zip(kernels().long.as_chunks::<256>().0) {
        for (s, &c) in sums.iter_mut().zip(row) {
            *s += x * c;
        }
    }
    for (out, s) in output.iter_mut().zip(sums) {
        *out = scale * s;
    }
}

/// 256-point forward MDCT used for one half of a short-block pair
/// (§8.2.3.2). The two halves do **not** share a kernel: the spec's
/// forward transform carries a phase-offset parameter
///
///   X[k] = (-2/N) · Σ x[n] · cos( (2π/4N)·(2n+1)·(2k+1)
///                                 + (π/4)·(2k+1)·(1+α) )
///
/// with α = −1 for the first short transform and α = +1 for the
/// second (§8.2.3.2). The corresponding decoder IMDCT
/// (`imdct_256_pair_fft`) realises that same α distinction through the
/// asymmetric §7.9.4.2 step-5 de-interleave, so the forward half must
/// pass the matching α to round-trip. The `-2/N` scale pairs with the
/// decoder's IMDCT scale + overlap-add gain to land on unity gain
/// under the spec's KBD window.
///
/// `input`  : 256 windowed time-domain samples (one half of the
///            short-block pair).
/// `second` : `false` for the first short transform (α = −1), `true`
///            for the second (α = +1).
/// `output` : 128 MDCT coefficients.
fn mdct_256_half(input: &[f32; 256], second: bool, output: &mut [f32; 128]) {
    let n: usize = 256;
    let scale: f32 = -2.0 / n as f32;
    let kernels = kernels();
    let table = if second {
        &kernels.short_second
    } else {
        &kernels.short_first
    };
    let mut sums = [0.0f32; 128];
    for (&x, row) in input.iter().zip(table.as_chunks::<128>().0) {
        for (s, &c) in sums.iter_mut().zip(row) {
            *s += x * c;
        }
    }
    for (out, s) in output.iter_mut().zip(sums) {
        *out = scale * s;
    }
}

/// Forward short-block MDCT pair (§8.2.3.2 + §7.9.4.2).
///
/// The 512-sample windowed input is split into two 256-sample halves;
/// each half is run through [`mdct_256_half`] to produce 128
/// coefficients, then the two coefficient sets are **interleaved** per
/// §7.9.4.2 step 1: `X[2k] = X1[k]`, `X[2k+1] = X2[k]`. This is the
/// exact layout `imdct_256_pair_fft` reads on the decoder side.
///
/// Note that AC-3's per-channel windowing differs slightly between
/// long-only / short-only / long-to-short / short-to-long block-type
/// transitions (§7.9.5). For now the encoder applies the symmetric
/// 512-point KBD window in **all** cases — long-only and short-only —
/// which is identical to the long-only window the decoder applies
/// after IMDCT regardless of `blksw[ch]`. The transition cases (where
/// one neighbour is long and the other short) introduce a small TDAC
/// mismatch in the overlap region; the encoder's transient-detection
/// heuristic deliberately picks short blocks in *runs* of 1+ blocks
/// to keep transitions outside the burst peak's overlap window, which
/// keeps the residual below the per-block quantisation noise floor.
///
/// `input`  : 512 windowed time-domain samples (covers two short halves).
/// `output` : 256 interleaved MDCT coefficients.
pub fn mdct_256_pair(input: &[f32; 512], output: &mut [f32; 256]) {
    let mut h1 = [0.0f32; 256];
    let mut h2 = [0.0f32; 256];
    h1.copy_from_slice(&input[..256]);
    h2.copy_from_slice(&input[256..]);
    let mut x1 = [0.0f32; 128];
    let mut x2 = [0.0f32; 128];
    // First short transform uses α=−1, the second α=+1 (§8.2.3.2).
    mdct_256_half(&h1, false, &mut x1);
    mdct_256_half(&h2, true, &mut x2);
    for k in 0..128 {
        output[2 * k] = x1[k];
        output[2 * k + 1] = x2[k];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audblk;
    use crate::tables::WINDOW;

    /// The tabulated transforms are the §8.2.3.2 sums, bit for bit: each
    /// coefficient accumulates `x[n]·cos(..)` over ascending `n` with the
    /// kernel evaluated by the same expression.
    #[test]
    fn tabulated_kernels_reproduce_the_direct_sums_exactly() {
        let mut seed = 0x1234_5678_u32;
        let mut input = [0.0f32; 512];
        for value in &mut input {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *value = (seed >> 8) as f32 / (1 << 23) as f32 - 1.0;
        }
        let mut long = [0.0f32; 256];
        mdct_512(&input, &mut long);
        let n = 512usize;
        let two_pi_over_4n = 2.0 * PI / (4.0 * n as f32);
        for k in 0..256 {
            let two_k_plus_1 = (2 * k + 1) as f32;
            let phase_b = (PI / 4.0) * two_k_plus_1;
            let mut s = 0.0f32;
            for (nn, x) in input.iter().enumerate() {
                let phase = two_pi_over_4n * (2 * nn + 1) as f32 * two_k_plus_1 + phase_b;
                s += x * phase.cos();
            }
            assert_eq!(
                long[k].to_bits(),
                ((-2.0 / n as f32) * s).to_bits(),
                "long k={k}"
            );
        }

        let mut pair = [0.0f32; 256];
        mdct_256_pair(&input, &mut pair);
        let n = 256usize;
        let pi_over_2n = PI / (2.0 * n as f32);
        for (half, alpha) in [(0usize, -1.0f32), (1, 1.0)] {
            for k in 0..128 {
                let two_k_plus_1 = (2 * k + 1) as f32;
                let phase_offset = (PI / 4.0) * two_k_plus_1 * (1.0 + alpha);
                let mut s = 0.0f32;
                for nn in 0..n {
                    let phase = pi_over_2n * (2 * nn + 1) as f32 * two_k_plus_1 + phase_offset;
                    s += input[half * 256 + nn] * phase.cos();
                }
                assert_eq!(
                    pair[2 * k + half].to_bits(),
                    ((-2.0 / n as f32) * s).to_bits(),
                    "short half={half} k={k}"
                );
            }
        }
    }

    /// Forward MDCT followed by the decoder's IMDCT should approximately
    /// invert under the AC-3 windowing + TDAC overlap-add rule. We feed
    /// the same windowed block in twice (blocks N and N+1), run the
    /// full analysis-synthesis chain, and expect the second block's
    /// output to match the (windowed^2) input in the middle region.
    #[test]
    fn mdct_imdct_roundtrip_identity_window_tdac() {
        // Construct a 768-sample ramp and encode two adjacent 512-sample
        // overlapping blocks out of it. Decoder's overlap-add needs two
        // blocks to produce valid output for the first block.
        let sig_len = 512 + 256;
        let mut sig = vec![0.0f32; sig_len];
        for (i, s) in sig.iter_mut().enumerate() {
            // 100 Hz-ish sine to keep the magnitudes sensible under a 48 kHz rate.
            let t = i as f32 / 48_000.0;
            *s = (2.0 * PI * 440.0 * t).sin() * 0.3;
        }

        // Build the symmetric 512-sample window from WINDOW[0..256] + mirror.
        let mut full_win = [0.0f32; 512];
        for n in 0..256 {
            full_win[n] = WINDOW[n];
            full_win[511 - n] = WINDOW[n];
        }

        // Window block 0 (samples 0..512).
        let mut blk0 = [0.0f32; 512];
        for n in 0..512 {
            blk0[n] = sig[n] * full_win[n];
        }
        let mut x0 = [0.0f32; 256];
        mdct_512(&blk0, &mut x0);

        // Window block 1 (samples 256..768).
        let mut blk1 = [0.0f32; 512];
        for n in 0..512 {
            blk1[n] = sig[256 + n] * full_win[n];
        }
        let mut x1 = [0.0f32; 256];
        mdct_512(&blk1, &mut x1);

        // IMDCT + window + overlap-add path, exactly as the decoder runs.
        let mut delay = [0.0f32; 256];

        // Block 0: IMDCT, window, OLA (primes delay; pcm0 is discarded).
        let mut time0 = [0.0f32; 512];
        audblk::imdct_512(&x0, &mut time0);
        for n in 0..256 {
            time0[n] *= WINDOW[n];
            time0[511 - n] *= WINDOW[n];
        }
        let mut _pcm0 = [0.0f32; 256];
        for n in 0..256 {
            _pcm0[n] = 2.0 * (time0[n] + delay[n]);
            delay[n] = time0[256 + n];
        }

        // Block 1: IMDCT, window, OLA → pcm1 should match input[256..512].
        let mut time1 = [0.0f32; 512];
        audblk::imdct_512(&x1, &mut time1);
        for n in 0..256 {
            time1[n] *= WINDOW[n];
            time1[511 - n] *= WINDOW[n];
        }
        let mut pcm1 = [0.0f32; 256];
        for n in 0..256 {
            pcm1[n] = 2.0 * (time1[n] + delay[n]);
        }

        // Compare pcm1 to input[256..512]. The overlap-add equation
        // pcm[n] = window[n]^2 * x[n] + window[n+256]^2 * x[n] = x[n]
        // (because AC-3's window satisfies w[n]^2 + w[n+256]^2 = 1 —
        // the Princen-Bradley condition for MDCT TDAC).
        let mut worst: f32 = 0.0;
        let mut sse: f32 = 0.0;
        for n in 0..256 {
            let err = (pcm1[n] - sig[256 + n]).abs();
            worst = worst.max(err);
            sse += err * err;
        }
        let rms = (sse / 256.0).sqrt();
        eprintln!("mdct-imdct roundtrip: worst={worst:.5}, rms={rms:.5}");
        // The window is only approximate in the tables (5-decimal rounding);
        // a few 1e-3 worst-case error is acceptable here.
        assert!(worst < 0.01, "worst {worst} too large");
        assert!(rms < 5e-3, "rms {rms} too large");
    }

    /// The 128 inverse-basis vectors for a short-block half (X1) span a
    /// 128-dimensional subspace of R^256. The encoder's forward MDCT is
    /// the orthogonal projector onto that subspace; the per-half
    /// MDCT-then-IMDCT round-trip recovers exactly the projection of
    /// the input. We assert the basis is orthogonal with uniform norm
    /// `N/2 = 128` here so any future change to the IMDCT polarity /
    /// scale is caught at this gate (and the encoder's scale stays
    /// derivable as `1/‖basis‖² = 2/N`).
    #[test]
    fn imdct_short_basis_is_uniform_orthogonal() {
        let mut basis = vec![[0.0f32; 256]; 128];
        for k in 0..128 {
            let mut x = [0.0f32; 256];
            x[2 * k] = 1.0;
            let mut t = [0.0f32; 512];
            crate::imdct::imdct_256_pair_fft(&x, &mut t);
            basis[k].copy_from_slice(&t[..256]);
        }
        let mut max_off = 0.0f32;
        let mut min_norm = f32::INFINITY;
        let mut max_norm = 0.0f32;
        for k in 0..128 {
            let n: f32 = basis[k].iter().map(|&v| v * v).sum();
            min_norm = min_norm.min(n);
            max_norm = max_norm.max(n);
            for j in (k + 1)..128 {
                let dot: f32 = basis[k]
                    .iter()
                    .zip(basis[j].iter())
                    .map(|(&a, &b)| a * b)
                    .sum();
                max_off = max_off.max(dot.abs());
            }
        }
        // Norm = N/2 = 128 (basis vectors are unit-amplitude cosines).
        assert!((min_norm - 128.0).abs() < 0.01, "min_norm={min_norm}");
        assert!((max_norm - 128.0).abs() < 0.01, "max_norm={max_norm}");
        assert!(max_off < 0.01, "off-diagonal {max_off}");
    }

    /// End-to-end forward + inverse round-trip on a TDAC-compatible
    /// input. Because the per-half MDCT only spans a 128-dim subspace
    /// of R^256, we feed an input that is *already in the subspace* —
    /// constructed by inverting an arbitrary 128-coeff bin pattern.
    /// The forward must then exactly recover those coefficients, and
    /// re-inverting must reproduce the original signal to f32
    /// precision.
    #[test]
    fn mdct_256_pair_recovers_subspace_signal() {
        // Pick an arbitrary 128-coefficient pattern for short1 +
        // short2 (X2 chosen to be a different low-order pattern so
        // the full 256 input has harmonic content in both halves).
        let mut x_target = [0.0f32; 256];
        for k in 0..16 {
            x_target[2 * k] = 0.7 * (k as f32).sin();
            x_target[2 * k + 1] = 0.5 * (k as f32 * 1.3).cos();
        }
        // Inverse → 512-sample signal (which lives in the subspace
        // by construction).
        let mut sig = [0.0f32; 512];
        crate::imdct::imdct_256_pair_fft(&x_target, &mut sig);
        // Forward → should recover x_target exactly.
        let mut x_back = [0.0f32; 256];
        mdct_256_pair(&sig, &mut x_back);
        let mut max_err: f32 = 0.0;
        for k in 0..256 {
            max_err = max_err.max((x_back[k] - x_target[k]).abs());
        }
        eprintln!("subspace round-trip: max coeff err = {max_err:.6e}");
        assert!(
            max_err < 1e-3,
            "forward/inverse mismatch on basis-subspace input: {max_err}"
        );
    }
}
