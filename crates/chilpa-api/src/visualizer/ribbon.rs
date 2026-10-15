//! Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
/// Pinned with `cxx/ribbon_item.cpp` and the Slint reference
/// (`shader_underlay.rs:44-45`).
pub(crate) const SPECTRAL_BANDS: usize = 512;
pub(crate) const SPECTRAL_COLUMNS: u32 = 2048;
pub(crate) const HEADER_BYTES: usize = 4 + 8 + 8;
pub(crate) const FRAME_BYTES: usize = HEADER_BYTES + SPECTRAL_BANDS;

/// Drain-local ribbon state (the viz drain thread — the linebed_qt.rs
/// pattern: per-tick state, never shared, never locked). The reference keeps
/// `last_track_id`/`last_progress` as drain-closure locals
/// (visualizer.rs:114-116); the Slint thread-local SPECTRO_LAST_COL moves
/// renderer-side with the texture (see the header).
#[derive(Default)]
pub(crate) struct RibbonState {
    last_track_id: u64,
    last_progress: f32,
    last_col: usize,
    history: Vec<u8>,
    epoch: u64,
    sequence: u64,
}

impl RibbonState {
    /// Build the frame for one fresh Spectral512 row, or `None` when the row
    /// is empty. `cursor` is `(track_id, progress 0..1)` from
    /// `now_playing::ribbon_cursor` (second-granular, like the reference's
    /// NowPlayingState read).
    ///
    /// Reset rule VERBATIM from visualizer.rs:248-250: track change, a
    /// backward step past 0.01, or a forward JUMP past 0.15 (a seek — the
    /// ~1 Hz progress granularity means normal playback never moves more
    /// than that between ticks except on VERY short tracks... exactly as the
    /// reference has it).
    pub(crate) fn frame(&mut self, bins: &[f32], cursor: (u64, f32)) -> Option<Vec<u8>> {
        if bins.is_empty() {
            return None;
        }
        let (track_id, progress) = cursor;
        let reset = track_id != self.last_track_id
            || progress + 0.01 < self.last_progress
            || progress > self.last_progress + 0.15;
        self.last_track_id = track_id;
        self.last_progress = progress;
        if reset {
            self.epoch = self.epoch.saturating_add(1);
        }
        self.sequence = self.sequence.saturating_add(1);

        let col = (progress.clamp(0.0, 1.0) * (SPECTRAL_COLUMNS - 1) as f32) as u32;
        let mut out = Vec::with_capacity(FRAME_BYTES);
        out.extend_from_slice(&col.to_le_bytes());
        out.extend_from_slice(&self.epoch.to_le_bytes());
        out.extend_from_slice(&self.sequence.to_le_bytes());
        for i in 0..SPECTRAL_BANDS {
            let v = if i < bins.len() {
                (bins[i].clamp(0.0, 1.0) * 255.0) as u8
            } else {
                0
            };
            out.push(v);
        }
        if self.history.is_empty() {
            self.history.resize(SPECTRAL_BANDS * SPECTRAL_COLUMNS as usize, 0);
        }
        if reset {
            self.history.fill(0);
            self.last_col = 0;
        }
        // Keep the same gap-fill as the GPU texture. This copy is retained
        // locally; only a newly mounted renderer asks for the full snapshot.
        let column = col as usize;
        let first = if column > self.last_col { self.last_col + 1 } else { column };
        for column in first..=column {
            self.history[column * SPECTRAL_BANDS..(column + 1) * SPECTRAL_BANDS].copy_from_slice(&out[HEADER_BYTES..]);
        }
        self.last_col = column;
        Some(out)
    }

