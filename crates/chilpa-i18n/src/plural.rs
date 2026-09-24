//! How many plural forms a language has, and which one a number takes.
//!
//! Read from the `Plural-Forms:` header of the catalog, because that is
//! where translators expect to declare it. Only the shapes that actually
//! appear in gettext headers are understood; anything else falls back to
//! the English rule, which is wrong for some languages but never crashes.

/// The plural rule of a catalog.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PluralRule {
    /// One form for everything: Japanese, Chinese, Korean.
    Single,
    /// `n != 1`: English, German, Dutch, Portuguese and the like.
    NotOne,
    /// `n > 1`: French, and Portuguese in Brazil.
    MoreThanOne,
    /// Three forms, the Slavic shape used by Russian.
    Slavic,
}

impl PluralRule {
    /// The rule a language uses when its catalog does not say.
    pub fn for_language(language: &str) -> Self {
        match language {
            "ja" | "zh" | "ko" => PluralRule::Single,
            "fr" => PluralRule::MoreThanOne,
            "ru" | "uk" | "pl" => PluralRule::Slavic,
            _ => PluralRule::NotOne,
        }
    }

    /// Reads the `Plural-Forms:` header, if there is one that is understood.
    pub fn from_header(header: &str) -> Option<Self> {
        let line = header.lines().find(|l| l.starts_with("Plural-Forms:"))?;
        let squeezed: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        let count: usize = squeezed
            .split("nplurals=")
            .nth(1)?
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .ok()?;
        let body = squeezed.split("plural=").nth(1).unwrap_or_default();
        Some(match (count, body) {
            (1, _) => PluralRule::Single,
            (2, b) if b.contains("n>1") => PluralRule::MoreThanOne,
            (2, _) => PluralRule::NotOne,
            (3, _) => PluralRule::Slavic,
            _ => return None,
        })
    }

    /// How many forms this rule has.
    pub fn count(&self) -> usize {
        match self {
            PluralRule::Single => 1,
            PluralRule::NotOne | PluralRule::MoreThanOne => 2,
            PluralRule::Slavic => 3,
        }
    }

    /// Which form a count takes.
    pub fn index(&self, n: i64) -> usize {
        match self {
            PluralRule::Single => 0,
            PluralRule::NotOne => usize::from(n != 1),
            PluralRule::MoreThanOne => usize::from(n > 1),
            PluralRule::Slavic => {
                let n = n.unsigned_abs();
                let (ten, hundred) = (n % 10, n % 100);
                if ten == 1 && hundred != 11 {
                    0
                } else if (2..=4).contains(&ten) && !(12..=14).contains(&hundred) {
                    1
                } else {
                    2
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_and_spanish_split_at_one() {
        let r = PluralRule::for_language("es");
        assert_eq!(r.index(0), 1, "zero is plural");
        assert_eq!(r.index(1), 0);
        assert_eq!(r.index(2), 1);
    }

    #[test]
    fn french_counts_zero_as_singular() {
        let r = PluralRule::for_language("fr");
        assert_eq!(r.index(0), 0);
        assert_eq!(r.index(1), 0);
        assert_eq!(r.index(2), 1);
    }

    #[test]
    fn a_language_with_one_form_always_picks_it() {
        let r = PluralRule::for_language("ja");
        assert_eq!(r.count(), 1);
        for n in [0, 1, 2, 11, 100] {
            assert_eq!(r.index(n), 0, "{n}");
        }
    }

    #[test]
    fn the_slavic_rule_hits_its_three_forms() {
        let r = PluralRule::Slavic;
        assert_eq!(r.index(1), 0);
        assert_eq!(r.index(21), 0);
        assert_eq!(r.index(11), 2, "eleven is not like one");
        assert_eq!(r.index(3), 1);
        assert_eq!(r.index(13), 2);
        assert_eq!(r.index(5), 2);
    }

    #[test]
    fn the_header_wins_over_the_language_default() {
        let r = PluralRule::from_header("Plural-Forms: nplurals=2; plural=(n > 1);\n").unwrap();
        assert_eq!(r, PluralRule::MoreThanOne);
        assert_eq!(
            PluralRule::from_header("Plural-Forms: nplurals=1; plural=0;\n").unwrap(),
            PluralRule::Single
        );
    }

    #[test]
    fn a_header_that_is_not_understood_does_not_decide() {
        assert_eq!(PluralRule::from_header("Language: es\n"), None);
        assert_eq!(PluralRule::from_header("Plural-Forms: nplurals=6; plural=x;"), None);
    }

    #[test]
    fn a_negative_count_does_not_panic() {
        assert_eq!(PluralRule::Slavic.index(-3), 1);
        assert_eq!(PluralRule::NotOne.index(-1), 1);
    }
}
