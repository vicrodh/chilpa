//! Servers of mine: OpenSubsonic, Jellyfin, Plex and DLNA media servers
//! (ADR-017).
//!
//! This crate speaks their protocols and nothing else. It has no network:
//! it builds each request and reads each answer, and whoever calls hands in
//! the request itself (`Net`). So every test runs against answers saved from
//! real servers, without going out.
//!
//! It is also the only place in Chilpa that names them. Outside it a
//! server is a `source` with a kind, and the library keeps what it holds in
//! the same tables as the local files.
//!
//! What goes in and out:
//!
//! - signing in, which each protocol does its own way (`subsonic`,
//!   `jellyfin`, `plex`), and finding a media server, which has none
//!   (`dlna`);
//! - the libraries a server offers, to choose which come in;
//! - the catalogue, a page at a time, and the detail of the tracks whose
//!   listing does not carry their quality;
//! - its own playlists, their entries in order, and the changes asked of
//!   one: create, rename, delete, add, take out and reorder (`playlist`).
//!   A media server has none (`Kind::has_playlists`);
//! - where to read a track's original bytes, and a record's cover.

pub mod dlna;
pub mod jellyfin;
pub mod model;
pub mod net;
pub mod playlist;
pub mod plex;
pub mod subsonic;
mod text;

pub use model::{Client, Credential, Details, Failure, Kind, Library, Page, Probe, Server, Stream, Track};
pub use net::{Net, Reply, Request};
pub use playlist::{Change, Entry, Playlist, PlaylistListing, PlaylistTracks, PLAYLISTS, PLAYLIST_TRACKS};

/// Whether the server answers and the credential is still good.
pub fn ping(net: &dyn Net, server: &Server, key: &Credential) -> Result<(), Failure> {
    match server.kind {
        Kind::Subsonic => subsonic::ping(net, server, key),
        Kind::Jellyfin => jellyfin::ping(net, server, key),
        Kind::Plex => plex::ping(net, server, key),
        Kind::Dlna => dlna::ping(net, server),
    }
}

/// The music libraries the server offers. A media server offers the
/// containers at its top; the ones below are opened one at a time
/// (`dlna::folders`).
pub fn libraries(net: &dyn Net, server: &Server, key: &Credential) -> Result<Vec<Library>, Failure> {
    match server.kind {
        Kind::Subsonic => subsonic::libraries(net, server, key),
        Kind::Jellyfin => jellyfin::libraries(net, server, key),
        Kind::Plex => plex::libraries(net, server, key),
        Kind::Dlna => dlna::folders(net, server, dlna::ROOT),
    }
}

/// One page of the catalogue. `None` starts from the beginning; the page
/// says where the next one starts, or that there is none.
pub fn page(net: &dyn Net, server: &Server, key: &Credential, cursor: Option<&str>) -> Result<Page, Failure> {
    match server.kind {
        Kind::Subsonic => subsonic::page(net, server, key, cursor),
        Kind::Jellyfin => jellyfin::page(net, server, key, cursor),
        Kind::Plex => plex::page(net, server, key, cursor),
        Kind::Dlna => dlna::page(net, server, cursor),
    }
}

/// The whole description of tracks whose listing came without their
/// quality. At most `DETAIL_BATCH` at a time.
/// How many tracks the server says the chosen libraries have, for a
/// protocol whose pages do not say it (Subsonic). `None` for the others:
/// their pages carry their totals.
pub fn census(net: &dyn Net, server: &Server, key: &Credential) -> Result<Option<u64>, Failure> {
    match server.kind {
        Kind::Subsonic => subsonic::census(net, server, key).map(Some),
        Kind::Jellyfin | Kind::Plex | Kind::Dlna => Ok(None),
    }
}

pub fn details(net: &dyn Net, server: &Server, key: &Credential, ids: &[String]) -> Result<Vec<Track>, Failure> {
    match server.kind {
        Kind::Subsonic | Kind::Dlna => Ok(Vec::new()),
        Kind::Jellyfin => jellyfin::details(net, server, key, ids),
        Kind::Plex => plex::details(net, server, key, ids),
    }
}

/// How many tracks `details` is asked about at once.
pub const DETAIL_BATCH: usize = 50;

/// The playlists the account can read on the server, music only, at most
/// `PLAYLISTS`. `account` is who signed in: on a Subsonic server it tells
/// the account's own playlists, the ones it may change, from the others.
/// A kind without playlists (`Kind::has_playlists`) lists none, whole, and
/// nothing is asked of the server.
pub fn playlists(net: &dyn Net, server: &Server, key: &Credential, account: &str) -> Result<PlaylistListing, Failure> {
    match server.kind {
        Kind::Subsonic => subsonic::playlists::playlists(net, server, key, account),
        Kind::Jellyfin => jellyfin::playlists::playlists(net, server, key),
        Kind::Plex => plex::playlists::playlists(net, server, key),
        Kind::Dlna => Ok(PlaylistListing { found: Vec::new(), whole: true }),
    }
}

