//! I group people for the panel without losing their separate stored roles.

use chilpa_models::credits::{Contribution, Contributor, CreditRole, CreditScope};
use chilpa_meta::contributions::{Contribution as ExternalContribution, Entity};
use chilpa_i18n as i18n;
use serde_json::{json, Value};

fn label(role: CreditRole, relation: &str) -> &str {
    match role {
        CreditRole::Producer => i18n::mark("Producer"),
        CreditRole::RecordingEngineer => i18n::mark("Recording engineer"),
        CreditRole::Engineer => i18n::mark("Engineer"),
        CreditRole::MixEngineer => i18n::mark("Mixing engineer"),
        CreditRole::MasteringEngineer => i18n::mark("Mastering engineer"),
        CreditRole::Arranger => i18n::mark("Arranger"),
        CreditRole::Writer => i18n::mark("Writer"),
        CreditRole::Composer => i18n::mark("Composer"),
        CreditRole::Lyricist => i18n::mark("Lyricist"),
        CreditRole::Performer if relation == "instrument" => i18n::mark("Instrument"),
        CreditRole::Performer if relation == "vocal" => i18n::mark("Vocals"),
        CreditRole::Performer => i18n::mark("Performer"),
        CreditRole::Other => relation,
    }
}

pub(super) fn stored(credit: &ExternalContribution) -> Contribution {
    let role = match credit.relation.as_str() {
        "producer" | "executive producer" => CreditRole::Producer,
        "recording" => CreditRole::RecordingEngineer,
        "engineer" | "audio" => CreditRole::Engineer,
        "mix" | "mix-DJ" => CreditRole::MixEngineer,
        "mastering" => CreditRole::MasteringEngineer,
        "arranger" | "instrument arranger" | "vocal arranger" | "orchestrator" => CreditRole::Arranger,
        "writer" => CreditRole::Writer,
        "composer" => CreditRole::Composer,
        "lyricist" => CreditRole::Lyricist,
        "instrument" | "vocal" | "performer" | "conductor" => CreditRole::Performer,
        _ => CreditRole::Other,
    };
    Contribution { contributor: Contributor { id: credit.id.clone(), name: credit.name.clone() },
        credited_name: credit.credited_name.clone(), role, relation: credit.relation.clone(), attributes: credit.attributes.clone(),
        scope: match credit.scope { Entity::Recording => CreditScope::Recording, Entity::Work => CreditScope::Work, Entity::Release => CreditScope::Release },
        entity_id: credit.entity_id.clone(), entity_title: credit.entity_title.clone() }
}

pub(super) fn rows(credits: &[ExternalContribution]) -> Vec<Value> {
    let mut rows: Vec<Value> = Vec::new();
    for external in credits {
        let credit = stored(external);
        let role = json!({"label":label(credit.role, &credit.relation), "attributes":credit.attributes});
        if let Some(row) = rows.iter_mut().find(|row| row["id"] == credit.contributor.id) {
            let roles = row["role_labels"].as_array_mut().unwrap();
            if !roles.contains(&role) { roles.push(role); }
        } else {
            rows.push(json!({"id":credit.contributor.id, "name":credit.credited_name, "role_labels":[role],
                "source":"MusicBrainz", "link":format!("https://musicbrainz.org/artist/{}", credit.contributor.id)}));
        }
    }
    rows
}

pub(super) fn show(row: &mut Value) {
    let Some(roles) = row["role_labels"].as_array() else { return; };
    let roles: Vec<String> = roles.iter().map(|role| {
        let label = i18n::t(role["label"].as_str().unwrap_or_default());
        let attributes = role["attributes"].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(", ");
        if attributes.is_empty() { label } else { format!("{label} ({attributes})") }
    }).collect();
    row["roles"] = json!(roles.join(", "));
}
