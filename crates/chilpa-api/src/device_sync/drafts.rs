//! I retain only explicit selection edits across navigation, scoped to their storage profile.
use super::{failure, selection::OptionRow, UiError};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Drafts {
    rows: BTreeMap<String, Vec<OptionRow>>,
    removals: BTreeMap<String, bool>,
}
impl Drafts {
    /// I bound all profiles together to 512 edits and 1 MiB, including display metadata.
    pub fn put(&mut self, profile: &str, row: OptionRow) -> Result<(), UiError> {
        let row_id = &row.id;
        let retained = self.rows.iter().flat_map(|(key, rows)| {
            rows.iter()
                .filter(move |r| key != profile || &r.id != row_id)
        });
        let mut count = 1;
        let mut bytes = size(&row);
        for saved in retained {
            count += 1;
            bytes = bytes.saturating_add(size(saved));
        }
        if count > 512 || bytes > 1024 * 1024 {
            return Err(failure(chilpa_i18n::mark("Device selections exceed 512 edits or 1 MiB. Save a selection before adding more."), ""));
        }
        let rows = self.rows.entry(profile.into()).or_default();
        rows.retain(|r| r.id != row.id);
        rows.push(row);
        Ok(())
    }
    pub fn apply(&self, profile: &str, options: &mut Vec<OptionRow>, removals: &mut bool) {
        if let Some(value) = self.removals.get(profile) {
            *removals = *value;
        }
        for row in self.rows.get(profile).into_iter().flatten() {
            options.retain(|r| r.id != row.id);
            options.insert(0, row.clone());
        }
    }
    pub fn removals(&mut self, profile: &str, value: bool) {
        self.removals.insert(profile.into(), value);
    }
    pub fn clear(&mut self, profile: &str) {
        self.rows.remove(profile);
        self.removals.remove(profile);
    }
    pub fn attach(&mut self, profile: &str) {
        if profile.is_empty() {
            return;
        }
        if let Some(rows) = self.rows.remove("") {
            let target = self.rows.entry(profile.into()).or_default();
            for row in rows {
                target.retain(|r| r.id != row.id);
                target.push(row);
            }
        }
        if let Some(value) = self.removals.remove("") {
            self.removals.insert(profile.into(), value);
        }
    }
}
fn size(row: &OptionRow) -> usize {
    serde_json::to_vec(&row.pick)
        .map_or(usize::MAX, |b| b.len())
        .saturating_add(row.name.len())
        .saturating_add(row.description.len())
        .saturating_add(row.id.len())
}
