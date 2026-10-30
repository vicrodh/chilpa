//! My servers: OpenSubsonic, Jellyfin and Plex, as sources of the library
//! (ADR-017).
//!
//! The protocols are `chilpa-servers`; the mirror, the availability and the
//! purge are `chilpa-library`. What is here is what ties them to the
//! window: adding a server, syncing it, knowing whether it answers, playing
//! its tracks and removing it. Everything that goes to the network runs on
//! a thread; `poll`, which the window calls four times a second, picks up
//! what came back.

pub(crate) mod keys;
mod net;
mod opener;
mod setup;
mod sync;
mod watch;

use crate::Chilpa;
use chilpa_models::RemoteEntry;
use keys::Keys;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub use chilpa_library::Removal as ServerRemoval;
pub use setup::ServerSetup;

/// A server as Settings shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerRow {
    /// Its source's id.
    pub id: i64,
    /// `subsonic`, `jellyfin`, `plex`.
    pub provider: String,
    pub name: String,
    /// What it says it is: "Navidrome 0.64.2".
    pub product: String,
    pub address: String,
    pub icon: String,
    pub reachable: bool,
    pub enabled: bool,
    pub syncing: bool,
    pub tracks: usize,
    pub albums: usize,
    /// Records and tracks edited here that the server no longer lists.
    /// Their edits are kept, in case it lists them again.
    pub orphans: usize,
    /// The line under its name: syncing, when it was, why not.
    pub state: String,
    /// Reached over plain http from outside this machine and the LAN it is
    /// on: the window says so.
    pub unencrypted: bool,
}

#[derive(Default)]
pub(crate) struct Servers {
    keys: Option<Arc<Keys>>,
    setup: setup::Setup,
    syncer: Option<sync::Syncer>,
    watcher: Option<watch::Watcher>,
    failures: Arc<Mutex<Vec<i64>>>,
    /// Why the last sync of each server did not finish.
    errors: HashMap<i64, String>,
    /// The servers that refused their credential at the last look.
    refused: std::collections::HashSet<i64>,
    /// The library changed underneath the window.
    changed: bool,
}

/// A sync is due when the last one finished more than this long ago.
const STALE: i64 = 60 * 60;

impl Chilpa {
    /// Starts what the servers need: their credentials, the player's opener,
    /// the sync, the watcher. The window calls it once, at startup, like
    /// `start_scrobbling`; a test never does.
    pub fn start_servers(&mut self) {
        if self.servers.keys.is_some() {
            return;
        }
        let keys = Arc::new(Keys::new(chilpa_dirs::data().join("servers")));
        self.sources.servers_opener = Some(Arc::new(opener::Opener::new(Arc::clone(&keys), Arc::clone(&self.servers.failures))));
        self.install_openers();
        self.servers.syncer = Some(sync::Syncer::start(Arc::clone(&keys)));
        self.servers.watcher = Some(watch::Watcher::start(Arc::clone(&keys)));
        self.servers.keys = Some(keys);
        // A queue restored before the opener existed could not name a
        // server's track; now it can.
        if on_a_server(&self.now.path) {
            self.now.title = crate::title_of(std::path::Path::new(&self.now.path));
        }
        let now = crate::now_unix();
        let stored = self.library().and_then(|lib| lib.servers().ok()).unwrap_or_default();
        // A server added before its kind had a glyph of its own takes it now.
        if let Some(lib) = self.library() {
            for s in &stored {
                if let Some(kind) = chilpa_servers::Kind::from_provider(&s.provider) {
                    let _ = lib.set_source_icon(s.source_id, kind.icon(&s.product));
                }
            }
        }
        let due: Vec<i64> = stored
            .into_iter()
            .filter(|s| s.enabled && s.synced_at.is_none_or(|at| now - at > STALE))
            .map(|s| s.source_id)
            .collect();
        for id in due {
            self.sync_server(id);
        }
    }

