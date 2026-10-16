//! Small readings the three protocols share.

use serde_json::Value;

/// Percent-encoding for a query value or a header, keeping only what can
/// never be mistaken for a separator.
pub(crate) fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// A string field, trimmed; `None` when it is missing or empty.
pub(crate) fn string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// An id, which some servers write as a number (Funkwhale) and the rest as
/// a string.
pub(crate) fn id(value: &Value, key: &str) -> Option<String> {
    match value.get(key)? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// A whole number, which some servers write as a string. Zero is what a
/// server writes when it does not know, so it reads as nothing.
pub(crate) fn number(value: &Value, key: &str) -> Option<u64> {
    let n = match value.get(key)? {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }?;
    (n > 0).then_some(n)
}

/// Seconds since 1970 of an ISO 8601 time in UTC ("2026-09-28T21:53:58.71Z").
/// Anything else, `None`: a date that cannot be read is no date at all.
pub(crate) fn unix_time(text: &str) -> Option<i64> {
    let date = text.get(..10)?;
    let mut parts = date.split('-');
    let (y, m, d): (i64, i64, i64) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let (hour, minute, second) = match text.get(11..19) {
        Some(time) => {
            let mut t = time.split(':');
            (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?)
        }
        None => (0, 0, 0),
    };
    // Days from the civil date, after Howard Hinnant.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// A random string of that many hex digits.
pub(crate) fn random_hex(digits: usize) -> String {
    use rand::RngExt;
    let mut bytes = vec![0u8; digits.div_ceil(2)];
    rand::rng().fill(&mut bytes[..]);
    let mut out: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    out.truncate(digits);
    out
}

/// Joins performers the way the library does (`chilpa_tags::Tags::artist_line`).
pub(crate) fn artist_line(names: &[String]) -> String {
    names.iter().map(|n| n.trim()).filter(|n| !n.is_empty()).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_reads_as_seconds_since_1970() {
        assert_eq!(unix_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(unix_time("2026-09-28T21:53:58.710398049Z"), Some(1_790_632_438));
        assert_eq!(unix_time("2000-02-29"), Some(951_782_400));
        assert_eq!(unix_time("yesterday"), None);
        assert_eq!(unix_time("2026-13-01"), None);
    }

    #[test]
    fn a_number_or_an_id_is_read_however_it_was_written() {
        let v: Value = serde_json::json!({"a": 7, "b": "8", "c": 0, "d": "", "e": 12.0, "id": 2059});
        assert_eq!((number(&v, "a"), number(&v, "b"), number(&v, "c"), number(&v, "d"), number(&v, "e")), (Some(7), Some(8), None, None, Some(12)));
        assert_eq!(id(&v, "id").as_deref(), Some("2059"));
    }

    #[test]
    fn a_value_is_encoded_so_nothing_in_it_can_cut_the_query() {
        assert_eq!(encode("a b&c=d/é"), "a%20b%26c%3Dd%2F%C3%A9");
    }

    #[test]
    fn a_salt_is_fresh_every_time() {
        let a = random_hex(12);
        assert_eq!(a.len(), 12);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, random_hex(12));
    }
}
