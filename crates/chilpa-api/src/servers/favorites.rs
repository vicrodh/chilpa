//! The listener's hearts on the servers whose kind keeps favorites
//! (`chilpa_servers::favorite`): read after the catalogue, written one change
//! at a time, turned off per server from its row in Settings or for every
//! server from Settings › Library.
//!
//! A server lists its favorites whole, so what it does not list is not one
//! of them: an item it held before and no longer lists was taken off there.
//! A listing cut short by its budget counts only for what it listed.

use super::keys::Keys;
use super::net;
use super::sync::Event;
use crate::ratings::sync::server_origin;
use crate::ratings::writer::Delivery;
use crate::Chilpa;
use chilpa_library::store::favorite_sync::{FavoriteRead, PendingFavorite};
use chilpa_servers::rating::Rated;
use chilpa_servers::{Failure, Net};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

/// Settings › Library: hearts go both ways with every server that keeps
/// them. On unless turned off.
pub(crate) const SYNC: &str = "library.syncFavorites";

/// What a favorites listing may take after the catalogue.
const PAGES: usize = 200;
const TIME: Duration = Duration::from_secs(120);

/// Whether hearts go both ways with servers at all (`SYNC`).
pub(crate) fn on_for_all() -> bool {
    chilpa_core::settings::load().window.get(SYNC).is_none_or(|value| value != "false")
}

/// Takes in a server's favorites once its catalogue ended with `event`.
pub(crate) fn finish(lib: &chilpa_library::Library, net: &dyn Net, keys: &Keys, source: i64, event: &Event, stopped: &dyn Fn() -> bool, all: bool) {
    if !all || stopped() || matches!(event, Event::Failed { unreachable: true, .. }) {
        return;
    }
    let Some(stored) = lib.server(source).ok().flatten() else { return };
    if !chilpa_servers::Kind::from_provider(&stored.provider).is_some_and(chilpa_servers::Kind::keeps_favorites) {
        return;
    }
    let origin = server_origin(&stored.instance);
    if !lib.favorite_sync_enabled(&origin).unwrap_or(false) {
        return;
    }
    let (Some(server), Some(key)) = (net::server(&stored), net::key(keys, &stored)) else { return };
    let read_ms = crate::ratings::sync::now_ms();
    let listed = match chilpa_servers::favorite::favorites(net, &server, &key, PAGES, Instant::now() + TIME) {
        Ok(listed) => listed,
        Err(failure) => {
            eprintln!("chilpa-servers: favorites of source {source} could not be read: {failure}");
            return;
        }
    };
    let held = lib.remote_hearts(&origin).unwrap_or_default();
    let mut changed = 0;
    for (kind, ids) in [("track", &listed.tracks), ("album", &listed.albums)] {
        let gone: BTreeSet<&String> = if listed.whole {
            held.iter().filter(|(k, _)| k == kind).map(|(_, id)| id).filter(|id| !ids.contains(*id)).collect()
        } else {
            BTreeSet::new()
        };
        let local = |id: &str| if kind == "track" { net::entry(&stored, id) } else { id.to_string() };
        let reads: Vec<FavoriteRead> = ids.iter().map(|id| (id, true)).chain(gone.into_iter().map(|id| (id, false)))
            .map(|(id, on)| FavoriteRead { id: id.clone(), local: local(id), on, read_ms })
            .collect();
        match lib.apply_remote_favorites(&origin, source, kind, &reads, listed.whole) {
            Ok((count, _)) => changed += count,
            Err(error) => eprintln!("chilpa-servers: {kind} favorites of source {source} could not be kept: {error}"),
        }
    }
    if changed > 0 {
        eprintln!("chilpa-servers: sync of source {source}: {changed} favorites changed on the server");
    }
}

