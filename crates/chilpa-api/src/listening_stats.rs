//! Listening stats as the window shows them: a period's report against the
//! one before it, and the history a page at a time
//! (borradores/listening-stats-v1-2026-10-09, and its calendar periods of
//! 2026-10-09).
//!
//! A period is the day, the week, the month or the year that holds a chosen
//! date, or the whole log, and it is compared with the one of its kind just
//! before. Everything comes from the play log through the library's
//! listening queries.

use chilpa_i18n as i18n;
use chilpa_library::store::listening::{ListenBucket, ListenDimension, ListenFilter, ListenOrder};
use chilpa_library::Library;
use serde_json::{json, Value};

use crate::artist_page::MONTHS;
use crate::listening_period::{self as period, Kind, Period};

/// A sample rate in kHz as a quality line says it: 44.1, 96.
pub(crate) fn khz(rate: u32) -> String {
    let k = f64::from(rate) / 1000.0;
    if k.fract() == 0.0 { format!("{}", k as u32) } else { format!("{k:.1}") }
}

/// How many of the top artists, records and tracks the report names.
const TOP: usize = 5;

/// I read the source id and a length-checked reference, including its colons.
fn source_key<'a>(key: &'a str, prefix: &str) -> Option<(i64, &'a str)> {
    let (source, rest) = key.strip_prefix(prefix)?.split_once(':')?;
    let (length, reference) = rest.split_once(':')?;
    (reference.chars().count() == length.parse::<usize>().ok()?)
        .then_some((source.parse().ok()?, reference))
}

fn plugin_art(catalog: &crate::source_catalog::Catalog, plugin: &str, track: &str, album: &str) -> String {
    if !track.is_empty() {
        let path = crate::source_opener::entry_of(plugin, track);
        if let Some((_, art)) = catalog.track_by_path(&path).filter(|(_, art)| !art.is_empty()) {
            return art;
        }
        catalog.want_art_of(&path);
    }
    if album.is_empty() { return String::new(); }
    catalog.card_by_identity(plugin, album).and_then(|card| catalog.card(card.id)).map(|card| card.art).unwrap_or_default()
}

/// The days of the week from Monday, as a date says them short.
const WEEKDAYS: [&str; 7] = [
    i18n::mark("Mon"), i18n::mark("Tue"), i18n::mark("Wed"), i18n::mark("Thu"), i18n::mark("Fri"), i18n::mark("Sat"), i18n::mark("Sun"),
];

/// A local date, "2026-10-09" (or the start of a local time), as its year,
/// month and day.
fn civil(day: &str) -> Option<(i64, u32, u32)> {
    period::parse(day).map(period::civil_from_days)
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
        // Its own msgid: "{} {}" is a month and its year, and a language
        // that orders those two apart from a day and its month ("Mai 1990",
        // "4. Okt.") needs one entry for each.
        i18n::t_args(i18n::mark("{1} {2}"), &[&date.to_string(), &month_name(month)])
    }
}

/// "Sat 4 Oct", or "Sat 4 Oct 2026".
pub(crate) fn day_label(day: &str, with_year: bool) -> String {
    match period::parse(day) {
        Some(days) => format!("{} {}", i18n::t(WEEKDAYS[period::weekday(days)]), date(day, with_year)),
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
        // One msgid for the whole range: a language that writes the year
        // first still keeps the two days together ("2026年10月3日–9日").
        (Some((y1, m1, d1)), Some((y2, m2, d2))) if y1 == y2 && m1 == m2 => i18n::t_args(
            i18n::mark("{1}–{2} {3} {4}"),
            &[&d1.to_string(), &d2.to_string(), &month_name(m2), &y2.to_string()],
        ),
        (Some((y1, ..)), Some((y2, ..))) if y1 == y2 => format!("{} \u{2013} {}", date(first, false), date(last, true)),
        _ => format!("{} \u{2013} {}", date(first, true), date(last, true)),
    }
}

