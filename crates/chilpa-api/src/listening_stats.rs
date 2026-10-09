//! Listening stats as the window shows them: a period's report against the
//! one before it, and the history a page at a time
//! (borradores/listening-stats-v1-2026-10-09).
//!
//! A period is a number of local days that ends today, or the whole log;
//! `back` walks to the periods before it. Everything comes from the play
//! log through the library's listening queries.

use chilpa_library::store::listening::{ListenBucket, ListenDimension, ListenFilter, ListenOrder};
use chilpa_library::Library;
use serde_json::{json, Value};

/// How many days a period holds; `None` is all of it.
fn days_of(period: &str) -> Option<i64> {
    match period {
        "week" => Some(7),
        "month" => Some(30),
        "year" => Some(365),
        _ => None,
    }
}

/// A sample rate in kHz as a quality line says it: 44.1, 96.
fn khz(rate: u32) -> String {
    let k = f64::from(rate) / 1000.0;
    if k.fract() == 0.0 { format!("{}", k as u32) } else { format!("{k:.1}") }
}

/// How many of the top artists, records and tracks the report names.
const TOP: usize = 5;

/// A bucket as the report's rows carry it.
fn plays_of(buckets: &[ListenBucket]) -> std::collections::HashMap<String, u64> {
    buckets.iter().filter_map(|b| b.key.clone().map(|k| (k, b.metrics.plays))).collect()
}

/// The change from one count to another, in whole percent; `None` when
/// there was nothing before to compare with.
fn change(now: u64, before: u64) -> Value {
    if before == 0 {
        Value::Null
    } else {
        json!(((now as f64 - before as f64) * 100.0 / before as f64).round() as i64)
    }
}

/// The series the hero draws: one bar a day for a week or a month, one a
/// month for a year or the whole log, each beside the bar of the period
/// before.
fn series(lib: &Library, days: Option<i64>, back: i64, now: &ListenFilter, before: &ListenFilter) -> Vec<Value> {
    let monthly = !matches!(days, Some(7) | Some(30));
    let dimension = if monthly { ListenDimension::Month } else { ListenDimension::Day };
    let ours = plays_of(&lib.listening_buckets(now, dimension, ListenOrder::Key, 1000).unwrap_or_default());
    let theirs = plays_of(&lib.listening_buckets(before, dimension, ListenOrder::Key, 1000).unwrap_or_default());
    let keys = |offset: i64, count: i64| -> Vec<String> {
        let mut found = lib.local_days(offset, count).unwrap_or_default();
        if monthly {
            found = found.into_iter().map(|d| d.chars().take(7).collect()).collect();
            found.dedup();
        }
        found
    };
    let (mine, previous) = match days {
        Some(n) => (keys(n * back + n - 1, n), keys(n * (back + 1) + n - 1, n)),
        None => {
            let mut all: Vec<String> = ours.keys().cloned().collect();
            all.sort();
            (all, Vec::new())
        }
    };
    mine.iter()
        .enumerate()
        .map(|(i, key)| {
            let prev = previous.get(i).and_then(|k| theirs.get(k)).copied().unwrap_or(0);
            json!({"key": key, "plays": ours.get(key).copied().unwrap_or(0), "prev": prev, "monthly": monthly})
        })
        .collect()
}

