//! The network the servers' protocols are handed, and how a server stored
//! in the library becomes one a request can be built for.

use chilpa_models::SourceId;
use chilpa_servers::{Client, Details, Kind, Net, Reply, Request, Server};

/// `chilpa_core::call_server`: plain http allowed, no redirect followed,
/// the system's certificates (ADR-017 §7-8).
pub(crate) struct Network;

/// The largest answer taken from a server: a page of its catalogue, or a
/// cover.
const MAX_REPLY: usize = 32 * 1024 * 1024;

impl Net for Network {
    fn send(&self, request: &Request) -> Result<Reply, String> {
        let body = request.body.as_ref().map(|(kind, bytes)| (kind.as_str(), bytes.as_slice()));
        let call = chilpa_core::Call {
            method: request.method,
            url: &request.url,
            headers: &request.headers,
            body,
            timeout: request.timeout,
            max_bytes: MAX_REPLY,
        };
        match chilpa_core::call_server(&call) {
            Ok(reply) => Ok(Reply { status: reply.status, headers: reply.headers, body: reply.body }),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// How Chilpa introduces itself to one server. The device id is the
/// server's instance: kept, and different for each server.
pub(crate) fn client(instance: &str) -> Client {
    let platform = match std::env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        _ => "Linux",
    };
    Client {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: platform.to_string(),
        device: format!("Chilpa ({platform})"),
        id: instance.to_string(),
    }
}

/// A server as the library keeps it, as the protocols want it.
pub(crate) fn server(stored: &chilpa_library::StoredServer) -> Option<Server> {
    Some(Server {
        kind: Kind::from_provider(&stored.provider)?,
        address: stored.address.clone(),
        client: client(&stored.instance),
        libraries: serde_json::from_str(&stored.libraries).unwrap_or_default(),
        details: serde_json::from_str::<Details>(&stored.details).unwrap_or_default(),
    })
}

/// The queue entry of a server's track: what the library keeps as its
/// `external_id`.
pub(crate) fn entry(stored: &chilpa_library::StoredServer, id: &str) -> String {
    chilpa_models::RemoteEntry::new(SourceId::new(stored.provider.clone(), stored.instance.clone()), id).to_entry()
}

/// A protocol's track in the library's words.
pub(crate) fn track(stored: &chilpa_library::StoredServer, t: chilpa_servers::Track) -> chilpa_library::ServerTrack {
    chilpa_library::ServerTrack {
        external_id: entry(stored, &t.id),
        title: t.title,
        artist: t.artist,
        album: t.album,
        album_artist: t.album_artist,
        album_key: t.album_id,
        cover: t.cover,
        disc_number: t.disc,
        disc_total: t.discs,
        track_number: t.number,
        year: t.year,
        genres: t.genres,
        duration_ms: t.duration_ms,
        codec: t.codec,
        bit_depth: t.bits,
        sample_rate_hz: t.rate,
        channels: t.channels,
        isrc: t.isrc,
        recording_mbid: t.recording_mbid,
        revision: t.revision,
        added_at: t.added_at,
    }
}
