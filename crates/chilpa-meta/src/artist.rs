//! An artist on MusicBrainz: when and where it began and ended, and who
//! it played with. Asked directly, like a release, under the same pace.

use crate::http::{Failure, Net};
use crate::musicbrainz::{get, API};
use crate::text::{encode, is_uuid, query_escape};
use serde::Serialize;
use serde_json::Value;

/// A search for an artist by name. `None` when there is no name.
pub fn search_url(name: &str, limit: usize) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let query = format!("artist:\"{}\"", query_escape(name));
    Some(format!("{API}/artist?query={}&fmt=json&limit={}", encode(&query), limit.clamp(1, 25)))
}

/// One artist, with its relations to other artists and its tags.
pub fn artist_url(id: &str) -> Result<String, Failure> {
    let id = id.trim().to_lowercase();
    if !is_uuid(&id) {
        return Err(Failure::BadId);
    }
    Ok(format!("{API}/artist/{id}?inc=artist-rels+tags&fmt=json"))
}

/// A date as MusicBrainz writes it: "1990", "1990-05" or "1990-05-14".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl Date {
    fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().split('-');
        let year = parts.next()?.parse().ok()?;
        let month = parts.next().and_then(|m| m.parse().ok()).unwrap_or(0);
        let day = parts.next().and_then(|d| d.parse().ok()).unwrap_or(0);
        Some(Self { year, month, day })
    }
}

/// Where and when an artist began and ended.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Origin {
    pub id: String,
    pub name: String,
    /// A person, as against a group, orchestra, choir or character.
    pub person: bool,
    pub begin: Option<Date>,
    pub end: Option<Date>,
    /// Where it began: the city or the country, whichever is known.
    pub place: String,
    pub country: String,
    /// The place names a city or something smaller, not only a country.
    pub exact: bool,
    pub tags: Vec<String>,
}

/// Another artist this one is tied to, with the role the tie names.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Relation {
    pub id: String,
    pub name: String,
    pub role: String,
    pub begin: String,
    pub end: String,
    pub ended: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Relationships {
    /// The members of a group, past and present.
    pub members: Vec<Relation>,
    /// The groups a person belongs or belonged to.
    pub groups: Vec<Relation>,
    pub collaborators: Vec<Relation>,
}