impl crate::Chilpa {
    /// The report of a period: "week", "month", "year" or "all", `back`
    /// periods before the one that ends today.
    pub fn stats_report_json(&mut self, period: &str, back: i64) -> String {
        let Some(lib) = self.library() else { return "{}".into() };
        let days = days_of(period);
        let back = if days.is_some() { back.max(0) } else { 0 };
        let (now, before) = match days {
            Some(n) => {
                let from = lib.local_day_start(n * back + n - 1).ok();
                let until = lib.local_day_start(n * back - 1).ok();
                let earlier = lib.local_day_start(n * (back + 1) + n - 1).ok();
                (ListenFilter { from, until, ..ListenFilter::default() }, ListenFilter { from: earlier, until: from, ..ListenFilter::default() })
            }
            None => (ListenFilter::default(), ListenFilter { from: Some(0), until: Some(1), ..ListenFilter::default() }),
        };
        let summary = lib.listening_summary(&now).unwrap_or_default();
        let previous = lib.listening_summary(&before).unwrap_or_default();
        let (artists, albums) = lib.listening_counts(&now).unwrap_or_default();
        let (artists_before, albums_before) = lib.listening_counts(&before).unwrap_or_default();
        let range = match days {
            Some(n) => lib.local_days(n * back + n - 1, n).unwrap_or_default(),
            None => Vec::new(),
        };
        let pictures = lib.artist_pictures().unwrap_or_default();
        let top = |dimension| lib.listening_buckets(&now, dimension, ListenOrder::Plays, TOP).unwrap_or_default();
        let top_artists: Vec<Value> = top(ListenDimension::Artist)
            .into_iter()
            .filter(|b| b.key.is_some())
            .map(|b| {
                let art = pictures.of(&b.label).map(str::to_string).or_else(|| lib.artist_cover(&b.label).ok().flatten()).unwrap_or_default();
                json!({"name": b.label, "plays": b.metrics.plays, "art": art})
            })
            .collect();
        let top_albums: Vec<Value> = top(ListenDimension::Album)
            .into_iter()
            .filter(|b| b.key.is_some())
            .map(|b| {
                let id = b.key.as_deref().and_then(|k| k.strip_prefix("album:")).and_then(|id| id.parse::<i64>().ok());
                let album = id.and_then(|id| lib.album(id).ok().flatten()).map(|(a, _)| a);
                json!({
                    "title": album.as_ref().map(|a| a.title.clone()).unwrap_or(b.label.clone()),
                    "artist": album.as_ref().map(|a| a.album_artist.clone()).unwrap_or_default(),
                    "art": album.as_ref().and_then(|a| a.artwork_path.clone()).unwrap_or_default(),
                    "id": id.unwrap_or(0),
                    "plays": b.metrics.plays,
                })
            })
            .collect();
        let top_tracks: Vec<Value> = top(ListenDimension::Track)
            .into_iter()
            .filter(|b| b.key.is_some())
            .map(|b| {
                let id = b.key.as_deref().and_then(|k| k.strip_prefix("track:")).and_then(|id| id.parse::<i64>().ok());
                let track = id.and_then(|id| lib.tracks_by_ids(&[id]).ok()).and_then(|mut rows| rows.pop());
                json!({
                    "title": b.label,
                    "artist": track.as_ref().map(|t| t.artist.clone()).unwrap_or_default(),
                    "art": track.as_ref().and_then(|t| t.artwork_path.clone()).unwrap_or_default(),
                    "plays": b.metrics.plays,
                })
            })
            .collect();
        let by_key = |dimension, limit| plays_of(&lib.listening_buckets(&now, dimension, ListenOrder::Key, limit).unwrap_or_default());
        let hours = by_key(ListenDimension::Hour, 24);
        let weekdays = by_key(ListenDimension::Weekday, 7);
        let genres: Vec<Value> = lib.listening_buckets(&now, ListenDimension::Genre, ListenOrder::Plays, 8).unwrap_or_default()
            .into_iter().filter(|b| b.key.is_some()).map(|b| json!({"name": b.label, "plays": b.metrics.plays})).collect();
        let mut decades: Vec<Value> = lib.listening_buckets(&now, ListenDimension::Decade, ListenOrder::Key, 20).unwrap_or_default()
            .into_iter().filter(|b| b.key.is_some()).map(|b| json!({"decade": b.label, "plays": b.metrics.plays})).collect();
        decades.reverse();
        let day_buckets = lib.listening_buckets(&now, ListenDimension::Day, ListenOrder::Plays, 1).unwrap_or_default();
        let best_day = day_buckets.first().filter(|b| b.key.is_some()).map(|b| json!({"date": b.label, "plays": b.metrics.plays}));
        let span = days.unwrap_or_else(|| lib.listening_buckets(&now, ListenDimension::Day, ListenOrder::Key, 1000).map(|d| d.len() as i64).unwrap_or(1)).max(1);
        let chart = series(lib, days, back, &now, &before);
        let by_hour: Vec<u64> = (0..24).map(|h| hours.get(&format!("{h:02}")).copied().unwrap_or(0)).collect();
        // Monday first; SQLite counts Sunday as 0.
        let by_weekday: Vec<u64> = [1, 2, 3, 4, 5, 6, 0].iter().map(|d| weekdays.get(&d.to_string()).copied().unwrap_or(0)).collect();
        json!({
            "period": period,
            "back": back,
            "first": range.first(),
            "last": range.last(),
            "plays": summary.plays,
            "plays_change": change(summary.plays, previous.plays),
            "listened_ms": summary.listened_ms,
            "listened_change": change(summary.listened_ms, previous.listened_ms),
            "series": chart,
            "artists": {"count": artists, "change": change(artists, artists_before), "top": top_artists},
            "albums": {"count": albums, "change": change(albums, albums_before), "top": top_albums},
            "tracks": {"count": summary.distinct_tracks, "change": change(summary.distinct_tracks, previous.distinct_tracks), "top": top_tracks},
            "hours": by_hour,
            "weekdays": by_weekday,
            "genres": genres,
            "decades": decades,
            "facts": {
                "listened_ms": summary.listened_ms,
                "per_day": (summary.plays as f64 / span as f64 * 10.0).round() / 10.0,
                "best_day": best_day,
                "bit_perfect": summary.bit_perfect,
                "skips": summary.skips,
                "events": summary.events,
            },
        })
        .to_string()
    }

