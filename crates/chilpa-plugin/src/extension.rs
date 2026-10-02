//! Data-only metadata from a compatibility package, before running its code.

use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionMetadata {
    pub namespace: String,
    pub version: String,
    pub author: String,
    pub homepage: String,
    pub icon: String,
    pub settings_declared: bool,
}

#[derive(Deserialize)]
struct Document {
    module: String,
    version: String,
    #[serde(default)] creator: String,
    #[serde(default, rename = "homepageURL")] homepage: String,
    #[serde(default, rename = "optionsURL")] settings: String,
    #[serde(default)] icon: String,
}

impl ExtensionMetadata {
    pub fn read(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > 1024 * 1024 { return Err("The extension metadata is too large."); }
        let text = std::str::from_utf8(bytes).map_err(|_| "The extension metadata is not UTF-8.")?;
        crate::catalog::validate_xml(text, b"extension").or_else(|_| crate::catalog::validate_xml(text, b"extensions")).map_err(|_| "The extension metadata is invalid.")?;
        let document: Document = quick_xml::de::from_str(text).map_err(|_| "The extension metadata is incomplete.")?;
        let namespace = document.module.strip_prefix("Plugins::").and_then(|rest| rest.strip_suffix("::Plugin"))
            .ok_or("The extension entry point is unsupported.")?;
        if namespace.is_empty() || namespace.len() > 128 || document.version.trim().is_empty()
            || !namespace.bytes().enumerate().all(|(index, byte)| byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())) {
            return Err("The extension entry point or version is invalid.");
        }
        Ok(Self { namespace: namespace.into(), version: document.version, author: document.creator,
            homepage: document.homepage, icon: document.icon, settings_declared: !document.settings.is_empty() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_cannot_choose_an_executable_or_another_module_tree() {
        let valid = "<extension><module>Plugins::Guest::Plugin</module><version>1.2</version><optionsURL>plugins/Guest/settings.html</optionsURL></extension>";
        let metadata = ExtensionMetadata::read(valid.as_bytes()).unwrap();
        assert_eq!(metadata.namespace, "Guest");
        assert!(metadata.settings_declared);
        assert_eq!(ExtensionMetadata::read(valid.replace("<extension>", "<extensions>").replace("</extension>", "</extensions>").as_bytes()).unwrap(), metadata);
        for module in ["Plugins::../Guest::Plugin", "Other::Guest::Plugin", "Plugins::Nested::Guest::Plugin", "Plugins::1Guest::Plugin"] {
            assert!(ExtensionMetadata::read(valid.replace("Plugins::Guest::Plugin", module).as_bytes()).is_err());
        }
        assert!(ExtensionMetadata::read(b"<!DOCTYPE extension><extension/>").is_err());
    }
}