impl Relationships {
    pub fn is_empty(&self) -> bool {
        self.members.is_empty() && self.groups.is_empty() && self.collaborators.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Artist {
    pub origin: Origin,
    pub relationships: Relationships,
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().trim().to_string()
}

/// Letters and digits only, lower case: how two names are told the same.
fn plain(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The id of the artist a search found with confidence: the best row, when
/// MusicBrainz scores it 90 or more and its name is the one asked for, or
/// one of its aliases is. Anything less is nobody's page.
pub fn confident(found: &Value, name: &str) -> Option<String> {
    let wanted = plain(name);
    let row = found["artists"].as_array()?.first()?;
    if row["score"].as_u64().unwrap_or(0) < 90 {
        return None;
    }
    let mut names = std::iter::once(text(&row["name"]))
        .chain(row["aliases"].as_array().into_iter().flatten().map(|alias| text(&alias["name"])));
    let same = names.any(|candidate| plain(&candidate) == wanted);
    same.then(|| text(&row["id"])).filter(|id| is_uuid(id))
}

fn origin_of(value: &Value) -> Origin {
    let begin_area = &value["begin-area"];
    let area = &value["area"];
    let where_from = if begin_area.is_object() { begin_area } else { area };
    let place = text(&where_from["name"]);
    let country = value["country"].as_str().map(str::to_string)
        .or_else(|| area["iso-3166-1-codes"][0].as_str().map(str::to_string))
        .unwrap_or_default();
    // A country has a two-letter code as its own; a city or a region does not.
    let exact = !place.is_empty() && where_from["iso-3166-1-codes"].as_array().is_none_or(|codes| codes.is_empty());
    let mut tags: Vec<(u64, String)> = value["tags"].as_array().into_iter().flatten()
        .filter_map(|tag| Some((tag["count"].as_u64()?, text(&tag["name"]))))
        .filter(|(count, name)| *count > 0 && !name.is_empty())
        .collect();
    tags.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    Origin {
        id: text(&value["id"]),
        name: text(&value["name"]),
        person: text(&value["type"]).eq_ignore_ascii_case("person"),
        begin: Date::parse(&text(&value["life-span"]["begin"])),
        end: Date::parse(&text(&value["life-span"]["end"])),
        place,
        country,
        exact,
        tags: tags.into_iter().map(|(_, name)| name).take(8).collect(),
    }
}

fn relationships_of(value: &Value, person: bool) -> Relationships {
    let mut out = Relationships::default();
    let mut seen = std::collections::HashSet::new();
    for relation in value["relations"].as_array().into_iter().flatten() {
        let other = &relation["artist"];
        let id = text(&other["id"]);
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        let kind = text(&relation["type"]);
        let backward = relation["direction"].as_str() == Some("backward");
        let attributes: Vec<String> = relation["attributes"].as_array().into_iter().flatten().map(text).filter(|a| !a.is_empty()).collect();
        let row = |role: &str| Relation {
            id: id.clone(),
            name: text(&other["name"]),
            role: attributes.first().cloned().unwrap_or_else(|| role.to_string()),
            begin: text(&relation["begin"]),
            end: text(&relation["end"]),
            ended: relation["ended"].as_bool().unwrap_or(false),
        };
        match kind.as_str() {
            // "member of band" points from the member to the group: read
            // backward from a group it names the members, forward from a
            // person it names the groups.
            "member of band" | "founder" if backward && !person => out.members.push(row("Band Member")),
            "member of band" | "founder" if !backward && person => out.groups.push(row("Band")),
            "collaboration" | "supporting musician" | "instrumental supporting musician" | "vocal supporting musician" => out.collaborators.push(row("Collaborator")),
            _ => {}
        }
    }
    out
}

pub fn artist_of(value: &Value) -> Artist {
    let origin = origin_of(value);
    let relationships = relationships_of(value, origin.person);
    Artist { origin, relationships }
}

/// Finds an artist by name, with confidence or not at all.
pub fn find(net: &Net, name: &str) -> Result<Option<String>, Failure> {
    let Some(url) = search_url(name, 5) else {
        return Ok(None);
    };
    Ok(confident(&get(net, &url)?, name))
}

/// One artist, whole.
pub fn artist(net: &Net, id: &str) -> Result<Artist, Failure> {
    Ok(artist_of(&get(net, &artist_url(id)?)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cut down from what MusicBrainz answers for Iron Maiden: the keys
    /// read here, two members, one collaboration and the tags.
    const MAIDEN: &str = r#"{"id":"ca891d65-d9b0-4258-89f7-e6ba29d83767","name":"Iron Maiden","type":"Group",
      "country":"GB","life-span":{"begin":"1975-12-25","end":null,"ended":false},
      "area":{"name":"United Kingdom","iso-3166-1-codes":["GB"]},
      "begin-area":{"name":"Leyton"},
      "tags":[{"count":12,"name":"heavy metal"},{"count":3,"name":"nwobhm"},{"count":0,"name":"ignored"}],
      "relations":[
        {"type":"member of band","direction":"backward","begin":"1975","end":"","ended":false,"attributes":["bass"],"artist":{"id":"3f2f5a2a-1111-4c2a-8c2a-aaaaaaaaaaaa","name":"Steve Harris"}},
        {"type":"member of band","direction":"backward","begin":"1980","end":"1981","ended":true,"attributes":[],"artist":{"id":"3f2f5a2a-2222-4c2a-8c2a-bbbbbbbbbbbb","name":"Dennis Stratton"}},
        {"type":"collaboration","direction":"forward","begin":"","end":"","ended":false,"attributes":[],"artist":{"id":"3f2f5a2a-3333-4c2a-8c2a-cccccccccccc","name":"Charlotte and the Harlots"}},
        {"type":"member of band","direction":"backward","attributes":[],"artist":{"id":"3f2f5a2a-1111-4c2a-8c2a-aaaaaaaaaaaa","name":"Steve Harris"}}
      ]}"#;

    #[test]
    fn a_group_has_its_members_its_place_and_its_tags() {
        let artist = artist_of(&serde_json::from_str(MAIDEN).unwrap());
        assert!(!artist.origin.person);
        assert_eq!(artist.origin.begin, Some(Date { year: 1975, month: 12, day: 25 }));
        assert_eq!(artist.origin.end, None);
        assert_eq!(artist.origin.place, "Leyton");
        assert_eq!(artist.origin.country, "GB");
        assert!(artist.origin.exact, "a city, not only a country");
        assert_eq!(artist.origin.tags, ["heavy metal", "nwobhm"]);
        let members: Vec<_> = artist.relationships.members.iter().map(|m| (m.name.as_str(), m.role.as_str(), m.ended)).collect();
        assert_eq!(members, [("Steve Harris", "bass", false), ("Dennis Stratton", "Band Member", true)], "each member once, with the first role credited");
        assert!(artist.relationships.groups.is_empty(), "a group is in no group");
        assert_eq!(artist.relationships.collaborators[0].name, "Charlotte and the Harlots");
    }

    #[test]
    fn a_person_lists_the_groups_and_a_country_alone_is_not_exact() {
        let value = serde_json::json!({"id":"3f2f5a2a-1111-4c2a-8c2a-aaaaaaaaaaaa","name":"Steve Harris","type":"Person",
            "life-span":{"begin":"1956-03-12"},"area":{"name":"United Kingdom","iso-3166-1-codes":["GB"]},
            "relations":[{"type":"member of band","direction":"forward","attributes":["bass"],"artist":{"id":"ca891d65-d9b0-4258-89f7-e6ba29d83767","name":"Iron Maiden"}}]});
        let artist = artist_of(&value);
        assert!(artist.origin.person);
        assert_eq!(artist.origin.place, "United Kingdom");
        assert!(!artist.origin.exact);
        assert_eq!(artist.relationships.groups[0].name, "Iron Maiden");
        assert!(artist.relationships.members.is_empty());
    }

    #[test]
    fn a_search_is_trusted_only_with_a_high_score_and_the_same_name() {
        let found = serde_json::json!({"artists":[{"id":"ca891d65-d9b0-4258-89f7-e6ba29d83767","score":100,"name":"Iron Maiden",
            "aliases":[{"name":"Maiden"}]}]});
        assert_eq!(confident(&found, "iron maiden").as_deref(), Some("ca891d65-d9b0-4258-89f7-e6ba29d83767"));
        assert_eq!(confident(&found, "Maiden").as_deref(), Some("ca891d65-d9b0-4258-89f7-e6ba29d83767"), "an alias counts");
        assert_eq!(confident(&found, "Iron Maidens"), None, "another name does not");
        let weak = serde_json::json!({"artists":[{"id":"ca891d65-d9b0-4258-89f7-e6ba29d83767","score":70,"name":"Iron Maiden"}]});
        assert_eq!(confident(&weak, "Iron Maiden"), None, "nor a low score");
    }

    #[test]
    fn the_addresses_say_what_they_ask() {
        assert_eq!(search_url("  ", 5), None);
        let url = search_url("Iron Maiden", 5).unwrap();
        assert!(url.starts_with("https://musicbrainz.org/ws/2/artist?query="), "{url}");
        assert!(url.ends_with("&fmt=json&limit=5"), "{url}");
        assert!(artist_url("nope").is_err());
        assert_eq!(artist_url("CA891D65-D9B0-4258-89F7-E6BA29D83767").unwrap(),
            "https://musicbrainz.org/ws/2/artist/ca891d65-d9b0-4258-89f7-e6ba29d83767?inc=artist-rels+tags&fmt=json");
    }
}
