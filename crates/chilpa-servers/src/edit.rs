//! What a server says of its records, tracks and artists, and changing it:
//! the fields Chilpa's metadata editor shows, and the image.
//!
//! What each protocol offers, and this follows:
//!
//! - **Plex** takes a field at a time on its library's item and keeps it in
//!   its own database, never in the music's files. Each field written is
//!   locked (`<field>.locked=1`), so its agents and a refresh leave it
//!   alone. A poster is uploaded beside the ones the item has and becomes
//!   the one shown. Nothing here deletes anything.
//! - **Jellyfin** takes the whole item back (`POST /Items/{id}`): a field
//!   left out of what is posted is cleared, so the item is read first and
//!   posted whole with the changes. Only an administrator may. It keeps the
//!   change in its database, and an image in its metadata folder, base64 in
//!   the request. A record's title, artist, date and genres are also
//!   carried by each of its tracks, and a change to the record does not
//!   reach them: they are written on each track too (`carried`).
//! - **OpenSubsonic** has no request that changes a tag or a cover: the
//!   server reads both from its files. Nothing is offered.
//! - **A media server** (DLNA) takes nothing.

use std::collections::BTreeMap;

use crate::model::{Credential, Failure, Kind, Server};
use crate::net::Net;

mod jellyfin;
mod plex;

/// What a change is made to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Entity {
    Album,
    Track,
    Artist,
}

/// What one item says, in the editor's words. A field the server does not
/// keep is empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Held {
    /// The record's title, the track's, or the artist's name.
    pub title: String,
    /// A track: its record's title, as the track carries it.
    pub album: String,
    pub album_artist: String,
    /// A track: its credit, as one line.
    pub artist: String,
    /// A track: its artists apart, when the server keeps them so.
    pub artists: Vec<String>,
    /// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`; empty when there is none.
    pub date: String,
    pub genres: Vec<String>,
    /// A record's label.
    pub label: String,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    /// An artist's biography, a record's review.
    pub summary: String,
    /// By the editor's name of the field (`musicbrainz_release_id`, ...).
    pub ids: BTreeMap<String, String>,
    /// The editor's names of the fields the server keeps locked.
    pub locked: Vec<String>,
}

/// A change to one item. `None` leaves that field alone; an empty value
/// clears it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Edit {
    pub title: Option<String>,
    /// A track: the record's title it carries (`carried`).
    pub album: Option<String>,
    pub album_artist: Option<String>,
    /// A track: its credit, kept as one artist.
    pub artist: Option<String>,
    /// A track: its artists apart. Wins over `artist`.
    pub artists: Option<Vec<String>>,
    pub date: Option<String>,
    pub genres: Option<Vec<String>>,
    pub label: Option<String>,
    pub track_number: Option<Option<u32>>,
    pub disc_number: Option<Option<u32>>,
    pub summary: Option<String>,
    /// By the editor's name of the field; empty clears it.
    pub ids: BTreeMap<String, String>,
}

impl Edit {
    pub fn is_empty(&self) -> bool {
        *self == Edit::default()
    }
}

/// Names of the editor's fields, as `editable` lists them.
pub mod field {
    pub const TITLE: &str = "title";
    pub const ALBUM_ARTIST: &str = "album_artist";
    pub const ARTIST: &str = "artist_credit";
    pub const ARTISTS: &str = "artists";
    pub const DATE: &str = "date";
    pub const GENRES: &str = "genres";
    pub const LABEL: &str = "label";
    pub const TRACK_NUMBER: &str = "track_number";
    pub const DISC_NUMBER: &str = "disc_number";
    pub const SUMMARY: &str = "summary";
    pub const RELEASE: &str = "musicbrainz_release_id";
    pub const RELEASE_GROUP: &str = "musicbrainz_release_group_id";
    pub const RECORDING: &str = "musicbrainz_recording_id";
    pub const TRACK: &str = "musicbrainz_track_id";
}

use field::*;

