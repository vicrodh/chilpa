//! Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
/// Line-bed lattice: 200 depth lines x 256 frequency points (matches the
/// Tauri LinebedPanel NUM_LINES / VISUAL_BANDS and the Slint consts,
/// `shader_underlay.rs:47-50`). The C++ item and the shaders pin the same
/// numbers.
pub(crate) const LINE_BED_LINES: usize = 200;
pub(crate) const LINE_BED_BANDS: usize = 256;
/// Backend FFT bands per spectral frame (the `Spectral512` stream).
const SPECTRAL_BANDS: usize = 512;

/// The reshaping + depth ring. Lives on the viz DRAIN thread as a plain/// local (the `ShaderPackState` pattern): per-tick state, never shared,
/// never locked. `Default` = the reference's `new()`: zeroed accumulator,
/// zeroed ring (a fresh bed starts flat and fills over the next 200
/// spectral frames).
#[derive(Clone)]
pub(crate) struct LineBedState {
    /// 512-band receive-IIR accumulator.
    smoothed: Vec<f32>,
    /// LINE_BED_LINES*LINE_BED_BANDS, depth-ordered (row 0 = newest).
    ring: Vec<f32>,
}

impl Default for LineBedState {
    fn default() -> Self {
        Self {
            smoothed: vec![0.0; SPECTRAL_BANDS],
            ring: vec![0.0; LINE_BED_LINES * LINE_BED_BANDS],
        }
    }
}

impl LineBedState {
    /// Receive-IIR a 512-band frame, reshape to 256 heights, push at the
    /// near row (shader_underlay.rs:163-174 — verbatim, including the
    /// 0.03/0.97 coefficient order: the accumulator keeps 3% and takes 97%
    /// of the new frame).
    pub(crate) fn push(&mut self, bins: &[f32]) {
        let n = self.smoothed.len().min(bins.len());
        for i in 0..n {
            self.smoothed[i] = self.smoothed[i] * 0.03 + bins[i] * 0.97;
        }
        let row = reshape_512_to_256(&self.smoothed);
        // Shift every row one slot deeper, then write the newest at row 0.
        self.ring
            .copy_within(0..(LINE_BED_LINES - 1) * LINE_BED_BANDS, LINE_BED_BANDS);
        self.ring[0..LINE_BED_BANDS].copy_from_slice(&row);
    }

    /// The ready ring as raw f32 bytes (256x200, R32F texture layout: row =
    /// depth line, column = band — the VS reads texel (band, line)).
    /// Soundness: plain `f32` storage, no padding holes — the same cast the
    /// reference does for its `write_texture` (`f32_bytes`,
    /// shader_underlay.rs:145-147).
    pub(crate) fn ring_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.ring.as_ptr() as *const u8,
                std::mem::size_of_val(self.ring.as_slice()),
            )
        }
    }
}

/// 512 backend bands → 256 line heights in [0.1, 84] — Tauri's
/// LinebedPanel chain (the backend bands are intentionally flat; this is
/// what makes the ridges): frequency-warp bin map → peak-preserving
/// smoothing → low-end tail roll-off → 3-point box → per-band gamma + soft
/// clip. Verbatim port of `shader_underlay.rs:193-231` (which is itself the
/// Tauri chain).
fn reshape_512_to_256(data: &[f32]) -> [f32; 256] {
    let mut vis = [0.0f32; 256];
    for i in 0..256 {
        let seg_start = (i as f32 / 256.0).powf(1.32);
        let seg_end = ((i + 1) as f32 / 256.0).powf(1.32);
        let s = 4.0 + (460.0 - 4.0) * seg_start;
        let e = 4.0 + (460.0 - 4.0) * seg_end;
        let lower = (s.floor() as usize).max(4);
        let upper = (e.ceil() as usize).min(460);
        let (mut sum, mut peak, mut count) = (0.0f32, 0.0f32, 0u32);
        let mut j = lower;
        while j <= upper && j < data.len() {
            sum += data[j];
            if data[j] > peak {
                peak = data[j];
            }
            count += 1;
            j += 1;
        }
        let avg = if count > 0 { sum / count as f32 } else { 0.0 };
        vis[i] = (avg * 0.52 + peak * 0.48) * 770.0;
    }
    apply_average(&mut vis);
    // Low-end tail roll-off (first 7 bins).
    for i in 0..7usize {
        vis[i] *= 0.013_334_120_966_221_101 * ((i + 1) as f32).powf(1.6) + 0.7;
    }
    smooth3(&mut vis);
    // Per-band gamma + soft clip + cap → [0.1, 84].
    for i in 0..256 {
        let frac = i as f32 / 255.0;
        let exp = 1.35 + (0.9 - 1.35) * frac * frac;
        let norm = (vis[i] / 770.0).max(0.0);
        let shaped = norm.powf(exp);
        let comp = 1.0 - (-shaped * 3.25).exp();
        vis[i] = (comp * 84.0).clamp(0.1, 84.0);
    }
    vis
}

