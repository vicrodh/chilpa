//! Replacing a text in one field of a record, in one step: the record's own
//! field, or that field of every track. What changes shows in the form,
//! unsaved, like any other edit.

use super::fields::{self, Kind, ALBUM, TRACK};
use super::State;
use crate::Chilpa;
use chilpa_library::sidecar::{join, split};

/// A field text can be replaced in, as the editor offers it: `album:title`
/// or `track:artist_credit`, with what it is called.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ReplaceField {
    pub key: String,
    pub label: String,
}

pub(crate) fn offered() -> Vec<ReplaceField> {
    let of = |scope: &str, list: &[fields::Field], of_track: bool| -> Vec<ReplaceField> {
        list.iter().filter(|field| fields::replaceable(field.kind))
            .map(|field| ReplaceField { key: format!("{scope}:{}", field.name), label: fields::label(field.name, of_track) })
            .collect()
    };
    of("album", &ALBUM, false).into_iter().chain(of("track", &TRACK, true)).collect()
}

/// A list has the text replaced in each of its values.
fn swapped(kind: Kind, value: &str, find: &str, to: &str, whole: bool, case: bool) -> String {
    match kind {
        Kind::List => join(&split(value).iter().map(|item| fields::substitute(item, find, to, whole, case))
            .filter(|item| !item.trim().is_empty()).collect::<Vec<_>>()),
        _ => fields::substitute(value, find, to, whole, case),
    }
}

impl Chilpa {
    /// Replaces `find` with `to` in one field of the open record: `key` is
    /// one of the view's `replaceable`. Returns how many values changed.
    pub fn metadata_substitute(&mut self, key: &str, find: &str, to: &str, whole: bool, case: bool) -> usize {
        let editor = &mut self.metadata;
        if editor.state != State::Ready || find.is_empty() {
            return 0;
        }
        let Some(draft) = editor.draft.as_mut() else { return 0 };
        let mut changed = 0;
        match key.split_once(':') {
            Some(("album", name)) => {
                if let Some(at) = fields::album(name).filter(|at| fields::replaceable(ALBUM[*at].kind)) {
                    let cell = draft.album_cell(name);
                    // Tracks that disagree have no one value to replace in.
                    if !cell.mixed {
                        let now = cell.now.clone();
                        let next = swapped(ALBUM[at].kind, &now, find, to, whole, case);
                        if next != now && draft.set_album(name, &next) {
                            changed += 1;
                        }
                    }
                }
            }
            Some(("track", name)) => {
                if let Some(at) = fields::track(name).filter(|at| fields::replaceable(TRACK[*at].kind)) {
                    let held: Vec<(i64, String)> = draft.rows.iter().map(|row| (row.track.id, row.cells[at].now.clone())).collect();
                    for (id, now) in held {
                        let next = swapped(TRACK[at].kind, &now, find, to, whole, case);
                        if next != now && draft.set_track(id, name, &next) {
                            changed += 1;
                        }
                    }
                }
            }
            _ => {}
        }
        editor.error.clear();
        editor.notice = if changed == 0 {
            chilpa_i18n::t("nothing matched")
        } else {
            chilpa_i18n::tf("{} value changed", "{} values changed", changed as i64, &[&changed.to_string()])
        };
        editor.refresh();
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_words_and_lists_are_offered_and_each_says_whose_field_it_is() {
        let keys: Vec<String> = offered().into_iter().map(|field| field.key).collect();
        assert!(keys.contains(&"album:title".to_string()) && keys.contains(&"track:title".to_string()));
        assert!(keys.contains(&"album:genres".to_string()) && keys.contains(&"track:composers".to_string()));
        assert!(!keys.iter().any(|key| key.contains("musicbrainz") || key.ends_with("track_number") || key.ends_with(":date")));
    }

    #[test]
    fn the_open_record_has_one_field_replaced_on_every_track_and_shows_it_unsaved() {
        let mut api = Chilpa::with_listed(Vec::new(), Vec::new());
        api.metadata.state = State::Ready;
        api.metadata.draft = Some(super::super::draft::testing::plain());
        assert_eq!(api.metadata_substitute("track:title", "track", "Part", false, false), 2);
        assert_eq!(api.metadata_substitute("album:title", "a record", "The Record", true, false), 1);
        assert_eq!(api.metadata_substitute("track:musicbrainz_recording_id", "a", "b", false, false), 0, "an identifier is not offered");
        assert_eq!(api.metadata_substitute("album:title", "nothing here", "x", false, false), 0);
        let draft = api.metadata.draft.as_ref().unwrap();
        let title = fields::track("title").unwrap();
        assert_eq!(draft.rows.iter().map(|row| row.cells[title].now.as_str()).collect::<Vec<_>>(), ["Part 1", "Part 2"]);
        assert_eq!(draft.album_cell("title").now, "The Record");
        assert!(draft.album_cell("title").dirty(), "unsaved, like any edit");
    }

    #[test]
    fn a_list_has_the_text_replaced_in_each_value() {
        assert_eq!(swapped(Kind::List, "Rock; Hard rock; Pop", "rock", "Metal", true, false), "Metal; Hard rock; Pop");
        assert_eq!(swapped(Kind::List, "Rock; Hard rock", "rock", "metal", false, true), "Rock; Hard metal");
        assert_eq!(swapped(Kind::List, "Rock; Pop", "Pop", "", true, true), "Rock", "a value replaced by nothing leaves the list");
    }
}