impl Kind {
    /// The editor's fields this kind of server takes a change to, for each
    /// kind of item. Empty: nothing of it can be changed there.
    pub fn editable(self, entity: Entity) -> &'static [&'static str] {
        match (self, entity) {
            (Kind::Plex, Entity::Album) => &[TITLE, DATE, GENRES, LABEL],
            (Kind::Plex, Entity::Track) => &[TITLE, ARTIST, TRACK_NUMBER, DISC_NUMBER],
            (Kind::Plex, Entity::Artist) => &[TITLE, GENRES, SUMMARY],
            (Kind::Jellyfin, Entity::Album) => &[TITLE, ALBUM_ARTIST, DATE, GENRES, LABEL, RELEASE, RELEASE_GROUP],
            (Kind::Jellyfin, Entity::Track) => &[TITLE, ARTIST, ARTISTS, TRACK_NUMBER, DISC_NUMBER, RECORDING, TRACK],
            (Kind::Jellyfin, Entity::Artist) => &[TITLE, GENRES, SUMMARY],
            (Kind::Subsonic | Kind::Dlna, _) => &[],
        }
    }

    /// Whether it takes an image for that kind of item: a record's cover,
    /// an artist's picture.
    pub fn takes_images(self, entity: Entity) -> bool {
        matches!((self, entity), (Kind::Plex | Kind::Jellyfin, Entity::Album | Entity::Artist))
    }

    /// The record's fields each of its tracks carries on this server, and
    /// that a change to the record writes on them too. Plex's tracks read
    /// their record's title, artist and year, but keep genres of their own.
    pub fn carried(self) -> &'static [&'static str] {
        match self {
            Kind::Jellyfin => &[TITLE, ALBUM_ARTIST, DATE, GENRES],
            Kind::Plex => &[GENRES],
            Kind::Subsonic | Kind::Dlna => &[],
        }
    }

    /// Why nothing of a record can be changed on this kind of server, in
    /// the user's words; empty when something can.
    pub fn not_editable(self) -> String {
        match self {
            Kind::Subsonic => chilpa_i18n::t("this server reads tags and covers from its files and offers no way to change them"),
            Kind::Dlna => chilpa_i18n::t("a media server (DLNA) takes no changes"),
            Kind::Plex | Kind::Jellyfin => String::new(),
        }
    }
}

/// Whether the signed-in account may change the server's items. `Ok` when
/// it may, or when the server only says so when asked to change one;
/// `Forbidden` says why not.
pub fn may_edit(net: &dyn Net, server: &Server, key: &Credential) -> Result<(), Failure> {
    match server.kind {
        Kind::Jellyfin => jellyfin::may_edit(net, server, key),
        Kind::Plex => Ok(()),
        Kind::Subsonic | Kind::Dlna => Err(Failure::Forbidden(server.kind.not_editable())),
    }
}

/// What the server says of one item, now.
pub fn held(net: &dyn Net, server: &Server, key: &Credential, entity: Entity, id: &str) -> Result<Held, Failure> {
    match server.kind {
        Kind::Jellyfin => jellyfin::held(net, server, key, id),
        Kind::Plex => plex::held(net, server, key, entity, id),
        Kind::Subsonic | Kind::Dlna => Err(Failure::Forbidden(server.kind.not_editable())),
    }
}

/// What the server says of several tracks, in one request for every
/// `crate::DETAIL_BATCH` of them. A track it does not describe is left out.
pub fn held_tracks(net: &dyn Net, server: &Server, key: &Credential, ids: &[String]) -> Result<Vec<(String, Held)>, Failure> {
    let mut out = Vec::with_capacity(ids.len());
    for batch in ids.chunks(crate::DETAIL_BATCH) {
        out.extend(match server.kind {
            Kind::Jellyfin => jellyfin::held_many(net, server, key, batch)?,
            Kind::Plex => plex::held_many(net, server, key, batch)?,
            Kind::Subsonic | Kind::Dlna => return Err(Failure::Forbidden(server.kind.not_editable())),
        });
    }
    Ok(out)
}

/// Asks the server for one change to one item. A field the server does not
/// take for that kind of item is refused before anything is sent.
pub fn write(net: &dyn Net, server: &Server, key: &Credential, entity: Entity, id: &str, edit: &Edit) -> Result<(), Failure> {
    if edit.is_empty() {
        return Ok(());
    }
    if server.kind.editable(entity).is_empty() {
        return Err(Failure::Forbidden(server.kind.not_editable()));
    }
    if let Some(field) = names(edit).into_iter().find(|name| !takes(server.kind, entity, name)) {
        return Err(Failure::Unexpected(chilpa_i18n::t_args("the server does not take \"{}\"", &[field])));
    }
    match server.kind {
        Kind::Jellyfin => jellyfin::write(net, server, key, id, edit),
        Kind::Plex => plex::write(net, server, key, entity, id, edit),
        Kind::Subsonic | Kind::Dlna => Err(Failure::Forbidden(server.kind.not_editable())),
    }
}