    /// The servers, in the order they were added.
    pub fn servers(&mut self) -> Vec<ServerRow> {
        let running = self.servers.syncer.as_ref().map(|s| s.running.clone()).unwrap_or_default();
        let errors = self.servers.errors.clone();
        let refused = self.servers.refused.clone();
        let Some(lib) = self.library() else { return Vec::new() };
        lib.servers()
            .unwrap_or_default()
            .into_iter()
            .map(|s| {
                let kind = chilpa_servers::Kind::from_provider(&s.provider);
                let state = if !s.enabled {
                    chilpa_i18n::t("Disabled. Its records stay in the library and its sign-in is saved.")
                } else if let Some(done) = running.get(&s.source_id) {
                    if *done == 0 {
                        chilpa_i18n::t("Syncing…")
                    } else {
                        let n = done.to_string();
                        chilpa_i18n::tf("Syncing: {} track read", "Syncing: {} tracks read", *done as i64, &[&n])
                    }
                } else if refused.contains(&s.source_id) {
                    chilpa_i18n::t("Refused the sign-in. Its records stay listed and cannot be played.")
                } else if !s.reachable {
                    chilpa_i18n::t("Does not answer. Its records stay listed and cannot be played.")
                } else if let Some(why) = errors.get(&s.source_id) {
                    why.clone()
                } else {
                    let (tracks, albums) = (s.tracks.to_string(), s.albums.to_string());
                    chilpa_i18n::t_args("{} tracks in {} records", &[&tracks, &albums])
                };
                ServerRow {
                    id: s.source_id,
                    provider: s.provider.clone(),
                    name: if s.label.is_empty() { kind.map(|k| k.name().to_string()).unwrap_or_default() } else { s.label.clone() },
                    product: s.product.clone(),
                    unencrypted: plain_and_outside(&s.address),
                    address: s.address.clone(),
                    icon: kind.map(|k| k.icon(&s.product)).unwrap_or("server").to_string(),
                    reachable: s.reachable,
                    enabled: s.enabled,
                    syncing: running.contains_key(&s.source_id),
                    tracks: s.tracks,
                    albums: s.albums,
                    orphans: lib.orphan_overlays(s.source_id).unwrap_or(0),
                    state,
                }
            })
            .collect()
    }

    /// The servers as Settings › Music servers lists them, and the row that adds
    /// one.
    pub(crate) fn server_rows(&mut self, notes: &crate::settings_rows::RowNotes) -> Vec<crate::SettingsRow> {
        let mut rows = Vec::new();
        for server in self.servers() {
            let key = format!("sources.server/{}", server.id);
            let mut description = format!("{} · {}. {}", server.product, server.address, server.state);
            if server.unencrypted {
                description.push(' ');
                description.push_str(&chilpa_i18n::t("The connection is not encrypted."));
            }
            if server.orphans > 0 {
                let n = server.orphans.to_string();
                description.push(' ');
                description.push_str(&chilpa_i18n::tf(
                    "{} record or track edited here is one the server no longer lists; its edits are kept.",
                    "{} records or tracks edited here are ones the server no longer lists; their edits are kept.",
                    server.orphans as i64,
                    &[&n],
                ));
            }
            if let Some((said_key, said)) = notes.said {
                if said_key == &key {
                    description = said.clone();
                }
            }
            let action = if !server.enabled {
                chilpa_i18n::t("Disabled")
            } else if server.syncing {
                chilpa_i18n::t("Syncing")
            } else if self.servers.refused.contains(&server.id) {
                chilpa_i18n::t("Signed out")
            } else if !server.reachable {
                chilpa_i18n::t("Not answering")
            } else {
                chilpa_i18n::t("Connected")
            };
            rows.push(crate::SettingsRow {
                kind: 1,
                key,
                title: server.name,
                description,
                control: "status".into(),
                value: serde_json::json!({
                    "icon": server.icon, "product": server.product, "tracks": server.tracks,
                    "enabled": server.enabled, "syncing": server.syncing,
                }).to_string(),
                action,
                values: if server.enabled { vec!["edit".into(), "disable".into(), "sync".into(), "remove".into()] }
                    else { vec!["edit".into(), "enable".into(), "remove".into()] },
                options: if server.enabled { vec![chilpa_i18n::t("Edit…"), chilpa_i18n::t("Disable"), chilpa_i18n::t("Sync now"), chilpa_i18n::t("Remove…")] }
                    else { vec![chilpa_i18n::t("Edit…"), chilpa_i18n::t("Enable"), chilpa_i18n::t("Remove…")] },
                ..Default::default()
            });
        }
        rows.push(crate::SettingsRow {
            kind: 1,
            key: "sources.addServer".into(),
            title: chilpa_i18n::t("Add a new server"),
            description: chilpa_i18n::t("OpenSubsonic (Navidrome, gonic…), Jellyfin or Plex. Its music joins the library."),
            control: "button".into(),
            action: chilpa_i18n::t("Add…"),
            ..Default::default()
        });
        rows
    }

