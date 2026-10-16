//! Suggested network permissions, reviewed before installation.
use crate::StagedPackage;
use std::collections::BTreeSet;

pub(crate) fn candidates(package: &StagedPackage) -> Vec<String> {
    let mut domains = BTreeSet::new();
    for path in package.files() {
        if !matches!(path.extension().and_then(|value| value.to_str()), Some("pm" | "xml" | "html")) { continue; }
        let Ok(bytes) = std::fs::read(package.root().join(path)) else { continue };
        let Ok(text) = std::str::from_utf8(&bytes) else { continue };
        for part in text.split(['\'', '"', '<', '>', ' ', '\n', '\t']) {
            let Ok(url) = url::Url::parse(part) else { continue };
            if !matches!(url.scheme(), "http" | "https") { continue; }
            let Some(host) = url.host_str() else { continue };
            if host.contains('.') && host.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-')) {
                domains.insert(host.to_string());
            }
        }
    }
    domains.into_iter().take(128).collect()
}
