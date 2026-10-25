use super::*;

fn release() -> CreditedRelease {
    // I retain only the relevant relationships from a public API response.
    release_of(&serde_json::from_str(include_str!("../tests/fixtures/mb-credits.json")).unwrap())
}

#[test]
fn release_credits_include_engineers_producers_and_every_work_role() {
    let release = release();
    let track = matching_track(&release, "La Bamba", "Ritchie Valens", 124_000).unwrap();
    let has = |name: &str, role| track.contributions.iter().any(|c| c.name == name && c.relation == role);
    assert!(has("Bob Keane", "producer"));
    assert!(has("Larry Levine", "recording"));
    assert!(has("Stan Ross", "recording"));
    assert!(has("Ritchie Valens", "arranger"));
    assert!(has("[traditional]", "composer"));
    let additional = track.contributions.iter().find(|c| c.name == "Ritchie Valens" && c.relation == "lyricist").unwrap();
    assert_eq!(additional.attributes, ["additional"]);
    assert_eq!(additional.scope, Entity::Work);
    assert!(release.contributions.iter().any(|c| c.name == "Steve Hoffman" && c.scope == Entity::Release));
    let roundtrip: CreditedRelease = serde_json::from_value(serde_json::to_value(&release).unwrap()).unwrap();
    assert_eq!(roundtrip.tracks[0].contributions, track.contributions);
}

#[test]
fn an_album_position_cannot_select_another_song_or_an_ambiguous_recording() {
    let mut release = release();
    assert!(matching_track(&release, "La Bamba", "Another Artist", 124_000).is_none());
    assert!(matching_track(&release, "La Bamba (alternate take)", "Ritchie Valens", 124_000).is_none());
    assert!(matching_track(&release, "La Bamba", "Ritchie Valens", 1).is_none());
    let last = release.tracks.iter_mut().rfind(|t| t.title == "La Bamba").unwrap();
    last.recording_id = "00000000-0000-4000-8000-000000000001".into();
    assert!(matching_track(&release, "La Bamba", "Ritchie Valens", 124_000).is_none());
}

#[test]
fn permanent_replacement_requires_explicit_recording_and_work_relationships() {
    let original: Value = serde_json::from_str(include_str!("../tests/fixtures/mb-credits.json")).unwrap();
    assert!(complete(&original));
    let mut missing = original.clone();
    missing["media"][0]["tracks"][0]["recording"].as_object_mut().unwrap().remove("relations");
    assert!(!complete(&missing));
    let mut missing = original.clone();
    let relations = missing["media"][0]["tracks"][0]["recording"]["relations"].as_array_mut().unwrap();
    relations.iter_mut().find(|relation| relation["type"] == "performance").unwrap()["work"].as_object_mut().unwrap().remove("relations");
    assert!(!complete(&missing));
    let mut changed_id = original;
    changed_id["media"][0]["tracks"][0]["recording"]["id"] = json!("invalid");
    assert!(!complete(&changed_id));
}