    /// Where adding a server stands.
    pub fn server_setup(&self) -> &ServerSetup {
        &self.servers.setup.view
    }

    /// The same, as JSON for the window's dialog.
    pub fn server_setup_json(&self) -> String {
        let v = &self.servers.setup.view;
        serde_json::json!({
            "step": v.step, "kind": v.kind, "address": v.address, "product": v.product,
            "label": v.label, "editing": v.editing, "selectedLibraries": v.selected_libraries,
            "apiKey": v.api_key, "quickConnect": v.quick_connect, "code": v.code, "link": v.link,
            "choices": v.choices.iter().map(|(id, name, detail)| serde_json::json!({"id": id, "name": name, "detail": detail})).collect::<Vec<_>>(),
            "busy": v.busy, "error": v.error,
        })
        .to_string()
    }

    pub fn setup_server(&mut self, provider: &str) {
        self.servers.setup.begin(provider);
    }

    pub fn setup_edit_server(&mut self, id: i64) {
        self.servers.setup.cancel();
        let Some(stored) = self.library().and_then(|lib| lib.server(id).ok().flatten()) else {
            self.servers.setup.failed(chilpa_i18n::t("This server is no longer available."));
            return;
        };
        let Some(key) = self.servers.keys.as_ref().and_then(|keys| keys.get(&stored.instance)) else {
            self.servers.setup.failed(chilpa_i18n::t("The saved sign-in could not be opened."));
            return;
        };
        self.servers.setup.begin_edit(&stored, key);
    }

    pub fn setup_reauthenticate(&mut self) { self.servers.setup.reauthenticate(); }

    pub fn setup_label(&mut self, label: &str) { self.servers.setup.label(label); }

    pub fn setup_address(&mut self, address: &str) {
        self.servers.setup.address(address);
    }

    pub fn setup_sign_in(&mut self, user: &str, secret: &str, as_key: bool) {
        self.servers.setup.sign_in(user, secret, as_key);
    }

    pub fn setup_quick_connect(&mut self) {
        self.servers.setup.quick_connect();
    }

    pub fn setup_pick_server(&mut self, identity: &str) {
        self.servers.setup.pick_server(identity);
    }

    /// Plex, without the PIN: for the lab tests only.
    #[doc(hidden)]
    pub fn setup_plex_account(&mut self, token: &str) {
        self.servers.setup.plex_account(token);
    }

    pub fn setup_cancel(&mut self) {
        self.servers.setup.cancel();
    }