/// Whether a kind of server takes that field of that kind of item: its
/// own, or, on a track, one of its record's it carries ("album" is the
/// record's title as a track carries it).
pub fn takes(kind: Kind, entity: Entity, name: &str) -> bool {
    if kind.editable(entity).contains(&name) {
        return true;
    }
    entity == Entity::Track
        && match name {
            "album" => kind.carried().contains(&TITLE),
            ALBUM_ARTIST | DATE | GENRES => kind.carried().contains(&name),
            _ => false,
        }
}

/// The names of what an edit changes. A track's record title is "album":
/// no field of the editor's own.
fn names(edit: &Edit) -> Vec<&'static str> {
    let mut out = Vec::new();
    let mut add = |on: bool, name: &'static str| {
        if on {
            out.push(name);
        }
    };
    add(edit.title.is_some(), TITLE);
    add(edit.album.is_some(), "album");
    add(edit.album_artist.is_some(), ALBUM_ARTIST);
    add(edit.artist.is_some(), ARTIST);
    add(edit.artists.is_some(), ARTISTS);
    add(edit.date.is_some(), DATE);
    add(edit.genres.is_some(), GENRES);
    add(edit.label.is_some(), LABEL);
    add(edit.track_number.is_some(), TRACK_NUMBER);
    add(edit.disc_number.is_some(), DISC_NUMBER);
    add(edit.summary.is_some(), SUMMARY);
    for name in edit.ids.keys() {
        out.push(match name.as_str() {
            RELEASE => RELEASE,
            RELEASE_GROUP => RELEASE_GROUP,
            RECORDING => RECORDING,
            TRACK => TRACK,
            _ => "an identifier",
        });
    }
    out
}

/// The largest image a server is sent, as the editor takes one.
pub const IMAGE_BYTES: usize = 25 * 1024 * 1024;

/// Gives one record or artist that image, which becomes the one it shows.
/// `mime` is "image/jpeg" or "image/png".
pub fn upload_image(net: &dyn Net, server: &Server, key: &Credential, entity: Entity, id: &str, bytes: &[u8], mime: &str) -> Result<(), Failure> {
    if !server.kind.takes_images(entity) {
        return Err(Failure::Forbidden(chilpa_i18n::t("the server takes no image for it")));
    }
    if bytes.is_empty() || bytes.len() > IMAGE_BYTES || !matches!(mime, "image/jpeg" | "image/png") {
        return Err(Failure::Unexpected(chilpa_i18n::t("that image cannot be sent")));
    }
    match server.kind {
        Kind::Jellyfin => jellyfin::upload_image(net, server, key, id, bytes, mime),
        Kind::Plex => plex::upload_image(net, server, key, id, bytes, mime),
        Kind::Subsonic | Kind::Dlna => Err(Failure::Forbidden(server.kind.not_editable())),
    }
}

/// The year a date starts with.
fn year_of(date: &str) -> Option<u32> {
    date.get(..4).filter(|y| y.bytes().all(|b| b.is_ascii_digit())).and_then(|y| y.parse().ok())
}

/// A date that names its day: `YYYY-MM-DD`.
fn whole_day(date: &str) -> bool {
    date.len() == 10 && date.as_bytes()[4] == b'-' && date.as_bytes()[7] == b'-'
}

/// What an answer to a change says went wrong, by its status.
fn refused(status: u16) -> Failure {
    match status {
        401 | 403 => Failure::Forbidden(chilpa_i18n::t("this account may not change that server's metadata")),
        404 | 410 => Failure::Gone(chilpa_i18n::t("the server no longer has it")),
        503 => Failure::Busy(chilpa_i18n::t("the server is starting; try again in a moment")),
        status => Failure::Unexpected(chilpa_i18n::t_args("the server did not take the change (HTTP {})", &[&status.to_string()])),
    }
}

#[cfg(test)]
mod tests;
