//! One confirmed music deletion, using the existing server writer. There
//! is at most one pending request; idle state starts no work or network I/O.

use super::{Chilpa, net, writing};
use chilpa_library::store::server_media::Target;
use chilpa_models::RemoteEntry;
use chilpa_servers::{MediaKind, Net};
use serde_json::{Value, json};

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct State {
    next: u64,
    job: Option<writing::Write>,
    started: bool,
    resume: bool,
    receipt: Value,
}

impl State {
    pub(super) fn affects(&self, source: i64) -> bool { self.job.as_ref().is_some_and(|job| job.source == source) }
}

fn remote_id(target: &Target, stored: &chilpa_library::StoredServer) -> Option<String> {
    let reference = if target.album { target.reference.clone() } else {
        let entry = RemoteEntry::parse(&target.reference)?;
        if entry.source.provider != stored.provider || entry.source.instance != stored.instance { return None; }
        entry.external_id
    };
    let compact: String = reference.chars().filter(|c| *c != '-').collect();
    (compact.len() == 32 && compact.bytes().all(|c| c.is_ascii_hexdigit())).then_some(reference)
}

fn target(api: &mut Chilpa, kind: &str, id: i64) -> Option<(Target, chilpa_library::StoredServer, String)> {
    let album = match kind { "album" => true, "track" => false, _ => return None };
    let lib = api.library()?;
    let target = lib.server_media_target(album, id).ok().flatten()?;
    let stored = lib.server(target.source).ok().flatten()?;
    if !stored.enabled || !chilpa_servers::Kind::from_provider(&stored.provider)?.deletes_media() { return None; }
    let reference = remote_id(&target, &stored)?;
    Some((target, stored, reference))
}

impl Chilpa {
    /// Local identity only: opening a menu or cancelling asks no network.
    pub fn server_media_info(&mut self, kind: &str, id: i64) -> String {
        match target(self, kind, id) {
            Some((item, stored, reference)) => json!({"kind":kind,"id":id,"title":item.title,"server":stored.label,
                "address":stored.address,"source":stored.source_id,"ref":reference}).to_string(),
            None => "{}".into(),
        }
    }

    /// `expected` is the identity displayed in the confirmation, not an
    /// arbitrary remote id. The current library must still name that item.
    pub fn server_media_delete(&mut self, kind: &str, id: i64, expected: &str, source: i64) -> String {
        let refused = |why: String| json!({"pending":false,"error":why}).to_string();
        if self.offline() { return refused(chilpa_i18n::t("Turn the network on to delete music from the server.")); }
        if self.servers.media.job.is_some() { return refused(chilpa_i18n::t("A server deletion is still in progress.")); }
        let Some((item, stored, reference)) = target(self, kind, id) else {
            return refused(chilpa_i18n::t("that music item is no longer available from this server"));
        };
        if source != stored.source_id || reference != expected {
            return refused(chilpa_i18n::t("The selected music changed. Open its menu and confirm again."));
        }
        if self.servers.writer.is_none() { return refused(chilpa_i18n::t("servers are not started")); }
        if self.removing_server(source) { return refused(chilpa_i18n::t("that server is being removed")); }
        let state = &mut self.servers.media;
        state.next = state.next.wrapping_add(1);
        let request = format!("server-media:{}", state.next);
        state.resume = self.servers.syncer.as_ref().is_some_and(|syncer| syncer.has(source));
        state.receipt = json!({"request_id":request,"pending":true});
        state.started = false;
        state.job = Some(writing::Write { source, playlist:id, request,
            change: writing::Change::DeleteMedia { target:item, remote:reference } });
        // A catalogue page already in flight finishes before deletion, so
        // an old reply cannot restore the item after the server removed it.
        if let Some(syncer) = self.servers.syncer.as_mut() { syncer.cancel(source); }
        self.poll_server_media();
        self.servers.media.receipt.to_string()
    }