/// Sends one heart to the server of that instance.
pub(crate) fn deliver(lib: &chilpa_library::Library, network: &dyn Net, keys: &Keys, instance: &str, change: &PendingFavorite) -> Delivery {
    let Some(stored) = lib.servers().ok().and_then(|all| all.into_iter().find(|s| s.instance == instance)) else {
        return Delivery::Never(chilpa_i18n::t("that server is no longer in the library"));
    };
    if !stored.enabled {
        return Delivery::Later(chilpa_i18n::t("that server is disabled"));
    }
    let keeps = chilpa_servers::Kind::from_provider(&stored.provider).is_some_and(chilpa_servers::Kind::keeps_favorites);
    let Some(rated) = Rated::from_name(&change.kind).filter(|_| keeps) else {
        return Delivery::Never(chilpa_i18n::t("this server keeps no favorites Chilpa can change"));
    };
    let (Some(server), Some(key)) = (net::server(&stored), net::key(keys, &stored)) else {
        return Delivery::Later(chilpa_i18n::t_args("sign in to {} again", &[&stored.label]));
    };
    match chilpa_servers::favorite::favor(network, &server, &key, rated, &change.id, change.on) {
        Ok(()) => Delivery::Sent(if change.on { 1.0 } else { 0.0 }),
        Err(Failure::Gone(why)) => Delivery::Never(why),
        Err(failure) => Delivery::Later(failure.to_string()),
    }
}

const OFF: &str = "favorites-off";
const ON: &str = "favorites-on";

impl Chilpa {
    /// A server's row: its menu turns favorite sync on or off, and its
    /// description says when it is off or when hearts wait to reach it.
    pub(crate) fn server_favorite_row(&mut self, source: i64, description: &mut String, values: &mut Vec<String>, options: &mut Vec<String>) {
        let Some(lib) = self.library() else { return };
        let Some(stored) = lib.server(source).ok().flatten() else { return };
        if !chilpa_servers::Kind::from_provider(&stored.provider).is_some_and(chilpa_servers::Kind::keeps_favorites) {
            return;
        }
        let origin = server_origin(&stored.instance);
        let on = lib.favorite_sync_enabled(&origin).unwrap_or(true);
        let failures = lib.favorite_failures().unwrap_or_default();
        values.push(if on { OFF } else { ON }.into());
        options.push(if on { chilpa_i18n::t("Stop syncing favorites") } else { chilpa_i18n::t("Sync favorites") });
        if !on {
            description.push(' ');
            description.push_str(&chilpa_i18n::t("Favorites are not synced with it."));
        } else if let Some((_, count, why)) = failures.into_iter().find(|(o, _, _)| *o == origin) {
            let n = count.to_string();
            description.push(' ');
            description.push_str(&chilpa_i18n::tf("{} favorite waits to reach it: {}", "{} favorites wait to reach it: {}", count as i64, &[&n, &why]));
        }
    }

    /// Settings: a server's menu, or the switch in Settings › Library.
    pub(crate) fn server_favorites_setting(&mut self, key: &str, value: &str) -> bool {
        if key == SYNC {
            self.remember_preference(SYNC, if value == "false" { "false" } else { "true" });
            if value == "false" {
                if let Some(lib) = self.library() {
                    if let Err(error) = lib.forget_favorite_changes() {
                        eprintln!("ratings: waiting favorite changes could not be dropped: {error}");
                    }
                }
            } else {
                self.sync_enabled_servers();
            }
            return true;
        }
        let Some(source) = key.strip_prefix("sources.server/").and_then(|id| id.parse::<i64>().ok()) else { return false };
        if value != ON && value != OFF {
            return false;
        }
        let Some(stored) = self.library().and_then(|lib| lib.server(source).ok().flatten()) else { return false };
        let kept = self.library().is_some_and(|lib| lib.set_favorite_sync(&server_origin(&stored.instance), value == ON).is_ok());
        if !kept {
            self.now.error = chilpa_i18n::t("the library could not be written");
        } else if value == ON {
            self.sync_server(source);
        }
        true
    }

    /// The switch in Settings › Library, as its row shows it.
    pub(crate) fn favorites_sync_row(&self, row: &mut crate::SettingsRow) -> bool {
        if row.key != SYNC {
            return false;
        }
        row.value = on_for_all().to_string();
        true
    }

    /// Turned on again, every server that keeps favorites is read again.
    fn sync_enabled_servers(&mut self) {
        let due: Vec<i64> = self.library().and_then(|lib| lib.servers().ok()).unwrap_or_default().into_iter()
            .filter(|s| s.enabled && chilpa_servers::Kind::from_provider(&s.provider).is_some_and(chilpa_servers::Kind::keeps_favorites))
            .map(|s| s.source_id)
            .collect();
        for id in due {
            self.sync_server(id);
        }
    }
}

#[cfg(test)]
mod tests;