    /// The libraries chosen: the server is added and its first sync starts.
    pub fn setup_finish(&mut self, libraries: &[String]) -> bool {
        let Some(ready) = self.servers.setup.finish(libraries) else { return false };
        let Some(keys) = self.servers.keys.clone() else {
            self.servers.setup.failed(chilpa_i18n::t("servers are not started"));
            return false;
        };
        let edit_id = self.servers.setup.edit_id;
        let Some(lib) = self.library() else { return false };
        if lib.server_with_identity(ready.kind.provider(), &ready.identity).ok().flatten().is_some_and(|id| Some(id) != edit_id) {
            self.servers.setup.failed(chilpa_i18n::t("that server is already in the library"));
            return false;
        }
        let new = chilpa_library::NewServer {
            provider: ready.kind.provider().into(), instance: ready.instance.clone(), label: ready.label.clone(),
            icon: ready.kind.icon(&ready.product).into(), address: ready.address.clone(), account: ready.account.clone(),
            identity: ready.identity.clone(), product: ready.product.clone(),
            libraries: serde_json::to_string(&ready.libraries).unwrap_or_else(|_| "[]".into()),
            details: serde_json::to_string(&ready.details).unwrap_or_else(|_| "{}".into()),
        };
        if let Some(id) = edit_id {
            let old_key = keys.get(&ready.instance);
            if !keys.put(&ready.instance, &ready.key) {
                self.servers.setup.failed(chilpa_i18n::t("the credential could not be kept safely on this system"));
                return false;
            }
            if lib.edit_server(id, &new).is_err() {
                if let Some(key) = old_key { keys.put(&ready.instance, &key); }
                self.servers.setup.failed(chilpa_i18n::t("the library could not be written"));
                return false;
            }
            if let Some(syncer) = self.servers.syncer.as_mut() { syncer.cancel(id); }
            if let Some(watcher) = &self.servers.watcher { watcher.ask(); }
            self.servers.errors.remove(&id);
            self.servers.refused.remove(&id);
            self.servers.changed = true;
            self.servers.setup.done(&ready.label);
            self.sync_server(id);
            return true;
        }
        // The credential first: a server whose key could not be kept would
        // not sign in again after a restart.
        if !keys.put(&ready.instance, &ready.key) {
            self.servers.setup.failed(chilpa_i18n::t("the credential could not be kept safely on this system"));
            return false;
        }
        let added = lib.add_server(&new);
        match added {
            Ok(id) => {
                self.servers.setup.done(&ready.label);
                self.sync_server(id);
                true
            }
            Err(_) => {
                keys.forget(&ready.instance);
                self.servers.setup.failed(chilpa_i18n::t("the library could not be written"));
                false
            }
        }
    }

    /// Reads the server's catalogue again.
    pub fn sync_server(&mut self, id: i64) {
        if !self.library().and_then(|lib| lib.server(id).ok().flatten()).is_some_and(|s| s.enabled) {
            return;
        }
        self.servers.errors.remove(&id);
        if let Some(syncer) = self.servers.syncer.as_mut() {
            syncer.ask(id);
        }
    }

    /// Pauses or resumes a server while retaining its catalogue and sign-in.
    pub fn set_server_enabled(&mut self, id: i64, enabled: bool) -> bool {
        let current = RemoteEntry::parse(&self.now.path);
        let Some(server) = self.library().and_then(|lib| lib.server(id).ok().flatten()) else { return false };
        let playing_here = !enabled && current.is_some_and(|entry| entry.source.provider == server.provider && entry.source.instance == server.instance);
        let changed = self.library().and_then(|lib| lib.set_server_enabled(id, enabled).ok()).unwrap_or(false);
        if !changed { return false; }
        if enabled {
            if let Some(watcher) = &self.servers.watcher { watcher.ask(); }
        } else if let Some(syncer) = self.servers.syncer.as_mut() {
            syncer.cancel(id);
        }
        if playing_here { self.stop(); }
        self.servers.errors.remove(&id);
        self.servers.refused.remove(&id);
        self.servers.changed = true;
        true
    }

    /// What removing a server takes out of the library, as the confirmation
    /// says it.
    pub fn server_removal(&mut self, id: i64) -> String {
        let Some(lib) = self.library() else { return String::new() };
        let (Ok(Some(server)), Ok(r)) = (lib.server(id), lib.removal(id)) else { return String::new() };
        // Each count is its own call, so the extractor finds its words.
        let (tracks, albums, ratings, plays) = (r.tracks.to_string(), r.albums.to_string(), r.ratings.to_string(), r.plays.to_string());
        let mut parts = vec![
            chilpa_i18n::tf("{} track", "{} tracks", r.tracks as i64, &[&tracks]),
            chilpa_i18n::tf("{} record", "{} records", r.albums as i64, &[&albums]),
        ];
        if r.ratings > 0 {
            parts.push(chilpa_i18n::tf("{} rating", "{} ratings", r.ratings as i64, &[&ratings]));
        }
        if r.plays > 0 {
            parts.push(chilpa_i18n::tf("{} play", "{} plays", r.plays as i64, &[&plays]));
        }
        let mut text = chilpa_i18n::t_args("This removes {} from your library.", &[&parts.join(", ")]);
        if r.playlist_entries > 0 {
            let (n, lists) = (r.playlist_entries.to_string(), r.playlists_touched.to_string());
            text.push(' ');
            text.push_str(&chilpa_i18n::t_args("{} of its tracks leave {} of your playlists.", &[&n, &lists]));
        }
        text.push(' ');
        text.push_str(&chilpa_i18n::t_args("{} and its files are not touched.", &[&server.label]));
        text
    }

