//! The calendar periods the stats report covers: a day, a week, a month or
//! a year that holds a chosen date, the one of the same kind before it, and
//! how they read. Dates are local and counted in days since 1970-01-01.

use chilpa_i18n as i18n;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Day,
    Week,
    Month,
    Year,
    All,
}

impl Kind {
    pub(crate) fn of(name: &str) -> Self {
        match name {
            "day" => Self::Day,
            "week" => Self::Week,
            "month" => Self::Month,
            "year" => Self::Year,
            _ => Self::All,
        }
    }
}

/// Days since 1970-01-01 of a civil date, by the usual era arithmetic.
pub(crate) fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// The civil date of a count of days since 1970-01-01.
pub(crate) fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// "2026-10-09" as days since 1970-01-01.
pub(crate) fn parse(date: &str) -> Option<i64> {
    let year = date.get(..4)?.parse().ok()?;
    let month: u32 = date.get(5..7)?.parse().ok()?;
    let day: u32 = date.get(8..10)?.parse().ok()?;
    ((1..=12).contains(&month) && (1..=31).contains(&day)).then(|| days_from_civil(year, month, day))
}

/// Days since 1970-01-01 as "2026-10-09".
pub(crate) fn iso(days: i64) -> String {
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The day of the week, Monday 0; 1970-01-01 was a Thursday.
pub(crate) fn weekday(days: i64) -> usize {
    (days + 3).rem_euclid(7) as usize
}

/// A run of local days: from `first`, up to and without `end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Period {
    pub kind: Kind,
    pub first: i64,
    pub end: i64,
}

impl Period {
    /// The period of `kind` that holds `day`, with weeks starting on
    /// `week_start` (Monday 0). `All` has no days of its own.
    pub(crate) fn holding(kind: Kind, day: i64, week_start: usize) -> Self {
        let (year, month, _) = civil_from_days(day);
        let (first, end) = match kind {
            Kind::Day => (day, day + 1),
            Kind::Week => {
                let first = day - ((weekday(day) + 7 - week_start % 7) % 7) as i64;
                (first, first + 7)
            }
            Kind::Month => {
                let next = if month == 12 { days_from_civil(year + 1, 1, 1) } else { days_from_civil(year, month + 1, 1) };
                (days_from_civil(year, month, 1), next)
            }
            Kind::Year => (days_from_civil(year, 1, 1), days_from_civil(year + 1, 1, 1)),
            Kind::All => (day, day),
        };
        Self { kind, first, end }
    }

    /// The period of the same kind just before this one.
    pub(crate) fn before(&self, week_start: usize) -> Self {
        Self::holding(self.kind, self.first - 1, week_start)
    }

    /// The period of the same kind just after this one.
    pub(crate) fn after(&self, week_start: usize) -> Self {
        Self::holding(self.kind, self.end, week_start)
    }

    pub(crate) fn holds(&self, day: i64) -> bool {
        self.first <= day && day < self.end
    }

    /// Its days, first to last, as "2026-10-09".
    pub(crate) fn days(&self) -> Vec<String> {
        (self.first..self.end).map(iso).collect()
    }
}

/// The comparison a period reads as while it holds today: "This week vs
/// Last week"; a period gone by reads as the one it is compared with.
pub(crate) fn current_comparison(kind: Kind) -> String {
    i18n::t(match kind {
        Kind::Day => i18n::mark("Today vs Yesterday"),
        Kind::Week => i18n::mark("This week vs Last week"),
        Kind::Month => i18n::mark("This month vs Last month"),
        Kind::Year => i18n::mark("This year vs Last year"),
        Kind::All => return String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_goes_to_days_and_back() {
        for date in ["1970-01-01", "2000-02-29", "2026-10-09", "2026-12-31", "1969-12-31"] {
            assert_eq!(iso(parse(date).unwrap()), date);
        }
        assert_eq!(parse("1970-01-02"), Some(1));
        assert_eq!(weekday(parse("2026-10-09").unwrap()), 4, "a Friday");
    }

    #[test]
    fn a_week_starts_on_the_day_the_language_says() {
        let friday = parse("2026-10-09").unwrap();
        let monday = Period::holding(Kind::Week, friday, 0);
        assert_eq!((iso(monday.first), iso(monday.end - 1)), ("2026-10-05".into(), "2026-10-11".into()));
        let sunday = Period::holding(Kind::Week, friday, 6);
        assert_eq!((iso(sunday.first), iso(sunday.end - 1)), ("2026-10-04".into(), "2026-10-10".into()));
        // A Sunday opens its own week when weeks start on Sunday.
        let on_sunday = Period::holding(Kind::Week, parse("2026-10-04").unwrap(), 6);
        assert_eq!(iso(on_sunday.first), "2026-10-04");
        assert_eq!(iso(sunday.before(6).first), "2026-09-27");
    }

    #[test]
    fn a_month_and_a_year_are_the_calendar_s() {
        let day = parse("2026-03-15").unwrap();
        let march = Period::holding(Kind::Month, day, 0);
        assert_eq!((iso(march.first), iso(march.end)), ("2026-03-01".into(), "2026-04-01".into()));
        let february = march.before(0);
        assert_eq!(february.end - february.first, 28);
        let december = Period::holding(Kind::Month, parse("2025-12-31").unwrap(), 0);
        assert_eq!(iso(december.after(0).first), "2026-01-01");
        let year = Period::holding(Kind::Year, day, 0);
        assert_eq!((iso(year.first), year.end - year.first), ("2026-01-01".into(), 365));
        assert_eq!(year.before(0).end - year.before(0).first, 365);
        assert_eq!(Period::holding(Kind::Year, parse("2024-06-01").unwrap(), 0).days().len(), 366);
    }
}