/// Two-pass peak-preserving smoothing (Tauri applyAverageTransform).
fn apply_average(d: &mut [f32; 256]) {
    let source = *d;
    for i in 0..256 {
        let prev = if i > 0 { source[i - 1] } else { source[i] };
        let next = if i < 255 { source[i + 1] } else { source[i] };
        let cur = source[i];
        d[i] = if cur >= prev && cur >= next {
            cur
        } else {
            (cur + prev.max(next)) / 2.0
        };
    }
    let second_source = *d;
    for i in 0..256 {
        let prev = if i > 0 { second_source[i - 1] } else { second_source[i] };
        let next = if i < 255 { second_source[i + 1] } else { second_source[i] };
        let cur = second_source[i];
        d[i] = if cur >= prev && cur >= next {
            cur
        } else {
            cur / 2.0 + prev.max(next) / 3.0 + prev.min(next) / 6.0
        };
    }
}

/// 3-point box smooth, one pass (Tauri smoothSpectrum).
fn smooth3(d: &mut [f32; 256]) {
    let source = *d;
    for i in 0..256 {
        let prev = if i > 0 { source[i - 1] } else { source[i] };
        let next = if i < 255 { source[i + 1] } else { source[i] };
        d[i] = (prev + source[i] + next) / 3.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Silence reshapes to the floor: every band clamps to 0.1 (the gamma +
    /// soft clip of a zero signal is 0, clamped up to 0.1).
    #[test]
    fn silence_reshapes_to_the_floor() {
        let vis = reshape_512_to_256(&[0.0; SPECTRAL_BANDS]);
        assert!(vis.iter().all(|&h| h == 0.1), "not all floor: {vis:?}");
    }

    /// The heights contract: [0.1, 84] for ANY input — including absurd
    /// ones (the soft clip, not the clamp, is what does the real work at
    /// the top; the clamp is the floor guard).
    #[test]
    fn heights_stay_in_bounds() {
        for data in [
            vec![0.0f32; SPECTRAL_BANDS],
            vec![1.0f32; SPECTRAL_BANDS],
            vec![100.0f32; SPECTRAL_BANDS],
        ] {
            let vis = reshape_512_to_256(&data);
            for (i, &h) in vis.iter().enumerate() {
                assert!((0.1..=84.0).contains(&h), "band {i} out of range: {h}");
            }
        }
    }

    /// A single-bin spike lands in the right output region: bin 100 of 512
    /// maps through the freq warp (4 + 456*(i/256)^1.32) to output ≈ 78-79.
    /// The peak-preserving smoothing keeps a lone spike alive.
    #[test]
    fn single_bin_spike_lands_in_the_right_region() {
        let mut data = [0.0f32; SPECTRAL_BANDS];
        data[100] = 1.0;
        let vis = reshape_512_to_256(&data);
        let peak = vis
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
            .unwrap();
        assert!(
            (72..=85).contains(&peak),
            "spike at bin 100 peaked at output {peak}, expected ≈78"
        );
        assert!(vis[peak] > 40.0, "the spike must tower: {}", vis[peak]);
        // Far from the spike the bed stays at the floor.
        assert_eq!(vis[10], 0.1);
        assert_eq!(vis[200], 0.1);
    }

    /// The receive-IIR: new frame takes 97%, the accumulator keeps 3%
    /// (coefficient ORDER matters — this is what the reference does, and a
    /// swapped pair would read as instant attack + slow release, the
    /// opposite of the intended behavior).
    #[test]
    fn accumulator_coefficients() {
        let mut lb = LineBedState::default();
        let mut bins = vec![0.0f32; SPECTRAL_BANDS];
        bins[300] = 1.0;
        lb.push(&bins);
        assert!((lb.smoothed[300] - 0.97).abs() < 1e-6);
        lb.push(&vec![0.0f32; SPECTRAL_BANDS]);
        assert!((lb.smoothed[300] - 0.97 * 0.03).abs() < 1e-6);
    }

    /// The depth ring: row 0 = newest, every push shifts the previous rows
    /// one slot deeper. Push a spike, then silence: row 0 flattens, row 1
    /// keeps the spike's reshape.
    #[test]
    fn ring_shifts_newest_first() {
        let mut lb = LineBedState::default();
        let mut spike = vec![0.0f32; SPECTRAL_BANDS];
        spike[100] = 1.0;
        lb.push(&spike);
        let after_spike = lb.ring[0..LINE_BED_BANDS].to_vec();
        let spiked = after_spike.iter().any(|&h| h > 40.0);
        assert!(spiked, "the spike reshape never reached the ring");

        lb.push(&vec![0.0f32; SPECTRAL_BANDS]);
        // Row 0 is the silence reshape (near floor: the IIR keeps 3% of the
        // spike, which reshapes well under the old peak)...
        let row0_max = lb.ring[0..LINE_BED_BANDS]
            .iter()
            .copied()
            .fold(0.0f32, f32::max);
        let row1 = &lb.ring[LINE_BED_BANDS..2 * LINE_BED_BANDS];
        assert!(row0_max < after_spike.iter().copied().fold(0.0f32, f32::max));
        // ...and row 1 is EXACTLY the previous row 0 (the shift).
        assert_eq!(row1, after_spike.as_slice());
    }

    /// `ring_bytes` is the whole 256x200 f32 ring, R32F texture size.
    #[test]
    fn ring_bytes_is_the_full_texture() {
        let lb = LineBedState::default();
        assert_eq!(
            lb.ring_bytes().len(),
            LINE_BED_LINES * LINE_BED_BANDS * std::mem::size_of::<f32>()
        );
    }
}