    /// Takes the server and everything that came from it out of the
    /// library, with its credential and its covers (ADR-017 §6).
    ///
    /// The credential goes first: if it cannot be deleted nothing else is
    /// touched, and the server stays in Settings to try again. The covers
    /// are a cache; if they cannot be deleted the removal still stands and
    /// the window says where they are.
    pub fn remove_server(&mut self, id: i64) -> bool {
        if let Some(syncer) = self.servers.syncer.as_mut() {
            syncer.cancel(id);
        }
        let Some(stored) = self.library().and_then(|lib| lib.server(id).ok().flatten()) else { return false };
        if self.servers.keys.as_ref().is_some_and(|keys| !keys.forget(&stored.instance)) {
            self.now.error = chilpa_i18n::t_args("the credential of {} could not be deleted; nothing was removed", &[&stored.label]);
            return false;
        }
        let prefix = RemoteEntry::new(chilpa_models::SourceId::new(stored.provider.clone(), stored.instance.clone()), "x").to_entry();
        let prefix = prefix.trim_end_matches('x').to_string();
        self.purge_queue(|path| path.starts_with(&prefix));
        let Some(lib) = self.library() else { return false };
        let chosen = lib.overlay_covers_dir(id);
        if lib.remove_source(id).is_err() {
            self.now.error = chilpa_i18n::t("the library could not be written");
            return false;
        }
        // Its own covers, and the ones chosen here for its records: the
        // overlays those went with left the library with the source.
        for covers in std::iter::once(sync::covers_dir(&stored.instance)).chain(chosen) {
            if let Err(e) = std::fs::remove_dir_all(&covers) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    self.now.error = chilpa_i18n::t_args("the covers of {} could not be deleted from {}", &[&stored.label, &covers.display().to_string()]);
                }
            }
        }
        self.servers.errors.remove(&id);
        self.servers.refused.remove(&id);
        self.servers.changed = true;
        self.persist_queue_session();
        true
    }

    /// Whether that record can be opened now: some track of it plays
    /// (ADR-017). The grid dims the ones that cannot.
    pub fn album_available(&mut self, id: i64) -> bool {
        if id < 0 { return self.catalog.card(id).is_none_or(|card| card.metadata["available"] != false); }
        self.library().and_then(|lib| lib.unavailable_albums(&[id]).ok()).is_none_or(|out| out.is_empty())
    }

    /// Says why a record cannot be opened, where the window shows problems.
    pub fn explain_unavailable_album(&mut self, id: i64) {
        self.now.error = self.album_unavailable_reason(id);
    }

    /// Why a record cannot be opened, as a click on it says.
    pub fn album_unavailable_reason(&mut self, id: i64) -> String {
        let disabled = self.library().and_then(|lib| lib.disabled_servers_of(id).ok()).unwrap_or_default();
        if !disabled.is_empty() {
            return chilpa_i18n::t_args("Enable {} in Settings › Music servers to play this record.", &[&disabled.join(", ")]);
        }
        let places = self.library().and_then(|lib| lib.unreachable_sources_of(id).ok()).unwrap_or_default();
        match places.as_slice() {
            [] => chilpa_i18n::t("This record cannot be played now."),
            [one] if one.starts_with('/') || one.contains(":\\") => chilpa_i18n::t_args("{} is not available. The record will open when it is back.", &[one]),
            [one] => chilpa_i18n::t_args("{} does not answer. The record will open when it is back.", &[one]),
            many => chilpa_i18n::t_args("{} are not available. The record will open when they are back.", &[&many.join(", ")]),
        }
    }

    /// The codec the library keeps for a server's track, which its entry
    /// does not show.
    pub(crate) fn remote_codec(&mut self, path: &str) -> Option<String> {
        if !RemoteEntry::is_remote(path) {
            return None;
        }
        // What the decoder found beats what anybody said of it.
        if let Some(codec) = chilpa_core::remote::opened_codec(path) {
            return Some(codec.to_string());
        }
        let lib = self.library()?;
        let (_, source) = lib.track_by_entry(path).ok()??;
        lib.server_track(source, path).ok()??.0
    }

    /// Whether the library changed underneath the window since this was
    /// last asked: a sync, a server or a folder that came or went.
    pub fn mark_library_changed(&mut self) { self.home.clear(); self.invalidate_artist_collections(); self.servers.changed = true; }

    pub fn library_changed(&mut self) -> bool {
        std::mem::take(&mut self.servers.changed)
    }

    /// What came back from the servers' threads. `poll` calls it.
    pub(crate) fn poll_servers(&mut self) {
        self.servers.setup.poll();
        let events = self.servers.syncer.as_mut().map(|s| s.poll()).unwrap_or_default();
        for event in events {
            match event {
                sync::Event::Progress { done, .. } if done % 1_000 < 250 => self.servers.changed = true,
                sync::Event::Progress { .. } => {}
                sync::Event::Finished { source, .. } => {
                    self.servers.errors.remove(&source);
                    self.servers.changed = true;
                }
                sync::Event::Covered { .. } => self.servers.changed = true,
                sync::Event::Failed { source, why, unreachable } => {
                    self.servers.errors.insert(source, why);
                    if unreachable {
                        if let Some(watcher) = &self.servers.watcher {
                            watcher.ask();
                        }
                    }
                    self.servers.changed = true;
                }
            }
        }
        let failed = std::mem::take(&mut *self.servers.failures.lock().unwrap_or_else(|e| e.into_inner()));
        if !failed.is_empty() {
            if let Some(watcher) = &self.servers.watcher {
                watcher.ask();
            }
        }
        if let Some(look) = self.servers.watcher.as_ref().and_then(|w| w.looked()) {
            if look.refused != self.servers.refused {
                self.servers.refused = look.refused;
                self.servers.changed = true;
            }
            self.servers.changed |= look.changed;
        }
    }
}