    pub fn server_media_status(&self, request: &str) -> String {
        if self.servers.media.receipt["request_id"].as_str() == Some(request) {
            self.servers.media.receipt.to_string()
        } else { json!({"pending":false,"error":chilpa_i18n::t("This deletion request is no longer active.")}).to_string() }
    }

    pub(super) fn poll_server_media(&mut self) {
        let state = &self.servers.media;
        let Some(job) = state.job.as_ref().filter(|_| !state.started).cloned() else { return };
        if self.servers.syncer.as_ref().is_some_and(|syncer| syncer.has(job.source)) { return; }
        // Keep media work bounded to one request instead of adding a queue.
        let sent = self.servers.writer.as_ref().is_some_and(|writer| writer.ask(job.clone()));
        self.servers.media.started = sent;
        if !sent {
            self.servers.media.receipt = json!({"request_id":job.request,"pending":false,"error":chilpa_i18n::t("servers are not started")});
            self.servers.media.job = None;
        }
    }

    pub(super) fn server_media_finished(&mut self, written: writing::Written) {
        let state = &mut self.servers.media;
        if !state.job.as_ref().is_some_and(|job| job.request == written.request) { return; }
        state.job = None;
        state.started = false;
        state.receipt = if written.why.is_empty() { json!({"request_id":written.request,"pending":false,"done":true}) }
            else { json!({"request_id":written.request,"pending":false,"error":written.why}) };
        let resume = std::mem::take(&mut state.resume) || written.missing;
        let succeeded = written.why.is_empty();
        if !succeeded { self.now.error = written.why; }
        self.servers.changed = true;
        if resume { self.sync_server(written.source); }
        else if succeeded {
            if let Some(syncer) = self.servers.syncer.as_mut() { syncer.ask_playlists(written.source); }
        }
    }
}

pub(super) fn apply(lib: &chilpa_library::Library, net: &dyn Net, keys: &super::keys::Keys,
    gate: &std::sync::Mutex<()>, write: &writing::Write, item: &Target, expected: &str) -> writing::Written {
    let failed = |why: String, missing| writing::Written { source:write.source, asked:write.playlist, playlist:0,
        request:write.request.clone(), why, missing, media:true };
    let Some(stored) = lib.server(write.source).ok().flatten().filter(|server| server.enabled) else {
        return failed(chilpa_i18n::t("that server is disabled or no longer in the library"), false);
    };
    if lib.server_media_target(item.album, item.local).ok().flatten().as_ref() != Some(item)
        || remote_id(item, &stored).as_deref() != Some(expected) {
        return failed(chilpa_i18n::t("The selected music changed. Open its menu and confirm again."), false);
    }
    let (Some(server), Some(key)) = (net::server(&stored), net::key(keys, &stored)) else {
        return failed(chilpa_i18n::t_args("sign in to {} again", &[&stored.label]), false);
    };
    let _writing = gate.lock().unwrap_or_else(|error| error.into_inner());
    let kind = if item.album { MediaKind::Album } else { MediaKind::Track };
    let observed = Attempt { net, sent: std::sync::atomic::AtomicBool::new(false) };
    if let Err(error) = chilpa_servers::media::delete(&observed, &server, &key, expected, kind) {
        // A lost answer after DELETE may mean the server already changed.
        // Reconcile once; never repeat the destructive request automatically.
        return failed(error.to_string(), observed.sent.load(std::sync::atomic::Ordering::Relaxed));
    }
    match lib.server_media_deleted(item) {
        Ok(_) => failed(String::new(), false),
        Err(_) => failed(chilpa_i18n::t("The server deleted the music, but the local catalogue could not be updated. Sync the server again."), true),
    }
}

struct Attempt<'a> { net: &'a dyn Net, sent: std::sync::atomic::AtomicBool }
impl Net for Attempt<'_> {
    fn send(&self, request: &chilpa_servers::Request) -> Result<chilpa_servers::Reply, String> {
        if request.method == "DELETE" { self.sent.store(true, std::sync::atomic::Ordering::Relaxed); }
        self.net.send(request)
    }
}