/// How a period reads: "Fri 9 Oct 2026", "4–10 Oct 2026", "Oct 2026",
/// "2026".
fn period_label(p: &Period) -> String {
    let first = period::iso(p.first);
    match p.kind {
        Kind::Day => day_label(&first, true),
        Kind::Week => range_label(&first, &period::iso(p.end - 1)),
        Kind::Month => month_label(&first[..7]),
        Kind::Year => first[..4].to_string(),
        Kind::All => String::new(),
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

/// The series the hero draws, each bar beside the bar of the same place in
/// the period before: one an hour for a day, one a day for a week or a
/// month, one a month for a year or the whole log.
fn series(lib: &Library, this: &Period, previous: &Period, now: &ListenFilter, before: &ListenFilter) -> Vec<Value> {
    let dimension = match this.kind {
        Kind::Day => ListenDimension::Hour,
        Kind::Week | Kind::Month => ListenDimension::Day,
        Kind::Year | Kind::All => ListenDimension::Month,
    };
    let ours = plays_of(&lib.listening_buckets(now, dimension, ListenOrder::Key, 1000).unwrap_or_default());
    let theirs = plays_of(&lib.listening_buckets(before, dimension, ListenOrder::Key, 1000).unwrap_or_default());
    let months = |p: &Period| -> Vec<String> { (1..=12).map(|m| format!("{}-{m:02}", &period::iso(p.first)[..4])).collect() };
    let (mine, earlier): (Vec<String>, Vec<String>) = match this.kind {
        Kind::Day => {
            let hours: Vec<String> = (0..24).map(|h| format!("{h:02}")).collect();
            (hours.clone(), hours)
        }
        Kind::Week | Kind::Month => (this.days(), previous.days()),
        Kind::Year => (months(this), months(previous)),
        Kind::All => {
            let mut all: Vec<String> = ours.keys().cloned().collect();
            all.sort();
            (all, Vec::new())
        }
    };
    mine.iter()
        .enumerate()
        .map(|(i, key)| {
            let prev = earlier.get(i).and_then(|k| theirs.get(k)).copied().unwrap_or(0);
            let (label, tip) = match this.kind {
                Kind::Day => (key.parse::<u32>().map(|h| h.to_string()).unwrap_or_default(), format!("{key}:00")),
                Kind::Week => (period::parse(key).map(|d| i18n::t(WEEKDAYS[period::weekday(d)])).unwrap_or_default(), day_label(key, false)),
                Kind::Month => (civil(key).map(|(_, _, d)| d.to_string()).unwrap_or_default(), day_label(key, false)),
                Kind::Year | Kind::All => (civil(&format!("{key}-01")).map(|(_, m, _)| month_name(m)).unwrap_or_default(), month_label(key)),
            };
            json!({"key": key, "plays": ours.get(key).copied().unwrap_or(0), "prev": prev, "label": label, "tip": tip})
        })
        .collect()
}

impl crate::Chilpa {
    /// The report of the period of `kind` ("day", "week", "month", "year"
    /// or "all") that holds `anchor`, a local date ("2026-10-09"; empty or
    /// in the future is today), with weeks starting on `first_day` as Qt
    /// counts it, Monday 1 to Sunday 7.
    pub fn stats_report_json(&mut self, kind: &str, anchor: &str, first_day: i64) -> String {
        let catalog = self.catalog.clone();
        let Some(lib) = self.library() else { return "{}".into() };
        let kind = Kind::of(kind);
        let week_start = (first_day - 1).rem_euclid(7) as usize;
        let today = lib.local_days(0, 1).ok().and_then(|days| days.first().and_then(|d| period::parse(d))).unwrap_or(0);
        let day = period::parse(anchor).filter(|&d| d <= today).unwrap_or(today);
        let this = Period::holding(kind, day, week_start);
        let previous = this.before(week_start);
        let bounds = |p: &Period| ListenFilter {
            from: lib.local_date_start(&period::iso(p.first)).ok(),
            until: lib.local_date_start(&period::iso(p.end)).ok(),
            ..ListenFilter::default()
        };
        let (now, before) = match kind {
            Kind::All => (ListenFilter::default(), ListenFilter { from: Some(0), until: Some(1), ..ListenFilter::default() }),
            _ => (bounds(&this), bounds(&previous)),
        };
        let summary = lib.listening_summary(&now).unwrap_or_default();
        let earlier_summary = lib.listening_summary(&before).unwrap_or_default();
        let (artists, albums) = lib.listening_counts(&now).unwrap_or_default();
        let (artists_before, albums_before) = lib.listening_counts(&before).unwrap_or_default();
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
                let source = b.key.as_deref().and_then(|key| source_key(key, "source:"))
                    .and_then(|(source_id, reference)| {
                        let (provider, plugin) = lib.listening_source(source_id).ok().flatten()?;
                        if provider != crate::source_opener::PROVIDER { return None; }
                        catalog.card_by_identity(&plugin, reference).and_then(|card| catalog.card(card.id))
                    });
                let art = album.as_ref().and_then(|a| a.artwork_path.clone()).filter(|art| !art.is_empty())
                    .or_else(|| source.as_ref().map(|c| c.art.clone()).filter(|art| !art.is_empty()))
                    .or_else(|| b.key.as_deref().and_then(|key| source_key(key, "source:"))
                        .and_then(|(source_id, reference)| lib.listening_source_art(source_id, reference, true).ok().flatten()))
                    .unwrap_or_default();
                json!({
                    "title": album.as_ref().map(|a| a.title.clone()).or_else(|| source.as_ref().map(|c| c.title.clone())).unwrap_or(b.label.clone()),
                    "artist": album.as_ref().map(|a| a.album_artist.clone()).or_else(|| source.as_ref().map(|c| c.artist.clone())).unwrap_or_default(),
                    "art": art,
                    "id": id.or_else(|| source.as_ref().map(|c| c.id)).unwrap_or(0),
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
                let source = b.key.as_deref().and_then(|key| source_key(key, ""))
                    .and_then(|(source_id, reference)| {
                        let (provider, plugin) = lib.listening_source(source_id).ok().flatten()?;
                        (provider == crate::source_opener::PROVIDER).then(|| {
                            let path = crate::source_opener::entry_of(&plugin, reference);
                            let track = catalog.track_by_path(&path);
                            if track.as_ref().is_none_or(|(_, art)| art.is_empty()) { catalog.want_art_of(&path); }
                            track
                        }).flatten()
                    });
                let art = track.as_ref().and_then(|t| t.artwork_path.clone()).filter(|art| !art.is_empty())
                    .or_else(|| source.as_ref().map(|(_, art)| art.clone()).filter(|art| !art.is_empty()))
                    .or_else(|| b.key.as_deref().and_then(|key| source_key(key, ""))
                        .and_then(|(source_id, reference)| lib.listening_source_art(source_id, reference, false).ok().flatten()))
                    .unwrap_or_default();
                json!({
                    "title": b.label,
                    "artist": track.as_ref().map(|t| t.artist.clone()).or_else(|| source.as_ref().map(|(t, _)| t.artist.clone())).unwrap_or_default(),
                    "art": art,
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
        // A period still running counts the days it has had so far.
        let span = match kind {
            Kind::All => lib.listening_buckets(&now, ListenDimension::Day, ListenOrder::Key, 1000).map(|d| d.len() as i64).unwrap_or(1),
            _ => this.end.min(today + 1) - this.first,
        }
        .max(1);
        let chart = series(lib, &this, &previous, &now, &before);
        let current = kind == Kind::All || this.holds(today);
        let range = match kind {
            Kind::All => chart.first().and_then(|p| p["key"].as_str()).map(|key| i18n::t_args(i18n::mark("Since {}"), &[&month_label(key)])).unwrap_or_default(),
            _ => period_label(&this),
        };
        let comparison = match kind {
            Kind::All => String::new(),
            _ if current => period::current_comparison(kind),
            _ => i18n::t_args(i18n::mark("vs {}"), &[&period_label(&previous)]),
        };
        let by_hour: Vec<u64> = (0..24).map(|h| hours.get(&format!("{h:02}")).copied().unwrap_or(0)).collect();
        // From the week's first day; SQLite counts Sunday as 0.
        let by_weekday: Vec<Value> = (0..7)
            .map(|i| (week_start + i) % 7)
            .map(|day| json!({"label": i18n::t(WEEKDAYS[day]), "plays": weekdays.get(&((day + 1) % 7).to_string()).copied().unwrap_or(0)}))
            .collect();
        json!({
            "kind": kind_name(kind),
            "anchor": period::iso(day),
            "now": period::iso(today),
            "today": day == today,
            "current": current,
            "earlier": if kind == Kind::All { String::new() } else { period::iso(previous.first) },
            "later": if kind == Kind::All || this.end > today { String::new() } else { period::iso(this.after(week_start).first) },
            "range": range,
            "comparison": comparison,
            "listened": crate::format::span(summary.listened_ms),
            "plays": summary.plays,
            "plays_change": change(summary.plays, earlier_summary.plays),
            "listened_ms": summary.listened_ms,
            "listened_change": change(summary.listened_ms, earlier_summary.listened_ms),
            "series": chart,
            "artists": {"count": artists, "change": change(artists, artists_before), "top": top_artists},
            "albums": {"count": albums, "change": change(albums, albums_before), "top": top_albums},
            "tracks": {"count": summary.distinct_tracks, "change": change(summary.distinct_tracks, earlier_summary.distinct_tracks), "top": top_tracks},
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
    /// `per_page` listens, 20, 50 or 100, and with `search` only the listens
    /// whose track, artist or record hold every word of it.
    pub fn listening_history_json(&mut self, page: i64, per_page: i64, search: &str) -> String {
        let catalog = self.catalog.clone();
        let Some(lib) = self.library() else { return "{}".into() };
        let per_page = if matches!(per_page, 20 | 50 | 100) { per_page as usize } else { 20 };
        let page = page.max(1) as usize;
        let (rows, total) = lib.listening_history(&ListenFilter::default(), search, (page - 1) * per_page, per_page).unwrap_or_default();
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
                let art = e.artwork_path.clone().filter(|art| !art.is_empty()).unwrap_or_else(|| {
                    let Some((provider, plugin)) = e.source_id.and_then(|id| lib.listening_source(id).ok().flatten()) else { return String::new(); };
                    if provider == crate::source_opener::PROVIDER {
                        plugin_art(&catalog, &plugin, &e.external_id, &e.album_external_id)
                    } else {
                        String::new()
                    }
                });
                json!({
                    "time": e.local_time, "day": day, "clock": e.local_time.get(11..16).unwrap_or_default(),
                    "played": crate::format::clock(e.played_ms),
                    "length": if e.duration_ms > 0 { crate::format::clock(e.duration_ms) } else { String::new() },
                    "title": e.title, "artist": e.artist, "album": e.album,
                    "played_ms": e.played_ms, "duration_ms": e.duration_ms, "skipped": e.skipped,
                    "bit_perfect": e.bit_perfect, "quality": quality(e.bit_depth, e.sample_rate_hz),
                    "art": art, "album_id": e.album_id.unwrap_or(0),
                })
            })
            .collect();
        json!({"page": page.min(pages), "pages": pages, "per_page": per_page, "total": total, "rows": rows}).to_string()
    }
}

/// "24/96" for 24 bits at 96 kHz; empty when either is unknown.
pub(crate) fn quality(bits: Option<u8>, rate: Option<u32>) -> String {
    match (bits, rate) {
        (Some(bits), Some(rate)) => format!("{bits}/{}", khz(rate)),
        _ => String::new(),
    }
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Day => "day",
        Kind::Week => "week",
        Kind::Month => "month",
        Kind::Year => "year",
        Kind::All => "all",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_keys_keep_the_plugin_reference_intact() {
        assert_eq!(source_key("source:3:10:album:mx:ñ", "source:"), Some((3, "album:mx:ñ")));
        assert_eq!(source_key("3:8:track:42", ""), Some((3, "track:42")));
        assert_eq!(source_key("source:3:9:album:mx:ñ", "source:"), None);
        assert_eq!(source_key("track:12", ""), None);
    }

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
        assert_eq!(quality(Some(24), Some(96_000)), "24/96");
        assert_eq!(quality(None, Some(96_000)), "");
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
    fn a_period_reads_as_its_kind_says() {
        let day = period::parse("2026-10-09").unwrap();
        assert_eq!(period_label(&Period::holding(Kind::Day, day, 0)), "Fri 9 Oct 2026");
        assert_eq!(period_label(&Period::holding(Kind::Week, day, 0)), "5\u{2013}11 Oct 2026");
        assert_eq!(period_label(&Period::holding(Kind::Week, day, 6)), "4\u{2013}10 Oct 2026");
        assert_eq!(period_label(&Period::holding(Kind::Month, day, 0)), "Oct 2026");
        assert_eq!(period_label(&Period::holding(Kind::Year, day, 0)), "2026");
        assert_eq!(Kind::of("week"), Kind::Week);
        assert_eq!(Kind::of("whatever"), Kind::All);
    }
}
