//! A record on a server of mine, saved to the server.
//!
//! The server is where its metadata lives: a change made here is written
//! to the server, and what the server then says is what the library shows.
//! The one exception is a change the listener keeps on this device on
//! purpose (the sidecar, `hosted`): it stays over what the server says
//! until it is taken off, and it is never made without being asked for.
//!
//! What the server says of the record is read when it opens, on the
//! worker, and stands in for what a file would have said. A save to the
//! server writes only the fields that changed, each to the item that holds
//! it, reads every track of the record back, and hands that to the window's
//! thread, which writes it into the library as a sync would. A field the
//! server does not keep refuses the save instead of going anywhere else.

use super::artwork::Waiting;
use super::draft::{Draft, Read};
use super::fields::{self, ALBUM, TRACK};
use super::save::HomePlan;
use super::worker::{Done, Job};
use super::State;
use crate::Chilpa;
use crate::servers::keys::Keys;
use chilpa_library::sidecar::{join, split, Override, Sidecar};
use chilpa_library::EditTrack;
use chilpa_servers::edit::{self, Edit, Entity, Held};
use chilpa_servers::{Credential, Failure, Kind, Net, Server};
use chilpa_tags::edit::Snapshot;
use std::sync::Arc;

/// How the worker reaches a record's server. Made on the window's thread,
/// where the library is; the credential is unsealed on the worker.
#[derive(Clone)]
pub(crate) struct Reach {
    pub net: Arc<dyn Net>,
    pub keys: Option<Arc<Keys>>,
    /// Stands for the sealed credential. Tests only.
    pub credential: Option<Credential>,
    pub stored: chilpa_library::StoredServer,
    /// Chilpa is set to stay off the network.
    pub offline: bool,
}

impl Reach {
    fn connect(&self) -> Result<(Server, Credential), String> {
        if self.offline {
            return Err(chilpa_i18n::t("Turn the network on to change the server's metadata."));
        }
        if !self.stored.enabled {
            return Err(chilpa_i18n::t("that server is disabled"));
        }
        let server = crate::servers::net::server(&self.stored).ok_or_else(|| chilpa_i18n::t("Chilpa cannot change this server's metadata"))?;
        let key = self
            .credential
            .clone()
            .or_else(|| self.keys.as_ref().and_then(|keys| crate::servers::net::key(keys, &self.stored)))
            .ok_or_else(|| chilpa_i18n::t_args("sign in to {} again", &[&self.stored.label]))?;
        Ok((server, key))
    }

    fn name(&self) -> String {
        if self.stored.label.is_empty() {
            Kind::from_provider(&self.stored.provider).map(|k| k.name().to_string()).unwrap_or_default()
        } else {
            self.stored.label.clone()
        }
    }
}

/// What the editor knows of a record's server.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ServerState {
    pub source_id: i64,
    pub name: String,
    pub kind: Option<Kind>,
    /// A save can go to it.
    pub writable: bool,
    /// Why not, in the user's words. Empty when it can.
    pub why: String,
    /// The record's id on the server.
    pub album_key: Option<String>,
}

impl ServerState {
    /// Whether the server keeps that field of a record or of a track.
    pub fn keeps(&self, of_a_track: bool, name: &str) -> bool {
        let Some(kind) = self.kind else { return false };
        kind.editable(if of_a_track { Entity::Track } else { Entity::Album }).contains(&name)
    }

    pub fn takes_cover(&self) -> bool {
        self.album_key.is_some() && self.kind.is_some_and(|kind| kind.takes_images(Entity::Album))
    }
}

