//! What a track list's columns hold at their widest, for the window to
//! measure and share the room by: the longest titles with their credits,
//! artists and records, by characters, a few of each, since the longest in
//! characters is not always the widest drawn.

use crate::{Chilpa, TrackRow};
use serde_json::{json, Value};

/// How many texts of each column the window measures.
const FEW: usize = 3;

fn longest(texts: impl Iterator<Item = (usize, Value)>) -> Vec<Value> {
    let mut texts: Vec<(usize, Value)> = texts.collect();
    texts.sort_by(|a, b| b.0.cmp(&a.0));
    let mut kept: Vec<Value> = Vec::new();
    for (_, text) in texts {
        if kept.len() == FEW { break; }
        if !kept.contains(&text) { kept.push(text); }
    }
    kept
}

impl Chilpa {
    /// `{"title": [[title, credit], ...], "artist": [...], "album": [...]}`.
    pub fn widest_texts(&self, rows: &[&TrackRow]) -> Value {
        let chars = |text: &str| text.chars().count();
        json!({
            "title": longest(rows.iter().map(|row| (chars(&row.title) + chars(&row.composer), json!([row.title, row.composer])))),
            "artist": longest(rows.iter().map(|row| (chars(&row.artist), json!(row.artist)))),
            "album": longest(rows.iter().map(|row| (chars(&row.album), json!(row.album)))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(title: &str, composer: &str, artist: &str, album: &str) -> TrackRow {
        let mut row = crate::browse::file_row("/music/a.flac");
        row.title = title.into(); row.composer = composer.into(); row.artist = artist.into(); row.album = album.into();
        row
    }

    #[test]
    fn each_column_names_its_few_longest_texts_once() {
        let api = Chilpa::new();
        let rows = [row("Dreams", "D. O'an, N. Hogan", "The Cranberries", "Everybody Else Is Doing It, So Why Can't We?"),
            row("Linger", "D. O'an", "The Cranberries", "Everybody Else Is Doing It, So Why Can't We?"),
            row("Zombie", "", "The Cranberries", "No Need to Argue"),
            row("I Still Do", "D. O'an, N. Hogan", "The Cranberries", "Everybody Else Is Doing It, So Why Can't We?")];
        let found = api.widest_texts(&rows.iter().collect::<Vec<_>>());
        assert_eq!(found["title"][0], json!(["I Still Do", "D. O'an, N. Hogan"]), "a title is as long as its credit with it");
        assert_eq!(found["artist"], json!(["The Cranberries"]), "the same text once");
        assert_eq!(found["album"], json!(["Everybody Else Is Doing It, So Why Can't We?", "No Need to Argue"]));
        assert!(api.widest_texts(&[])["title"].as_array().unwrap().is_empty());
    }
}
