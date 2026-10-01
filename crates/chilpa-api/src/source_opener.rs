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

pub(crate) struct Openers {
    pub(crate) servers: Option<Arc<dyn RemoteOpener>>,
    pub(crate) plugins: Option<Arc<Mutex<Plugins>>>,
    library: Mutex<Option<chilpa_library::Library>>,
}

impl Openers {
    pub(crate) fn new(servers: Option<Arc<dyn RemoteOpener>>, plugins: Option<Arc<Mutex<Plugins>>>) -> Self {
        Self { servers, plugins, library: Mutex::new(None) }
    }

    fn with_library<T>(&self, f: impl FnOnce(&chilpa_library::Library) -> Option<T>) -> Option<T> {
        let mut library = self.library.lock().unwrap_or_else(|e| e.into_inner());
        if library.is_none() {
            *library = chilpa_core::open_library().ok();
        }
        f(library.as_ref()?)
    }

    fn locate_in_plugin(&self, entry: &RemoteEntry) -> Result<Located, String> {
        let plugin = &entry.source.instance;
        let Some(plugins) = &self.plugins else {
            return Err(chilpa_i18n::t("the plugin sandbox is not started"));
        };
        let ask = messages::write(&Resolve::new(&entry.external_id));
        let (answer, name) = {
            let mut plugins = plugins.lock().map_err(|_| chilpa_i18n::t("the plugin sandbox is not started"))?;
            let name = plugins.panels().into_iter().find(|p| &p.0 == plugin).map(|p| p.1).unwrap_or_default();
            (plugins.call_waiting(plugin, messages::op::RESOLVER, &ask), name)
        };
        let stream: Stream = messages::read(&answer?)?;
        let url = crate::sources::playable_url(&stream)?;
        if url.contains(' ') {
            return Err(chilpa_i18n::t("it comes in several parts, which cannot be read as one file"));
        }
        Ok(Located {
            request: chilpa_core::RangedRequest { url: final_address(&url), headers: Vec::new() },
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
fn final_address(address: &str) -> String {
    let mut at = address.to_string();
    for _ in 0..HOPS {
        let range = [("Range".to_string(), "bytes=0-0".to_string())];
        let asked = chilpa_core::Call {
            method: "GET",
            url: &at,
            headers: &range,
            body: None,
            timeout: Duration::from_secs(10),
            max_bytes: 64 * 1024,
        };
        let Ok(reply) = chilpa_core::call_server(&asked) else { return at };
        let Some(next) = reply.headers.get("location").filter(|_| (300..400).contains(&reply.status)) else {
            return at;
        };
        at = joined(&at, next);
    }
    at
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
}
