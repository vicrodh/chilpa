//! Whether star ratings are used at all: one toggle under Library, on by
//! default, that every surface reads.
//!
//! Off, nothing shows, sets, filters or groups by stars: the window hides
//! them and the rating column, filters drop every rating condition, and
//! Refine and `:help filter` leave them out. Nothing kept is touched. The
//! stars given stay in the library, a rating condition stays in the query
//! and the screen that holds it, and a saved column layout keeps its rating
//! column; turned on again, all of it is back as it was.

use crate::settings_rows::SettingsRow;
use crate::{Chilpa, Filter};
use chilpa_library::store::query::Field;
use serde_json::Value;
use std::borrow::Cow;

/// The toggle under Library.
pub const ENABLED: &str = "library.ratings";

/// Whether ratings are used: on unless the listener turned them off. Read
/// from the kept preferences, so a query worker answers as the window does.
pub fn on() -> bool {
    chilpa_core::settings::load().window.get(ENABLED).is_none_or(|value| value != "false")
}

/// `filter` as it applies. With ratings on it is unchanged. Off, it keeps
/// out the Library's rating column, the Tracks screen's stars and every
/// `rating:` condition of its query; the rest of each group still holds.
pub(crate) fn applied(filter: &Filter, on: bool) -> Cow<'_, Filter> {
    let rated = !filter.ratings.is_empty() || !filter.track_ratings.is_empty()
        || filter.query.as_ref().is_some_and(|query| crate::library_query::refine::has_field(query, Field::Rating));
    if on || !rated {
        return Cow::Borrowed(filter);
    }
    let mut out = filter.clone();
    out.ratings.clear();
    out.track_ratings.clear();
    out.query = out.query.take().and_then(|query| crate::library_query::columns::without_field(query, Field::Rating));
    Cow::Owned(out)
}

/// Refine's rows as they show. Off, a rating condition has no row, and a
/// group left with none goes too. The ids of the others stay the places
/// the facade gave them, so an edit still reaches the condition it names.
pub(crate) fn rows(rows: Vec<Value>, on: bool) -> Vec<Value> {
    if on {
        return rows;
    }
    rows.into_iter().filter_map(|mut row| {
        if row["field"] == "rating" {
            return None;
        }
        if let Some(inner) = row.get_mut("rows").and_then(Value::as_array_mut) {
            inner.retain(|condition| condition["field"] != "rating");
            if inner.is_empty() {
                return None;
            }
        }
        Some(row)
    }).collect()
}

/// The examples `:help filter` lists: off, without the one about stars.
pub(crate) fn examples(on: bool) -> Vec<&'static (&'static str, &'static str)> {
    crate::library_query::EXAMPLES.iter().filter(|(example, _)| on || !example.contains("rating:")).collect()
}

impl Chilpa {
    /// Whether ratings show anywhere.
    pub fn ratings_shown(&self) -> bool {
        on()
    }

    pub(crate) fn rating_switch_setting(&mut self, key: &str, value: &str) -> bool {
        if key != ENABLED || !matches!(value, "true" | "false") {
            return false;
        }
        self.remember_preference_checked(key, value).is_ok()
    }

    pub(crate) fn rating_switch_row(&self, row: &mut SettingsRow) -> bool {
        if row.key != ENABLED {
            return false;
        }
        row.value = on().to_string();
        true
    }
}

#[cfg(test)]
mod tests;
