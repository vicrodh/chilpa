use super::*;
use crate::{Client, Details};
use crate::net::testing::Canned;

fn lab() -> (Server, Credential) {
    let server = Server { kind:crate::Kind::Plex, address:"http://fixture.invalid".into(), libraries:vec![],
        client:Client { version:"0.1.3".into(), platform:"Linux".into(), device:"Fixture".into(), id:"device".into() },
        details:Details { machine:"fixture-machine".into(), ..Default::default() } };
    (server, Credential::Plex { account:"fixture-account".into(), server:"fixture-session".into() })
}
fn policy(allowed: Option<bool>, machine: &str) -> Vec<u8> {
    serde_json::json!({"MediaContainer":{"allowMediaDeletion":allowed,"machineIdentifier":machine}}).to_string().into_bytes()
}
fn item(kind: &str, id: &str) -> Vec<u8> {
    serde_json::json!({"MediaContainer":{"Metadata":[{"type":kind,"ratingKey":id}]}}).to_string().into_bytes()
}
#[test]
fn typed_music_with_current_permission_sends_exactly_one_delete() {
    let (server,key) = lab();
    for (kind,expected) in [(MediaKind::Track,"track"),(MediaKind::Album,"album")] {
        let net = Canned::new().answer(200,&policy(Some(true),"fixture-machine")).answer(200,&item(expected,"123")).answer(200,b"<html>Deleted</html>");
        crate::media::delete(&net,&server,&key,"123",kind).unwrap();
        let asked = net.asked();
        assert_eq!(asked.iter().map(|request| request.method).collect::<Vec<_>>(),["GET","GET","DELETE"]);
        assert_eq!(asked[2].url,"http://fixture.invalid/library/metadata/123");
        assert!(asked.iter().all(|request| request.headers.iter().any(|(name,_)| name == "X-Plex-Token")));
    }
}
#[test]
fn disabled_deletion_or_a_different_server_never_sends_delete() {
    let (server,key) = lab();
    for (allowed,machine) in [(None,"fixture-machine"),(Some(false),"fixture-machine"),(Some(true),"other-machine")] {
        let net = Canned::new().answer(200,&policy(allowed,machine));
        assert!(crate::media::delete(&net,&server,&key,"123",MediaKind::Track).is_err());
        assert_eq!(net.asked().len(),1);
    }
}
#[test]
fn wrong_items_and_permission_changes_do_not_delete_or_retry() {
    let (server,key) = lab();
    for (kind,id) in [("movie","123"),("album","123"),("track","456")] {
        let net = Canned::new().answer(200,&policy(Some(true),"fixture-machine")).answer(200,&item(kind,id));
        assert!(crate::media::delete(&net,&server,&key,"123",MediaKind::Track).is_err());
        assert_eq!(net.asked().len(),2);
    }
    for status in [401,403,500] {
        let net = Canned::new().answer(200,&policy(Some(true),"fixture-machine")).answer(200,&item("track","123")).answer(status,b"");
        assert!(crate::media::delete(&net,&server,&key,"123",MediaKind::Track).is_err());
        assert_eq!(net.asked().len(),3);
    }
}
