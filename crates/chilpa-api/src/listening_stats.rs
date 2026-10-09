//! Listening stats as the window shows them: a period's report against the
//! one before it, and the history a page at a time
//! (borradores/listening-stats-v1-2026-10-09).
//!
//! A period is a number of local days that ends today, or the whole log;
//! `back` walks to the periods before it. Everything comes from the play
//! log through the library's listening queries.

use chilpa_library::store::listening::{ListenBucket, ListenDimension, ListenFilter, ListenOrder};
use chilpa_i18n as i18n;
use chilpa_library::Library;
use serde_json::{json, Value};

use crate::artist_page::MONTHS;

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

/// The days of the week from Monday, as a date says them short.
const WEEKDAYS: [&str; 7] = [
    i18n::mark("Mon"), i18n::mark("Tue"), i18n::mark("Wed"), i18n::mark("Thu"), i18n::mark("Fri"), i18n::mark("Sat"), i18n::mark("Sun"),
];

/// A local date, "2026-10-09" (or the start of a local time), as its year,
/// month and day.
fn civil(day: &str) -> Option<(i64, u32, u32)> {
    let year = day.get(..4)?.parse().ok()?;
    let month: u32 = day.get(5..7)?.parse().ok()?;
    let date: u32 = day.get(8..10)?.parse().ok()?;
    ((1..=12).contains(&month) && (1..=31).contains(&date)).then_some((year, month, date))
}

/// The day of the week of a date, Monday 0: its days since 1970 by the
/// usual era arithmetic, and 1970-01-01 was a Thursday.
fn weekday(year: i64, month: u32, day: u32) -> usize {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    (days + 3).rem_euclid(7) as usize
}

fn month_name(month: u32) -> String {
    i18n::t(MONTHS[month as usize - 1])
}

/// "4 Oct", or with its year "4 Oct 2026": the artist page's forms, so a
/// language joins them once.
fn date(day: &str, with_year: bool) -> String {
    let Some((year, month, date)) = civil(day) else { return day.to_string() };
    if with_year {
        i18n::t_args(i18n::mark("{} {} {}"), &[&date.to_string(), &month_name(month), &year.to_string()])
    } else {
        i18n::t_args(i18n::mark("{} {}"), &[&date.to_string(), &month_name(month)])
    }
}

/// "Sat 4 Oct", or "Sat 4 Oct 2026".
fn day_label(day: &str, with_year: bool) -> String {
    match civil(day) {
        Some((year, month, date_of)) => format!("{} {}", i18n::t(WEEKDAYS[weekday(year, month, date_of)]), date(day, with_year)),
        None => day.to_string(),
    }
}

/// "Oct 2026", for a month's key "2026-10".
fn month_label(key: &str) -> String {
    match civil(&format!("{key}-01")) {
        Some((year, month, _)) => i18n::t_args(i18n::mark("{} {}"), &[&month_name(month), &year.to_string()]),
        None => key.to_string(),
    }
}

