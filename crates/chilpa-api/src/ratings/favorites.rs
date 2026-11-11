//! Hearts the listener changes here, for the server that holds the item,
//! when its kind keeps favorites and they are synced (`servers::favorites`).
//! A source plugin's hearts are its own and go through `source_favorites`.

use chilpa_library::store::favorite_sync::Hearted;
use crate::Chilpa;

impl Chilpa {
    /// Whether each of those tracks or records carries the heart now.
    pub(crate) fn hearts_before(&mut self, kind: &str, ids: &[i64]) -> Vec<bool> {
        let Some(lib) = self.library() else { return vec![false; ids.len()] };
        ids.iter().map(|id| hearted(kind, *id).is_some_and(|target| lib.is_favorite(target).unwrap_or(false))).collect()
    }

    /// The listener changed a heart here: it goes to the item's server when
    /// it keeps favorites. Only queued here.
    pub(crate) fn heart_changed(&mut self, kind: &str, id: i64, before: bool, on: bool) {
        if before == on || !crate::servers::favorites::on_for_all() {
            return;
        }
        let Some(target) = hearted(kind, id) else { return };
        let Some(lib) = self.library() else { return };
        let rated = match target {
            Hearted::Track(id) => chilpa_library::store::ratings::RatingTarget::Track(id),
            Hearted::Album(id) => chilpa_library::store::ratings::RatingTarget::Album(id),
        };
        let Some((source, local)) = lib.rated_on_server(rated).ok().flatten() else { return };
        let Some(stored) = lib.server(source).ok().flatten() else { return };
        if !chilpa_servers::Kind::from_provider(&stored.provider).is_some_and(chilpa_servers::Kind::keeps_favorites) {
            return;
        }
        let origin = super::sync::server_origin(&stored.instance);
        if !lib.favorite_sync_enabled(&origin).unwrap_or(false) {
            return;
        }
        let own = if kind == "track" {
            match chilpa_models::RemoteEntry::parse(&local) {
                Some(entry) => entry.external_id,
                None => return,
            }
        } else {
            local
        };
        let remote = lib.remote_favorite(&origin, kind, &own).ok().flatten();
        match lib.queue_favorite(&origin, kind, &own, on, remote) {
            Ok(true) => self.servers.ratings.due_now(),
            Ok(false) => {}
            Err(error) => eprintln!("ratings: a heart for {origin} could not be queued: {error}"),
        }
    }
}

fn hearted(kind: &str, id: i64) -> Option<Hearted> {
    match kind {
        "track" if id > 0 => Some(Hearted::Track(id)),
        "album" if id > 0 => Some(Hearted::Album(id)),
        _ => None,
    }
}
