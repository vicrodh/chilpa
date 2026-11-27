//! The listens of a chosen span of days, as the history and the rankings
//! read them: the artists, records or genres a span was made of, all of
//! them, a page at a time, each with its plays and the time it was heard.

use chilpa_library::store::listening::{ListenDimension, ListenFilter, ListenOrder};
use chilpa_library::Library;
use serde_json::{json, Value};

use crate::listening_period as period;
use crate::listening_tops as tops;

/// The days from `from` to `to`, both whole and both given as "2026-10-09":
/// the filter that reads them and the two days as they were understood. An
/// empty or unreadable end leaves that side open, and ends given the wrong
/// way round are put right.
pub(crate) fn span(lib: &Library, from: &str, to: &str) -> (ListenFilter, String, String) {
    let (mut first, mut last) = (period::parse(from), period::parse(to));
    if let (Some(a), Some(b)) = (first, last) {
        if a > b {
            (first, last) = (Some(b), Some(a));
        }
    }
    let filter = ListenFilter {
        from: first.and_then(|day| lib.local_date_start(&period::iso(day)).ok()),
        until: last.and_then(|day| lib.local_date_end(&period::iso(day)).ok()),
        ..ListenFilter::default()
    };
    (filter, first.map(period::iso).unwrap_or_default(), last.map(period::iso).unwrap_or_default())
}

/// What a ranking of `kind` ranks: "artists", "albums" or "genres".
fn dimension(kind: &str) -> Option<ListenDimension> {
    match kind {
        "artists" => Some(ListenDimension::Artist),
        "albums" => Some(ListenDimension::Album),
        "genres" => Some(ListenDimension::Genre),
        _ => None,
    }
}

/// The days a span can be chosen within: the first listen there is, and
/// today.
pub(crate) fn bounds(lib: &Library) -> (String, String) {
    let first = lib.listening_first_day().ok().flatten().unwrap_or_default();
    let today = lib.local_days(0, 1).ok().and_then(|mut days| days.pop()).unwrap_or_default();
    (first, today)
}

impl crate::Chilpa {
    /// One page of the ranking of `kind`, "artists", "albums" or "genres",
    /// over the days from `from` to `to` (either may be empty for no
    /// bound): `page` from 1, with 20, 50 or 100 entries, most played
    /// first, each with its place, its plays and the time it was heard.
    pub fn listening_ranking_json(&mut self, kind: &str, from: &str, to: &str, page: i64, per_page: i64) -> String {
        let catalog = self.catalog.clone();
        let Some(dimension) = dimension(kind) else { return "{}".into() };
        let Some(lib) = self.library() else { return "{}".into() };
        let per_page = if matches!(per_page, 20 | 50 | 100) { per_page as usize } else { 50 };
        let (filter, from, to) = span(lib, from, to);
        let read = |page: usize| lib.listening_ranking(&filter, dimension, ListenOrder::Plays, (page - 1) * per_page, per_page).unwrap_or_default();
        let mut page = page.max(1) as usize;
        let (mut buckets, total) = read(page);
        let pages = (total as usize).div_ceil(per_page).max(1);
        // A page past the last, as after a narrower span, is the last.
        if page > pages {
            page = pages;
            buckets = read(page).0;
        }
        // The first of the whole ranking sets how long a bar is on every page.
        let most = lib.listening_ranking(&filter, dimension, ListenOrder::Plays, 0, 1).ok()
            .and_then(|(first, _)| first.first().map(|b| b.metrics.plays)).unwrap_or(0);
        let mut rows: Vec<Value> = match dimension {
            ListenDimension::Artist => tops::artist_rows(lib, buckets),
            ListenDimension::Album => tops::album_rows(lib, &catalog, buckets),
            _ => buckets.into_iter().map(|b| json!({"name": b.label, "plays": b.metrics.plays, "listened_ms": b.metrics.listened_ms})).collect(),
        };
        for (at, row) in rows.iter_mut().enumerate() {
            let listened = row["listened_ms"].as_u64().unwrap_or(0);
            row["rank"] = json!((page - 1) * per_page + at + 1);
            row["listened"] = json!(crate::format::span(listened));
        }
        let (first, today) = bounds(lib);
        json!({
            "kind": kind, "from": from, "to": to, "first": first, "today": today,
            "page": page, "pages": pages, "per_page": per_page, "total": total, "most": most, "rows": rows,
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listened(api: &mut crate::Chilpa, track: i64, at: i64) {
        let lib = api.library().unwrap();
        let context = lib.listen_context(track).unwrap();
        lib.record_listen(&chilpa_library::store::listening::ListenEvent {
            context, started_at: at, played_ms: 1000, duration_ms: 1000, skipped: false,
            sample_rate_hz: Some(44_100), bit_depth: Some(16), bit_perfect: false,
        }).unwrap();
    }

    #[test]
    fn ends_given_the_wrong_way_round_are_put_right_and_an_empty_one_stays_open() {
        let lib = Library::in_memory().unwrap();
        let (filter, from, to) = span(&lib, "2026-10-09", "2026-10-01");
        assert_eq!((from.as_str(), to.as_str()), ("2026-10-01", "2026-10-09"));
        assert_eq!(filter.from, lib.local_date_start("2026-10-01").ok());
        assert_eq!(filter.until, lib.local_date_end("2026-10-09").ok(), "the last day is whole");
        let (open, from, to) = span(&lib, "", "not a day");
        assert_eq!((open.from, open.until, from.as_str(), to.as_str()), (None, None, "", ""));
    }

    #[test]
    fn a_ranking_page_numbers_its_entries_from_the_whole_ranking() {
        let mut api = crate::Chilpa::with_listed(Vec::new(), Vec::new());
        let lib = api.library().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.connection().execute_batch(&format!(
            "INSERT INTO tracks(id, source_id, external_id, title, artist, added_at) VALUES
             (9101, {source}, '/ranking/a.flac', 'A', 'Ranked First', 0),
             (9102, {source}, '/ranking/b.flac', 'B', 'Ranked Second', 0);")).unwrap();
        // Far from any other test's listens, so the span holds only these;
        // at noon, so the day is the same in any time zone.
        let day = 20_000 * 86_400 + 43_200;
        for (track, at) in [(9101, day + 10), (9101, day + 20), (9102, day + 30)] {
            listened(&mut api, track, at);
        }
        let date = period::iso(20_000);
        let doc: Value = serde_json::from_str(&api.listening_ranking_json("artists", &date, &date, 2, 20)).unwrap();
        assert_eq!(doc["total"], 2);
        assert_eq!(doc["page"], 1, "a page past the last is the last");
        assert_eq!(doc["rows"].as_array().unwrap().len(), 2, "and shows what the last holds");
        let doc: Value = serde_json::from_str(&api.listening_ranking_json("artists", &date, &date, 1, 20)).unwrap();
        let names: Vec<(&str, u64)> = doc["rows"].as_array().unwrap().iter().map(|r| (r["name"].as_str().unwrap(), r["rank"].as_u64().unwrap())).collect();
        assert_eq!(names, [("Ranked First", 1), ("Ranked Second", 2)]);
        assert_eq!(doc["most"], 2);
        assert_eq!(doc["rows"][0]["listened"], crate::format::span(2000));
        assert_eq!(api.listening_ranking_json("tracks", "", "", 1, 20), "{}", "only artists, records and genres are ranked");
    }
}
