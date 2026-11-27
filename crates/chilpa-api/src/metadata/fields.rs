//! The fields the editor edits, what each accepts and how it is written
//! down.
//!
//! Every value is text, in the form it is shown: a list is its values a
//! semicolon apart, a flag is "true" or "false", a number is its digits or
//! nothing.

use chilpa_library::sidecar::{join, split};

/// The most values a list takes, and the most characters a value of one.
const MOST_VALUES: usize = 64;
const LONGEST_VALUE: usize = 512;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    /// A text of up to that many characters.
    Text(usize),
    List,
    Flag,
    Advisory,
    Date,
    Number,
    Uuid,
    UuidList,
    Digits,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Field {
    pub name: &'static str,
    pub kind: Kind,
    /// It cannot be left empty.
    pub required: bool,
}

const fn field(name: &'static str, kind: Kind, required: bool) -> Field {
    Field { name, kind, required }
}

/// A record's fields, in the order they are shown.
pub(crate) const ALBUM: [Field; 12] = [
    field("title", Kind::Text(512), true),
    field("album_artist", Kind::Text(512), true),
    field("album_artists", Kind::List, false),
    field("compilation", Kind::Flag, false),
    field("date", Kind::Date, false),
    field("genres", Kind::List, false),
    field("catalog_number", Kind::Text(128), false),
    field("label", Kind::Text(256), false),
    field("musicbrainz_release_id", Kind::Uuid, false),
    field("musicbrainz_release_group_id", Kind::Uuid, false),
    field("musicbrainz_album_artist_ids", Kind::UuidList, false),
    field("discogs_release_id", Kind::Digits, false),
];

/// A track's.
pub(crate) const TRACK: [Field; 13] = [
    field("title", Kind::Text(512), true),
    field("artist_credit", Kind::Text(512), true),
    field("artists", Kind::List, false),
    field("track_number", Kind::Number, false),
    field("disc_number", Kind::Number, false),
    field("disc_subtitle", Kind::Text(512), false),
    field("work", Kind::Text(512), false),
    field("composers", Kind::List, false),
    field("performers", Kind::List, false),
    field("musicbrainz_recording_id", Kind::Uuid, false),
    field("musicbrainz_track_id", Kind::Uuid, false),
    field("musicbrainz_artist_ids", Kind::UuidList, false),
    field("explicit", Kind::Advisory, false),
];

pub(crate) fn album(name: &str) -> Option<usize> {
    ALBUM.iter().position(|f| f.name == name)
}

pub(crate) fn track(name: &str) -> Option<usize> {
    TRACK.iter().position(|f| f.name == name)
}

/// Whether a field's values can have text replaced in them: words, and the
/// words of a list. Numbers, dates and identifiers cannot.
pub(crate) fn replaceable(kind: Kind) -> bool {
    matches!(kind, Kind::Text(_) | Kind::List)
}

/// `text` with `find` replaced by `to`. `whole` replaces the value only when
/// it is `find`, ignoring the spaces around it; otherwise every occurrence
/// is replaced. `case` tells capitals apart. Letters are compared one by one,
/// so no character is ever cut in half.
pub(crate) fn substitute(text: &str, find: &str, to: &str, whole: bool, case: bool) -> String {
    if find.is_empty() {
        return text.to_string();
    }
    let same = |a: char, b: char| if case { a == b } else { a == b || a.to_lowercase().eq(b.to_lowercase()) };
    let needle: Vec<char> = find.chars().collect();
    if whole {
        let held: Vec<char> = text.trim().chars().collect();
        let wanted: Vec<char> = find.trim().chars().collect();
        let matches = held.len() == wanted.len() && held.iter().zip(&wanted).all(|(a, b)| same(*a, *b));
        return if matches { to.to_string() } else { text.to_string() };
    }
    let letters: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < letters.len() {
        let end = at + needle.len();
        if end <= letters.len() && letters[at..end].iter().zip(&needle).all(|(a, b)| same(*a, *b)) {
            out.push_str(to);
            at = end;
        } else {
            out.push(letters[at]);
            at += 1;
        }
    }
    out
}

