//! Explicit deletion of one music item on a server that supports it.

use crate::{Credential, Failure, Kind, Net, Server};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind { Track, Album }

impl Kind {
    pub fn deletes_media(self) -> bool { self == Self::Jellyfin }
}

/// The caller confirms the filesystem deletion first. The protocol then
/// rereads the item's type and the signed-in user's permission before DELETE.
/// This operation is never retried automatically.
pub fn delete(net: &dyn Net, server: &Server, key: &Credential, id: &str, kind: MediaKind) -> Result<(), Failure> {
    if !server.kind.deletes_media() {
        return Err(Failure::Forbidden(chilpa_i18n::t("this server does not support deleting music from Chilpa")));
    }
    crate::jellyfin::media::delete(net, server, key, id, kind)
}
