//! I keep performing, featured and writing credits apart, with a route per person.
//! A row owns this bounded metadata; no worker, timer or global cache is added.
use serde::Serialize;
use serde_json::{json, Value};

const MAX_PEOPLE: usize = 64;
const MAX_NAME_BYTES: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Person { pub name: String, pub target: String, pub key: String }
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrackCredits { pub artists: Vec<Person>, pub featured: Vec<Person>, pub composers: Vec<Person> }

fn person(name: &str, reference: &str, plugin: &str) -> Option<Person> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_NAME_BYTES { return None; }
    let key = if plugin.is_empty() { name.to_owned() }
        else if reference.starts_with("artist:") && reference.len() <= MAX_NAME_BYTES { format!("{plugin} native:{reference}") }
        else { format!("{plugin} matching-artist-v3:{name}") };
    Some(Person { name:name.into(), target:if plugin.is_empty() { "artist" } else { "source" }.into(), key })
}
fn add(people: &mut Vec<Person>, value: Option<Person>) {
    let Some(value) = value else { return };
    if let Some(known) = people.iter_mut().find(|p| p.key == value.key || (p.name.eq_ignore_ascii_case(&value.name) && (p.key.contains(" matching-artist-v3:") || value.key.contains(" matching-artist-v3:")))) {
        if known.key.contains(" matching-artist-v3:") && value.key.contains(" native:artist:") { *known = value; }
    } else if people.len() < MAX_PEOPLE { people.push(value); }
}
fn roles(value: &Value) -> Vec<String> {
    let value = value.get("roles").or_else(|| value.get("role")).unwrap_or(&Value::Null);
    let words: Vec<&str> = match value { Value::String(s) => s.split(',').take(MAX_PEOPLE).collect(), Value::Array(a) => a.iter().take(MAX_PEOPLE).filter_map(Value::as_str).collect(), _ => Vec::new() };
    words.into_iter().filter(|s| s.len() <= 128).map(|s| s.chars().filter(|c| c.is_ascii_alphanumeric()).flat_map(char::to_lowercase).collect()).collect()
}
fn entry(value: &Value, plugin: &str) -> Option<Person> {
    if let Some(name) = value.as_str() { return person(name, "", plugin); }
    let name = value["name"].as_str().or_else(|| value["title"].as_str())?;
    let reference = value["ref"].as_str().or_else(|| value["artist_ref"].as_str()).filter(|r| r.len() <= MAX_NAME_BYTES).map(str::to_owned)
        .or_else(|| value["id"].as_i64().map(|id| format!("artist:{id}"))).unwrap_or_default();
    person(name, &reference, plugin)
}
fn entries(value: &Value) -> impl Iterator<Item = &Value> { value.as_array().into_iter().flatten().take(MAX_PEOPLE) }
fn is_featured(roles: &[String]) -> bool { roles.iter().any(|r| matches!(r.as_str(), "featuredartist" | "featured" | "guestartist")) }
fn is_artist(roles: &[String]) -> bool { roles.iter().any(|r| matches!(r.as_str(), "mainartist" | "primaryartist" | "artist" | "performer")) }

