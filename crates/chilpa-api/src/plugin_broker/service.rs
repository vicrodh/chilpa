//! Public reads from Chilpa's own service, made by Chilpa for a process
//! plugin. The service answers signed requests, and the key never enters a
//! plugin's memory, so a plugin that needs one of these reads asks the host
//! instead of going out itself.
use serde_json::{json, Map, Value};
use url::Url;

/// What a plugin may read: a playlist and its items, a batch of tracks, one
/// track and one album, under any provider prefix of the service. Sign-in
/// handshakes, scrobbling and the rest of the service stay with the host.
const READS: [&str; 4] = ["playlist", "tracks", "track", "album"];

/// More than this is not a read of one of the above.
const MAX_SEGMENTS: usize = 4;
const MAX_PAIRS: usize = 16;
const MAX_VALUE: usize = 2048;

/// The host name the plugin's manifest has to declare, as the user approved it.
pub(super) fn host() -> String {
    Url::parse(chilpa_meta::proxy::ORIGIN)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_default()
}

/// The address of a read, or why it is not one a plugin may ask for.
/// `query` is an object of text values, encoded here.
pub(super) fn address(path: &str, query: &Value) -> Result<Url, String> {
    let refused = || "The service does not offer this read to plugins.".to_string();
    let segments: Vec<&str> = path.strip_prefix('/').ok_or_else(refused)?.split('/').collect();
    let plain = |segment: &&str| {
        !segment.is_empty() && segment.len() <= 64 && segment.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    };
    let provider = segments[0];
    let fits = (2..=MAX_SEGMENTS).contains(&segments.len())
        && !provider.is_empty()
        && provider.len() <= 32
        && provider.bytes().all(|b| b.is_ascii_lowercase())
        && READS.contains(&segments[1])
        && segments[2..].iter().all(plain);
    if !fits {
        return Err(refused());
    }
    let pairs = match query {
        Value::Null => Map::new(),
        Value::Object(pairs) => pairs.clone(),
        _ => return Err("The query of a read is an object.".into()),
    };
    if pairs.len() > MAX_PAIRS {
        return Err("The read has too many query parameters.".into());
    }
    let mut url = Url::parse(chilpa_meta::proxy::ORIGIN).map_err(|e| e.to_string())?;
    url.set_path(path);
    if !pairs.is_empty() {
        let mut query = url.query_pairs_mut();
        for (name, value) in &pairs {
            let value = value.as_str().ok_or("Each query value of a read is text.")?;
            if name.is_empty() || name.len() > 64 || value.len() > MAX_VALUE {
                return Err("A query parameter of the read is too long.".into());
            }
            query.append_pair(name, value);
        }
    }
    Ok(url)
}

/// Makes the read. The service's status and body come back as they were,
/// since the plugin knows what the reply means; only not reaching the
/// service at all is an error.
pub(super) fn get(path: &str, query: &Value) -> Result<Value, String> {
    let url = address(path, query)?;
    let request = json!({"url": url.as_str(), "method": "GET"});
    let raw = chilpa_core::send_request(request.to_string().as_bytes(), None);
    let reply: Value = serde_json::from_slice(&raw).map_err(|_| "The service answered something unreadable.".to_string())?;
    if let Some(error) = reply.get("error") {
        return Err(error.as_str().unwrap_or("The service could not be reached.").to_string());
    }
    Ok(json!({"status": reply["status"], "body": reply["body"]}))
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0";

    #[test]
    fn a_read_goes_to_the_service_with_its_query_encoded() {
        let items = address(&format!("/source/playlist/{UUID}/items"), &json!({"countryCode": "US", "page[cursor]": "a+b/c="})).unwrap();
        assert_eq!(items.host_str(), Some(host().as_str()));
        assert_eq!(items.path(), format!("/source/playlist/{UUID}/items"));
        let pairs: Vec<(String, String)> = items.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
        assert_eq!(pairs, vec![("countryCode".into(), "US".into()), ("page[cursor]".into(), "a+b/c=".into())]);
        let batch = address("/source/tracks", &json!({"filter[id]": "1,2,3"})).unwrap();
        assert!(batch.as_str().ends_with("/source/tracks?filter%5Bid%5D=1%2C2%2C3"), "{batch}");
        assert_eq!(address("/source/track/77646168", &Value::Null).unwrap().query(), None, "no query, no question mark");
        assert!(address("/source/album/1", &json!({})).is_ok());
    }

    #[test]
    fn only_the_public_reads_are_offered() {
        for path in [
            "/source/account/session",
            "/source/oauth/token",
            "/source/track/../account",
            "/source/track/1?secret=1",
            "/source/track/1/a/b",
            "/source/track/",
            "/Source/track/1",
            "/source",
            "/",
            "",
            "source/track/1",
            "/source/track/1#x",
        ] {
            assert!(address(path, &Value::Null).is_err(), "{path}");
        }
    }

    #[test]
    fn a_query_is_a_few_short_text_values() {
        assert!(address("/source/track/1", &json!("countryCode=US")).is_err());
        assert!(address("/source/track/1", &json!({"countryCode": 1})).is_err());
        assert!(address("/source/track/1", &json!({"include": "x".repeat(MAX_VALUE + 1)})).is_err());
        let many: Map<String, Value> = (0..=MAX_PAIRS).map(|i| (format!("p{i}"), json!("v"))).collect();
        assert!(address("/source/track/1", &Value::Object(many)).is_err());
    }
}