/// Whether a queue entry, or a track's path, is a track on a server.
pub fn on_a_server(path: &str) -> bool {
    RemoteEntry::is_remote(path)
}

/// Plain http to an address that is not this machine or a private one.
fn plain_and_outside(address: &str) -> bool {
    let Some(rest) = address.strip_prefix("http://") else { return false };
    let authority = rest.split('/').next().unwrap_or("");
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => authority.rsplit_once(':').map_or(authority, |(h, _)| h),
    };
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => !(ip.is_private() || ip.is_loopback() || ip.is_link_local()),
        // Unique local (fc00::/7), link-local (fe80::/10) and this machine.
        Ok(std::net::IpAddr::V6(ip)) => {
            let first = ip.segments()[0];
            !(ip.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80)
        }
        // A name: one without a dot, or under .local or .lan, is the LAN's.
        Err(_) => !(host == "localhost" || host.ends_with(".local") || host.ends_with(".lan") || !host.contains('.')),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_http_is_pointed_out_only_outside_the_lan() {
        for inside in ["http://192.168.0.69:4533", "http://nas.lan:8096", "http://music:4533", "http://172.20.0.2", "http://10.0.0.5", "https://anything.example.org"] {
            assert!(!plain_and_outside(inside), "{inside}");
        }
        for inside in ["http://[fd12::5]:4533", "http://[fe80::1]", "http://[::1]:8096", "http://127.0.0.1:4533"] {
            assert!(!plain_and_outside(inside), "{inside}");
        }
        for outside in ["http://music.example.org", "http://203.0.113.4:4533", "http://172.32.0.1", "http://[2001:db8::1]:4533", "http://[2001:db8::1]"] {
            assert!(plain_and_outside(outside), "{outside}");
        }
    }
}