impl TrackCredits {
    pub(crate) fn source(artist: &str, metadata: &Value, plugin: &str) -> Self {
        let mut credits = Self::default();
        for value in entries(&metadata["artists"]) {
            let roles = roles(value);
            if is_featured(&roles) { add(&mut credits.featured, entry(value, plugin)); }
            else if roles.is_empty() || is_artist(&roles) { add(&mut credits.artists, entry(value, plugin)); }
        }
        for value in entries(&metadata["featured_artists"]) { add(&mut credits.featured, entry(value, plugin)); }
        for value in entries(&metadata["credits"]).chain(entries(&metadata["performers"])) {
            let roles = roles(value);
            if is_featured(&roles) { add(&mut credits.featured, entry(value, plugin)); }
            else if is_artist(&roles) { add(&mut credits.artists, entry(value, plugin)); }
            if roles.iter().any(|r| r == "composer") { add(&mut credits.composers, entry(value, plugin)); }
        }
        // This textual performer contract states both names and roles; I never split a
        // plain artist name on punctuation or infer a featured role from the title.
        if let Some(text) = metadata["performers"].as_str() {
            for participant in text.lines().flat_map(|line| line.split(" - ")).take(MAX_PEOPLE) {
                let mut pieces = participant.split(", ");
                let name = pieces.next().unwrap_or_default();
                if name.len() > MAX_NAME_BYTES { continue; }
                let value = json!({"name":name,"roles":pieces.take(MAX_PEOPLE).filter(|role| role.len() <= 128).collect::<Vec<_>>()});
                let roles = roles(&value);
                if is_featured(&roles) { add(&mut credits.featured, entry(&value, plugin)); }
                else if is_artist(&roles) { add(&mut credits.artists, entry(&value, plugin)); }
                if roles.iter().any(|r| r == "composer") { add(&mut credits.composers, entry(&value, plugin)); }
            }
        }
        if credits.artists.is_empty() { add(&mut credits.artists, person(artist, metadata["artist_ref"].as_str().unwrap_or_default(), plugin)); }
        else if let Some(main) = credits.artists.iter_mut().find(|p| p.name.eq_ignore_ascii_case(artist.trim())) {
            if let Some(routed) = person(&main.name, metadata["artist_ref"].as_str().unwrap_or_default(), plugin) {
                if routed.key.contains(" native:artist:") { *main = routed; }
            }
        }
        for value in entries(&metadata["composers"]) { add(&mut credits.composers, entry(value, plugin)); }
        if credits.composers.is_empty() { add(&mut credits.composers, person(metadata["composer"].as_str().unwrap_or_default(), metadata["composer_ref"].as_str().unwrap_or_default(), plugin)); }
        if let Some(name) = metadata["composer"].as_str() {
            if let Some(routed) = person(name, metadata["composer_ref"].as_str().unwrap_or_default(), plugin) {
                if routed.key.contains(" native:artist:") { add(&mut credits.composers, Some(routed)); }
            }
        }
        let mut people = 0;
        let mut bytes = 0;
        for group in [&mut credits.artists, &mut credits.featured, &mut credits.composers] {
            group.retain(|person| {
                people += 1;
                bytes += person.name.len() + person.key.len();
                people <= MAX_PEOPLE && bytes <= 16 * 1024
            });
        }
        credits
    }
    pub fn document(&self) -> Value {
        let mut links = Vec::new();
        let mut section = |people: &[Person]| people.iter().map(|p| {
            let at = links.len(); links.push(p.clone());
            format!("<a style=\"text-decoration:none\" href=\"{at}\">{}</a>", escape(&p.name))
        }).collect::<Vec<_>>().join(", ");
        let artist_html = section(&self.artists);
        let featured_html = section(&self.featured);
        let composer_html = section(&self.composers);
        let names = |people: &[Person]| people.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
        let featured = if featured_html.is_empty() { String::new() } else { chilpa_i18n::t_args("feat. {}", &[&featured_html]) };
        let composer = if composer_html.is_empty() { String::new() } else { chilpa_i18n::t_args("Composer: {}", &[&composer_html]) };
        let html = if featured.is_empty() { artist_html.clone() } else { format!("{artist_html} · {featured}") };
        json!({"artist_html":artist_html,"featured_html":featured,"composer_html":composer,"html":html,
            "artist":names(&self.artists),"featured":names(&self.featured),"composer":names(&self.composers),"links":links})
    }
}
fn escape(text: &str) -> String { text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn performing_featured_and_writing_credits_keep_distinct_identity_routes() {
        let metadata = json!({"artist_ref":"artist:lead", "composer":"Writer", "performers":[
            {"name":"Lead", "roles":["MainArtist"], "ref":"artist:lead"},
            {"name":"Guest", "roles":["Featured Artist"], "ref":"artist:guest"},
            {"name":"Writer", "roles":["Composer", "Producer"], "ref":"artist:writer"}
        ]});
        let credits = TrackCredits::source("Lead", &metadata, "fixture");
        assert_eq!(credits.artists.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Lead"]);
        assert_eq!(credits.featured[0].name, "Guest");
        assert_eq!(credits.composers[0].name, "Writer");
        let document = credits.document();
        assert_eq!(document["links"][0]["key"], "fixture native:artist:lead");
        assert_eq!(document["links"][1]["key"], "fixture native:artist:guest");
        assert_eq!(document["links"][2]["key"], "fixture native:artist:writer");
        assert!(!document["html"].as_str().unwrap().contains("Writer"));
        assert!(document["featured_html"].as_str().unwrap().contains("Guest"));
    }

    #[test]
    fn textual_performer_roles_are_structured_without_splitting_plain_artist_names() {
        let credits = TrackCredits::source("Band & Friends", &json!({
            "performers":"Writer, Composer, Producer - Guest, FeaturedArtist - Band & Friends, MainArtist"
        }), "fixture");
        assert_eq!(credits.artists.len(), 1);
        assert_eq!(credits.artists[0].name, "Band & Friends");
        assert_eq!(credits.featured[0].key, "fixture matching-artist-v3:Guest");
        assert_eq!(credits.composers[0].name, "Writer");
        let plain = TrackCredits::source("Earth, Wind & Fire", &json!({"composer":"Writer"}), "fixture");
        assert_eq!(plain.artists.len(), 1);
        assert_eq!(plain.artists[0].name, "Earth, Wind & Fire");
    }

    #[test]
    fn row_credits_have_one_shared_people_and_text_budget() {
        let artists = (0..128).map(|n| json!({"name":format!("Artist {n} {}", "x".repeat(300)),"ref":format!("artist:{n}")})).collect::<Vec<_>>();
        let credits = TrackCredits::source("", &json!({"artists":artists,"featured_artists":["Guest"],"composers":["Writer"]}), "fixture");
        let people = credits.artists.iter().chain(&credits.featured).chain(&credits.composers).collect::<Vec<_>>();
        assert!(people.len() <= MAX_PEOPLE);
        assert!(people.iter().map(|p| p.name.len() + p.key.len()).sum::<usize>() <= 16 * 1024);
        assert!(credits.artists.len() > 1);
    }

    #[test]
    fn equal_names_with_distinct_source_ids_stay_clickable_and_markup_is_escaped() {
        let credits = TrackCredits::source("", &json!({"artists":[
            {"name":"Same & <Name>", "ref":"artist:1"}, {"name":"Same & <Name>", "ref":"artist:2"},
            {"name":"Same & <Name>", "ref":"artist:1"}
        ]}), "fixture");
        assert_eq!(credits.artists.len(), 2);
        let document = credits.document();
        assert!(document["artist_html"].as_str().unwrap().contains("Same &amp; &lt;Name&gt;"));
        assert_ne!(document["links"][0]["key"], document["links"][1]["key"]);
    }
}
