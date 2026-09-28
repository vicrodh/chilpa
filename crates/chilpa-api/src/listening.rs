//! I expose listening queries without exposing a database to the shell.

use crate::Chilpa;
pub use chilpa_library::store::listening::{
    ListenBucket, ListenContext, ListenDimension, ListenEvent, ListenFilter, ListenMetrics,
    ListenOrder, TimeBasis,
};

impl crate::InProgress {
    pub(super) fn heard_ms(&self) -> u64 {
        self.carried_ms.saturating_add(
            self.played_frames.saturating_mul(1000) / u64::from(self.sample_rate_hz.max(1)),
        )
    }
}

impl Chilpa {
    pub(super) fn resume_listen(
        &mut self,
        path: &std::path::Path,
        rate: u32,
        bits: Option<u8>,
        perfect: bool,
    ) -> bool {
        let Some(current) = self
            .in_progress
            .as_mut()
            .filter(|c| c.resuming && c.path == path)
        else {
            return false;
        };
        current.carried_ms = current.heard_ms();
        current.played_frames = 0;
        current.sample_rate_hz = rate;
        current.bit_depth = bits;
        current.bit_perfect &= perfect;
        current.resuming = false;
        true
    }
    pub fn listening_summary(&mut self, filter: &ListenFilter) -> Result<ListenMetrics, String> {
        self.library()
            .ok_or_else(|| chilpa_i18n::t("the library is unavailable"))?
            .listening_summary(filter)
            .map_err(|e| e.to_string())
    }

    pub fn listening_buckets(
        &mut self,
        filter: &ListenFilter,
        dimension: ListenDimension,
        order: ListenOrder,
        limit: usize,
    ) -> Result<Vec<ListenBucket>, String> {
        self.library()
            .ok_or_else(|| chilpa_i18n::t("the library is unavailable"))?
            .listening_buckets(filter, dimension, order, limit)
            .map_err(|e| e.to_string())
    }

    pub(super) fn context_at_start(
        &mut self,
        path: &std::path::Path,
        tags: Option<&chilpa_tags::Tags>,
        started: i64,
    ) -> ListenContext {
        let mut context = ListenContext::default();
        let Some(lib) = self.library() else {
            return context;
        };
        let local = path.is_absolute() || path.to_string_lossy().starts_with("sacd:");
        if local {
            context = lib
                .local_listen_context(&path.to_string_lossy())
                .ok()
                .flatten()
                .unwrap_or_default();
            if context.source_id.is_none() {
                context.source_id = lib.source_id("local", "default").ok();
                if context.source_id.is_some() {
                    context.external_id = path.to_string_lossy().into_owned();
                }
            }
        }
        if context.track_id.is_none() {
            if let Some(tags) = tags {
                context.title = tags.title.clone().unwrap_or_default();
                context.artist = tags.artist_line();
                context.album = tags.album.clone().unwrap_or_default();
                context.album_artist = tags.album_artist.clone().unwrap_or_default();
                context.genres = tags.genres.clone();
            }
        }
        context.utc_offset_seconds = lib.local_utc_offset(started).ok().flatten();
        context
    }

    pub(super) fn log_play(&mut self, skipped: bool) {
        let Some(current) = self.in_progress.take() else {
            return;
        };
        if current.sample_rate_hz == 0 || current.heard_ms() == 0 {
            return;
        }
        let played_ms = current.heard_ms();
        let event = ListenEvent {
            context: current.context,
            started_at: current.started_at,
            played_ms,
            duration_ms: current.duration_ms,
            skipped,
            sample_rate_hz: Some(current.sample_rate_hz),
            bit_depth: current.bit_depth,
            bit_perfect: current.bit_perfect,
        };
        if let Some(lib) = self.library() {
            if let Err(error) = lib.record_listen(&event) {
                self.now.error =
                    chilpa_i18n::t_args("the listen could not be saved: {}", &[&error.to_string()]);
                eprintln!("listening.record_failed error={error}");
            }
        }
    }
}
