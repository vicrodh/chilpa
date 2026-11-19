//! A year of listening as a mosaic: a square a day, a column a week, each
//! square as strong as the day was played against the year's busiest.
//!
//! The facade lays it out -- which column and row a day falls on, where a
//! month's name goes, how strong a square is -- so what draws it only
//! places squares.

use chilpa_i18n as i18n;
use chilpa_library::store::listening::{ListenDimension, ListenFilter, ListenOrder};
use serde_json::{json, Value};

use crate::listening_period::{self as period, Kind, Period};
use crate::listening_stats::{day_label, month_name, WEEKDAYS};

/// How many strengths a played day has; a day with nothing is 0.
const LEVELS: u64 = 4;

/// A day's strength, 1 to `LEVELS`, against the busiest day's plays.
fn level(plays: u64, most: u64) -> u64 {
    if plays == 0 || most == 0 {
        return 0;
    }
    ((plays * LEVELS).div_ceil(most)).clamp(1, LEVELS)
}

/// The first day of the grid: the start of the week that holds `first`,
/// with weeks starting on `week_start` (Monday 0).
fn grid_start(first: i64, week_start: usize) -> i64 {
    first - ((period::weekday(first) + 7 - week_start % 7) % 7) as i64
}

/// The squares of the year `this`, a day each, and where each month starts.
fn layout(this: &Period, today: i64, week_start: usize, plays: &std::collections::HashMap<String, u64>) -> (Vec<Value>, Vec<Value>, u64) {
    let start = grid_start(this.first, week_start);
    let most = plays.values().copied().max().unwrap_or(0);
    let mut months = Vec::new();
    let cells = (this.first..this.end)
        .map(|day| {
            let date = period::iso(day);
            let (_, month, date_of_month) = period::civil_from_days(day);
            let column = (day - start) / 7;
            if date_of_month == 1 {
                months.push(json!({"label": month_name(month), "column": column}));
            }
            let count = plays.get(&date).copied().unwrap_or(0);
            json!({
                "date": date, "column": column, "row": (day - start) % 7,
                "plays": count, "level": level(count, most), "later": day > today,
                "tip": day_label(&date, true),
            })
        })
        .collect();
    (cells, months, most)
}

impl crate::Chilpa {
    /// The mosaic of the year that holds `anchor` (a local date; empty or
    /// in the future is today), `back` years before it (after it, below
    /// zero), with weeks starting
    /// on `first_day` as Qt counts it, Monday 1 to Sunday 7.
    pub fn stats_mosaic_json(&mut self, anchor: &str, back: i64, first_day: i64) -> String {
        let Some(lib) = self.library() else { return "{}".into() };
        let week_start = (first_day - 1).rem_euclid(7) as usize;
        let today = lib.local_days(0, 1).ok().and_then(|days| days.first().and_then(|d| period::parse(d))).unwrap_or(0);
        let (now_year, ..) = period::civil_from_days(today);
        let first_year = lib.listening_first_day().ok().flatten().and_then(|day| period::parse(&day)).map_or(now_year, |day| period::civil_from_days(day).0);
        let day = period::parse(anchor).filter(|&d| d <= today).unwrap_or(today);
        let year = (period::civil_from_days(day).0 - back).clamp(first_year.min(now_year), now_year);
        let this = Period::holding(Kind::Year, period::days_from_civil(year, 1, 1), week_start);
        let filter = ListenFilter {
            from: lib.local_date_start(&period::iso(this.first)).ok(),
            until: lib.local_date_start(&period::iso(this.end)).ok(),
            ..ListenFilter::default()
        };
        let plays: std::collections::HashMap<String, u64> = lib
            .listening_buckets(&filter, ListenDimension::Day, ListenOrder::Key, 1000)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|b| b.key.map(|key| (key, b.metrics.plays)))
            .collect();
        let (cells, months, most) = layout(&this, today, week_start, &plays);
        let rows: Vec<String> = (0..7).map(|i| i18n::t(WEEKDAYS[(week_start + i) % 7])).collect();
        json!({
            "year": year,
            "earlier": year > first_year,
            "later": year < now_year,
            "columns": cells.last().and_then(|cell| cell["column"].as_i64()).unwrap_or(0) + 1,
            "cells": cells, "months": months, "rows": rows,
            "most": most, "levels": LEVELS,
            "plays": plays.values().sum::<u64>(),
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_is_as_strong_as_its_share_of_the_busiest() {
        assert_eq!(level(0, 40), 0, "a day with nothing has no strength");
        assert_eq!(level(1, 40), 1);
        assert_eq!(level(10, 40), 1);
        assert_eq!(level(11, 40), 2);
        assert_eq!(level(40, 40), 4);
        assert_eq!(level(3, 1), 4, "never past the strongest");
        assert_eq!(level(1, 1), 4, "the only day played is the busiest");
    }

    #[test]
    fn a_year_is_laid_out_a_week_a_column_from_the_week_that_holds_its_first_day() {
        // 2026 starts on a Thursday.
        let first = period::parse("2026-01-01").unwrap();
        let this = Period::holding(Kind::Year, first, 0);
        let today = period::parse("2026-11-18").unwrap();
        let plays = [("2026-01-01".to_string(), 2), ("2026-11-18".to_string(), 8)].into_iter().collect();
        let (cells, months, most) = layout(&this, today, 0, &plays);
        assert_eq!(cells.len(), 365);
        assert_eq!(most, 8);
        assert_eq!((cells[0]["column"].as_i64(), cells[0]["row"].as_i64()), (Some(0), Some(3)), "Thursday, with weeks from Monday");
        assert_eq!(cells[0]["level"], 1);
        assert_eq!((cells[4]["column"].as_i64(), cells[4]["row"].as_i64()), (Some(1), Some(0)), "the first Monday opens the second column");
        assert_eq!(cells[364]["column"], 52);
        assert_eq!(months.len(), 12);
        assert_eq!(months[1]["column"], 4, "February starts in the fifth week");
        let busiest = &cells[(today - first) as usize];
        assert_eq!((busiest["level"].as_u64(), busiest["later"].as_bool()), (Some(4), Some(false)));
        assert_eq!(cells[364]["later"], true, "a day still to come");
        // With weeks from Sunday the same Thursday is the fifth row.
        let (sunday, ..) = layout(&this, today, 6, &plays);
        assert_eq!(sunday[0]["row"], 4);
    }
}