    /// The full-size payload restores the texture when mounting a renderer.
    /// Epoch and sequence let the GPU reject older queued rows afterward.
    pub(crate) fn snapshot(&self) -> Option<Vec<u8>> {
        if self.history.is_empty() {
            return None;
        }
        let mut out = Vec::with_capacity(HEADER_BYTES + self.history.len());
        out.extend_from_slice(&(self.last_col as u32).to_le_bytes());
        out.extend_from_slice(&self.epoch.to_le_bytes());
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(&self.history);
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn epoch(frame: &[u8]) -> u64 {
        u64::from_le_bytes(frame[4..12].try_into().unwrap())
    }

    fn sequence(frame: &[u8]) -> u64 {
        u64::from_le_bytes(frame[12..20].try_into().unwrap())
    }

    /// The layout is shared with C++: column, epoch, sequence, spectral row.
    #[test]
    fn frame_layout_is_pinned() {
        let mut st = RibbonState::default();
        let bins = vec![0.5f32; 512];
        let f = st.frame(&bins, (7, 0.25)).expect("non-empty row");
        assert_eq!(f.len(), FRAME_BYTES);
        let col = u32::from_le_bytes([f[0], f[1], f[2], f[3]]);
        assert_eq!(col, (0.25 * (SPECTRAL_COLUMNS - 1) as f32) as u32);
        assert_eq!(epoch(&f), 1, "first frame starts an epoch");
        assert_eq!(sequence(&f), 1);
        assert!(f[HEADER_BYTES..].iter().all(|&b| b == 127)); // 0.5 * 255 = 127.5 -> 127
    }

    /// The reset rule (visualizer.rs:248-250 verbatim): same track + small
    /// forward progress does NOT reset; a backward step, a forward jump past
    /// 0.15, or a track change DO.
    #[test]
    fn reset_rule_matches_the_reference() {
        let mut st = RibbonState::default();
        let bins = vec![0.1f32; 8]; // shorter than 512 — zero-padded
        let f = st.frame(&bins, (1, 0.0)).unwrap();
        assert_eq!(epoch(&f), 1, "first ever frame resets (track 0 -> 1)");
        let f = st.frame(&bins, (1, 0.05)).unwrap();
        assert_eq!(epoch(&f), 1, "normal progress keeps the epoch");
        let f = st.frame(&bins, (1, 0.045)).unwrap();
        assert_eq!(epoch(&f), 1, "a sub-0.01 backward nudge is NOT a seek");
        let f = st.frame(&bins, (1, 0.02)).unwrap();
        assert_eq!(epoch(&f), 2, "a backward step past 0.01 is a seek");
        let f = st.frame(&bins, (1, 0.20)).unwrap();
        assert_eq!(epoch(&f), 3, "a forward jump past 0.15 is a seek");
        let f = st.frame(&bins, (2, 0.20)).unwrap();
        assert_eq!(epoch(&f), 4, "a track change resets");
        let f = st.frame(&bins, (2, 0.21)).unwrap();
        assert_eq!(epoch(&f), 4);
        // Rows shorter than 512 pad with zeros.
        assert!(f[HEADER_BYTES + 8..].iter().all(|&b| b == 0));
        assert!(st.frame(&[], (2, 0.22)).is_none(), "an empty row skips");
    }

    #[test]
    fn a_snapshot_restores_every_column_and_a_seek_discards_old_history() {
        let mut state = RibbonState::default();
        assert!(state.snapshot().is_none());
        state.frame(&[0.5; 512], (1, 0.05));
        state.frame(&[0.8; 512], (1, 0.10));
        let snapshot = state.snapshot().unwrap();
        assert_eq!(snapshot.len(), HEADER_BYTES + 512 * 2048);
        assert_eq!(epoch(&snapshot), 1);
        assert_eq!(sequence(&snapshot), 2);
        assert_eq!(snapshot[HEADER_BYTES + 20 * 512], 127, "the first spectrum remains in its time columns");
        assert_eq!(snapshot[HEADER_BYTES + 150 * 512], 204, "later columns retain the later spectrum");
        state.frame(&[0.2; 512], (1, 0.0));
        let after_seek = state.snapshot().unwrap();
        assert_eq!(epoch(&after_seek), 2);
        assert_eq!(after_seek[HEADER_BYTES], 51);
        assert_eq!(after_seek[HEADER_BYTES + 150 * 512], 0);
    }

    #[test]
    fn epochs_survive_coalescing_and_snapshots_identify_older_queued_rows() {
        let mut state = RibbonState::default();
        let first = state.frame(&[0.3; 512], (1, 0.0)).unwrap();
        let next = state.frame(&[0.4; 512], (1, 0.01)).unwrap();
        assert_eq!(epoch(&first), epoch(&next), "dropping the first row still leaves the reset epoch");
        assert!(sequence(&first) < sequence(&next));
        let snapshot = state.snapshot().unwrap();
        assert_eq!(epoch(&snapshot), epoch(&next));
        assert_eq!(sequence(&snapshot), sequence(&next));
        assert!(sequence(&first) < sequence(&snapshot), "the older row cannot overwrite the restored snapshot");
        let new_track = state.frame(&[0.5; 512], (2, 0.0)).unwrap();
        let after_reset = state.frame(&[0.6; 512], (2, 0.01)).unwrap();
        assert_eq!(epoch(&new_track), epoch(&after_reset));
        assert!(epoch(&after_reset) > epoch(&snapshot));
        assert!(sequence(&after_reset) > sequence(&snapshot));
    }
}