/// What a track says, from what its server says of it and of its record,
/// as a file would have said it.
fn snapshot(track: &Held, album: Option<&Held>) -> Snapshot {
    let text = |s: &str| (!s.is_empty()).then(|| s.to_string());
    let record = album.unwrap_or(track);
    let id = |held: &Held, name: &str| held.ids.get(name).cloned();
    Snapshot {
        title: text(&track.title),
        artist_credit: text(&track.artist).into_iter().collect(),
        artists: track.artists.clone(),
        album: text(if album.is_some() { &record.title } else { &track.album }),
        album_artist: text(&record.album_artist),
        date: text(&record.date),
        genres: record.genres.clone(),
        label: text(&record.label),
        track_number: track.track_number,
        disc_number: track.disc_number,
        musicbrainz_release_id: id(record, edit::field::RELEASE),
        musicbrainz_release_group_id: id(record, edit::field::RELEASE_GROUP),
        musicbrainz_recording_id: id(track, edit::field::RECORDING),
        musicbrainz_track_id: id(track, edit::field::TRACK),
        ..Snapshot::default()
    }
}

/// What a server says of the record's tracks, by their place among
/// `tracks`, and what it lets be done. Tracks on no server read nothing
/// here. Runs on the worker.
pub(crate) fn read(reach: Option<&Reach>, tracks: &[EditTrack]) -> (Vec<Option<Read>>, Option<ServerState>) {
    let mut reads: Vec<Option<Read>> = vec![None; tracks.len()];
    let Some(first) = tracks.iter().find_map(|t| t.hosted.as_ref()) else { return (reads, None) };
    let mut state = ServerState { source_id: first.source_id, album_key: first.album_key.clone(), ..ServerState::default() };
    let Some(reach) = reach else {
        state.why = chilpa_i18n::t("that server is no longer in the library");
        return (reads, Some(state));
    };
    state.name = reach.name();
    state.kind = Kind::from_provider(&reach.stored.provider);
    let (server, key) = match reach.connect() {
        Ok(found) => found,
        Err(why) => {
            state.why = why;
            return (reads, Some(state));
        }
    };
    let kind = server.kind;
    if kind.editable(Entity::Album).is_empty() && kind.editable(Entity::Track).is_empty() {
        state.why = kind.not_editable();
        return (reads, Some(state));
    }
    let net = reach.net.as_ref();
    let unreadable = |failure: Failure| chilpa_i18n::t_args("the server could not be read: {}", &[&failure.to_string()]);
    match edit::may_edit(net, &server, &key) {
        Ok(()) => state.writable = true,
        Err(Failure::Forbidden(why)) => state.why = why,
        Err(failure) => {
            state.why = unreadable(failure);
            return (reads, Some(state));
        }
    }
    let album = match &state.album_key {
        Some(album) => match edit::held(net, &server, &key, Entity::Album, album) {
            Ok(held) => Some(held),
            Err(failure) => {
                state.writable = false;
                state.why = unreadable(failure);
                return (reads, Some(state));
            }
        },
        None => None,
    };
    let ids: Vec<String> = tracks.iter().filter(|t| t.hosted.is_some()).map(|t| server_id(&t.external_id)).collect();
    let held = match edit::held_tracks(net, &server, &key, &ids) {
        Ok(held) => held,
        Err(failure) => {
            state.writable = false;
            state.why = unreadable(failure);
            return (reads, Some(state));
        }
    };
    for (at, track) in tracks.iter().enumerate() {
        if track.hosted.is_none() {
            continue;
        }
        let id = server_id(&track.external_id);
        if let Some((_, said)) = held.iter().find(|(held_id, _)| *held_id == id) {
            reads[at] = Some(Read { said: snapshot(said, album.as_ref()), known: true, placed: Vec::new() });
        }
    }
    (reads, Some(state))
}

/// A track's id on its server, from its queue entry.
pub(crate) fn server_id(entry: &str) -> String {
    chilpa_models::RemoteEntry::parse(entry).map(|e| e.external_id).unwrap_or_else(|| entry.to_string())
}

