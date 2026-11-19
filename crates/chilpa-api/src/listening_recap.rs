//! The recap: one month or one year gone by, as a card to look back at.
//! Its first artist, record and track with the four behind each, and nine
//! numbers: the minutes, the plays and the plays a day; how many tracks,
//! records and artists; and how many of each were heard for the first time.
//!
//! The numbers are the play log's, like the stats report's: a play is a
//! track heard at least by half, and the minutes are the time that
//! sounded, not the length of what was started.

use chilpa_i18n as i18n;
use chilpa_library::store::listening::ListenFilter;
use serde_json::{json, Value};

use crate::listening_period::{self as period, Kind, Period};
use crate::listening_tops as tops;

/// How many of each the card names: the first, large, and four behind it.
const TOP: usize = 5;

/// The month before the one that holds `today`, as (year, month).
fn month_before(today: i64) -> (i64, u32) {
    let (year, month, _) = period::civil_from_days(today);
    if month == 1 { (year - 1, 12) } else { (year, month - 1) }
}

/// A month's full name, or nothing for a number that is no month.
fn month_name(month: u32) -> String {
    crate::album_versions::MONTH_NAMES.get(month.wrapping_sub(1) as usize).map(|name| i18n::t(name)).unwrap_or_default()
}

/// The card's title: "Your November 2026 recap", "Your 2026 recap".
fn title(year: i64, month: u32) -> String {
    if month == 0 {
        i18n::t_args(i18n::mark("Your {} recap"), &[&year.to_string()])
    } else {
        i18n::t_args(i18n::mark("Your {} {} recap"), &[&month_name(month), &year.to_string()])
    }
}

/// Each of a top's rows with its time in whole minutes, as the card says it.
fn with_minutes(rows: Vec<Value>) -> Vec<Value> {
    rows.into_iter()
        .map(|mut row| {
            let minutes = row["listened_ms"].as_u64().unwrap_or(0) / 60_000;
            row["minutes"] = json!(minutes);
            row
        })
        .collect()
}

impl crate::Chilpa {
    /// The recap of a month (`month` 1 to 12) or of a whole year (`month`
    /// 0). A year of 0 is the month before this one, where the page opens.
    /// What cannot be shown -- a month still to come, one before the first
    /// listen -- comes back empty, with what can be chosen instead.
    pub fn recap_json(&mut self, year: i64, month: i64) -> String {
        let catalog = self.catalog.clone();
        let Some(lib) = self.library() else { return "{}".into() };
        let today = lib.local_days(0, 1).ok().and_then(|days| days.first().and_then(|d| period::parse(d))).unwrap_or(0);
        let (now_year, now_month, _) = period::civil_from_days(today);
        let first = lib.listening_first_day().ok().flatten().and_then(|day| period::parse(&day)).map(period::civil_from_days);
        let (first_year, first_month) = first.map_or((now_year, now_month), |(y, m, _)| (y, m));
        let (year, month) = if year <= 0 { month_before(today) } else { (year.clamp(1970, 9999), month.clamp(0, 12) as u32) };
        let this = if month == 0 {
            Period::holding(Kind::Year, period::days_from_civil(year, 1, 1), 0)
        } else {
            Period::holding(Kind::Month, period::days_from_civil(year, month, 1), 0)
        };
        let months: Vec<String> = (1..=12).map(|m| crate::listening_stats::month_name(m)).collect();
        let choices = json!({
            "year": year, "month": month, "title": title(year, month),
            "first_year": first_year, "first_month": first_month,
            "now_year": now_year, "now_month": now_month,
            "months": months, "any": first.is_some(),
        });
        // Nothing sounded in a period that has not started.
        if this.first > today {
            return merged(choices, json!({"empty": true})).to_string();
        }
        let filter = ListenFilter {
            from: lib.local_date_start(&period::iso(this.first)).ok(),
            until: lib.local_date_start(&period::iso(this.end)).ok(),
            ..ListenFilter::default()
        };
        let summary = lib.listening_summary(&filter).unwrap_or_default();
        let (artists, albums) = lib.listening_counts(&filter).unwrap_or_default();
        let (new_tracks, new_albums, new_artists) = lib.listening_firsts(&filter).unwrap_or_default();
        let top_artists = with_minutes(tops::artists(lib, &filter, TOP));
        let top_albums = with_minutes(tops::albums(lib, &catalog, &filter, TOP));
        let top_tracks = with_minutes(tops::tracks(lib, &catalog, &filter, TOP));
        // A period still running counts the days it has had so far.
        let days = (this.end.min(today + 1) - this.first).max(1);
        let report = json!({
            // The card needs a first of each to be a card.
            "empty": top_artists.is_empty() || top_albums.is_empty() || top_tracks.is_empty(),
            "artists": top_artists, "albums": top_albums, "tracks": top_tracks,
            "minutes": summary.listened_ms / 60_000,
            "plays": summary.plays,
            "per_day": (summary.plays as f64 / days as f64 * 10.0).round() / 10.0,
            "tracks_count": summary.distinct_tracks, "albums_count": albums, "artists_count": artists,
            "new_tracks": new_tracks, "new_albums": new_albums, "new_artists": new_artists,
        });
        merged(choices, report).to_string()
    }
}

/// Two documents as one: the second's fields beside the first's.
fn merged(mut into: Value, from: Value) -> Value {
    if let (Some(into), Value::Object(from)) = (into.as_object_mut(), from) {
        into.extend(from);
    }
    into
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_opens_on_the_month_before_this_one() {
        let day = |date| period::parse(date).unwrap();
        assert_eq!(month_before(day("2026-11-18")), (2026, 10));
        assert_eq!(month_before(day("2026-01-03")), (2025, 12), "December, not the whole year before");
        assert_eq!(month_before(day("2026-03-31")), (2026, 2), "whatever day of the month it is");
    }

    #[test]
    fn a_top_row_says_its_time_in_whole_minutes() {
        let rows = with_minutes(vec![json!({"name": "A", "listened_ms": 179_999}), json!({"name": "B"})]);
        assert_eq!(rows[0]["minutes"], 2);
        assert_eq!(rows[1]["minutes"], 0);
    }

    #[test]
    fn two_documents_read_as_one() {
        assert_eq!(merged(json!({"year": 2026}), json!({"empty": true})), json!({"year": 2026, "empty": true}));
    }
}
