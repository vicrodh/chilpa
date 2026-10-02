//! What the player asks when something of a plugin comes up in the queue:
//! where its bytes are and what to call it (`chilpa_core::remote`).
//!
//! An episode goes in the queue as a remote entry, like a track on a
//! server: named by its plugin and its id, never by an address. The address
//! is asked of the plugin when it plays, so a link that expired while it
//! waited in the queue is simply asked for again, and a saved queue carries
//! nothing that goes stale.
//!
//! The player holds one opener. This is that one: it sends what belongs to
//! a plugin here and the rest to the servers' opener.

use crate::plugins::Plugins;
use chilpa_core::remote::{Described, Located, RemoteOpener};
use chilpa_models::{RemoteEntry, SourceId};
use chilpa_plugin::messages::{self, Resolve, Stream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The provider a plugin's entries carry. The instance is the plugin's id.
pub(crate) const PROVIDER: &str = "plugin";

/// How many redirects are followed to reach a file.
const HOPS: usize = 10;

/// The queue entry of something a plugin listed.
pub(crate) fn entry_of(plugin: &str, external_id: &str) -> String {
    RemoteEntry::new(SourceId::new(PROVIDER, plugin), external_id).to_entry()
}

/// The plugin and the id of a queue entry, when it is a plugin's.
pub(crate) fn of_plugin(path: &str) -> Option<(String, String)> {
    let entry = RemoteEntry::parse(path)?;
    (entry.source.provider == PROVIDER).then_some((entry.source.instance, entry.external_id))
}

/// How long an address resolved ahead is used. A source's signed address
/// lasts minutes; this is well inside that.
const AHEAD_FOR: Duration = Duration::from_secs(120);

pub(crate) struct Openers {
    pub(crate) servers: Option<Arc<dyn RemoteOpener>>,
    pub(crate) plugins: Option<Arc<Mutex<Plugins>>>,
    catalog: Arc<crate::source_catalog::Catalog>,
    library: Mutex<Option<chilpa_library::Library>>,
    /// Addresses asked for before their track came up, by queue entry: the
    /// next track's is resolved while the one before it ends, so the gap
    /// between them is not a round trip to the plugin.
    ahead: Mutex<std::collections::HashMap<String, (std::time::Instant, Located)>>,
}

impl Openers {
    pub(crate) fn new(servers: Option<Arc<dyn RemoteOpener>>, plugins: Option<Arc<Mutex<Plugins>>>, catalog: Arc<crate::source_catalog::Catalog>) -> Self {
        Self { servers, plugins, catalog, library: Mutex::new(None), ahead: Mutex::new(std::collections::HashMap::new()) }
    }

    /// Resolves a plugin's queue entry on a thread of its own and keeps the
    /// address for when the player asks for it.
    pub(crate) fn resolve_ahead(self: &Arc<Self>, path: String) {
        let Some(entry) = RemoteEntry::parse(&path).filter(|e| e.source.provider == PROVIDER) else { return };
        let me = self.clone();
        let _ = std::thread::Builder::new().name("chilpa-resolve-ahead".into()).spawn(move || {
            let started = std::time::Instant::now();
            let found = me.locate_in_plugin(&entry);
            eprintln!("resolve-ahead: ok={} elapsed_ms={}", found.is_ok(), started.elapsed().as_millis());
            if let Ok(located) = found {
                me.ahead.lock().unwrap_or_else(|e| e.into_inner()).insert(path, (std::time::Instant::now(), located));
            }
        });
    }

    fn take_ahead(&self, entry: &RemoteEntry) -> Option<Located> {
        let mut ahead = self.ahead.lock().unwrap_or_else(|e| e.into_inner());
        ahead.retain(|_, (at, _)| at.elapsed() < AHEAD_FOR);
        ahead.remove(&entry.to_entry()).map(|(_, located)| located)
    }

    fn with_library<T>(&self, f: impl FnOnce(&chilpa_library::Library) -> Option<T>) -> Option<T> {
        let mut library = self.library.lock().unwrap_or_else(|e| e.into_inner());
        if library.is_none() {
            *library = chilpa_library::Library::open_read_only(&chilpa_library::Library::default_path()).ok();
        }
        f(library.as_ref()?)
    }

    /// A process plugin gives the address itself, signed and direct: it is
    /// asked without the sandbox lock and its redirects are not walked.
    fn locate_in_native(&self, natives: &std::sync::Arc<crate::natives::Natives>, plugin: &str, reference: &str) -> Result<Located, String> {
        let answer = natives.call(plugin, "track.resolve", serde_json::json!({"ref": reference, "purpose": "play",
            "output": {"max_rate": 192_000, "max_bits": 24}}), Duration::from_secs(20))?;
        let url = answer.get("url").and_then(serde_json::Value::as_str)
            .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
            .ok_or_else(|| chilpa_i18n::t("the source gave no address to play"))?;
        Ok(Located {
            request: chilpa_core::RangedRequest { url: url.to_string(), headers: Vec::new() },
            extension: answer.pointer("/format/codec").and_then(serde_json::Value::as_str).filter(|c| !c.is_empty() && *c != "unknown").map(str::to_string),
            server: self.catalog.name(plugin).unwrap_or_else(|| plugin.to_string()),
        })
    }

    fn locate_in_plugin(&self, entry: &RemoteEntry) -> Result<Located, String> {
        let plugin = &entry.source.instance;
        if let Some(natives) = self.catalog.natives().filter(|natives| natives.is_native(plugin)) {
            return self.locate_in_native(&natives, plugin, &entry.external_id);
        }
        let Some(plugins) = &self.plugins else {
            return Err(chilpa_i18n::t("the plugin sandbox is not started"));
        };
        let ask = messages::write(&Resolve::new(&entry.external_id));
        let (answer, name, domains, requests) = {
            let mut plugins = plugins.lock().map_err(|_| chilpa_i18n::t("the plugin sandbox is not started"))?;
            let name = plugins.panels().into_iter().find(|p| &p.0 == plugin).map(|p| p.1).unwrap_or_default();
            let domains = plugins.worker_domains(plugin);
            (plugins.call_waiting(plugin, messages::op::RESOLVER, &ask), name, domains, plugins.network_requests.clone())
        };
        let stream: Stream = messages::read(&answer?)?;
        let url = crate::sources::playable_url(&stream)?;
        if url.contains(' ') {
            return Err(chilpa_i18n::t("it comes in several parts, which cannot be read as one file"));
        }
        Ok(Located {
            request: chilpa_core::RangedRequest { url: final_address_with(&url, domains.as_deref(), |address| {
                if let Some(host) = url::Url::parse(address).ok().and_then(|url| url.host_str().map(str::to_string)) {
                    if let Ok(mut requests) = requests.lock() {
                        let pending = requests.entry(plugin.clone()).or_default();
                        if pending.len() < 128 { pending.insert(host); }
                    }
                }
            })?, headers: Vec::new() },
            extension: stream.codec.map(|c| c.to_ascii_lowercase()).filter(|c| !c.is_empty()),
            server: name,
        })
    }
}

/// Where an address ends up once its redirects are followed.
///
/// The reader of remote files follows none, on purpose: for a server of
/// mine a redirect would carry a credential somewhere else. A published
/// file is the opposite case -- it carries no credential and is reached
/// through several redirects -- so they are followed here, before the
/// address is handed over. Whatever goes wrong leaves the address as it
/// was: opening it will say what is the matter.
#[cfg(test)]
fn final_address(address: &str, domains: Option<&[String]>) -> Result<String, String> {
    final_address_with(address, domains, |_| {})
}

fn final_address_with(address: &str, domains: Option<&[String]>, denied: impl Fn(&str)) -> Result<String, String> {
    let mut at = address.to_string();
    for _ in 0..HOPS {
        if domains.is_some_and(|domains| !chilpa_core::worker_url_allowed(&at, domains)) {
            denied(&at);
            return Err(chilpa_i18n::t("the stream needs a network domain that this plugin was not granted"));
        }
        let range = [("Range".to_string(), "bytes=0-0".to_string())];
        let asked = chilpa_core::Call {
            method: "GET",
            url: &at,
            headers: &range,
            body: None,
            timeout: Duration::from_secs(10),
            max_bytes: 64 * 1024,
        };
        let Ok(reply) = chilpa_core::call_server(&asked) else { return Ok(at) };
        let Some(next) = reply.headers.get("location").filter(|_| (300..400).contains(&reply.status)) else {
            return Ok(at);
        };
        at = joined(&at, next);
    }
    if domains.is_some_and(|domains| !chilpa_core::worker_url_allowed(&at, domains)) {
        denied(&at);
        return Err(chilpa_i18n::t("the stream needs a network domain that this plugin was not granted"));
    }
    Ok(at)
}

/// A redirect's target, which may be given relative to where it came from.
fn joined(from: &str, to: &str) -> String {
    if to.starts_with("http://") || to.starts_with("https://") {
        return to.to_string();
    }
    let Some((scheme, rest)) = from.split_once("://") else { return to.to_string() };
    let host = rest.split('/').next().unwrap_or(rest);
    if let Some(path) = to.strip_prefix("//") {
        return format!("{scheme}://{path}");
    }
    if to.starts_with('/') {
        return format!("{scheme}://{host}{to}");
    }
    let folder = from.rsplit_once('/').map(|(f, _)| f).unwrap_or(from);
    format!("{folder}/{to}")
}

impl RemoteOpener for Openers {
    fn locate(&self, entry: &RemoteEntry) -> Result<Located, String> {
        if entry.source.provider == PROVIDER {
            if let Some(located) = self.take_ahead(entry) {
                return Ok(located);
            }
            return self.locate_in_plugin(entry);
        }
        match &self.servers {
            Some(servers) => servers.locate(entry),
            None => Err(chilpa_i18n::t("this track is on a server, and servers are not connected")),
        }
    }

    fn describe(&self, entry: &RemoteEntry) -> Described {
        if entry.source.provider != PROVIDER {
            return self.servers.as_ref().map(|s| s.describe(entry)).unwrap_or_default();
        }
        // What the catalog read of it: its record, its artist and its cover.
        if let Some((track, art)) = self.catalog.track_by_path(&entry.to_entry()) {
            let album_artist = self.catalog.cards(&[track.album_id]).pop().map(|c| c.artist).filter(|a| !a.is_empty());
            return Described { tags: chilpa_tags::Tags {
                title: Some(track.title), artists: if track.artist.is_empty() { Vec::new() } else { vec![track.artist] },
                album: Some(track.album).filter(|a| !a.is_empty()), album_artist,
                ..Default::default()
            }, artwork: Some(art).filter(|a| !a.is_empty()).map(std::path::PathBuf::from) };
        }
        if let Some((title, artist, album, album_artist, artwork)) = self.with_library(|lib| lib.server_track_card(&entry.to_entry()).ok().flatten()) {
            return Described { tags: chilpa_tags::Tags {
                title: Some(title), artists: if artist.is_empty() { Vec::new() } else { vec![artist] },
                album: Some(album).filter(|a| !a.is_empty()), album_artist: Some(album_artist).filter(|a| !a.is_empty()),
                ..Default::default()
            }, artwork: artwork.map(std::path::PathBuf::from) };
        }
        let heard = self.with_library(|lib| lib.heard(&entry.source.instance, &entry.external_id).ok().flatten());
        let Some(heard) = heard else { return Described::default() };
        let tags = chilpa_tags::Tags {
            title: Some(heard.title),
            artists: if heard.subtitle.is_empty() { Vec::new() } else { vec![heard.subtitle] },
            ..Default::default()
        };
        // Only a picture already on disk: this runs on the player's thread,
        // which does not wait on the network for a cover.
        let artwork = crate::pictures::kept(&heard.image_url).map(std::path::PathBuf::from);
        Described { tags, artwork }
    }

    fn failed(&self, entry: &RemoteEntry, why: &str) {
        if entry.source.provider != PROVIDER {
            if let Some(servers) = &self.servers {
                servers.failed(entry, why);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_of_a_plugin_round_trips_and_a_server_s_is_not_one() {
        let path = entry_of("app.example.shows", "ep:https://a.b/c d.mp3?x=1&y=2");
        assert_eq!(of_plugin(&path), Some(("app.example.shows".into(), "ep:https://a.b/c d.mp3?x=1&y=2".into())));
        let server = RemoteEntry::new(SourceId::new("example-server", "abc"), "tr-1").to_entry();
        assert_eq!(of_plugin(&server), None);
        assert_eq!(of_plugin("/music/a.flac"), None);
    }

    #[test]
    fn a_redirect_is_joined_to_where_it_came_from() {
        let from = "https://a.b/x/y.mp3?q=1";
        assert_eq!(joined(from, "https://c.d/z.mp3"), "https://c.d/z.mp3");
        assert_eq!(joined(from, "//c.d/z.mp3"), "https://c.d/z.mp3");
        assert_eq!(joined(from, "/z.mp3"), "https://a.b/z.mp3");
        assert_eq!(joined(from, "z.mp3"), "https://a.b/x/z.mp3");
    }

    #[test]
    fn a_worker_stream_cannot_redirect_outside_its_approved_domains() {
        assert!(final_address("https://denied.example/audio", Some(&[])).is_err());
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/audio", server.server_addr());
        let thread = std::thread::spawn(move || {
            let request = server.recv_timeout(Duration::from_secs(3)).unwrap().unwrap();
            let response = tiny_http::Response::empty(302).with_header(
                tiny_http::Header::from_bytes("Location", "https://denied.example/audio").unwrap());
            request.respond(response).unwrap();
        });
        let result = final_address(&url, Some(&["127.0.0.1".into()]));
        thread.join().unwrap();
        assert!(result.unwrap_err().contains("not granted"));
    }
}
