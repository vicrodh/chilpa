//! Data-only catalog readers. Reading a catalog never executes a package.

use serde::{Deserialize, Serialize};

pub const MAX_CATALOG_BYTES: usize = 1024 * 1024;
pub const MAX_CATALOG_ENTRIES: usize = 5000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digest {
    pub algorithm: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub category: String,
    pub author: String,
    pub repository: String,
    pub package: String,
    pub digest: Digest,
    pub adapter: String,
    /// Catalog problems prevent installation but remain visible in the list.
    pub problem: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogError { TooLarge, Invalid, Unsupported }

pub fn repository_url(input: &str) -> Result<String, CatalogError> {
    if input.len() > 4096 { return Err(CatalogError::TooLarge); }
    let mut url = url::Url::parse(input.trim()).map_err(|_| CatalogError::Invalid)?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none()
        || !url.username().is_empty() || url.password().is_some() {
        return Err(CatalogError::Invalid);
    }
    url.set_fragment(None);
    Ok(url.to_string())
}

pub fn redirect_url(base: &str, location: &str) -> Result<String, CatalogError> {
    let base = url::Url::parse(base).map_err(|_| CatalogError::Invalid)?;
    let joined = base.join(location).map_err(|_| CatalogError::Invalid)?;
    repository_url(joined.as_str())
}

pub fn read_catalog(bytes: &[u8], language: &str) -> Result<Vec<CatalogEntry>, CatalogError> {
    if bytes.len() > MAX_CATALOG_BYTES { return Err(CatalogError::TooLarge); }
    let text = std::str::from_utf8(bytes).map_err(|_| CatalogError::Invalid)?;
    let text = text.trim_start_matches('\u{feff}').trim_start();
    let entries = if text.starts_with('<') {
        read_extensions(text, language)?
    } else {
        read_native(text)?
    };
    if entries.len() > MAX_CATALOG_ENTRIES { return Err(CatalogError::TooLarge); }
    Ok(entries.into_iter().map(validate_entry).collect())
}

fn validate_entry(mut entry: CatalogEntry) -> CatalogEntry {
    let digest_length = match entry.digest.algorithm.as_str() { "sha256" => 64, "sha1" => 40, _ => 0 };
    if entry.id.is_empty() || entry.id.len() > 256 || entry.name.trim().is_empty() {
        entry.problem = "The catalog does not provide a valid plugin identity.".into();
    } else if repository_url(&entry.package).is_err() {
        entry.problem = "The catalog does not provide a usable package URL.".into();
    } else if digest_length == 0 || entry.digest.value.len() != digest_length
        || !entry.digest.value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        entry.problem = "The catalog does not provide a supported package checksum.".into();
    }
    entry.digest.value.make_ascii_lowercase();
    entry
}

#[derive(Deserialize)]
struct NativeCatalog { version: u32, plugins: Vec<NativeEntry> }
#[derive(Deserialize)]
struct ApiRange { min: u32, max: u32 }
#[derive(Deserialize)]
struct NativeEntry {
    id: String, name: String, repository: String, package: String, sha256: String,
    api: ApiRange,
    #[serde(default)] version: String,
    #[serde(default)] description: String,
    #[serde(default)] author: String,
    #[serde(default)] category: String,
    #[serde(default)] packages: std::collections::BTreeMap<String, NativePackage>,
}

#[derive(Deserialize)]
struct NativePackage { package: String, sha256: String }

fn read_native(text: &str) -> Result<Vec<CatalogEntry>, CatalogError> {
    read_native_for_platform(text, &format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH))
}

fn read_native_for_platform(text: &str, platform: &str) -> Result<Vec<CatalogEntry>, CatalogError> {
    let catalog: NativeCatalog = serde_json::from_str(text).map_err(|_| CatalogError::Invalid)?;
    if catalog.version != 1 { return Err(CatalogError::Unsupported); }
    if catalog.plugins.len() > MAX_CATALOG_ENTRIES { return Err(CatalogError::TooLarge); }
    Ok(catalog.plugins.into_iter().filter_map(|mut entry| {
        let problem = if entry.api.min <= crate::API_VERSION && entry.api.max >= crate::API_VERSION { String::new() }
            else { "This plugin requires another version of the plugin API.".into() };
        if !entry.packages.is_empty() {
            match entry.packages.remove(platform) {
                Some(package) => { entry.package = package.package; entry.sha256 = package.sha256; }
                None => return None,
            }
        }
        Some(CatalogEntry {
            id: entry.id, name: entry.name, version: entry.version, description: entry.description,
            author: entry.author, category: entry.category, repository: entry.repository, package: entry.package,
            digest: Digest { algorithm: "sha256".into(), value: entry.sha256 }, adapter: "native".into(), problem,
        })
    }).collect())
}

