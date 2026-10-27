//! A credit as the window draws it: the text its tag wrote, with each of
//! the artists it names as a place to click.

use crate::Chilpa;
mod track;
pub use track::TrackCredits;
pub const SHOW_COMPOSERS: &str = "navigation.trackComposers";

/// Where each artist of a credit starts and ends inside it, in the units
/// a window counts text in (UTF-16), one pair an artist. Empty when the
/// credit names one artist, or does not carry its artists as they are
/// named: then the whole credit is one place.
pub fn stops(credit: &str, names: &[String]) -> Vec<usize> {
    if names.len() < 2 {
        return Vec::new();
    }
    let lower = credit.to_lowercase();
    // Lowering can change a letter's length; then the places do not match.
    if lower.len() != credit.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut at = 0;
    for name in names {
        let Some(found) = lower[at..].find(&name.to_lowercase()) else { return Vec::new() };
        let (start, end) = (at + found, at + found + name.len());
        if !credit.is_char_boundary(start) || !credit.is_char_boundary(end) {
            return Vec::new();
        }
        out.push(credit[..start].encode_utf16().count());
        out.push(credit[..end].encode_utf16().count());
        at = end;
    }
    out
}

impl Chilpa {
    pub fn track_composers_shown(&self) -> bool { self.preference(SHOW_COMPOSERS).as_deref() == Some("true") }
    pub(crate) fn track_credits_setting(&mut self, key: &str, value: &str) -> bool {
        if key != SHOW_COMPOSERS || !matches!(value, "true" | "false") { return false; }
        self.remember_preference_checked(key, value).is_ok()
    }
    pub fn source_credit_document(&self, album: i64, path: &str) -> String {
        if let Some((track, _)) = self.catalog.track_by_path(path) {
            return TrackCredits::source(&track.artist, &track.metadata, &track.plugin).document().to_string();
        }
        if let Some(card) = (album < 0).then(|| self.catalog.card(album)).flatten() {
            return TrackCredits::source(&card.artist, &card.metadata, &card.plugin).document().to_string();
        }
        String::new()
    }

    /// The artists a credit names, in its order: itself when it is one.
    pub fn credit_names(&mut self, credit: &str) -> Vec<String> {
        // Whatever wrote to the library since, here or on another
        // connection, may have changed who a credit names.
        let now = self.library().map(|lib| lib.written()).unwrap_or_default();
        if now != self.credits_read_at {
            self.credits.clear();
            self.credits_read_at = now;
        }
        if let Some(names) = self.credits.get(credit) {
            return names.clone();
        }
        let names = self.library().and_then(|lib| lib.credit_names(credit).ok()).unwrap_or_else(|| vec![credit.to_string()]);
        self.credits.insert(credit.to_string(), names.clone());
        names
    }

    /// Where each artist of the credit is inside it, as "0,5,7,22".
    pub fn credit_stops(&mut self, credit: &str) -> String {
        let names = self.credit_names(credit);
        stops(credit, &names).iter().map(|s| s.to_string()).collect::<Vec<_>>().join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::stops;

    fn names(of: &[&str]) -> Vec<String> {
        of.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn each_artist_of_a_credit_has_its_place_in_the_text() {
        assert_eq!(stops("May'n, Megumi Nakajima", &names(&["May'n", "Megumi Nakajima"])), [0, 5, 7, 22]);
        assert_eq!(stops("A & b", &names(&["a", "B"])), [0, 1, 4, 5], "as the tag spells them");
        assert_eq!(stops("\u{1f3b5} A, B", &names(&["A", "B"])), [3, 4, 6, 7], "counted as a window counts");
    }

    #[test]
    fn one_artist_or_a_credit_that_does_not_carry_its_artists_is_one_place() {
        assert!(stops("AC/DC", &names(&["AC/DC"])).is_empty());
        assert!(stops("A, B", &names(&["A", "C"])).is_empty());
    }
}