/// Everything a save to the server is going to do, decided before any of
/// it is sent.
pub(crate) struct ServerPlan {
    /// The record's id on the server.
    pub record: Option<String>,
    /// The record's id on the server, and what changes in it.
    pub album: Option<(String, Edit)>,
    /// Each track's id on the server and what changes in it, in the
    /// record's order. Tracks nothing changes in are left out.
    pub tracks: Vec<(String, Edit)>,
    pub cover: Option<Waiting>,
    /// Every track of the record by its id on the server: what is read back.
    pub every: Vec<String>,
    /// What is asked of each item, to compare with what it says after.
    pub asked: Vec<Asked>,
    /// The overlays that stop laying what now goes to the server.
    pub homes: Vec<HomePlan>,
}

/// One field asked of one item.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Asked {
    pub entity: Entity,
    pub id: String,
    pub name: &'static str,
    pub value: String,
}

fn put_album(edit: &mut Edit, name: &str, value: &str) {
    match name {
        "title" => edit.title = Some(value.to_string()),
        "album_artist" => edit.album_artist = Some(value.to_string()),
        "date" => edit.date = Some(value.to_string()),
        "genres" => edit.genres = Some(split(value)),
        "label" => edit.label = Some(value.to_string()),
        "musicbrainz_release_id" | "musicbrainz_release_group_id" => {
            edit.ids.insert(name.to_string(), value.to_string());
        }
        _ => {}
    }
}

/// A record's field as each of its tracks carries it on the server.
fn put_carried(edit: &mut Edit, name: &str, value: &str) {
    match name {
        "title" => edit.album = Some(value.to_string()),
        "album_artist" => edit.album_artist = Some(value.to_string()),
        "date" => edit.date = Some(value.to_string()),
        "genres" => edit.genres = Some(split(value)),
        _ => {}
    }
}

fn put_track(edit: &mut Edit, name: &str, value: &str) {
    match name {
        "title" => edit.title = Some(value.to_string()),
        "artist_credit" => edit.artist = Some(value.to_string()),
        "artists" => edit.artists = Some(split(value)),
        "track_number" => edit.track_number = Some(value.parse().ok()),
        "disc_number" => edit.disc_number = Some(value.parse().ok()),
        "musicbrainz_recording_id" | "musicbrainz_track_id" => {
            edit.ids.insert(name.to_string(), value.to_string());
        }
        _ => {}
    }
}

/// Why a save to the server is refused for a field changed that the server
/// does not keep.
pub(crate) fn not_kept() -> String {
    chilpa_i18n::t("this server does not keep this field; save it to the sidecar on this device, or put it back")
}

