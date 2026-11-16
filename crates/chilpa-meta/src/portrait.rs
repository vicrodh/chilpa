//! Identity-linked, freely licensed portraits. No name-only image search.
use crate::{artist, text::{encode, is_uuid}, Failure, Net};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Portrait {
    pub mbid: String,
    pub entity: String,
    pub file: String,
    pub image: String,
    pub page: String,
    pub credit: String,
    pub license: String,
    pub license_url: String,
}

fn json(net: &Net, url: &str) -> Result<Value, Failure> {
    net.pace.wait();
    let reply = net.http.get(url, "application/json", Duration::from_secs(6), 1024 * 1024)
        .map_err(Failure::Unreachable)?;
    if reply.status != 200 { return Err(Failure::Unexpected(reply.status.to_string())); }
    serde_json::from_slice(&reply.body).map_err(|error| Failure::Unexpected(error.to_string()))
}

fn plain(text: &str) -> String {
    text.chars().filter(|letter| letter.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Several exact homonyms remain unresolved, even when the first scores 100.
pub fn unique(found: &Value, name: &str) -> Option<String> {
    if found["artists"][0]["score"].as_u64().unwrap_or(0) < 95 { return None; }
    let best = artist::confident(found, name)?;
    let wanted = plain(name);
    let candidates: std::collections::HashSet<_> = found["artists"].as_array()?.iter().filter(|row| {
        std::iter::once(row["name"].as_str().unwrap_or_default()).chain(
                row["aliases"].as_array().into_iter().flatten().filter_map(|alias| alias["name"].as_str()))
                .any(|name| plain(name) == wanted)
    }).filter_map(|row| row["id"].as_str().filter(|id| is_uuid(id))).collect();
    (candidates.len() == 1 && candidates.contains(best.as_str())).then_some(best)
}

fn entity(artist: &Value) -> Option<String> {
    let entities: std::collections::HashSet<_> = artist["relations"].as_array()?.iter()
        .filter(|relation| relation["type"] == "wikidata")
        .filter_map(|relation| relation["url"]["resource"].as_str())
        .filter_map(|url| url.strip_prefix("https://www.wikidata.org/wiki/").or_else(|| url.strip_prefix("http://www.wikidata.org/wiki/")))
        .filter(|id| id.starts_with('Q') && id.len() <= 20 && id.len() > 1 && id[1..].bytes().all(|byte| byte.is_ascii_digit())).collect();
    (entities.len() == 1).then(|| entities.into_iter().next().unwrap().to_string())
}

pub fn image_file(entity: &Value) -> Option<String> {
    let claims = entity["claims"]["P18"].as_array()?;
    let best = claims.iter().find(|claim| claim["rank"] == "preferred")
        .or_else(|| claims.iter().find(|claim| claim["rank"] == "normal"))?;
    best["mainsnak"]["datavalue"]["value"].as_str()
        .filter(|file| !file.is_empty() && file.len() <= 512).map(str::to_string)
}

/// Commons metadata contains HTML. Credits are rendered as plain text.
pub fn credit_text(html: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for letter in html.chars().take(8192) {
        match letter {
            '<' => { tag = true; out.push(' '); }
            '>' => tag = false,
            _ if !tag => out.push(letter),
            _ => {}
        }
    }
    let text = quick_xml::escape::unescape(&out).map_or(out.clone(), |text| text.into_owned());
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(1024).collect()
}

pub fn commons(info: &Value) -> Option<Portrait> {
    let info = info["query"]["pages"].as_array()?.first()?.get("imageinfo")?.as_array()?.first()?;
    let metadata = &info["extmetadata"];
    let field = |key: &str| credit_text(metadata[key]["value"].as_str().unwrap_or_default());
    let license = field("LicenseShortName");
    let license_url = field("LicenseUrl").replacen("http://creativecommons.org/", "https://creativecommons.org/", 1);
    let allowed = license == "Public domain" || license == "CC0" || license.starts_with("CC BY ") || license.starts_with("CC BY-SA ");
    if !allowed { return None; }
    let image = info["thumburl"].as_str().filter(|url| url.starts_with("https://upload.wikimedia.org/wikipedia/commons/") || url.starts_with("https://thumb.wikimedia.org/wikipedia/commons/"))?;
    let page = info["descriptionurl"].as_str().filter(|url| url.starts_with("https://commons.wikimedia.org/wiki/File:"))?;
    if image.len() > 2048 || page.len() > 2048 || license_url.len() > 512 { return None; }
    let credit = field("Artist");
    if license != "Public domain" && license != "CC0" && (credit.is_empty() || !license_url.starts_with("https://creativecommons.org/")) { return None; }
    Some(Portrait { image:image.into(), page:page.into(), credit, license, license_url, ..Default::default() })
}

pub fn commons_url(file: &str) -> String {
    format!("https://commons.wikimedia.org/w/api.php?action=query&format=json&formatversion=2&prop=imageinfo&titles={}&iiprop=url%7Cextmetadata&iiurlwidth=800&iiextmetadatafilter=Artist%7CLicenseShortName%7CLicenseUrl", encode(&format!("File:{file}")))
}

pub fn lookup(net: &Net, name: &str, id: Option<&str>) -> Result<Option<Portrait>, Failure> {
    if name.trim().is_empty() || name.len() > 512 { return Ok(None); }
    let id = match id.filter(|id| is_uuid(id)) {
        Some(id) => id.to_string(),
        None => {
            let Some(url) = artist::search_url(name, 5) else { return Ok(None); };
            let Some(id) = unique(&json(net, &url)?, name) else { return Ok(None); };
            id
        }
    };
    let url = format!("https://musicbrainz.org/ws/2/artist/{id}?inc=url-rels&fmt=json");
    let Some(entity) = entity(&json(net, &url)?) else { return Ok(None); };
    let data = json(&Net::free(net.http), &format!("https://www.wikidata.org/w/api.php?action=wbgetentities&format=json&props=claims&ids={entity}"))?;
    let Some(file) = image_file(&data["entities"][&entity]) else { return Ok(None); };
    let Some(mut portrait) = commons(&json(&Net::free(net.http), &commons_url(&file))?) else { return Ok(None); };
    portrait.mbid = id; portrait.entity = entity; portrait.file = file;
    Ok(Some(portrait))
}

#[cfg(test)]
mod tests;
