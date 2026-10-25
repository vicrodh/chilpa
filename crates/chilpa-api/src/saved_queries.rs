//! I keep bounded Refine bookmarks under one settings key, independently of library results.
use crate::{library_query, Chilpa};
use chilpa_core::settings;
use serde::{Deserialize, Serialize};

pub const MAX_ENTRIES: usize = 64;
pub const MAX_NAME_CHARS: usize = 80;
pub const MAX_EXPRESSION_BYTES: usize = 16 * 1024;
pub const MAX_CATALOG_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedQuery { pub id: String, pub name: String, pub expression: String }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub version: u32,
    pub revision: u64,
    next_id: u64,
    pub entries: Vec<SavedQuery>,
}

impl Default for Catalog {
    fn default() -> Self { Self { version: 1, revision: 0, next_id: 1, entries: Vec::new() } }
}

fn invalid() -> String { chilpa_i18n::t("The saved query catalog is damaged; I kept its original data.") }
fn storage(error: impl std::fmt::Display) -> String {
    chilpa_i18n::t_args("The saved queries could not be read or saved: {}", &[&error.to_string()])
}
fn validate_expression(text: &str) -> Result<(), String> {
    if text.len() > MAX_EXPRESSION_BYTES { return Err(chilpa_i18n::t("A saved query may use at most 16 KiB.")); }
    if text.trim().is_empty() { return Err(chilpa_i18n::t("Add a condition before saving a query.")); }
    library_query::read(text).map(|_| ())
}
fn name(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_NAME_CHARS || text.chars().any(char::is_control) {
        return Err(chilpa_i18n::t("Use a name of 1 to 80 characters, without line breaks."));
    }
    Ok(text.to_owned())
}

impl Catalog {
    fn read(raw: Option<&str>) -> Result<Self, String> {
        let Some(raw) = raw else { return Ok(Self::default()) };
        if raw.len() > MAX_CATALOG_BYTES { return Err(chilpa_i18n::t("The saved query catalog exceeds 1 MiB.")); }
        let header: serde_json::Value = serde_json::from_str(raw).map_err(|_| invalid())?;
        if header.get("version").and_then(|v| v.as_u64()) != Some(1) {
            return Err(chilpa_i18n::t("This saved query catalog uses an unsupported version; I kept its original data."));
        }
        let catalog: Self = serde_json::from_value(header).map_err(|_| invalid())?;
        if catalog.entries.len() > MAX_ENTRIES || catalog.next_id == 0 { return Err(invalid()); }
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for entry in &catalog.entries {
            let number = entry.id.strip_prefix('q').and_then(|id| id.parse::<u64>().ok()).ok_or_else(invalid)?;
            if number == 0 || number >= catalog.next_id || !ids.insert(&entry.id) || !names.insert(&entry.name)
                || name(&entry.name)? != entry.name { return Err(invalid()); }
            validate_expression(&entry.expression)?;
        }
        Ok(catalog)
    }
    fn encode(&self) -> Result<String, String> {
        let raw = serde_json::to_string(self).map_err(storage)?;
        if raw.len() > MAX_CATALOG_BYTES { return Err(chilpa_i18n::t("The saved query catalog exceeds 1 MiB.")); }
        Ok(raw)
    }
    fn next(&mut self) -> Result<(), String> {
        self.revision = self.revision.checked_add(1).ok_or_else(invalid)?;
        Ok(())
    }
    fn unused_name(&self, text: &str, except: Option<&str>) -> Result<String, String> {
        let name = name(text)?;
        if self.entries.iter().any(|entry| entry.name == name && Some(entry.id.as_str()) != except) {
            return Err(chilpa_i18n::t("A saved query already has that name."));
        }
        Ok(name)
    }
    fn add(&mut self, label: &str, text: &str) -> Result<(), String> {
        if self.entries.len() >= MAX_ENTRIES { return Err(chilpa_i18n::t("You can save up to 64 queries.")); }
        let name = self.unused_name(label, None)?;
        validate_expression(text)?;
        let id = format!("q{}", self.next_id);
        self.next_id = self.next_id.checked_add(1).ok_or_else(invalid)?;
        self.entries.push(SavedQuery { id, name, expression: text.to_owned() });
        self.next()
    }
    fn rename(&mut self, id: &str, label: &str) -> Result<(), String> {
        let name = self.unused_name(label, Some(id))?;
        let entry = self.entries.iter_mut().find(|entry| entry.id == id).ok_or_else(missing)?;
        if entry.name == name { return Ok(()); }
        entry.name = name;
        self.next()
    }
    fn remove(&mut self, id: &str) -> Result<(), String> {
        let at = self.entries.iter().position(|entry| entry.id == id).ok_or_else(missing)?;
        self.entries.remove(at);
        self.next()
    }
}
fn missing() -> String { chilpa_i18n::t("That saved query no longer exists.") }

fn mutate_at(file: &std::path::Path, change: impl FnOnce(&mut Catalog) -> Result<(), String>) -> Result<Catalog, String> {
    settings::update_value_at(file, settings::SAVED_QUERIES_KEY, |raw| {
        let mut candidate = Catalog::read(raw)?;
        change(&mut candidate)?;
        Ok((candidate.encode()?, candidate))
    })
}

/// I read on demand without evaluating expressions against the library.
pub fn list() -> Result<Catalog, String> {
    Catalog::read(settings::read_value(settings::SAVED_QUERIES_KEY).map_err(storage)?.as_deref())
}
pub fn save(name: &str, text: &str) -> Result<Catalog, String> {
    mutate_at(&settings::path(), |catalog| catalog.add(name, text)).map_err(storage)
}
pub fn rename(id: &str, name: &str) -> Result<Catalog, String> {
    mutate_at(&settings::path(), |catalog| catalog.rename(id, name)).map_err(storage)
}
pub fn remove(id: &str) -> Result<Catalog, String> {
    mutate_at(&settings::path(), |catalog| catalog.remove(id)).map_err(storage)
}
/// I validate before the caller changes its filter and preserve the expression verbatim.
pub fn expression(id: &str) -> Result<String, String> {
    let catalog = list()?;
    let entry = catalog.entries.iter().find(|entry| entry.id == id).ok_or_else(missing)?;
    validate_expression(&entry.expression)?;
    Ok(entry.expression.clone())
}
impl Catalog {
    /// I publish only identities and names to the shared view, never duplicate expression payloads.
    pub fn summary_json(&self) -> String {
        let entries: Vec<_> = self.entries.iter().map(|entry| serde_json::json!({"id":entry.id,"name":entry.name})).collect();
        serde_json::json!({"revision":self.revision,"entries":entries}).to_string()
    }
}
impl Chilpa {
    pub fn saved_queries(&self) -> Result<Catalog, String> { list() }
    pub fn save_query(&mut self, name: &str, text: &str) -> Result<Catalog, String> { save(name, text) }
    pub fn rename_query(&mut self, id: &str, name: &str) -> Result<Catalog, String> { rename(id, name) }
    pub fn remove_query(&mut self, id: &str) -> Result<Catalog, String> { remove(id) }
    pub fn saved_query(&self, id: &str) -> Result<String, String> { expression(id) }
}

#[cfg(test)]
mod tests;
