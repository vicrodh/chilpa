//! Embedded pages have public browser state, never access to host or plugin credentials.
use crate::natives::Natives;
use serde_json::Value;
use url::Url;

fn public_url(address: &str) -> Option<Url> {
    if address.len() > 8192 {
        return None;
    }
    let url = Url::parse(address).ok()?;
    (url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443))
    .then_some(url)
}

pub(crate) fn allowed(domains: &[&str], origins: &Value, initial: &str, requested: &str) -> bool {
    let (Some(initial), Some(requested)) = (public_url(initial), public_url(requested)) else {
        return false;
    };
    if initial.origin() != requested.origin() {
        return false;
    }
    let Some(origins) = origins.as_array().filter(|origins| origins.len() <= 8) else {
        return false;
    };
    let host = initial.host_str().unwrap_or_default();
    if !domains
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
    {
        return false;
    }
    origins
        .iter()
        .filter_map(Value::as_str)
        .filter_map(public_url)
        .any(|origin| {
            origin.path() == "/"
                && origin.query().is_none()
                && origin.fragment().is_none()
                && origin.origin() == initial.origin()
        })
}

pub(crate) fn validate(natives: &Natives, plugin: &str, document: &Value) -> Result<(), String> {
    let initial = document["web"]["url"]
        .as_str()
        .ok_or("The embedded page has no address")?;
    if !natives.web_allowed(plugin, initial, initial) {
        return Err("The embedded page is outside this plugin's approved HTTPS origins".into());
    }
    Ok(())
}

impl crate::Chilpa {
    pub fn source_web_allowed(&self, plugin: &str, initial: &str, address: &str) -> bool {
        !chilpa_core::network::current().offline
            && self.natives.web_allowed(plugin, initial, address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn browser_navigation_cannot_expand_an_origin_or_manifest_grant() {
        let domains = vec!["example.test"];
        let origins = json!(["https://www.example.test"]);
        assert!(allowed(
            &domains,
            &origins,
            "https://www.example.test/journal",
            "https://www.example.test/article?lang=en#top"
        ));
        for address in [
            "file:///etc/passwd",
            "data:text/html,content",
            "javascript:alert(1)",
            "http://www.example.test/",
            "https://www.example.test:444/",
            "https://user:secret@www.example.test/",
            "https://other.example.test/",
            "https://www.example.test.evil.test/",
        ] {
            assert!(!allowed(
                &domains,
                &origins,
                "https://www.example.test/journal",
                address
            ));
        }
        assert!(!allowed(
            &[],
            &origins,
            "https://www.example.test/journal",
            "https://www.example.test/article"
        ));
    }
}