pub(crate) fn label(name: &str, of_track: bool) -> String {
    let label = match name {
        "title" if of_track => chilpa_i18n::mark("Title"), "title" => chilpa_i18n::mark("Album"), "album_artist" => chilpa_i18n::mark("Album artist"), "album_artists" => chilpa_i18n::mark("Album artists"),
        "compilation" => chilpa_i18n::mark("Compilation"), "date" => chilpa_i18n::mark("Date"), "genres" => chilpa_i18n::mark("Genres"), "catalog_number" => chilpa_i18n::mark("Catalog number"), "label" => chilpa_i18n::mark("Label"),
        "musicbrainz_release_id" => chilpa_i18n::mark("MusicBrainz release"), "musicbrainz_release_group_id" => chilpa_i18n::mark("MusicBrainz release group"),
        "musicbrainz_album_artist_ids" => chilpa_i18n::mark("MusicBrainz album artists"), "discogs_release_id" => chilpa_i18n::mark("Discogs release"),
        "artist_credit" => chilpa_i18n::mark("Artist"), "artists" => chilpa_i18n::mark("Artists"), "track_number" => chilpa_i18n::mark("Track"), "disc_number" => chilpa_i18n::mark("Disc"), "disc_subtitle" => chilpa_i18n::mark("Disc subtitle"),
        "work" => chilpa_i18n::mark("Work"), "composers" => chilpa_i18n::mark("Composers"), "performers" => chilpa_i18n::mark("Performers"), "musicbrainz_recording_id" => chilpa_i18n::mark("MusicBrainz recording"),
        "musicbrainz_track_id" => chilpa_i18n::mark("MusicBrainz track"), "musicbrainz_artist_ids" => chilpa_i18n::mark("MusicBrainz artists"), "explicit" => chilpa_i18n::mark("Content advisory"), _ => chilpa_i18n::mark("Metadata"),
    };
    chilpa_i18n::t(label)
}

/// What somebody typed, as it is kept.
pub(crate) fn normalize(kind: Kind, text: &str) -> String {
    match kind {
        Kind::Text(_) | Kind::Date | Kind::Number | Kind::Digits | Kind::Advisory => text.trim().to_string(),
        Kind::List => join(&split(text)),
        Kind::Flag => (text.trim() == "true").to_string(),
        Kind::Uuid => text.trim().to_lowercase(),
        Kind::UuidList => join(&split(&text.to_lowercase())),
    }
}

/// Whether two values say the same. Two lists do in any order and any case.
pub(crate) fn same(kind: Kind, a: &str, b: &str) -> bool {
    match kind {
        Kind::List | Kind::UuidList => {
            let sorted = |text: &str| {
                let mut values: Vec<String> = split(text).into_iter().map(|v| v.to_lowercase()).collect();
                values.sort();
                values
            };
            sorted(a) == sorted(b)
        }
        _ => a == b,
    }
}

/// A year, a year and a month, or a whole day.
pub(crate) fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if !matches!(bytes.len(), 4 | 7 | 10) {
        return false;
    }
    let shaped = bytes.iter().enumerate().all(|(i, b)| match i {
        4 | 7 => *b == b'-',
        _ => b.is_ascii_digit(),
    });
    if !shaped {
        return false;
    }
    let part = |from: usize, to: usize| text.get(from..to).and_then(|p| p.parse::<u32>().ok());
    let year = part(0, 4).is_some_and(|y| (1..=3000).contains(&y));
    let month = bytes.len() < 7 || part(5, 7).is_some_and(|m| (1..=12).contains(&m));
    let day = bytes.len() < 10 || part(8, 10).is_some_and(|d| (1..=31).contains(&d));
    year && month && day
}

/// The year a date starts with.
pub(crate) fn year_of(date: &str) -> Option<i32> {
    is_date(date).then(|| date.get(..4)?.parse().ok()).flatten()
}

fn is_uuid(text: &str) -> bool {
    chilpa_meta::text::is_uuid(text)
}