/// Decides what a save to the server does. Nothing is read and nothing is
/// written. `Err` says why it cannot go: a field changed that the server
/// does not keep, or a cover it takes none of.
pub(crate) fn plan(draft: &Draft, state: &ServerState, cover: Option<Waiting>, now: i64) -> Result<ServerPlan, String> {
    let kind = state.kind.ok_or_else(|| state.why.clone())?;
    let mut sidecars: Vec<Sidecar> = draft.homes.iter().map(|h| h.sidecar.clone().unwrap_or_else(|| Sidecar::new(now))).collect();
    let mut album = Edit::default();
    let mut tracks: Vec<Edit> = draft.rows.iter().map(|_| Edit::default()).collect();
    let mut asked = Vec::new();
    let album_id = state.album_key.clone();

    for (at, field) in ALBUM.iter().enumerate() {
        let cell = &draft.album[at];
        if !cell.dirty() {
            continue;
        }
        if !state.keeps(false, field.name) {
            return Err(not_kept());
        }
        let theirs = draft.rows.first().map(|row| row.album_file[at].clone()).unwrap_or_default();
        if !fields::same(field.kind, &cell.now, &theirs) {
            if let Some(id) = &album_id {
                put_album(&mut album, field.name, &cell.now);
                asked.push(Asked { entity: Entity::Album, id: id.clone(), name: field.name, value: cell.now.clone() });
            }
            if kind.carried().contains(&field.name) || album_id.is_none() {
                for edit in &mut tracks {
                    put_carried(edit, field.name, &cell.now);
                }
            }
        }
        for sidecar in &mut sidecars {
            sidecar.album.put(field.name, Override::Absent);
        }
    }
    for (i, row) in draft.rows.iter().enumerate() {
        for (at, field) in TRACK.iter().enumerate() {
            let cell = &row.cells[at];
            if !cell.dirty() {
                continue;
            }
            if !state.keeps(true, field.name) {
                return Err(not_kept());
            }
            if !fields::same(field.kind, &cell.now, &row.file[at]) {
                put_track(&mut tracks[i], field.name, &cell.now);
                asked.push(Asked { entity: Entity::Track, id: server_id(&row.track.external_id), name: field.name, value: cell.now.clone() });
            }
            let sidecar = &mut sidecars[row.home];
            if sidecar.tracks.contains_key(&row.key) {
                sidecar.track_mut(&row.key).put(field.name, Override::Absent);
            }
        }
    }
    if cover.is_some() {
        if !state.takes_cover() {
            return Err(chilpa_i18n::t("this server takes no cover from Chilpa; save the cover to the sidecar on this device"));
        }
        // The server's cover is the one shown from now on.
        for sidecar in &mut sidecars {
            sidecar.album.cover = Override::Absent;
        }
    }
    for sidecar in &mut sidecars {
        sidecar.tracks.retain(|_, track| !track.is_empty());
    }
    let homes = draft
        .homes
        .iter()
        .zip(sidecars)
        .enumerate()
        .filter(|(_, (state, sidecar))| state.sidecar.as_ref().is_some_and(|was| was != sidecar))
        .map(|(at, (state, sidecar))| HomePlan {
            home: state.home.clone(),
            of: state.of.clone(),
            sidecar,
            cover: false,
            tracks: draft.rows.iter().filter(|row| row.home == at).map(|row| row.track.id).collect(),
        })
        .collect();
    Ok(ServerPlan {
        record: album_id.clone(),
        album: album_id.filter(|_| !album.is_empty()).map(|id| (id, album)),
        tracks: draft.rows.iter().zip(tracks).filter(|(_, edit)| !edit.is_empty()).map(|(row, edit)| (server_id(&row.track.external_id), edit)).collect(),
        cover,
        every: draft.rows.iter().filter(|row| row.hosted()).map(|row| server_id(&row.track.external_id)).collect(),
        asked,
        homes,
    })
}

