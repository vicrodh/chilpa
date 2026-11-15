//! Genre choices use the checked settings writer and their own artwork folder.
//! At most 256 genres with two roles each / 64 KiB of JSON and 8 MiB per
//! encoded image. One page reads the map once and drops it; image bytes and
//! global caches are absent. Two temporary encoded copies add at most 16 MiB
//! during a choice. Replacement and reset remove files after their last use.
use super::*;
use std::path::{Path, PathBuf};

// Plain settings keys stay outside legacy window snapshots. V2 is authoritative
// once present, including an empty map; older choices cannot revive a reset role.
const LEGACY_KEY: &str = "genre_artwork_v1";
const SETTINGS_KEY: &str = "genre_artwork_v2";
static CHOOSING: std::sync::Mutex<()> = std::sync::Mutex::new(());
const MAX_CHOICES: usize = 256;
const MAX_JSON: usize = 64 * 1024;
const MAX_IMAGE: u64 = 8 * 1024 * 1024;
type Choices = BTreeMap<String, Roles>;

#[derive(Clone, Copy)]
enum Role { Card, Header }

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Roles {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    card: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    header: Option<String>,
}

impl Roles {
    fn get(&self, role: Role) -> Option<&str> {
        match role { Role::Card => self.card.as_deref(), Role::Header => self.header.as_deref() }
    }
    fn set(&mut self, role: Role, file: Option<String>) {
        match role { Role::Card => self.card = file, Role::Header => self.header = file }
    }
    fn empty(&self) -> bool { self.card.is_none() && self.header.is_none() }
}

fn folder() -> PathBuf { chilpa_dirs::data().join("genre-artwork") }
fn invalid() -> String { chilpa_i18n::t("The genre image preferences are invalid.") }
fn valid_identity(key: &str) -> bool { key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()) }
fn valid_file(file: &str) -> bool {
    file.rsplit_once('.').is_some_and(|(stem, extension)| stem.len() == 16 &&
        stem.bytes().all(|byte| byte.is_ascii_hexdigit()) && matches!(extension, "jpg" | "png" | "webp"))
}
fn checked_text(text: Option<&str>) -> Result<&str, String> {
    let text = text.unwrap_or("{}");
    if text.len() > MAX_JSON { return Err(chilpa_i18n::t("The genre image preferences are too large.")); }
    Ok(text)
}
fn decode(text: Option<&str>) -> Result<Choices, String> {
    let choices: Choices = serde_json::from_str(checked_text(text)?).map_err(|error| error.to_string())?;
    if choices.len() > MAX_CHOICES || choices.iter().any(|(key, roles)| !valid_identity(key) ||
        roles.empty() || roles.card.iter().chain(&roles.header).any(|file| !valid_file(file))) { return Err(invalid()); }
    Ok(choices)
}
fn legacy(text: Option<&str>) -> Result<Choices, String> {
    let choices: BTreeMap<String, String> = serde_json::from_str(checked_text(text)?).map_err(|error| error.to_string())?;
    if choices.len() > MAX_CHOICES || choices.iter().any(|(key, file)| !valid_identity(key) || !valid_file(file)) { return Err(invalid()); }
    Ok(choices.into_iter().map(|(key, file)| (key, Roles { card:Some(file.clone()), header:Some(file) })).collect())
}
fn read_choices(settings: &Path) -> Result<Choices, String> {
    let current = chilpa_core::settings::read_value_from(settings, SETTINGS_KEY).map_err(|error| error.to_string())?;
    match current {
        Some(text) => decode(Some(&text)),
        None => legacy(chilpa_core::settings::read_value_from(settings, LEGACY_KEY).map_err(|error| error.to_string())?.as_deref()),
    }
}
fn referenced(choices: &Choices, file: &str) -> bool {
    choices.values().any(|roles| roles.card.as_deref() == Some(file) || roles.header.as_deref() == Some(file))
}

pub(super) fn choices() -> Choices { read_choices(&chilpa_core::settings::path()).unwrap_or_default() }

pub(super) fn portrait(choices: &Choices, name: &str) -> String {
    choices.get(&presentation::token(name)).and_then(|roles| roles.get(Role::Card))
        .map(|file| folder().join(file).to_string_lossy().into_owned()).unwrap_or_default()
}

fn keep_image(picked: &Path, into: &Path) -> Result<String, String> {
    use std::io::Read;
    let input = std::fs::File::open(picked).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(into).map_err(|error| error.to_string())?;
    let mut temporary = tempfile::NamedTempFile::new_in(into).map_err(|error| error.to_string())?;
    let size = std::io::copy(&mut input.take(MAX_IMAGE + 1), &mut temporary).map_err(|error| error.to_string())?;
    if size > MAX_IMAGE { return Err(chilpa_i18n::t("The image is too large")); }
    crate::chosen_artwork::keep(temporary.path(), into)
}

fn choose(settings: &Path, into: &Path, identity: &str, role: Role, picked: Option<&Path>) -> Result<bool, String> {
    choose_using(settings, into, identity, role, picked, keep_image)
}