#[derive(Deserialize)]
struct Extensions { #[serde(default)] plugins: ExtensionList }
#[derive(Default, Deserialize)]
struct ExtensionList { #[serde(default, rename = "plugin")] entries: Vec<Extension> }
#[derive(Deserialize)]
struct Localized {
    #[serde(rename = "@lang", default)] language: String,
    #[serde(rename = "$text", default)] text: String,
}
#[derive(Deserialize)]
struct Extension {
    #[serde(rename = "@name")] name: String,
    #[serde(rename = "@version", default)] version: String,
    #[serde(rename = "@creator", default)] author: String,
    #[serde(rename = "@category", default)] category: String,
    #[serde(rename = "@link", default)] repository: String,
    #[serde(rename = "@url", default)] package: String,
    #[serde(rename = "@sha", default)] sha: String,
    #[serde(default)] title: Vec<Localized>,
    #[serde(default, rename = "desc")] descriptions: Vec<Localized>,
}

fn localized(values: &[Localized], language: &str) -> String {
    values.iter().find(|value| value.language.eq_ignore_ascii_case(language))
        .or_else(|| values.iter().find(|value| value.language.eq_ignore_ascii_case("EN")))
        .or_else(|| values.first()).map(|value| value.text.trim().to_string()).unwrap_or_default()
}

fn read_extensions(text: &str, language: &str) -> Result<Vec<CatalogEntry>, CatalogError> {
    validate_xml(text, b"extensions")?;
    let catalog: Extensions = quick_xml::de::from_str(text).map_err(|_| CatalogError::Invalid)?;
    Ok(catalog.plugins.entries.into_iter().map(|entry| {
        let title = localized(&entry.title, language);
        CatalogEntry { name: if title.is_empty() { entry.name.clone() } else { title }, id: entry.name,
            version: entry.version, author: entry.author, category: entry.category, description: localized(&entry.descriptions, language),
            repository: entry.repository, package: entry.package,
            digest: Digest { algorithm: "sha1".into(), value: entry.sha }, adapter: "perl".into(), problem: String::new() }
    }).collect())
}

pub(crate) fn validate_xml(text: &str, expected_root: &[u8]) -> Result<(), CatalogError> {
    use quick_xml::{events::Event, Reader};
    let mut reader = Reader::from_str(text);
    let mut depth = 0usize;
    let mut root = false;
    loop {
        match reader.read_event().map_err(|_| CatalogError::Invalid)? {
            Event::DocType(_) => return Err(CatalogError::Invalid),
            Event::Start(tag) => {
                if depth == 0 {
                    if root || tag.name().as_ref() != expected_root { return Err(CatalogError::Unsupported); }
                    root = true;
                }
                depth += 1;
                if depth > 32 { return Err(CatalogError::TooLarge); }
            }
            Event::End(_) => { depth = depth.checked_sub(1).ok_or(CatalogError::Invalid)?; }
            Event::Empty(tag) if depth == 0 => {
                if root || tag.name().as_ref() != expected_root { return Err(CatalogError::Unsupported); }
                root = true;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !root || depth != 0 { return Err(CatalogError::Invalid); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_catalog_is_localized_without_importing_applet_code_or_following_repositories() {
        let xml = br#"<extensions><applets><applet name="Ignored"/></applets><plugins>
            <plugin name="Guest" version="1" category="tools" creator="Author" url="https://example.org/package.zip" sha="0123456789012345678901234567890123456789">
            <title lang="EN">Music &amp; more</title><title lang="ES">Music</title><desc lang="EN">Description</desc></plugin>
            </plugins><repos><repo>https://elsewhere.example/automatic.xml</repo></repos></extensions>"#;
        let entries = read_catalog(xml, "EN").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Music & more");
        assert_eq!(entries[0].adapter, "perl");
        assert_eq!(entries[0].category, "tools");
        assert!(entries[0].problem.is_empty());
        assert_eq!(read_catalog(xml, "ES").unwrap()[0].name, "Music");
    }

    #[test]
    fn native_api_and_package_integrity_are_checked_without_running_anything() {
        let native = format!(r#"{{"version":1,"plugins":[{{"id":"org.example.guest","name":"Guest",
            "repository":"https://example.org/source","package":"https://example.org/package.zip",
            "sha256":"{}","api":{{"min":1,"max":1}}}}]}}"#, "a".repeat(64));
        assert!(read_catalog(native.as_bytes(), "EN").unwrap()[0].problem.is_empty());
        let incompatible = native.replace("\"min\":1", "\"min\":9");
        assert!(!read_catalog(incompatible.as_bytes(), "EN").unwrap()[0].problem.is_empty());
        assert_eq!(read_catalog(br#"{"version":1,"plugins":[]}"#, "EN").unwrap().len(), 0);
    }

    #[test]
    fn external_entities_credentials_and_unbounded_inputs_are_rejected() {
        assert_eq!(read_catalog(br#"<!DOCTYPE extensions [<!ENTITY x SYSTEM "file:///private">]><extensions/>"#, "EN"), Err(CatalogError::Invalid));
        assert_eq!(read_catalog(&vec![b'x'; MAX_CATALOG_BYTES + 1], "EN"), Err(CatalogError::TooLarge));
        assert!(repository_url("https://user:password@example.org/repo").is_err());
        assert!(repository_url("file:///plugins.json").is_err());
        assert_eq!(repository_url(" https://EXAMPLE.org/repo#part ").unwrap(), "https://example.org/repo");
    }

    #[test]
    fn a_platform_package_has_its_own_url_and_checksum() {
        let catalog = serde_json::json!({"version":1,"plugins":[{
            "id":"app.example.worker","name":"Worker","repository":"https://example.org/source",
            "package":"https://example.org/legacy.zip","sha256":"a".repeat(64),"api":{"min":1,"max":1},
            "packages":{
                "linux-x86_64":{"package":"https://example.org/linux.zip","sha256":"b".repeat(64)},
                "macos-aarch64":{"package":"https://example.org/macos.zip","sha256":"c".repeat(64)}
            }
        }]}).to_string();
        let linux = read_native_for_platform(&catalog, "linux-x86_64").unwrap().remove(0);
        assert_eq!(linux.package, "https://example.org/linux.zip");
        assert_eq!(linux.digest.value, "b".repeat(64));
        let mac = read_native_for_platform(&catalog, "macos-aarch64").unwrap().remove(0);
        assert_eq!(mac.package, "https://example.org/macos.zip");
        assert_eq!(mac.digest.value, "c".repeat(64));
        assert!(validate_entry(mac).problem.is_empty());
        for platform in ["windows-x86_64", "macos-x86_64", "linux-aarch64"] {
            assert!(read_native_for_platform(&catalog, platform).unwrap().is_empty());
        }
    }

    #[test]
    fn portable_packages_remain_visible_when_native_packages_have_no_matching_target() {
        let portable = serde_json::json!({"id":"app.example.portable","name":"Portable",
            "repository":"https://example.org/source","package":"https://example.org/plugin.wasm.zip",
            "sha256":"a".repeat(64),"api":{"min":1,"max":1}});
        let mut worker = portable.clone();
        worker["id"] = "app.example.worker".into();
        worker["packages"] = serde_json::json!({"linux-x86_64":{
            "package":"https://example.org/linux.zip","sha256":"b".repeat(64)}});
        let catalog = serde_json::json!({"version":1,"plugins":[worker,portable]}).to_string();
        for platform in ["macos-aarch64", "macos-x86_64", "linux-aarch64", "windows-x86_64"] {
            let entries = read_native_for_platform(&catalog, platform).unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].id, "app.example.portable");
        }
        assert_eq!(read_native_for_platform(&catalog, "linux-x86_64").unwrap().len(), 2);
    }

    #[test]
    fn a_platform_package_does_not_bypass_integrity_checks() {
        let catalog = serde_json::json!({"version":1,"plugins":[{
            "id":"app.example.worker","name":"Worker","repository":"https://example.org/source",
            "package":"https://example.org/legacy.zip","sha256":"a".repeat(64),"api":{"min":1,"max":1},
            "packages":{"macos-aarch64":{"package":"https://example.org/macos.zip","sha256":"invalid"}}
        }]}).to_string();
        let entry = read_native_for_platform(&catalog, "macos-aarch64").unwrap().remove(0);
        assert!(validate_entry(entry).problem.contains("checksum"));
    }
}
