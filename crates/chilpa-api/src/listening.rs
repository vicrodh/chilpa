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

impl crate::plays::ledger::Heard for crate::InProgress {
    fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn set_frames(&mut self, frames: u64) {
        self.played_frames = frames;
    }
}

impl crate::InProgress {
    /// The same track goes on at another output: what was heard so far is
    /// kept, and the count starts again at the new one's rate.
    pub(super) fn picked_up(&mut self, rate: u32, bits: Option<u8>, perfect: bool) {
        self.carried_ms = self.heard_ms();
        self.played_frames = 0;
        self.sample_rate_hz = rate;
        self.bit_depth = bits;
        self.bit_perfect &= perfect;
    }
}

impl Chilpa {
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
        cover: String,
    ) -> ListenContext {
        let mut context = ListenContext::default();
        // A source's track: its stable id, and its record's as the catalog
        // read it, so the history and the home can name the record and find
        // it again. Asked before the library, which holds the facade.
        let from_source = crate::source_opener::of_plugin(&path.to_string_lossy()).map(|(plugin, id)| {
            let track = self.catalog.track_by_path(&path.to_string_lossy()).map(|(track, _)| track);
            let record = track.as_ref().and_then(|t| self.catalog.card(t.album_id)).map(|card| card.identity);
            (plugin, id, track, record)
        });
        let Some(lib) = self.library() else {
            return context;
        };
        let local = path.is_absolute() || path.to_string_lossy().starts_with("sacd:");
        // A server's track: its entry is its id in the library, and the
        // source it comes from goes with the listen (ADR-017).
        if let Some((track, _)) = lib.track_by_entry(&path.to_string_lossy()).ok().flatten() {
            context = lib.listen_context(track).unwrap_or_default();
        } else if local {
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
        } else if let Some((plugin, id, track, record)) = from_source {
            context.source_id = lib.source_id(crate::source_opener::PROVIDER, &plugin).ok();
            if context.source_id.is_some() {
                context.external_id = id;
                context.album_external_id = record.unwrap_or_default();
            }
            if let Some(track) = track {
                context.title = track.title;
                context.artist = track.artist;
                context.album = track.album;
            }
        }
        if context.track_id.is_none() && context.title.is_empty() {
            if let Some(tags) = tags {
                context.title = tags.title.clone().unwrap_or_default();
                context.artist = tags.artist_line();
                context.album = tags.album.clone().unwrap_or_default();
                context.album_artist = tags.album_artist.clone().unwrap_or_default();
                context.genres = tags.genres.clone();
            }
        }
        if std::path::Path::new(&cover).is_absolute() {
            context.artwork_path = cover;
        }
        context.utc_offset_seconds = lib.local_utc_offset(started).ok().flatten();
        context
    }

    /// Writes a listen to the history. One nothing was heard of is none.
    /// A failure to save is said in the window only when `shown`.
    pub(super) fn record_listen(&mut self, current: crate::InProgress, skipped: bool, shown: bool) {
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
                if shown {
                    self.now.error =
                        chilpa_i18n::t_args("the listen could not be saved: {}", &[&error.to_string()]);
                }
                eprintln!("listening.record_failed error={error}");
                chilpa_core::trace_playback("facade", "listen_not_saved", format_args!("shown={shown} error={error}"));
            } else {
                self.home.changed();
            }
        }
    }
}