fn choose_using(settings: &Path, into: &Path, identity: &str, role: Role, picked: Option<&Path>,
    copy: impl FnOnce(&Path, &Path) -> Result<String, String>) -> Result<bool, String> {
    let _guard = CHOOSING.lock().unwrap_or_else(|error| error.into_inner());
    // Read before copying so damaged preferences never silently lose choices.
    let existing = read_choices(settings)?;
    if !valid_identity(identity) { return Err(invalid()); }
    if picked.is_some() && !existing.contains_key(identity) && existing.len() >= MAX_CHOICES {
        return Err(chilpa_i18n::t("Remove a chosen genre image before adding another."));
    }
    if picked.is_none() && existing.get(identity).and_then(|roles| roles.get(role)).is_none() { return Ok(false); }
    let path = picked.map(|picked| copy(picked, into)).transpose()?;
    let filename = path.as_deref().and_then(|path| Path::new(path).file_name()).and_then(|name| name.to_str()).map(str::to_owned);
    let updated = chilpa_core::settings::update_value_at(settings, SETTINGS_KEY, |text| {
        // The settings writer rechecks V2 under its lock. On the first write,
        // every legacy choice is carried into both roles in this same commit.
        let mut choices = match text { Some(_) => decode(text)?, None => existing.clone() };
        let before = choices.get(identity).and_then(|roles| roles.get(role)).map(str::to_owned);
        if before == filename {
            if let Some(text) = text { return Ok((text.to_owned(), (false, None))); }
        }
        let roles = choices.entry(identity.to_string()).or_default();
        roles.set(role, filename.clone());
        if roles.empty() { choices.remove(identity); }
        let encoded = serde_json::to_string(&choices).map_err(|error| error.to_string())?;
        decode(Some(&encoded))?;
        let removed = before.as_ref().filter(|file| !referenced(&choices, file)).cloned();
        Ok((encoded, (before != filename, removed)))
    });
    let (changed, removed) = match updated {
        Ok(updated) => updated,
        Err(error) => {
            // Recheck committed references before cleaning a newly copied file.
            // Keep the file while its committed references are unknown.
            if let Some(file) = filename.as_ref() {
                if read_choices(settings).is_ok_and(|choices| !referenced(&choices, file)) {
                    let _ = std::fs::remove_file(into.join(file));
                }
            }
            return Err(error);
        }
    };
    if let Some(removed) = removed { let _ = std::fs::remove_file(into.join(removed)); }
    Ok(changed)
}

fn selection(key: &str) -> Result<Selection, String> {
    Selection::parse(key).filter(|selected| selected.kind == "genre")
        .ok_or_else(|| chilpa_i18n::t("Invalid discovery selection."))
}

fn identity(selected: &Selection, library: Option<&chilpa_library::Library>, catalog: Option<&crate::source_catalog::Catalog>) -> Result<String, String> {
    if selected.genre_token.is_empty() {
        if selected.name.trim().is_empty() { return Err(chilpa_i18n::t("Invalid discovery selection.")); }
        return Ok(presentation::token(&selected.name));
    }
    let matches = |name: &str| !name.trim().is_empty() && presentation::token(name) == selected.genre_token;
    let found = if selected.genre_id > 0 || selected.album > 0 {
        let library = library.ok_or_else(|| chilpa_i18n::t("The library could not be opened."))?;
        if selected.genre_id > 0 {
            library.genre_name_by_id(selected.genre_id).map_err(|error| error.to_string())?.is_some_and(|name| matches(&name))
        } else {
            let mut found = false;
            library.visit_album_genres(selected.album, |name, _| { found |= matches(name); Ok(()) }).map_err(|error| error.to_string())?;
            found
        }
    } else {
        catalog.is_some_and(|catalog| catalog.genre_anchor_matches(selected.album, selected.genre_track, matches))
    };
    if !found { return Err(chilpa_i18n::t("This genre is no longer available.")); }
    Ok(selected.genre_token.clone())
}

#[cfg(test)]
impl Reader {
    pub(crate) fn genre_art_identity(&self, key: &str) -> Result<String, String> {
        identity(&selection(key)?, Some(&self.lib), self.catalog.as_deref())
    }
}

impl crate::Chilpa {
    fn genre_art_identity(&self, key: &str) -> Result<String, String> {
        let selected = selection(key)?;
        let reader = if !selected.genre_token.is_empty() && (selected.genre_id > 0 || selected.album > 0) { self.reader() } else { None };
        identity(&selected, reader.as_ref().map(|reader| &reader.lib), Some(&self.catalog))
    }

    fn genre_role_art(&mut self, key: &str, role: Role) -> String {
        let choices = choices();
        if !choices.values().any(|roles| roles.get(role).is_some()) { return String::new(); }
        let Ok(identity) = self.genre_art_identity(key) else { return String::new() };
        choices.get(&identity).and_then(|roles| roles.get(role))
            .map(|file| folder().join(file).to_string_lossy().into_owned()).unwrap_or_default()
    }

