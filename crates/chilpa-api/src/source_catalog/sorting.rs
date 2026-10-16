//! Source library ordering uses the metadata supplied with each entity.
use super::store::Card;
use serde_json::Value;
use std::cmp::Ordering;

pub(super) fn matches(card: &Card, words: &[String]) -> bool {
    let text = format!("{} {} {}", card.title, card.artist, card.genre).to_lowercase();
    words.iter().all(|word| text.contains(word))
}

fn compare(a: &Value, b: &Value, field: &str, descending: bool) -> Ordering {
    let missing = |v: &Value| v.is_null() || v.as_str() == Some("");
    match (missing(&a[field]), missing(&b[field])) {
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {},
    }
    let value = compare_present(a, b, field);
    if descending { value.reverse() } else { value }
}
fn compare_present(a: &Value, b: &Value, field: &str) -> Ordering {
    match (a[field].as_f64(), b[field].as_f64()) {
        (Some(a), Some(b)) => a.total_cmp(&b),
        _ => a[field].as_str().unwrap_or_default().to_lowercase().cmp(&b[field].as_str().unwrap_or_default().to_lowercase()),
    }
}
fn field(order: &str) -> &str {
    match order { "added" => "added_at", "updated" => "updated_at", "duration" => "duration_ms", "track-count" => "tracks", "release-date" => "release_date", "release" => "album", _ => order }
}
pub(super) fn sort_cards(cards: &mut [Card], order: &str) {
    let descending = order.ends_with(":desc");
    let order = order.split(':').next().unwrap_or_default();
    if order.is_empty() || order == "default" { if descending { cards.reverse(); } return; }
    cards.sort_by(|a, b| {
        let compared = match order {
            "artist" => (a.artist.to_lowercase(), a.year, a.title.to_lowercase()).cmp(&(b.artist.to_lowercase(), b.year, b.title.to_lowercase())),
            "title" => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
            "year" => a.year.cmp(&b.year),
            "genre" => a.genre.to_lowercase().cmp(&b.genre.to_lowercase()),
            _ => return compare(&a.metadata, &b.metadata, field(order), descending),
        };
        if descending { compared.reverse() } else { compared }
    });
}
impl crate::Chilpa {
    /// Route, order, grouping and filter travel together, separated by U+001F.
    pub fn source_track_entries(&mut self, query: &str) -> Vec<crate::TrackEntry> {
        let mut parts = query.splitn(4, '\u{1f}');
        let key = parts.next().unwrap_or_default();
        let order = parts.next().unwrap_or_default();
        let group = parts.next().unwrap_or_default();
        let text = parts.next().unwrap_or_default().to_lowercase();
        self.catalog.shelf(key);
        let mut tracks = self.catalog.shelf_tracks(key);
        tracks.retain(|(t, _)| text.split_whitespace().all(|w| format!("{} {} {}", t.title, t.artist, t.album).to_lowercase().contains(w)));
        let descending = order.ends_with(":desc");
        let field = field(order.split(':').next().unwrap_or_default());
        if !matches!(field, "" | "default") { tracks.sort_by(|(a, _), (b, _)| { compare(&a.metadata, &b.metadata, field, descending) }); }
        if matches!(field, "" | "default") && descending { tracks.reverse(); }
        let mut rows: Vec<_> = tracks.into_iter().map(|(t, art)| super::view::row_of(&t, art)).collect();
        if !matches!(group, "album" | "artist" | "name") { return rows.into_iter().map(crate::TrackEntry::Track).collect(); }
        rows.sort_by_cached_key(|t| crate::group_of(group, t).to_lowercase());
        let mut result = Vec::new();
        let mut previous: Option<String> = None;
        for row in rows {
            let label = crate::group_of(group, &row);
            if previous.as_ref().is_none_or(|old| !old.eq_ignore_ascii_case(&label)) {
                result.push(crate::TrackEntry::header(label.clone(), String::new()));
                previous = Some(label);
            }
            result.push(crate::TrackEntry::Track(row));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn every_genre_remains_searchable_when_only_the_first_is_displayed() {
        let card = Card { title: "Record".into(), genre: "Rock, Alternative, Pop".into(), ..Default::default() };
        assert!(matches(&card, &["alternative".into()]));
        assert!(matches(&card, &["record".into(), "pop".into()]));
        assert!(!matches(&card, &["jazz".into()]));
    }
    #[test]
    fn sorting_keeps_missing_dates_last_and_reverses_the_source_order() {
        let mut cards = vec![Card { id: -1, metadata: json!({"added_at":20}), ..Default::default() },
            Card { id: -2, ..Default::default() }, Card { id: -3, metadata: json!({"added_at":10}), ..Default::default() }];
        sort_cards(&mut cards, "added:asc");
        assert_eq!(cards.iter().map(|c| c.id).collect::<Vec<_>>(), [-3,-1,-2]);
        sort_cards(&mut cards, "added:desc");
        assert_eq!(cards.iter().map(|c| c.id).collect::<Vec<_>>(), [-1,-3,-2]);
        sort_cards(&mut cards, "default:desc");
        assert_eq!(cards.iter().map(|c| c.id).collect::<Vec<_>>(), [-2,-3,-1]);
    }
    #[test]
    fn numeric_and_text_metadata_have_distinct_ordering() {
        assert_eq!(compare(&json!({"duration_ms":9}), &json!({"duration_ms":100}), "duration_ms", false), Ordering::Less);
        assert_eq!(compare(&json!({"label":"alpha"}), &json!({"label":"Beta"}), "label", false), Ordering::Less);
        assert_eq!(compare(&json!({"release_date":"2020-01-01"}), &json!({"release_date":"2021-01-01"}), "release_date", true), Ordering::Greater);
    }
}
