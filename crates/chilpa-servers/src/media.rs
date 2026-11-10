//! Explicit deletion of one music item on a server that supports it.

use crate::{Credential, Failure, Kind, Net, Server};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind { Track, Album }

impl Kind {
    pub fn deletes_media(self) -> bool { matches!(self, Self::Jellyfin | Self::Plex) }
}

pub fn valid_id(kind: Kind, id: &str) -> bool {
    match kind {
        Kind::Plex => !id.is_empty() && id.len() <= 20 && id.bytes().all(|byte| byte.is_ascii_digit()),
        Kind::Jellyfin => {
            let compact: String = id.chars().filter(|character| *character != '-').collect();
            compact.len() == 32 && compact.bytes().all(|byte| byte.is_ascii_hexdigit())
        }
        _ => false,
    }
}

/// The caller confirms the filesystem deletion first. The protocol then
/// rereads the item's type and the signed-in user's permission before DELETE.
/// This operation is never retried automatically.
pub fn delete(net: &dyn Net, server: &Server, key: &Credential, id: &str, kind: MediaKind) -> Result<(), Failure> {
    if !valid_id(server.kind, id) {
        return Err(Failure::Forbidden(chilpa_i18n::t("this server does not support deleting music from Chilpa")));
    }
    match server.kind {
        Kind::Plex => crate::plex::media::delete(net, server, key, id, kind),
        _ => crate::jellyfin::media::delete(net, server, key, id, kind),
    }
}
