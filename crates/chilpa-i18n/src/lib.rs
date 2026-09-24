//! The translation catalog, with no frontend attached.
//!
//! The source language is English and the msgid **is** the English string.
//! That is what makes the rule of ADR-001 workable: the whole repository is
//! written in English, and Spanish lives in `translations/`, not in the
//! code. A string with no translation shows its msgid, which is a correct
//! English sentence rather than a placeholder.
//!
//! Nothing here knows about Qt. The window asks through the bridge, and a
//! headless build asks directly.

pub mod plural;
pub mod po;

pub use plural::PluralRule;
pub use po::Catalog;

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// The languages that ship, by the index stored in [`CURRENT`].
const LANGUAGES: [&str; 2] = ["en", "es"];

/// The catalogs, owned by this crate. Keeping them here and not next to a
/// frontend means replacing the window cannot break the translations.
const PO: [&str; 2] = [
    include_str!("../translations/en/LC_MESSAGES/chilpa.po"),
    include_str!("../translations/es/LC_MESSAGES/chilpa.po"),
];

static CURRENT: AtomicU8 = AtomicU8::new(0);
static CATALOGS: [OnceLock<Catalog>; 2] = [OnceLock::new(), OnceLock::new()];

fn index_of(language: &str) -> Option<u8> {
    LANGUAGES.iter().position(|l| *l == language).map(|i| i as u8)
}

fn catalog(index: u8) -> &'static Catalog {
    let index = usize::from(index).min(LANGUAGES.len() - 1);
    CATALOGS[index].get_or_init(|| Catalog::parse(LANGUAGES[index], PO[index]))
}

fn current() -> &'static Catalog {
    catalog(CURRENT.load(Ordering::Relaxed))
}

/// The languages that can be chosen.
pub fn languages() -> &'static [&'static str] {
    &LANGUAGES
}

/// Sets the language. An unknown code leaves it as it was, because falling
/// back silently is better than a window that suddenly speaks English.
pub fn set_language(language: &str) {
    if let Some(i) = index_of(language) {
        CURRENT.store(i, Ordering::Relaxed);
    }
}

pub fn current_language() -> &'static str {
    LANGUAGES[usize::from(CURRENT.load(Ordering::Relaxed))]
}

/// What the environment asks for, in POSIX order: `LC_ALL`, `LC_MESSAGES`,
/// `LANG`. Does not change anything; the caller decides.
pub fn from_environment() -> &'static str {
    let raw = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default();
    let prefix: String =
        raw.chars().take_while(char::is_ascii_alphabetic).collect::<String>().to_lowercase();
    match index_of(&prefix) {
        Some(i) => LANGUAGES[usize::from(i)],
        None => "en",
    }
}

/// Marks a literal so the extractor finds it, without translating here.
///
/// For strings that are defined in one place and shown in another: the
/// definition gets `mark`, the place that shows it calls [`t`].
pub const fn mark(s: &'static str) -> &'static str {
    s
}

/// Translates. Falls back to the msgid, which is already English.
pub fn t(msgid: &str) -> String {
    current().get(msgid).unwrap_or(msgid).to_string()
}

/// Translates a count. Falls back to the English pair.
pub fn tn(singular: &str, plural: &str, n: i64) -> String {
    let cat = current();
    if let Some(found) = cat.get_plural(singular, cat.plural_rule().index(n)) {
        return found.to_string();
    }
    if n == 1 { singular.to_string() } else { plural.to_string() }
}

/// Translates and fills in `{}` holes, in order.
pub fn t_args(msgid: &str, args: &[&str]) -> String {
    fill(&t(msgid), args)
}

/// Both at once: a count and its holes.
pub fn tf(singular: &str, plural: &str, n: i64, args: &[&str]) -> String {
    fill(&tn(singular, plural, n), args)
}

/// Replaces each `{}` with the next argument.
///
/// Holes that have no argument are left alone rather than swallowed: a
/// translation with one `{}` too many should look wrong, not lose text.
fn fill(text: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut next = args.iter();
    while let Some(at) = rest.find("{}") {
        out.push_str(&rest[..at]);
        match next.next() {
            Some(a) => out.push_str(a),
            None => out.push_str("{}"),
        }
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untranslated_string_shows_its_english() {
        // The msgid is the English sentence, so the fallback is already a
        // correct one and never a placeholder.
        set_language("es");
        assert_eq!(t("A string nobody has translated"), "A string nobody has translated");
        set_language("en");
    }

    #[test]
    fn an_unknown_language_leaves_the_current_one_alone() {
        set_language("es");
        set_language("klingon");
        assert_eq!(current_language(), "es");
        set_language("en");
    }

    #[test]
    fn holes_are_filled_in_order() {
        assert_eq!(fill("{} of {}", &["3", "10"]), "3 of 10");
        assert_eq!(fill("no holes", &["x"]), "no holes");
    }

    #[test]
    fn a_hole_with_no_argument_is_left_showing() {
        // Swallowing it would hide a broken translation; leaving it makes
        // the mistake visible where it happens.
        assert_eq!(fill("{} and {}", &["one"]), "one and {}");
    }

    #[test]
    fn a_count_falls_back_to_the_english_pair() {
        set_language("en");
        assert_eq!(tn("{} track", "{} tracks", 1), "{} track");
        assert_eq!(tn("{} track", "{} tracks", 2), "{} tracks");
        assert_eq!(tf("{} track", "{} tracks", 3, &["3"]), "3 tracks");
    }

    #[test]
    fn the_environment_is_read_in_posix_order() {
        std::env::set_var("LC_ALL", "es_MX.UTF-8");
        assert_eq!(from_environment(), "es");
        std::env::set_var("LC_ALL", "");
        std::env::set_var("LANG", "en_US.UTF-8");
        assert_eq!(from_environment(), "en");
        std::env::set_var("LANG", "xx_YY");
        assert_eq!(from_environment(), "en", "an unknown one is English");
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LANG");
    }

    #[test]
    fn every_shipped_catalog_parses() {
        for (i, code) in LANGUAGES.iter().enumerate() {
            let c = catalog(i as u8);
            assert_eq!(c.language(), *code);
        }
    }
}