/// What a save to the server did, and what the server says after it.
#[derive(Debug, Default)]
pub(crate) struct Outcome {
    /// The items the server took a change to, and how many were asked.
    pub sent: usize,
    pub total: usize,
    pub cover_sent: bool,
    /// Why it stopped. `None` when everything was taken.
    pub failed: Option<String>,
    /// Every track of the record as the server describes it now, as its
    /// catalogue lists it.
    pub tracks: Vec<chilpa_servers::Track>,
    /// The record could not be read back: why.
    pub unread: Option<String>,
    /// The fields the server says otherwise than they were sent.
    pub unlike: usize,
    /// The record's cover as the server gives it now, and its extension.
    pub cover: Option<(Vec<u8>, &'static str)>,
}

/// The size covers are asked for at, as a sync asks for them.
const COVER_SIZE: u32 = 1200;

/// What one field of an item says, as the form holds it.
fn said(held: &Held, entity: Entity, name: &str) -> String {
    match (entity, name) {
        (_, "title") => held.title.clone(),
        (_, "album_artist") => held.album_artist.clone(),
        (_, "artist_credit") => held.artist.clone(),
        (_, "artists") => join(&held.artists),
        (_, "date") => held.date.clone(),
        (_, "genres") => join(&held.genres),
        (_, "label") => held.label.clone(),
        (_, "track_number") => held.track_number.map(|n| n.to_string()).unwrap_or_default(),
        (_, "disc_number") => held.disc_number.map(|n| n.to_string()).unwrap_or_default(),
        (_, other) => held.ids.get(other).cloned().unwrap_or_default(),
    }
}

/// Sends what was planned and reads the record back. Runs on the worker.
pub(crate) fn run(session: u64, plan: ServerPlan, reach: Reach) -> Done {
    let mut outcome = Outcome { total: usize::from(plan.album.is_some()) + plan.tracks.len() + usize::from(plan.cover.is_some()), ..Outcome::default() };
    let (server, key) = match reach.connect() {
        Ok(found) => found,
        Err(why) => {
            outcome.failed = Some(why);
            return Done::ServerSaved { session, outcome };
        }
    };
    let net = reach.net.as_ref();
    // The tracks before their record: a server that works a record out of
    // its tracks (Jellyfin, on a refresh it may run at any moment) finds
    // them saying what the record will.
    let changes = plan.tracks.iter().map(|(id, edit)| (Entity::Track, id, edit)).chain(plan.album.iter().map(|(id, edit)| (Entity::Album, id, edit)));
    for (entity, id, edit) in changes {
        match edit::write(net, &server, &key, entity, id, edit) {
            Ok(()) => outcome.sent += 1,
            Err(failure) => {
                outcome.failed = Some(failure.to_string());
                break;
            }
        }
    }
    if let (None, Some(waiting), Some(record)) = (&outcome.failed, &plan.cover, &plan.record) {
        match edit::upload_image(net, &server, &key, Entity::Album, record, &waiting.bytes, waiting.mime) {
            Ok(()) => outcome.cover_sent = true,
            Err(failure) => outcome.failed = Some(failure.to_string()),
        }
    }
    if outcome.sent == 0 && !outcome.cover_sent {
        return Done::ServerSaved { session, outcome };
    }

    // What the server says now: the record's tracks as its catalogue lists
    // them, which is what the library takes, and each field asked of it.
    match read_back(net, &server, &key, &plan.every) {
        Ok(tracks) => outcome.tracks = tracks,
        Err(failure) => outcome.unread = Some(failure.to_string()),
    }
    outcome.unlike = unlike(net, &server, &key, &plan.asked);
    if outcome.cover_sent {
        let named = outcome.tracks.iter().find_map(|t| t.cover.clone());
        if let Some(name) = named {
            let request = chilpa_servers::cover(&server, &key, &name, COVER_SIZE);
            if let Ok(reply) = net.send(&request) {
                let kind = reply.headers.get("content-type").cloned().unwrap_or_default();
                let ext = image_kind(&kind, &reply.body);
                if reply.status == 200 && !ext.is_empty() {
                    outcome.cover = Some((reply.body, ext));
                }
            }
        }
    }
    Done::ServerSaved { session, outcome }
}

fn read_back(net: &dyn Net, server: &Server, key: &Credential, every: &[String]) -> Result<Vec<chilpa_servers::Track>, Failure> {
    let mut out = Vec::with_capacity(every.len());
    for batch in every.chunks(chilpa_servers::DETAIL_BATCH) {
        out.extend(chilpa_servers::details(net, server, key, batch)?);
    }
    Ok(out)
}

/// How many of the fields asked the server now says otherwise. One that
/// cannot be read back counts as said otherwise.
fn unlike(net: &dyn Net, server: &Server, key: &Credential, asked: &[Asked]) -> usize {
    let mut items: Vec<(Entity, &str)> = Vec::new();
    for one in asked {
        if !items.contains(&(one.entity, one.id.as_str())) {
            items.push((one.entity, one.id.as_str()));
        }
    }
    let mut count = 0;
    for (entity, id) in items {
        let held = edit::held(net, server, key, entity, id).ok();
        for one in asked.iter().filter(|a| a.entity == entity && a.id == id) {
            let kind = if entity == Entity::Album { fields::album(one.name).map(|at| ALBUM[at].kind) } else { fields::track(one.name).map(|at| TRACK[at].kind) };
            let same = match (&held, kind) {
                (Some(held), Some(kind)) => fields::same(kind, &fields::normalize(kind, &said(held, entity, one.name)), &one.value),
                _ => false,
            };
            count += usize::from(!same);
        }
    }
    count
}

/// The extension a cover is kept with, by its type or its first bytes.
/// Empty when it is not an image.
fn image_kind(kind: &str, body: &[u8]) -> &'static str {
    if body.starts_with(&[0x89, b'P', b'N', b'G']) || kind.contains("png") {
        "png"
    } else if body.starts_with(&[0xFF, 0xD8, 0xFF]) || kind.contains("jpeg") || kind.contains("jpg") {
        "jpg"
    } else {
        ""
    }
}

