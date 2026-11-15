//! Genre choices use the checked settings writer and their own artwork folder.
//! At most 256 choices / 64 KiB of JSON and 8 MiB per encoded image. One page
//! reads the map once and drops it; no image bytes or global cache remain live.
//! Two temporary encoded copies add at most 16 MiB during a choice; replacement
//! and reset remove unreferenced owned files. Hidden views run no work.
use super::*;
use std::path::{Path, PathBuf};

// A plain settings key stays outside legacy window snapshots, so an old
// Settings::save cannot restore an image choice that was removed meanwhile.
const SETTINGS_KEY: &str = "genre_artwork_v1";
static CHOOSING: std::sync::Mutex<()> = std::sync::Mutex::new(());
const MAX_CHOICES: usize = 256;
const MAX_JSON: usize = 64 * 1024;
const MAX_IMAGE: u64 = 8 * 1024 * 1024;
type Choices = BTreeMap<String, String>;

fn folder() -> PathBuf { chilpa_dirs::data().join("genre-artwork") }

fn decode(text: Option<&str>) -> Result<Choices, String> {
    let text = text.unwrap_or("{}");
    if text.len() > MAX_JSON { return Err(chilpa_i18n::t("The genre image preferences are too large.")); }
    let choices: Choices = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if choices.len() > MAX_CHOICES || choices.iter().any(|(key, file)| {
        key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) ||
        !file.rsplit_once('.').is_some_and(|(stem, extension)| stem.len() == 16 &&
            stem.bytes().all(|b| b.is_ascii_hexdigit()) && matches!(extension, "jpg" | "png" | "webp"))
    }) { return Err(chilpa_i18n::t("The genre image preferences are invalid.")); }
    Ok(choices)
}

pub(super) fn choices() -> Choices {
    chilpa_core::settings::read_value(SETTINGS_KEY).ok()
        .and_then(|text| decode(text.as_deref()).ok()).unwrap_or_default()
}

pub(super) fn portrait(choices: &Choices, name: &str) -> String {
    choices.get(&presentation::token(name)).map(|file| folder().join(file).to_string_lossy().into_owned()).unwrap_or_default()
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

fn choose(settings: &Path, into: &Path, identity: &str, picked: Option<&Path>) -> Result<bool, String> {
    let _guard = CHOOSING.lock().unwrap_or_else(|error| error.into_inner());
    // Read before copying so damaged preferences never silently lose choices.
    let existing = decode(chilpa_core::settings::read_value_from(settings, SETTINGS_KEY).map_err(|error| error.to_string())?.as_deref())?;
    if picked.is_some() && !existing.contains_key(identity) && existing.len() >= MAX_CHOICES {
        return Err(chilpa_i18n::t("Remove a chosen genre image before adding another."));
    }
    let path = picked.map(|picked| keep_image(picked, into)).transpose()?;
    let filename = path.as_deref().and_then(|path| Path::new(path).file_name()).and_then(|name| name.to_str()).map(str::to_owned);
    let updated = chilpa_core::settings::update_value_at(settings, SETTINGS_KEY, |text| {
        let mut choices = decode(text)?;
        let before = choices.get(identity).cloned();
        match &filename {
            Some(file) => { choices.insert(identity.to_string(), file.clone()); }
            None => { choices.remove(identity); }
        }
        let encoded = serde_json::to_string(&choices).map_err(|error| error.to_string())?;
        decode(Some(&encoded))?;
        let removed = before.as_ref().filter(|file| !choices.values().any(|used| used == *file)).cloned();
        Ok((encoded, (before != filename, removed)))
    });
    let (changed, removed) = match updated {
        Ok(updated) => updated,
        Err(error) => {
            if let Some(file) = filename.as_ref().filter(|file| !existing.values().any(|used| used == *file)) {
                let _ = std::fs::remove_file(into.join(file));
            }
            return Err(error);
        }
    };
    if let Some(removed) = removed { let _ = std::fs::remove_file(into.join(removed)); }
    Ok(changed)
}

impl Reader {
    fn genre_art_identity(&self, key: &str) -> Result<String, String> {
        let selected = Selection::parse(key).filter(|selected| selected.kind == "genre")
            .ok_or_else(|| chilpa_i18n::t("Invalid discovery selection."))?;
        let selected = self.resolve_album_genre_using(selected, &self.source_index().1)?;
        if selected.name.trim().is_empty() { return Err(chilpa_i18n::t("Invalid discovery selection.")); }
        Ok(presentation::token(&selected.name))
    }
}

impl crate::Chilpa {
    pub fn genre_art(&mut self, key: &str) -> String {
        let Some(reader) = self.reader() else { return String::new() };
        let Ok(identity) = reader.genre_art_identity(key) else { return String::new() };
        choices().get(&identity).map(|file| folder().join(file).to_string_lossy().into_owned()).unwrap_or_default()
    }

    pub fn genre_art_chosen(&mut self, key: &str) -> bool { !self.genre_art(key).is_empty() }

    pub fn choose_genre_art(&mut self, key: &str, picked: Option<&Path>) -> bool {
        let result = self.reader().ok_or_else(|| chilpa_i18n::t("The library could not be opened."))
            .and_then(|reader| reader.genre_art_identity(key))
            .and_then(|identity| choose(&chilpa_core::settings::path(), &folder(), &identity, picked));
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
        assert!(choose(&settings, &into, &identity, Some(&image)).unwrap());
        chilpa_core::settings::save_to(&settings, &stale_settings).unwrap();
        let saved = std::fs::read(&settings).unwrap();
        assert!(!choose(&settings, &into, &identity, Some(&image)).unwrap());
        assert_eq!(std::fs::read(&settings).unwrap(), saved);
        let choices = decode(chilpa_core::settings::read_value_from(&settings, SETTINGS_KEY).unwrap().as_deref()).unwrap();
        assert!(into.join(&choices[&identity]).is_file());
        assert!(choose(&settings, &into, &identity, None).unwrap());
        assert_eq!(std::fs::read_dir(&into).unwrap().count(), 0);
        let large = temp.path().join("large.png");
        std::fs::File::create(&large).unwrap().set_len(MAX_IMAGE + 1).unwrap();
        let before = std::fs::read(&settings).unwrap();
        assert!(choose(&settings, &into, &identity, Some(&large)).is_err());
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
