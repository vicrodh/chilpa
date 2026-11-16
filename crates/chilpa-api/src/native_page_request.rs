//! Host-owned query envelopes keep provider references opaque.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};

const PREFIX: &str = "_host:v1:";
const MAX_BYTES: usize = 12 * 1024;

pub(crate) struct Request {
    pub params: Value,
    pub layout: String,
}

pub(crate) fn valid_reference(reference: &str) -> bool {
    !reference.is_empty()
        && reference.len() <= 8192
        && !reference
            .chars()
            .any(|c| c.is_whitespace() || c == '|' || c.is_control())
}

fn valid_filter(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => true,
        Value::String(s) => s.len() <= 1024,
        Value::Array(values) => {
            values.len() <= 64
                && values
                    .iter()
                    .all(|v| v.as_str().is_some_and(|s| s.len() <= 512))
        }
        _ => false,
    }
}

fn validate(mut value: Value) -> Result<Request, String> {
    let object = value.as_object_mut().ok_or("The page request is invalid")?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "ref" | "filters" | "order" | "limit" | "layout"
        )
    }) {
        return Err("The page request has an unsupported field".into());
    }
    if !object
        .get("ref")
        .and_then(Value::as_str)
        .is_some_and(valid_reference)
    {
        return Err("The page reference is invalid".into());
    }
    if let Some(filters) = object.get("filters") {
        let filters = filters.as_object().ok_or("The page filters are invalid")?;
        if filters.len() > 32
            || filters
                .iter()
                .any(|(key, value)| key.is_empty() || key.len() > 64 || !valid_filter(value))
        {
            return Err("The page filters exceed their limits".into());
        }
    }
    if object
        .get("order")
        .is_some_and(|order| !order.as_str().is_some_and(|s| s.len() <= 64))
    {
        return Err("The page order is invalid".into());
    }
    if object
        .get("limit")
        .is_some_and(|limit| !limit.as_u64().is_some_and(|n| (1..=100).contains(&n)))
    {
        return Err("The page size is invalid".into());
    }
    let layout = match object.remove("layout") {
        None => String::new(),
        Some(Value::String(layout)) if matches!(layout.as_str(), "collection" | "article") => layout,
        _ => return Err("The page layout is invalid".into()),
    };
    Ok(Request {
        params: value,
        layout,
    })
}

pub(crate) fn decode(reference: &str) -> Result<Request, String> {
    let Some(encoded) = reference.strip_prefix(PREFIX) else {
        return valid_reference(reference)
            .then(|| Request {
                params: json!({"ref": reference}),
                layout: String::new(),
            })
            .ok_or_else(|| "The page reference is invalid".into());
    };
    if encoded.len() > MAX_BYTES * 4 / 3 {
        return Err("The page query exceeds its limit".into());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "The page query is invalid")?;
    if bytes.len() > MAX_BYTES {
        return Err("The page query exceeds its limit".into());
    }
    validate(serde_json::from_slice(&bytes).map_err(|_| "The page query is invalid")?)
}

pub(crate) fn encode(reference: &str, query: Value, layout: &str) -> Result<String, String> {
    let mut params = query
        .as_object()
        .cloned()
        .ok_or("The page query is invalid")?;
    params.insert("ref".into(), reference.into());
    if !layout.is_empty() {
        params.insert("layout".into(), layout.into());
    }
    let mut checked = validate(Value::Object(params))?;
    if !checked.layout.is_empty() {
        checked.params["layout"] = checked.layout.into();
    }
    let bytes = serde_json::to_vec(&checked.params).map_err(|_| "The page query is invalid")?;
    if bytes.len() > MAX_BYTES {
        return Err("The page query exceeds its limit".into());
    }
    Ok(format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes)))
}

pub(crate) fn route(plugin: &str, reference: &str, query: Value, layout: &str) -> String {
    encode(reference, query, layout)
        .map(|encoded| format!("{plugin} native:{encoded}"))
        .unwrap_or_default()
}

impl crate::Chilpa {
    pub fn source_page_request_key(&self, key: &str, query: &str) -> String {
        let (plugin, rest) = crate::source_catalog::split(key);
        let Some(reference) = rest.strip_prefix("native:") else {
            return String::new();
        };
        let Ok(base) = decode(reference) else {
            return String::new();
        };
        let Some(reference) = base.params["ref"].as_str() else {
            return String::new();
        };
        let Ok(query) = serde_json::from_str(query) else {
            return String::new();
        };
        route(
            plugin,
            reference,
            query,
            if base.layout.is_empty() {
                "collection"
            } else {
                &base.layout
            },
        )
    }
    pub fn source_cover_override(&self, id: i64) -> String {
        self.catalog
            .chosen_art(id)
            .and_then(|path| url::Url::from_file_path(path).ok())
            .map(|url| url.to_string())
            .unwrap_or_default()
    }
    pub fn source_page_same_navigation(&self, key: &str, navigation: &str) -> bool {
        if key == navigation {
            return true;
        }
        let (plugin, reference) = crate::source_catalog::split(key);
        let (other, target) = crate::source_catalog::split(navigation);
        if plugin != other {
            return false;
        }
        let (Some(reference), Some(target)) = (
            reference.strip_prefix("native:"),
            target.strip_prefix("native:"),
        ) else {
            return false;
        };
        let (Ok(request), Ok(target)) = (decode(reference), decode(target)) else {
            return false;
        };
        if request.params["ref"] == target.params["ref"] {
            return true;
        }
        self.catalog.stored_shelf(key).is_some_and(|shelf| {
            shelf.document["query_ref"].is_string()
                && shelf.document["query_ref"] == target.params["ref"]
        })
    }
    pub fn source_page_revalidate(&mut self, key: &str, manual: bool) {
        if manual {
            self.catalog.manual_page(key);
        } else {
            self.catalog.poll_page(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_references_survive_queries_without_becoming_section_paths() {
        let encoded = encode(
            "opaque:cursor/with?parameters",
            json!({"filters":{"hidden":true,"genres":["12"]},"order":"title"}),
            "collection",
        )
        .unwrap();
        assert!(!encoded.contains('|') && !encoded.contains(' '));
        let request = decode(&encoded).unwrap();
        assert_eq!(request.params["ref"], "opaque:cursor/with?parameters");
        assert_eq!(request.params["filters"]["hidden"], true);
        assert_eq!(request.layout, "collection");
        assert!(request.params.get("layout").is_none());
    }
    #[test]
    fn query_envelopes_cannot_override_method_refresh_or_resource_limits() {
        for query in [
            json!({"refresh":true}),
            json!({"method":"other"}),
            json!({"limit":101}),
            json!({"filters":{"nested":{"secret":"value"}}}),
        ] {
            assert!(encode("saved:all", query, "collection").is_err());
        }
        assert!(
            encode(
                "saved:all",
                json!({"filters":{"text":"x".repeat(MAX_BYTES)}}),
                "collection"
            )
            .is_err()
        );
    }

    #[test]
    fn article_routes_keep_the_reference_opaque_and_preserve_the_layout_hint() {
        let encoded = encode("article:opaque/identifier", json!({}), "article").unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.layout, "article");
        assert_eq!(decoded.params, json!({"ref":"article:opaque/identifier"}));
    }

    #[test]
    fn saved_web_routes_cannot_restore_an_embedded_page() {
        let saved = format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&json!({"ref":"journal:home","layout":"web"})).unwrap()
        ));
        assert!(decode(&saved).is_err());
        assert!(encode("journal:home", json!({}), "web").is_err());
    }
}
