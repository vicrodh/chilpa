//! A typed music item is removed once, after checking the server's policy.

use super::{headers, read, server_token};
use crate::{Credential, Failure, MediaKind, Net, Request, Server};
use crate::text::{id as item_id, string};
use serde_json::Value;

pub(crate) fn delete(net: &dyn Net, server: &Server, key: &Credential, id: &str, kind: MediaKind) -> Result<(), Failure> {
    let denied = || Failure::Forbidden(chilpa_i18n::t("this user may not delete that music on the server"));
    if server_token(key).is_none() || server.details.machine.is_empty() { return Err(denied()); }
    let request_headers = headers(&server.client, server_token(key));
    let info = checked(net, Request::get(format!("{}/", server.address), request_headers.clone()))?;
    let info = info.get("MediaContainer").ok_or_else(denied)?;
    if info.get("allowMediaDeletion").and_then(Value::as_bool) != Some(true)
        || string(info,"machineIdentifier").as_deref() != Some(&server.details.machine) {
        return Err(denied());
    }
    let url = format!("{}/library/metadata/{id}", server.address);
    let found = checked(net, Request::get(url.clone(), request_headers.clone()))?;
    let items = found.get("MediaContainer").and_then(|container| container.get("Metadata")).and_then(Value::as_array);
    let item = items.filter(|items| items.len() == 1).and_then(|items| items.first()).ok_or_else(denied)?;
    let expected = match kind { MediaKind::Track => "track", MediaKind::Album => "album" };
    if item_id(item,"ratingKey").as_deref() != Some(id) || string(item,"type").as_deref() != Some(expected) {
        return Err(Failure::Forbidden(chilpa_i18n::t("the selected item is not the music item that was confirmed")));
    }
    let reply = net.send(&Request::delete(url, request_headers)).map_err(Failure::Unreachable)?;
    // An item that disappeared after verification is already in the intended state.
    match reply.status {
        200..=299 | 404 => Ok(()),
        401 | 403 => Err(denied()),
        _ => read(&reply).map(|_| ()),
    }
}

fn checked(net: &dyn Net, request: Request) -> Result<Value, Failure> {
    let reply = net.send(&request).map_err(Failure::Unreachable)?;
    match reply.status {
        401 | 403 => Err(Failure::Forbidden(chilpa_i18n::t("this user may not delete that music on the server"))),
        404 => Err(Failure::Gone(chilpa_i18n::t("the server no longer has it"))),
        _ => read(&reply),
    }
}

#[cfg(test)]
mod tests;