impl Chilpa {
    /// How the worker reaches the server a record's tracks are on; `None`
    /// when none is on a server, or the library no longer has that server.
    pub(super) fn metadata_reach(&mut self, tracks: &[EditTrack]) -> Option<Reach> {
        let source = tracks.iter().find_map(|t| t.hosted.as_ref())?.source_id;
        let offline = self.offline();
        let stored = self.library()?.server(source).ok().flatten()?;
        let (net, credential): (Arc<dyn Net>, Option<Credential>) = match &self.metadata.server_access {
            Some((net, credential)) => (net.clone(), Some(credential.clone())),
            None => (Arc::new(crate::servers::net::Network), None),
        };
        Some(Reach { net, keys: self.servers.keys.clone(), credential, stored, offline })
    }

    /// A save in the server's mode, past the checks every save makes.
    pub(super) fn metadata_save_to_server(&mut self) -> bool {
        let tracks: Vec<EditTrack> = self.metadata.draft.as_ref().map(|d| d.rows.iter().map(|r| r.track.clone()).collect()).unwrap_or_default();
        let reach = self.metadata_reach(&tracks);
        let removing = self.metadata.server.as_ref().is_some_and(|s| self.removing_server(s.source_id));
        let now = crate::now_unix();
        let editor = &mut self.metadata;
        let Some(draft) = editor.draft.as_ref() else { return false };
        let state = editor.server.clone().unwrap_or_default();
        if !state.writable {
            let why = if state.why.is_empty() { chilpa_i18n::t("Chilpa cannot change this server's metadata") } else { state.why.clone() };
            return editor.refuse(why);
        }
        let Some(reach) = reach else { return editor.refuse(chilpa_i18n::t("that server is no longer in the library")) };
        if removing {
            return editor.refuse(chilpa_i18n::t("that server is being removed"));
        }
        let mut made = match plan(draft, &state, editor.waiting.clone(), now) {
            Ok(made) => made,
            Err(why) => return editor.refuse(why),
        };
        let homes = std::mem::take(&mut made.homes);
        if made.album.is_none() && made.tracks.is_empty() && made.cover.is_none() {
            // Nothing to send: what was typed is what the server says, and
            // all there is to do is take what this device laid over it off.
            if homes.is_empty() {
                return editor.refuse(chilpa_i18n::t("there is nothing to save"));
            }
            let words = self.metadata.draft.as_ref().map(|draft| super::hosted::words(draft, &homes)).unwrap_or_default();
            let landed = self.library().map(|lib| super::hosted::land(lib, &homes, None, now, &words));
            let editor = &mut self.metadata;
            match landed {
                Some((_, Some(why))) => return editor.refuse(why),
                None => return editor.refuse(chilpa_i18n::t("the library could not be written")),
                Some((_, None)) => {}
            }
            let album = editor.album_id;
            editor.forget();
            editor.album_id = album;
            editor.error.clear();
            editor.notice = chilpa_i18n::t("the changes were saved");
            editor.refresh();
            return true;
        }
        editor.pending_hosted = homes;
        editor.state = State::Saving;
        editor.progress = (0, 0);
        editor.error.clear();
        editor.notice.clear();
        editor.lookup_turn += 1;
        editor.artwork_turn += 1;
        editor.searching = false;
        editor.loading = false;
        editor.artwork_searching = false;
        editor.artwork_loading = false;
        let session = editor.session;
        if !editor.ask(Job::SaveServer { session, plan: made, reach }) {
            editor.state = State::Ready;
            editor.pending_hosted.clear();
            return editor.refuse(chilpa_i18n::t("the editor could not be started"));
        }
        editor.refresh();
        true
    }

