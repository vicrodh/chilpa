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

#[cfg(test)]
mod tests {
    use super::*;

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
