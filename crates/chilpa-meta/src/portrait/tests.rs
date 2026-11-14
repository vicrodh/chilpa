use super::*;
use serde_json::json;

const FIRST: &str = "ca891d65-d9b0-4258-89f7-e6ba29d83767";
const SECOND: &str = "3d3207f9-c4c2-425c-a364-f359b4320805";

#[test]
fn ambiguous_artist_names_do_not_get_an_unrelated_portrait() {
    let found = json!({"artists":[{"id":FIRST,"name":"Homonym","score":100},{"id":SECOND,"name":"Homonym","score":99}]});
    assert!(unique(&found, "Homonym").is_none());
    assert!(unique(&json!({"artists":[{"id":FIRST,"name":"Homonym","score":100},{"id":SECOND,"name":"Homonym","score":94}]}), "Homonym").is_none());
    assert_eq!(unique(&json!({"artists":[{"id":FIRST,"name":"Known name","score":100}]}), "KNOWN NAME").as_deref(), Some(FIRST));
    assert!(unique(&json!({"artists":[{"id":FIRST,"name":"Known name","score":94}]}), "Known name").is_none());
}

#[test]
fn preferred_images_win_and_deprecated_claims_are_never_selected() {
    let data = json!({"claims":{"P18":[
        {"rank":"deprecated","mainsnak":{"datavalue":{"value":"Wrong.jpg"}}},
        {"rank":"normal","mainsnak":{"datavalue":{"value":"Old.jpg"}}},
        {"rank":"preferred","mainsnak":{"datavalue":{"value":"Portrait.jpg"}}}]}});
    assert_eq!(image_file(&data).as_deref(), Some("Portrait.jpg"));
    assert_eq!(image_file(&json!({"claims":{"P18":[{"rank":"deprecated"}]}})), None);
}

fn image(license: &str) -> Value {
    json!({"query":{"pages":[{"imageinfo":[{
        "thumburl":"https://upload.wikimedia.org/wikipedia/commons/thumb/a/ab/Portrait.jpg/800px-Portrait.jpg",
        "descriptionurl":"https://commons.wikimedia.org/wiki/File:Portrait.jpg",
        "extmetadata":{"Artist":{"value":"<a href='https://example.org'>A &amp; B</a>"},
            "LicenseShortName":{"value":license},"LicenseUrl":{"value":"https://creativecommons.org/licenses/by-sa/4.0/"}}
    }]}]}})
}

#[test]
fn only_free_commons_thumbnails_with_their_credits_are_used() {
    let portrait = commons(&image("CC BY-SA 4.0")).unwrap();
    assert_eq!(portrait.credit, "A & B");
    assert!(commons(&image("Fair use")).is_none());
    assert!(commons(&image("CC BY-NC 4.0")).is_none());
    let mut forged = image("CC BY-SA 4.0");
    forged["query"]["pages"][0]["imageinfo"][0]["thumburl"] = json!("https://upload.wikimedia.org.example/portrait.jpg");
    assert!(commons(&forged).is_none());
}

#[test]
fn identity_relations_are_exact_and_duplicate_ids_are_harmless() {
    let data = json!({"relations":[{"type":"wikidata","url":{"resource":"https://www.wikidata.org/wiki/Q42"}}]});
    assert_eq!(entity(&data).as_deref(), Some("Q42"));
    assert!(entity(&json!({"relations":[{"type":"wikidata","url":{"resource":"https://www.wikidata.org.example/wiki/Q42"}}]})).is_none());
    assert!(entity(&json!({"relations":[{"type":"wikidata","url":{"resource":"https://www.wikidata.org/wiki/Q42/other"}}]})).is_none());
}

#[test]
fn a_lookup_follows_identity_relations_and_returns_attribution() {
    let http = crate::http::testing::Canned::new()
        .json(&json!({"artists":[{"id":FIRST,"name":"Known name","score":100}]}).to_string())
        .json(r#"{"relations":[{"type":"wikidata","url":{"resource":"https://www.wikidata.org/wiki/Q42"}}]}"#)
        .json(r#"{"entities":{"Q42":{"claims":{"P18":[{"rank":"normal","mainsnak":{"datavalue":{"value":"Portrait.jpg"}}}]}}}}"#)
        .json(&image("CC BY-SA 4.0").to_string());
    let portrait = lookup(&Net::free(&http), "Known name", None).unwrap().unwrap();
    assert_eq!((portrait.mbid.as_str(),portrait.entity.as_str(),portrait.file.as_str()), (FIRST,"Q42","Portrait.jpg"));
    assert_eq!(http.asked().len(), 4);
    assert!(http.asked()[1].contains("inc=url-rels"));
    assert!(http.asked()[3].contains("File%3APortrait.jpg"));
}
