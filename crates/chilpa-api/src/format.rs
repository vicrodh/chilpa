//! I format durations consistently for every shell.

/// A track's length: "3:32", or "1:02:03" past the hour.
pub fn clock(ms: u64) -> String {
    let s = ms / 1000;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// A long span: "4 h 18 min", "55 min", "6 s".
pub fn span(ms: u64) -> String {
    let minutes = ms / 60_000;
    if minutes == 0 {
        // A six second test disc does not last "0 min".
        return format!("{} s", ms / 1000);
    }
    let (h, m) = (minutes / 60, minutes % 60);
    if h == 0 {
        format!("{minutes} min")
    } else {
        format!("{h} h {m} min")
    }
}

/// The day a moment falls on, as "2026-09-25": the one way of writing a
/// date that reads the same in every language. In UTC, which is what a
/// source publishes in. `None` for a moment before 1970.
pub fn day(unix: i64) -> Option<String> {
    if unix < 0 {
        return None;
    }
    // Days since 1970 to a civil date, by the usual era arithmetic.
    let z = unix / 86_400 + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moment_reads_as_its_day() {
        assert_eq!(day(0).as_deref(), Some("1970-01-01"));
        assert_eq!(day(1_700_000_000).as_deref(), Some("2023-11-14"));
        assert_eq!(day(951_782_400).as_deref(), Some("2000-02-29"), "a leap day");
        assert_eq!(day(1_790_000_000).as_deref(), Some("2026-09-21"));
        assert_eq!(day(-1), None);
    }

    #[test]
    fn a_track_reads_as_minutes_and_seconds() {
        assert_eq!(clock(212_000), "3:32");
        assert_eq!(clock(5_000), "0:05");
        assert_eq!(clock(3_723_000), "1:02:03");
    }

    #[test]
    fn a_long_span_reads_in_hours_and_minutes() {
        assert_eq!(span(15_480_000), "4 h 18 min");
        assert_eq!(span(3_300_000), "55 min");
        assert_eq!(span(6_000), "6 s");
    }
}