    /// What a save to the server left is written into the library -- the
    /// server's word for every track of the record, its cover, and the
    /// overlays that no longer lay what went to the server -- and the editor
    /// is left as it has to be.
    pub(super) fn metadata_server_saved(&mut self, outcome: Outcome) {
        let homes = std::mem::take(&mut self.metadata.pending_hosted);
        let words = self.metadata.draft.as_ref().map(|draft| super::hosted::words(draft, &homes)).unwrap_or_default();
        let state = self.metadata.server.clone().unwrap_or_default();
        let album_id = self.metadata.album_id;
        if outcome.sent == 0 && !outcome.cover_sent {
            // Nothing reached the server: the form is as it was.
            let editor = &mut self.metadata;
            editor.state = State::Ready;
            editor.progress = (0, 0);
            let why = outcome.failed.unwrap_or_default();
            editor.error = chilpa_i18n::t_args("nothing was saved to the server: {}", &[&why]);
            editor.notice.clear();
            editor.refresh();
            return;
        }
        let now = crate::now_unix();
        let mut told: Option<String> = None;
        if let Some(lib) = self.library() {
            // Only once everything went: after a failure the changes that
            // did not go stay where they were.
            if outcome.failed.is_none() && !homes.is_empty() {
                told = super::hosted::land(lib, &homes, None, now, &words).1;
            }
            if let Ok(Some(stored)) = lib.server(state.source_id) {
                if !outcome.tracks.is_empty() {
                    let tracks: Vec<chilpa_library::ServerTrack> = outcome.tracks.into_iter().map(|t| crate::servers::net::track(&stored, t)).collect();
                    if let Err(e) = lib.refresh_server_tracks(state.source_id, &tracks) {
                        told = Some(chilpa_i18n::t_args("what the server says now could not be written down in the library: {}; sync it to see it", &[&e.to_string()]));
                    }
                }
                if let Some((bytes, ext)) = &outcome.cover {
                    let dir = crate::servers::covers_dir(&stored.instance);
                    let path = dir.join(format!("{album_id}.{ext}"));
                    if std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, bytes)).is_ok() {
                        let other = dir.join(format!("{album_id}.{}", if *ext == "png" { "jpg" } else { "png" }));
                        let _ = std::fs::remove_file(other);
                        let _ = lib.set_server_cover(album_id, Some(&path.display().to_string()));
                    }
                }
            }
        }
        let reached = outcome.sent + usize::from(outcome.cover_sent);
        let editor = &mut self.metadata;
        editor.forget();
        editor.album_id = album_id;
        editor.error.clear();
        editor.notice.clear();
        match (&outcome.failed, &outcome.unread) {
            (Some(why), _) => {
                editor.error = chilpa_i18n::t_args("{} of {} changes reached the server before it failed: {}", &[&reached.to_string(), &outcome.total.to_string(), why]);
            }
            (None, Some(why)) => {
                editor.error = chilpa_i18n::t_args("the changes were saved to the server, but it could not be read back: {}; sync it to see them", &[why]);
            }
            (None, None) if outcome.unlike > 0 => {
                editor.notice = chilpa_i18n::tf(
                    "saved; the server keeps {} change differently, and what it keeps is shown",
                    "saved; the server keeps {} changes differently, and what it keeps is shown",
                    outcome.unlike as i64,
                    &[&outcome.unlike.to_string()],
                );
            }
            (None, None) => editor.notice = chilpa_i18n::t("the changes were saved to the server"),
        }
        if let Some(why) = told {
            editor.error = why;
        }
        editor.refresh();
    }
}

#[cfg(test)]
mod tests;