    /// The history a page at a time, newest first: `page` from 1, with
    /// `per_page` listens, 20, 50 or 100.
    pub fn listening_history_json(&mut self, page: i64, per_page: i64) -> String {
        let Some(lib) = self.library() else { return "{}".into() };
        let per_page = if matches!(per_page, 20 | 50 | 100) { per_page as usize } else { 20 };
        let page = page.max(1) as usize;
        let (rows, total) = lib.listening_history(&ListenFilter::default(), (page - 1) * per_page, per_page).unwrap_or_default();
        let pages = (total as usize).div_ceil(per_page).max(1);
        let rows: Vec<Value> = rows
            .into_iter()
            .map(|e| {
                let quality = match (e.bit_depth, e.sample_rate_hz) {
                    (Some(bits), Some(rate)) => format!("{bits}/{}", khz(rate)),
                    _ => String::new(),
                };
                json!({
                    "time": e.local_time, "title": e.title, "artist": e.artist, "album": e.album,
                    "played_ms": e.played_ms, "duration_ms": e.duration_ms, "skipped": e.skipped,
                    "bit_perfect": e.bit_perfect, "quality": quality,
                    "art": e.artwork_path.unwrap_or_default(), "album_id": e.album_id.unwrap_or(0),
                })
            })
            .collect();
        json!({"page": page.min(pages), "pages": pages, "per_page": per_page, "total": total, "rows": rows}).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_is_a_whole_percent_and_nothing_before_is_no_change() {
        assert_eq!(change(536, 428), json!(25));
        assert_eq!(change(10, 20), json!(-50));
        assert_eq!(change(5, 0), Value::Null);
    }

    #[test]
    fn a_rate_reads_as_a_quality_line_says_it() {
        assert_eq!(khz(44_100), "44.1");
        assert_eq!(khz(96_000), "96");
    }

    #[test]
    fn a_period_is_a_week_a_month_a_year_or_everything() {
        assert_eq!(days_of("week"), Some(7));
        assert_eq!(days_of("month"), Some(30));
        assert_eq!(days_of("year"), Some(365));
        assert_eq!(days_of("all"), None);
    }
}
