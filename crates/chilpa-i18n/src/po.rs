//! A gettext `.po` catalog, parsed once and read many times.
//!
//! Only what a catalog actually needs: `msgid`, `msgstr`, and the plural
//! pair. No `msgctxt`: two identical English strings that need different
//! translations are a sign the English is too vague, and rewording the
//! source is a better fix than disambiguating it.

use std::collections::HashMap;

/// The translations for one language.
pub struct Catalog {
    language: String,
    singular: HashMap<String, String>,
    /// Keyed by the singular msgid; the vector is indexed by plural form.
    plural: HashMap<String, Vec<String>>,
    rule: crate::PluralRule,
}

impl Catalog {
    /// Reads a `.po` file. Anything malformed is skipped rather than
    /// refused: a broken line should cost one string, not the whole
    /// interface.
    pub fn parse(language: &str, source: &str) -> Self {
        let mut singular = HashMap::new();
        let mut plural: HashMap<String, Vec<String>> = HashMap::new();
        let mut header = String::new();

        let mut id = String::new();
        let mut id_plural: Option<String> = None;
        let mut forms: Vec<String> = Vec::new();
        let mut current: Option<Slot> = None;

        for raw in source.lines() {
            let line = raw.trim();
            if line.is_empty() {
                flush(&mut id, &mut id_plural, &mut forms, &mut singular, &mut plural, &mut header);
                current = None;
                continue;
            }
            if line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("msgid_plural ") {
                id_plural = Some(unquote(rest));
                current = Some(Slot::IdPlural);
            } else if let Some(rest) = line.strip_prefix("msgid ") {
                flush(&mut id, &mut id_plural, &mut forms, &mut singular, &mut plural, &mut header);
                id = unquote(rest);
                current = Some(Slot::Id);
            } else if let Some(rest) = line.strip_prefix("msgstr[") {
                let (index, text) = match rest.split_once(']') {
                    Some((i, t)) => (i.trim().parse::<usize>().unwrap_or(0), unquote(t.trim())),
                    None => continue,
                };
                while forms.len() <= index {
                    forms.push(String::new());
                }
                forms[index] = text;
                current = Some(Slot::Form(index));
            } else if let Some(rest) = line.strip_prefix("msgstr ") {
                forms = vec![unquote(rest)];
                current = Some(Slot::Form(0));
            } else if line.starts_with('"') {
                // A string split across several lines.
                let more = unquote(line);
                match current {
                    Some(Slot::Id) => id.push_str(&more),
                    Some(Slot::IdPlural) => {
                        if let Some(p) = id_plural.as_mut() {
                            p.push_str(&more);
                        }
                    }
                    Some(Slot::Form(i)) => {
                        if let Some(f) = forms.get_mut(i) {
                            f.push_str(&more);
                        }
                    }
                    None => {}
                }
            }
        }
        flush(&mut id, &mut id_plural, &mut forms, &mut singular, &mut plural, &mut header);

        let rule = crate::PluralRule::from_header(&header).unwrap_or_else(|| {
            crate::PluralRule::for_language(language)
        });
        Self { language: language.to_string(), singular, plural, rule }
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn plural_rule(&self) -> &crate::PluralRule {
        &self.rule
    }

    /// The translation for a msgid, or `None` when it is untranslated. An
    /// empty `msgstr` counts as untranslated: that is what a fresh catalog
    /// entry looks like, and showing an empty label would be worse than
    /// showing English.
    pub fn get(&self, msgid: &str) -> Option<&str> {
        self.singular.get(msgid).map(String::as_str).filter(|s| !s.is_empty())
    }

    pub fn get_plural(&self, msgid: &str, form: usize) -> Option<&str> {
        self.plural.get(msgid)?.get(form).map(String::as_str).filter(|s| !s.is_empty())
    }

    pub fn len(&self) -> usize {
        self.singular.len() + self.plural.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

enum Slot {
    Id,
    IdPlural,
    Form(usize),
}

#[allow(clippy::too_many_arguments)]
fn flush(
    id: &mut String,
    id_plural: &mut Option<String>,
    forms: &mut Vec<String>,
    singular: &mut HashMap<String, String>,
    plural: &mut HashMap<String, Vec<String>>,
    header: &mut String,
) {
    if forms.is_empty() {
        id.clear();
        *id_plural = None;
        return;
    }
    if id.is_empty() {
        // The entry with an empty msgid is the header, and that is where
        // the plural rule lives.
        header.push_str(&forms[0]);
    } else if id_plural.is_some() {
        plural.insert(std::mem::take(id), std::mem::take(forms));
    } else {
        singular.insert(std::mem::take(id), forms[0].clone());
    }
    id.clear();
    *id_plural = None;
    forms.clear();
}

/// Takes the quotes off a `.po` value and undoes its escapes.
fn unquote(raw: &str) -> String {
    let inner = raw.trim().trim_start_matches('"').trim_end_matches('"');
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_simple_entry_reads_back() {
        let c = Catalog::parse("es", "msgid \"Nothing playing\"\nmsgstr \"Nada sonando\"\n");
        assert_eq!(c.get("Nothing playing"), Some("Nada sonando"));
        assert_eq!(c.get("Something else"), None);
    }

    #[test]
    fn an_untranslated_entry_counts_as_missing() {
        // An empty msgstr is what a fresh entry looks like. Showing an
        // empty label would be worse than showing the English.
        let c = Catalog::parse("es", "msgid \"Play\"\nmsgstr \"\"\n");
        assert_eq!(c.get("Play"), None);
    }

    #[test]
    fn a_string_split_across_lines_is_joined() {
        let c = Catalog::parse(
            "es",
            "msgid \"\"\n\"a long one\"\nmsgstr \"\"\n\"uno largo\"\n",
        );
        assert_eq!(c.get("a long one"), Some("uno largo"));
    }

    #[test]
    fn escapes_survive_the_round_trip() {
        let c = Catalog::parse("es", r#"msgid "a\nb\"c""# .to_string().as_str());
        assert!(c.is_empty(), "no msgstr, no entry");
        let c = Catalog::parse("es", "msgid \"a\\nb\"\nmsgstr \"c\\td\"\n");
        assert_eq!(c.get("a\nb"), Some("c\td"));
    }

    #[test]
    fn plurals_are_kept_by_form() {
        let c = Catalog::parse(
            "es",
            "msgid \"{} track\"\nmsgid_plural \"{} tracks\"\nmsgstr[0] \"{} pista\"\n\
             msgstr[1] \"{} pistas\"\n",
        );
        assert_eq!(c.get_plural("{} track", 0), Some("{} pista"));
        assert_eq!(c.get_plural("{} track", 1), Some("{} pistas"));
        assert_eq!(c.get_plural("{} track", 2), None);
    }

    #[test]
    fn a_broken_line_costs_one_string_and_not_the_catalog() {
        let c = Catalog::parse(
            "es",
            "msgid \"good\"\nmsgstr \"bueno\"\n\nesto no es nada\n\nmsgid \"also\"\n\
             msgstr \"también\"\n",
        );
        assert_eq!(c.get("good"), Some("bueno"));
        assert_eq!(c.get("also"), Some("también"));
    }

    #[test]
    fn the_header_is_not_a_translation() {
        let c = Catalog::parse(
            "es",
            "msgid \"\"\nmsgstr \"Plural-Forms: nplurals=2; plural=(n != 1);\\n\"\n\n\
             msgid \"x\"\nmsgstr \"equis\"\n",
        );
        assert_eq!(c.len(), 1, "only the real entry");
        assert_eq!(c.plural_rule().count(), 2);
    }
}
