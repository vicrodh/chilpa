//! The command grammar is independent of the window and its input method.
#[derive(Debug, PartialEq, Eq)]
pub struct Line {
    pub name: String,
    pub target: String,
    pub argument: String,
}

pub fn parse(input: &str) -> Result<Line, String> {
    let input = input.trim().trim_start_matches([':', '/']).trim();
    let (head, argument) = input.split_once(char::is_whitespace).unwrap_or((input, ""));
    let (name, target) = head.split_once(':').unwrap_or((head, ""));
    let word = |s: &str| {
        s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if name.is_empty() || !word(name) || !word(target) {
        return Err(chilpa_i18n::t(
            "Enter a valid command; :help lists the commands.",
        ));
    }
    let argument = argument.trim();
    let argument = if argument.starts_with(['\'', '"']) {
        let quote = argument.chars().next().unwrap();
        if argument.len() < 2 || !argument.ends_with(quote) {
            return Err(chilpa_i18n::t("Close the quotation marks."));
        }
        &argument[1..argument.len() - 1]
    } else {
        argument
    };
    Ok(Line {
        name: name.to_ascii_lowercase(),
        target: target.into(),
        argument: argument.into(),
    })
}

pub fn number(input: &str, clock: bool) -> Result<(f64, bool), String> {
    let relative = input.starts_with(['+', '-']);
    let magnitude = input
        .strip_prefix('+')
        .or_else(|| input.strip_prefix('-'))
        .unwrap_or(input);
    if magnitude.starts_with(['+', '-']) {
        return Err(chilpa_i18n::t("Enter a valid number."));
    }
    let parts: Vec<_> = magnitude.split(':').collect();
    if parts.len() > if clock { 3 } else { 1 } || parts.iter().any(|p| p.is_empty()) {
        return Err(chilpa_i18n::t("Enter a valid number or time."));
    }
    let mut value = 0.0;
    for (i, part) in parts.iter().enumerate() {
        let n: f64 = part
            .parse()
            .map_err(|_| chilpa_i18n::t("Enter a valid number."))?;
        if !n.is_finite() || n < 0.0 || (i > 0 && n >= 60.0) {
            return Err(chilpa_i18n::t("Time is out of range."));
        }
        value = value * 60.0 + n;
        if !value.is_finite() { return Err(chilpa_i18n::t("Time is out of range.")); }
    }
    Ok((
        if input.starts_with('-') {
            -value
        } else {
            value
        },
        relative,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_keep_the_target_and_free_text() {
        assert_eq!(
            parse("/search:provider Iron Maiden").unwrap(),
            parse(":search:provider Iron Maiden").unwrap()
        );
        assert_eq!(
            parse("queue save \"Night songs\"").unwrap().argument,
            "save \"Night songs\""
        );
        assert_eq!(
            parse("search \"Iron Maiden\"").unwrap().argument,
            "Iron Maiden"
        );
        assert!(parse("search \"unfinished").is_err());
        assert!(parse("!shell").is_err());
    }
    #[test]
    fn time_and_relative_values_are_unambiguous() {
        assert_eq!(number("1:30", true).unwrap(), (90.0, false));
        assert_eq!(number("-10", true).unwrap(), (-10.0, true));
        for value in ["NaN", "inf", "1:60", "1::2", "--1", "1e308:0"] {
            assert!(number(value, true).is_err(), "{value}");
        }
        assert!(number("1:30", false).is_err());
    }
}