/// Why a value cannot be saved, in the user's language. Empty when it can.
/// `of_a_track` says whose field it is, for the words of what is missing.
pub(crate) fn invalid(field: &Field, of_a_track: bool, value: &str) -> String {
    if value.is_empty() {
        if !field.required {
            return String::new();
        }
        return match (of_a_track, field.name) {
            (false, "title") => chilpa_i18n::t("the record has no title"),
            (false, _) => chilpa_i18n::t("the record has no artist"),
            (true, "title") => chilpa_i18n::t("a track has no title"),
            (true, _) => chilpa_i18n::t("a track has no artist"),
        };
    }
    let too_long = |most: usize| chilpa_i18n::t_args("that is longer than {} characters", &[&most.to_string()]);
    match field.kind {
        Kind::Advisory if !matches!(value, "true" | "false") => chilpa_i18n::t("Choose Unknown, Clean or Explicit"),
        Kind::Text(most) if value.chars().count() > most => too_long(most),
        Kind::List | Kind::UuidList => {
            let values = split(value);
            if values.len() > MOST_VALUES {
                return chilpa_i18n::t_args("that is more than {} values", &[&MOST_VALUES.to_string()]);
            }
            if values.iter().any(|v| v.chars().count() > LONGEST_VALUE) {
                return too_long(LONGEST_VALUE);
            }
            if field.kind == Kind::UuidList && !values.iter().all(|v| is_uuid(v)) {
                return chilpa_i18n::t("that is not a MusicBrainz id");
            }
            String::new()
        }
        Kind::Date if !is_date(value) => {
            chilpa_i18n::t("a date is a year, a year and a month, or a full day: 1984, 1984-09, 1984-09-03")
        }
        Kind::Number if !value.parse::<u32>().is_ok_and(|n| (1..=9999).contains(&n)) => {
            chilpa_i18n::t("a number from 1 to 9999, or nothing")
        }
        Kind::Uuid if !is_uuid(value) => chilpa_i18n::t("that is not a MusicBrainz id"),
        Kind::Digits if !chilpa_meta::text::is_number(value) => chilpa_i18n::t("that is not a Discogs release number"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_has_text_replaced_in_one_step() {
        assert_eq!(substitute("Live in Berlin (Live)", "Live", "Vivo", false, true), "Vivo in Berlin (Vivo)", "every occurrence");
        assert_eq!(substitute("Feat. Somebody", "feat.", "feat.", false, false), "feat. Somebody", "capitals ignored unless asked");
        assert_eq!(substitute("Feat. Somebody", "feat.", "with", false, true), "Feat. Somebody", "asked, they count");
        assert_eq!(substitute(" The Band ", "the band", "Band, The", true, false), "Band, The", "the whole value, spaces around aside");
        assert_eq!(substitute("The Band Live", "the band", "x", true, false), "The Band Live", "a part is not the whole");
        assert_eq!(substitute("Çé Çé", "çé", "ok", false, false), "ok ok", "letters, not bytes");
        assert_eq!(substitute("İstanbul", "i̇", "I", false, false), "İstanbul", "a letter that lowers to two never matches half of it");
        assert_eq!(substitute("value", "", "x", false, false), "value", "nothing to find changes nothing");
    }

    #[test]
    fn the_fields_are_the_ones_a_sidecar_holds_in_the_same_order() {
        let names = |fields: &[Field]| fields.iter().map(|f| f.name).collect::<Vec<_>>();
        assert_eq!(names(&ALBUM), chilpa_library::sidecar::ALBUM_FIELDS);
        assert_eq!(names(&TRACK), chilpa_library::sidecar::TRACK_FIELDS);
    }

    #[test]
    fn a_date_is_a_year_a_month_or_a_day() {
        for date in ["1984", "1984-09", "1984-09-03", "0001", "3000-12-31"] {
            assert!(is_date(date), "{date}");
        }
        for date in ["", "84", "1984-9", "1984-13", "1984-00", "1984-09-32", "1984/09/03", "3001", "0000", "1984-09-03T00:00:00", "19840903"] {
            assert!(!is_date(date), "{date}");
        }
        assert_eq!(year_of("1984-09-03"), Some(1984));
        assert_eq!(year_of("sometime"), None);
    }

    #[test]
    fn what_is_typed_is_kept_tidy() {
        assert_eq!(normalize(Kind::Text(512), "  A title  "), "A title");
        assert_eq!(normalize(Kind::List, "Rock;Blues ; rock\nSoul"), "Rock; Blues; Soul");
        assert_eq!(normalize(Kind::Uuid, " 3D3207F9-C4C2-425C-A364-F359B4320805 "), "3d3207f9-c4c2-425c-a364-f359b4320805");
        assert_eq!(normalize(Kind::Flag, "true"), "true");
        assert_eq!(normalize(Kind::Flag, "yes"), "false");
    }

    #[test]
    fn a_list_is_the_same_in_any_order_and_any_case() {
        assert!(same(Kind::List, "Rock; Blues", "blues; ROCK"));
        assert!(!same(Kind::List, "Rock; Blues", "Rock"));
        assert!(!same(Kind::Text(512), "Rock", "rock"), "a title is not a list");
    }

    #[test]
    fn a_value_that_cannot_be_saved_says_why() {
        let of = |fields: &[Field], name: &str| *fields.iter().find(|f| f.name == name).unwrap();
        assert!(!invalid(&of(&ALBUM, "title"), false, "").is_empty());
        assert!(!invalid(&of(&ALBUM, "album_artist"), false, "").is_empty());
        assert!(invalid(&of(&ALBUM, "genres"), false, "").is_empty(), "what is not required may be empty");
        assert!(!invalid(&of(&ALBUM, "catalog_number"), false, &"x".repeat(129)).is_empty());
        assert!(invalid(&of(&ALBUM, "catalog_number"), false, &"ñ".repeat(128)).is_empty(), "characters, not bytes");
        assert!(!invalid(&of(&ALBUM, "date"), false, "1984-9-3").is_empty());
        assert!(!invalid(&of(&ALBUM, "discogs_release_id"), false, "r1234").is_empty());
        assert!(!invalid(&of(&ALBUM, "musicbrainz_release_id"), false, "1234").is_empty());
        assert!(!invalid(&of(&ALBUM, "musicbrainz_album_artist_ids"), false, "3d3207f9-c4c2-425c-a364-f359b4320805; nope").is_empty());
        assert!(!invalid(&of(&TRACK, "track_number"), true, "0").is_empty());
        assert!(!invalid(&of(&TRACK, "track_number"), true, "10000").is_empty());
        assert!(invalid(&of(&TRACK, "track_number"), true, "12").is_empty());
        let many = (0..65).map(|n| n.to_string()).collect::<Vec<_>>().join("; ");
        assert!(!invalid(&of(&TRACK, "composers"), true, &many).is_empty());
    }
}