    pub fn genre_art(&mut self, key: &str) -> String { self.genre_role_art(key, Role::Card) }
    pub fn genre_header_art(&mut self, key: &str) -> String { self.genre_role_art(key, Role::Header) }
    pub fn genre_art_chosen(&mut self, key: &str) -> bool { !self.genre_art(key).is_empty() }

    pub fn choose_genre_art(&mut self, key: &str, picked: Option<&Path>) -> bool {
        self.choose_genre_role_art(key, picked, Role::Card)
    }
    pub fn choose_genre_header_art(&mut self, key: &str, picked: Option<&Path>) -> bool {
        self.choose_genre_role_art(key, picked, Role::Header)
    }
    fn choose_genre_role_art(&mut self, key: &str, picked: Option<&Path>, role: Role) -> bool {
        let result = self.genre_art_identity(key)
            .and_then(|identity| choose(&chilpa_core::settings::path(), &folder(), &identity, role, picked));
        match result {
            Ok(true) => { self.genre_art_changed(); true }
            Ok(false) => false,
            Err(error) => { self.now.error = error; false }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn genre_images_share_strong_aliases_persist_and_clear_without_changing_tags() {
        let temp = tempfile::tempdir().unwrap();
        let settings = temp.path().join("settings.conf");
        let into = temp.path().join("images");
        let image = temp.path().join("picked.png");
        image::RgbImage::new(2, 2).save(&image).unwrap();
        let identity = presentation::token("Heavy Metal");
        assert_eq!(identity, presentation::token("heavy-metal"));
        assert_eq!(presentation::token("Personal-Shelf"), presentation::token("personal shelf"));
        assert_ne!(presentation::token("Progresive Metal"), presentation::token("Progressive Metal"));
        let stale_settings = chilpa_core::settings::load_from(&settings);
        assert!(choose(&settings, &into, &identity, Role::Card, Some(&image)).unwrap());
        chilpa_core::settings::save_to(&settings, &stale_settings).unwrap();
        let saved = std::fs::read(&settings).unwrap();
        assert!(!choose(&settings, &into, &identity, Role::Card, Some(&image)).unwrap());
        assert_eq!(std::fs::read(&settings).unwrap(), saved);
        let choices = decode(chilpa_core::settings::read_value_from(&settings, SETTINGS_KEY).unwrap().as_deref()).unwrap();
        assert!(into.join(choices[&identity].card.as_ref().unwrap()).is_file());
        assert!(choose(&settings, &into, &identity, Role::Card, None).unwrap());
        assert_eq!(std::fs::read_dir(&into).unwrap().count(), 0);
        let large = temp.path().join("large.png");
        std::fs::File::create(&large).unwrap().set_len(MAX_IMAGE + 1).unwrap();
        let before = std::fs::read(&settings).unwrap();
        assert!(choose(&settings, &into, &identity, Role::Card, Some(&large)).is_err());
        assert_eq!(std::fs::read(&settings).unwrap(), before);
        assert_eq!(std::fs::read_dir(&into).unwrap().count(), 0);
        assert!(decode(Some(&format!("{{\"{}\":\"../escape.png\"}}", identity))).is_err());
        assert!(decode(Some(&"x".repeat(MAX_JSON + 1))).is_err());
        let (reader, _) = super::super::source_tests::fixture();
        let revision = reader.lib.read_revision().unwrap();
        let tags = reader.lib.track_genres(1).unwrap();
        let key = reader.album_genre_links(1)["items"][0]["key"].as_str().unwrap().to_string();
        assert!(reader.genre_art_identity(&key).is_ok());
        assert_eq!(reader.lib.track_genres(1).unwrap(), tags);
        assert_eq!(reader.lib.read_revision().unwrap(), revision);
    }

    #[test]
    fn image_identity_resolves_full_opaque_names_and_rejects_stale_tokens() {
        let (reader, _) = super::super::source_tests::fixture();
        let name = format!("{}-distinct-ending", "x".repeat(1024 * 1024 - 16));
        reader.lib.set_track_genres(1, &[name.clone()]).unwrap();
        let key = json!({"kind":"genre", "name":"short label", "genreId":reader.lib.genre_id_for_name(&name).unwrap().unwrap(),
            "genreToken":presentation::token(&name)});
        let revision = reader.lib.read_revision().unwrap();
        assert_eq!(reader.genre_art_identity(&key.to_string()).unwrap(), presentation::token(&name));
        let mut stale = key;
        stale["genreToken"] = json!("0".repeat(64));
        assert!(reader.genre_art_identity(&stale.to_string()).is_err());
        assert_eq!(reader.lib.read_revision().unwrap(), revision);
        assert_eq!(reader.lib.track_genres(1).unwrap(), [name]);
    }
}

#[cfg(test)]
#[path = "chosen_art_roles_tests.rs"]
mod roles_tests;
