//! Who made a recording, as MusicBrainz credits it: who played, sang,
//! produced and engineered it, and who wrote the work it performs. One
//! read per recording, with its own relations and its works' inline.

use crate::http::{Failure, Net};
use crate::musicbrainz::{get, isrc_url, API};
use crate::text::is_uuid;
use serde_json::Value;

/// One person or group and what they did on the recording.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Role {
    pub name: String,
    pub roles: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordingCredits {
    pub id: String,
    /// Its page, for somebody to open.
    pub url: String,
    pub composers: Vec<String>,
    pub lyricists: Vec<String>,
    pub performers: Vec<Role>,
}

impl RecordingCredits {
    pub fn is_empty(&self) -> bool {
        self.composers.is_empty() && self.lyricists.is_empty() && self.performers.is_empty()
    }
}

/// The first recording an ISRC names, when MusicBrainz knows it.
pub fn recording_by_isrc(net: &Net, isrc: &str) -> Result<Option<String>, Failure> {
    let Some(url) = isrc_url(isrc) else { return Ok(None) };
    let answer = match get(net, &url) {
        Err(Failure::Unexpected(reason)) if reason == "not found" => return Ok(None),
        result => result?,
    };
    Ok(answer["recordings"].as_array().and_then(|found| found.first())
        .and_then(|recording| recording["id"].as_str()).filter(|id| is_uuid(id)).map(str::to_owned))
}

/// A recording's credits, read once.
pub fn recording(net: &Net, id: &str) -> Result<RecordingCredits, Failure> {
    if !is_uuid(id) { return Err(Failure::BadId); }
    let answer = get(net, &format!("{API}/recording/{id}?inc=artist-rels+work-rels+work-level-rels&fmt=json"))?;
    Ok(credits_of(id, &answer))
}

/// The name as credited on the relation, or the artist's own.
fn credited(relation: &Value) -> String {
    relation["target-credit"].as_str().filter(|name| !name.trim().is_empty())
        .or_else(|| relation["artist"]["name"].as_str()).unwrap_or_default().trim().to_string()
}

/// What a relation says someone did: the instrument or voice for a
/// performance, the relation's own name for the rest ("producer", "mix").
fn role_of(relation: &Value) -> String {
    let attributes: Vec<&str> = relation["attributes"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    match relation["type"].as_str().unwrap_or_default() {
        "instrument" | "vocal" if !attributes.is_empty() => attributes.join(", "),
        "vocal" => "vocals".into(),
        other => other.replace(" and ", ", "),
    }
}

fn push_unique(names: &mut Vec<String>, name: String) {
    if !name.is_empty() && !names.iter().any(|known| known.eq_ignore_ascii_case(&name)) { names.push(name); }
}

pub(crate) fn credits_of(id: &str, answer: &Value) -> RecordingCredits {
    let mut credits = RecordingCredits { id: id.into(), url: format!("https://musicbrainz.org/recording/{id}"), ..Default::default() };
    let mut writers = Vec::new();
    for relation in answer["relations"].as_array().into_iter().flatten() {
        if relation["target-type"] == "work" || relation["work"].is_object() {
            for written in relation["work"]["relations"].as_array().into_iter().flatten() {
                match written["type"].as_str() {
                    Some("composer") => push_unique(&mut credits.composers, credited(written)),
                    Some("writer") => push_unique(&mut writers, credited(written)),
                    Some("lyricist") => push_unique(&mut credits.lyricists, credited(written)),
                    _ => {}
                }
            }
            continue;
        }
        if !relation["artist"].is_object() { continue; }
        let (name, role) = (credited(relation), role_of(relation));
        if name.is_empty() || role.is_empty() { continue; }
        match credits.performers.iter_mut().find(|known| known.name.eq_ignore_ascii_case(&name)) {
            Some(known) => if !known.roles.contains(&role) { known.roles.push(role) },
            None => credits.performers.push(Role { name, roles: vec![role] }),
        }
    }
    // A work's writers stand in for its composers only when none is named.
    if credits.composers.is_empty() { credits.composers = writers; }
    credits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::testing::Canned;
    use serde_json::json;

    const RECORDING: &str = "00000000-0000-4000-8000-000000000001";

    #[test]
    fn a_recording_names_who_played_what_and_who_wrote_its_work() {
        let answer = json!({"relations":[
            {"target-type":"artist","type":"instrument","attributes":["guitar"],"artist":{"name":"Peter Buck"}},
            {"target-type":"artist","type":"instrument","attributes":["mandolin"],"artist":{"name":"Peter Buck"}},
            {"target-type":"artist","type":"vocal","attributes":["lead vocals"],"artist":{"name":"Michael Stipe"}},
            {"target-type":"artist","type":"vocal","attributes":[],"target-credit":"M. Mills","artist":{"name":"Mike Mills"}},
            {"target-type":"artist","type":"producer","artist":{"name":"Scott Litt"}},
            {"target-type":"work","type":"performance","work":{"relations":[
                {"type":"writer","artist":{"name":"Bill Berry"}},
                {"type":"lyricist","artist":{"name":"Michael Stipe"}}
            ]}}
        ]});
        let credits = credits_of(RECORDING, &answer);
        assert_eq!(credits.performers, [
            Role { name: "Peter Buck".into(), roles: vec!["guitar".into(), "mandolin".into()] },
            Role { name: "Michael Stipe".into(), roles: vec!["lead vocals".into()] },
            Role { name: "M. Mills".into(), roles: vec!["vocals".into()] },
            Role { name: "Scott Litt".into(), roles: vec!["producer".into()] },
        ]);
        assert_eq!(credits.composers, ["Bill Berry"], "writers stand in when the work names no composer");
        assert_eq!(credits.lyricists, ["Michael Stipe"]);
        assert_eq!(credits.url, format!("https://musicbrainz.org/recording/{RECORDING}"));
    }

    #[test]
    fn a_named_composer_keeps_the_writers_out() {
        let answer = json!({"relations":[{"target-type":"work","work":{"relations":[
            {"type":"writer","artist":{"name":"Writer"}},{"type":"composer","artist":{"name":"Composer"}}]}}]});
        assert_eq!(credits_of(RECORDING, &answer).composers, ["Composer"]);
    }

    #[test]
    fn one_read_per_recording_and_an_isrc_names_its_first() {
        let http = Canned::new()
            .json(&json!({"recordings":[{"id":RECORDING},{"id":"00000000-0000-4000-8000-000000000009"}]}).to_string())
            .json(&json!({"relations":[]}).to_string());
        let net = Net::free(&http);
        assert_eq!(recording_by_isrc(&net, "usabc1200001").unwrap().as_deref(), Some(RECORDING));
        assert!(recording(&net, RECORDING).unwrap().is_empty());
        assert_eq!(http.asked(), [
            format!("{API}/isrc/USABC1200001?inc=recordings&fmt=json"),
            format!("{API}/recording/{RECORDING}?inc=artist-rels+work-rels+work-level-rels&fmt=json"),
        ]);
        assert_eq!(recording(&net, "../x"), Err(Failure::BadId));
        assert_eq!(recording_by_isrc(&net, "not an isrc").unwrap(), None);
    }
}