/// The entries of one playlist, in its order, at most `PLAYLIST_TRACKS`.
pub fn playlist_tracks(net: &dyn Net, server: &Server, key: &Credential, id: &str) -> Result<PlaylistTracks, Failure> {
    match server.kind {
        Kind::Subsonic => subsonic::playlists::playlist_tracks(net, server, key, id),
        Kind::Jellyfin => jellyfin::playlists::playlist_tracks(net, server, key, id),
        Kind::Plex => plex::playlists::playlist_tracks(net, server, key, id),
        Kind::Dlna => Err(playlist::not_offered()),
    }
}

/// Whether the account may change a playlist whose listing did not say
/// (`Playlist::editable` is `None`). Anything that is not a yes is a no.
pub fn playlist_editable(net: &dyn Net, server: &Server, key: &Credential, id: &str) -> bool {
    match server.kind {
        Kind::Jellyfin => jellyfin::playlists::playlist_editable(net, server, key, id),
        Kind::Subsonic | Kind::Plex | Kind::Dlna => false,
    }
}

/// Whether the server takes a playlist's description from Chilpa, and not
/// only its name.
pub fn playlist_describable(kind: Kind) -> bool {
    match kind {
        Kind::Subsonic | Kind::Plex => true,
        Kind::Jellyfin | Kind::Dlna => false,
    }
}

/// Whether the server makes a playlist only with tracks in it. Asked for an
/// empty one, Plex answers 400; its own apps make one only from something
/// to put in it.
pub fn playlist_needs_tracks(kind: Kind) -> bool {
    match kind {
        Kind::Plex => true,
        Kind::Subsonic | Kind::Jellyfin | Kind::Dlna => false,
    }
}

/// Asks the server for one change of a playlist, read just before as
/// `as_read` (a new one is `""`, read as nothing). Returns the id of a
/// playlist it created, when the server says it. A change made of several
/// requests stops at the first that fails: the playlist is read again to
/// know where it stands.
pub fn change_playlist(net: &dyn Net, server: &Server, key: &Credential, playlist: &str, as_read: &PlaylistTracks, change: &Change) -> Result<Option<String>, Failure> {
    match server.kind {
        Kind::Subsonic => subsonic::playlists::change_playlist(net, server, key, playlist, as_read, change),
        Kind::Jellyfin => jellyfin::playlists::change_playlist(net, server, key, playlist, as_read, change),
        Kind::Plex => plex::playlists::change_playlist(net, server, key, playlist, as_read, change),
        Kind::Dlna => Err(playlist::not_offered()),
    }
}

/// Where a track's original bytes are read from. `codec` and `revision`
/// are what the library kept of it.
pub fn stream(server: &Server, key: &Credential, id: &str, codec: Option<&str>, revision: Option<&str>) -> Stream {
    match server.kind {
        Kind::Subsonic => subsonic::stream(server, key, id, codec),
        Kind::Jellyfin => jellyfin::stream(server, key, id, codec),
        Kind::Plex => plex::stream(server, key, id, codec, revision),
        Kind::Dlna => dlna::stream(server, codec, revision),
    }
}

/// The request for a record's cover, at about that size in pixels.
pub fn cover(server: &Server, key: &Credential, cover: &str, size: u32) -> Request {
    match server.kind {
        Kind::Subsonic => subsonic::cover(server, key, cover, size),
        Kind::Jellyfin => jellyfin::cover(server, key, cover, size),
        Kind::Plex => plex::cover(server, key, cover, size),
        Kind::Dlna => dlna::cover(server, cover),
    }
}

/// An address as the user typed it, as the base every request is built on:
/// a scheme, no trailing slash.
pub fn normalize(address: &str) -> Result<String, Failure> {
    let address = address.trim().trim_end_matches('/');
    if address.is_empty() {
        return Err(Failure::Unexpected(chilpa_i18n::t("that is not a server address")));
    }
    let address = if address.contains("://") { address.to_string() } else { format!("http://{address}") };
    if !address.starts_with("http://") && !address.starts_with("https://") {
        return Err(Failure::Unexpected(chilpa_i18n::t("that is not a server address")));
    }
    // The address is kept in the library and shown in Settings: a user and
    // password in it, or a query, would be a credential in the open.
    let authority = address.split_once("://").map_or("", |(_, rest)| rest.split('/').next().unwrap_or(""));
    if authority.is_empty() || authority.contains('@') || address.contains(['?', '#']) || address.contains(char::is_whitespace) {
        return Err(Failure::Unexpected(chilpa_i18n::t("write the address alone: no user, password or query in it")));
    }
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_gets_a_scheme_and_loses_its_trailing_slash() {
        assert_eq!(normalize(" music.lan:4533/ ").unwrap(), "http://music.lan:4533");
        assert_eq!(normalize("https://jf.example.org/jellyfin/").unwrap(), "https://jf.example.org/jellyfin");
        assert!(normalize("").is_err());
        assert!(normalize("ftp://host").is_err());
        assert!(normalize("https://alice:secret@host/base").is_err(), "no credential in the address");
        assert!(normalize("http://host:4533/?u=alice&p=secret").is_err());
        assert!(normalize("http://host#x").is_err());
        assert_eq!(
            Failure::moved("https://alice:pw@new.example.org/rest/ping.view?u=alice&t=abc&s=salt").to_string(),
            Failure::Moved("https://new.example.org/rest/ping.view".into()).to_string(),
            "a redirect's error carries no credential"
        );
    }
}