/// The days a period runs over: "3–9 Oct 2026", "28 Sep – 4 Oct 2026",
/// "10 Oct 2025 – 9 Oct 2026".
fn range_label(first: &str, last: &str) -> String {
    match (civil(first), civil(last)) {
        (Some((y1, m1, d1)), Some((y2, m2, _))) if y1 == y2 && m1 == m2 => format!("{d1}\u{2013}{}", date(last, true)),
        (Some((y1, ..)), Some((y2, ..))) if y1 == y2 => format!("{} \u{2013} {}", date(first, false), date(last, true)),
        _ => format!("{} \u{2013} {}", date(first, true), date(last, true)),
    }
}

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
            let (label, tip) = if monthly {
                (civil(&format!("{key}-01")).map(|(_, m, _)| month_name(m)).unwrap_or_default(), month_label(key))
            } else if days == Some(7) {
                (civil(key).map(|(y, m, d)| i18n::t(WEEKDAYS[weekday(y, m, d)])).unwrap_or_default(), day_label(key, false))
            } else {
                (civil(key).map(|(_, _, d)| d.to_string()).unwrap_or_default(), day_label(key, false))
            };
            json!({"key": key, "plays": ours.get(key).copied().unwrap_or(0), "prev": prev, "monthly": monthly, "label": label, "tip": tip})
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
        let best_day = day_buckets.first().filter(|b| b.key.is_some()).map(|b| json!({"date": b.label, "label": day_label(&b.label, true), "plays": b.metrics.plays}));
        let span = days.unwrap_or_else(|| lib.listening_buckets(&now, ListenDimension::Day, ListenOrder::Key, 1000).map(|d| d.len() as i64).unwrap_or(1)).max(1);
        let chart = series(lib, days, back, &now, &before);
        let range = match (range.first(), range.last()) {
            (Some(first), Some(last)) => range_label(first, last),
            _ => chart.first().and_then(|p| p["key"].as_str()).map(|key| i18n::t_args(i18n::mark("Since {}"), &[&month_label(key)])).unwrap_or_default(),
        };
        let by_hour: Vec<u64> = (0..24).map(|h| hours.get(&format!("{h:02}")).copied().unwrap_or(0)).collect();
        // Monday first; SQLite counts Sunday as 0.
        let by_weekday: Vec<u64> = [1, 2, 3, 4, 5, 6, 0].iter().map(|d| weekdays.get(&d.to_string()).copied().unwrap_or(0)).collect();
        json!({
            "period": period,
            "back": back,
            "range": range,
            "listened": crate::format::span(summary.listened_ms),
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
        let today = lib.local_days(0, 1).unwrap_or_default().pop().unwrap_or_default();
        let yesterday = lib.local_days(1, 1).unwrap_or_default().pop().unwrap_or_default();
        let rows: Vec<Value> = rows
            .into_iter()
            .map(|e| {
                let day = e.local_time.get(..10).unwrap_or_default();
                let day = if day == today {
                    i18n::t(i18n::mark("Today"))
                } else if day == yesterday {
                    i18n::t(i18n::mark("Yesterday"))
                } else {
                    day_label(day, true)
                };
                let quality = match (e.bit_depth, e.sample_rate_hz) {
                    (Some(bits), Some(rate)) => format!("{bits}/{}", khz(rate)),
                    _ => String::new(),
                };
                json!({
                    "time": e.local_time, "day": day, "clock": e.local_time.get(11..16).unwrap_or_default(),
                    "played": crate::format::clock(e.played_ms),
                    "length": if e.duration_ms > 0 { crate::format::clock(e.duration_ms) } else { String::new() },
                    "title": e.title, "artist": e.artist, "album": e.album,
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
    fn a_date_knows_its_day_of_the_week() {
        assert_eq!(weekday(1970, 1, 1), 3, "a Thursday");
        assert_eq!(weekday(2026, 10, 9), 4, "a Friday");
        assert_eq!(weekday(2024, 2, 29), 3, "a leap day, a Thursday");
        assert_eq!(weekday(2026, 10, 12), 0, "a Monday");
    }

    #[test]
    fn a_range_says_its_month_and_year_once() {
        assert_eq!(range_label("2026-10-03", "2026-10-09"), "3\u{2013}9 Oct 2026");
        assert_eq!(range_label("2026-09-28", "2026-10-04"), "28 Sep \u{2013} 4 Oct 2026");
        assert_eq!(range_label("2025-10-10", "2026-10-09"), "10 Oct 2025 \u{2013} 9 Oct 2026");
        assert_eq!(day_label("2026-10-04", true), "Sun 4 Oct 2026");
        assert_eq!(month_label("2026-10"), "Oct 2026");
        assert_eq!(civil("2026-10-09 08:12:00"), Some((2026, 10, 9)));
        assert_eq!(civil("2026-13-01"), None);
    }

    #[test]
    fn a_period_is_a_week_a_month_a_year_or_everything() {
        assert_eq!(days_of("week"), Some(7));
        assert_eq!(days_of("month"), Some(30));
        assert_eq!(days_of("year"), Some(365));
        assert_eq!(days_of("all"), None);
    }
}
