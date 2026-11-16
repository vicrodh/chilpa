//! Music deletion follows LibraryController.DeleteItem and the user-scoped
//! CanDelete field in DtoService (10.10.7); it removes the physical files.

use super::{headers, read};
use crate::{Credential, Failure, MediaKind, Net, Request, Server};
use crate::text::{encode, string};
use serde_json::Value;

pub(crate) fn delete(net: &dyn Net, server: &Server, key: &Credential, id: &str, kind: MediaKind) -> Result<(), Failure> {
    let denied = || Failure::Forbidden(chilpa_i18n::t("this user may not delete that music on the server"));
    // Without a user the server may describe system-wide permission instead.
    if id.is_empty() || server.details.user_id.is_empty() { return Err(denied()); }
    let request = Request::get(format!(
        "{}/Items?userId={}&Ids={}&Fields=CanDelete&EnableImages=false&EnableUserData=false",
        server.address, encode(&server.details.user_id), encode(id)), headers(&server.client, Some(key)));
    let reply = net.send(&request).map_err(Failure::Unreachable)?;
    match reply.status {
        401 | 403 => return Err(denied()),
        404 => return Err(Failure::Gone(chilpa_i18n::t("the server no longer has it"))),
        _ => {},
    }
    let found = read(&reply)?;
    let item = found.get("Items").and_then(Value::as_array).and_then(|items|
        items.iter().find(|item| string(item, "Id").is_some_and(|value| value.eq_ignore_ascii_case(id))))
        .ok_or_else(|| Failure::Unexpected(chilpa_i18n::t("the server did not describe the selected music item")))?;
    let expected = match kind { MediaKind::Track => "Audio", MediaKind::Album => "MusicAlbum" };
    if string(item, "Type").as_deref() != Some(expected) {
        return Err(Failure::Forbidden(chilpa_i18n::t("the selected item is not the music item that was confirmed")));
    }
    if item.get("CanDelete").and_then(Value::as_bool) != Some(true) { return Err(denied()); }
    let reply = net.send(&Request::delete(format!("{}/Items/{}", server.address, encode(id)), headers(&server.client, Some(key))))
        .map_err(Failure::Unreachable)?;
    match reply.status {
        401 | 403 => Err(denied()),
        // It disappeared after permission was checked: the intended state.
        404 => Ok(()),
        _ => read(&reply).map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Client, Kind};
    use crate::net::testing::Canned;

    const ITEM: &str = "d29818a3701ef1b2d7fb8c90e2c0f2b2";
    fn lab() -> (Server, Credential) {
        (crate::jellyfin::server("http://fixture.invalid", Client {
            version: "0.1.3".into(), platform: "Linux".into(), device: "Fixture".into(), id: "device".into(),
        }, vec![], "501512e000854c8d832d2f1d2a26a0a0"), Credential::Token { token: "fixture-token".into() })
    }
    fn answer(kind: &str, can_delete: Option<bool>) -> Vec<u8> {
        serde_json::json!({"Items":[{"Id":ITEM,"Type":kind,"CanDelete":can_delete}]}).to_string().into_bytes()
    }
    #[test]
    fn permission_and_type_are_checked_for_the_same_item_before_one_delete() {
        let (server, key) = lab();
        for (kind, expected) in [(MediaKind::Track, "Audio"), (MediaKind::Album, "MusicAlbum")] {
            let response = answer(expected, Some(true));
            let net = Canned::new().answer(200, &response).answer(204, b"");
            crate::media::delete(&net, &server, &key, ITEM, kind).unwrap();
            let asked = net.asked();
            assert_eq!(asked.len(), 2);
            assert!(asked[0].url.contains("userId=501512e000854c8d832d2f1d2a26a0a0"));
            assert!(asked[0].url.contains("Fields=CanDelete"));
            assert_eq!(asked[1].method, "DELETE");
            assert_eq!(asked[1].url, format!("http://fixture.invalid/Items/{ITEM}"));
            assert!(asked.iter().all(|request| request.headers.iter().any(|(name, _)| name == "Authorization")));
        }
    }
    #[test]
    fn absent_permission_wrong_types_and_other_protocols_send_no_delete() {
        let (mut server, key) = lab();
        for (kind, permission) in [("Audio", None), ("Audio", Some(false)), ("Folder", Some(true)), ("MusicAlbum", Some(true))] {
            let response = answer(kind, permission);
            let net = Canned::new().answer(200, &response);
            assert!(crate::media::delete(&net, &server, &key, ITEM, MediaKind::Track).is_err());
            assert_eq!(net.asked().len(), 1);
        }
        server.kind = Kind::Plex;
        let net = Canned::new();
        assert!(crate::media::delete(&net, &server, &key, ITEM, MediaKind::Track).is_err());
        assert!(net.asked().is_empty());
    }
    #[test]
    fn permission_revoked_before_delete_is_a_failure_and_never_retried() {
        let (server, key) = lab();
        let response = answer("Audio", Some(true));
        for status in [401, 403, 500] {
            let net = Canned::new().answer(200, &response).answer(status, b"");
            assert!(crate::media::delete(&net, &server, &key, ITEM, MediaKind::Track).is_err());
            assert_eq!(net.asked().len(), 2);
        }
    }
}
