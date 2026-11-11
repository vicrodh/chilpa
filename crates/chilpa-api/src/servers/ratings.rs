//! The listener's ratings on the servers whose kind keeps them
//! (`chilpa_servers::rating`): read with the catalogue, written one change
//! at a time, and turned off per server from its row in Settings.
//!
//! A sync reads each track's rating with its page, and each record's with
//! the page or, when the track listing does not carry it, from a listing of
//! the records after the catalogue, within `RECORD_PAGES` and
//! `RECORD_TIME`. What it read goes into the library when the catalogue
//! ends, whole or not: a reading that did not cover everything is taken in
//! for what it covered, and does not count as the first whole one.

use super::keys::Keys;
use super::net;
use super::sync::Event;
use crate::ratings::scale::Scale;
use crate::ratings::sync::server_origin;
use crate::ratings::writer::Delivery;
use crate::Chilpa;
use chilpa_library::store::rating_sync::{Holder, Incoming, Pending};
use chilpa_servers::rating::{RatingRead, Rated};
use chilpa_servers::{Failure, Net};
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// What a listing of the records may take after the catalogue.
const RECORD_PAGES: usize = 400;
const RECORD_TIME: Duration = Duration::from_secs(120);

/// The network the writer sends with: the servers' own.
pub(crate) fn network() -> net::Network {
    net::Network
}

/// The library, for the writer's thread.
pub(crate) fn open_library() -> Option<chilpa_library::Library> {
    super::sync::open_library()
}

/// The ratings a sync read, until the catalogue ends.
pub(crate) struct Reads {
    origin: String,
    scale: Scale,
    /// What the server rated when it was last read: only for these does an
    /// unrated reading say something.
    rated: HashSet<(String, String)>,
    tracks: Vec<(i64, RatingRead)>,
    albums: Vec<(i64, RatingRead)>,
}

impl Reads {
    /// `None` when that server's kind keeps no ratings or they are not
    /// synced with it.
    pub(crate) fn begin(lib: &chilpa_library::Library, source: i64) -> Option<Reads> {
        let stored = lib.server(source).ok().flatten()?;
        let scale = chilpa_servers::Kind::from_provider(&stored.provider)?.rating_scale().and_then(Scale::of_server)?;
        let origin = server_origin(&stored.instance);
        if !lib.rating_sync_enabled(&origin).ok()? {
            return None;
        }
        let rated = lib.remote_rated(&origin).ok()?;
        Some(Reads { origin, scale, rated, tracks: Vec::new(), albums: Vec::new() })
    }

    /// A page's ratings, read from a request asked at `read_ms`.
    pub(crate) fn take(&mut self, read_ms: i64, ratings: Vec<RatingRead>) {
        for read in ratings {
            let kind = read.rated.name();
            if read.value <= 0.0 && !self.rated.contains(&(kind.to_string(), read.id.clone())) {
                continue;
            }
            match read.rated {
                Rated::Track => self.tracks.push((read_ms, read)),
                Rated::Album => self.albums.push((read_ms, read)),
            }
        }
    }

    fn incoming(&self, reads: &[(i64, RatingRead)], local: impl Fn(&str) -> String) -> Vec<Incoming> {
        reads.iter()
            .map(|(read_ms, read)| Incoming { id: read.id.clone(), local: local(&read.id), raw: read.value, stars: self.scale.stars(read.value), read_ms: *read_ms })
            .collect()
    }
}

/// Takes in what a sync read, once its catalogue ended with `event`, and
/// the records' ratings the track listing did not carry.
pub(crate) fn finish(lib: &chilpa_library::Library, net: &dyn Net, keys: &Keys, source: i64, mut reads: Reads, event: &Event, stopped: &dyn Fn() -> bool) {
    let Some(stored) = lib.server(source).ok().flatten() else { return };
    let Some(kind) = chilpa_servers::Kind::from_provider(&stored.provider) else { return };
    let whole = matches!(event, Event::Finished { .. });
    let (holder, origin) = (Holder::Server(source), reads.origin.clone());
    let mut changed = 0;
    let mut keep = |kind: &str, incoming: Vec<Incoming>, whole: bool| {
        if incoming.is_empty() && !whole {
            return;
        }
        match lib.apply_remote_ratings(&origin, &holder, kind, &incoming, whole) {
            Ok(applied) => changed += applied.changed,
            Err(error) => eprintln!("chilpa-servers: {kind} ratings of source {source} could not be kept: {error}"),
        }
    };
    keep("track", reads.incoming(&reads.tracks, |id| net::entry(&stored, id)), whole);
    let unreachable = matches!(event, Event::Failed { unreachable: true, .. });
    if !kind.lists_record_ratings_apart() {
        keep("album", reads.incoming(&reads.albums, str::to_string), whole);
    } else if !unreachable && !stopped() {
        if let (Some(server), Some(key)) = (net::server(&stored), net::key(keys, &stored)) {
            let asked_ms = crate::ratings::sync::now_ms();
            match chilpa_servers::rating::album_ratings(net, &server, &key, RECORD_PAGES, Instant::now() + RECORD_TIME) {
                Ok(listed) => {
                    reads.albums.clear();
                    reads.take(asked_ms, listed.found);
                    keep("album", reads.incoming(&reads.albums, str::to_string), listed.whole);
                }
                Err(failure) => eprintln!("chilpa-servers: record ratings of source {source} could not be read: {failure}"),
            }
        }
    }
    if changed > 0 {
        eprintln!("chilpa-servers: sync of source {source}: {changed} ratings changed on the server");
    }
}

/// Sends one change to the server of that instance.
pub(crate) fn deliver(lib: &chilpa_library::Library, network: &dyn Net, keys: &Keys, instance: &str, change: &Pending) -> Delivery {
    let Some(stored) = lib.servers().ok().and_then(|all| all.into_iter().find(|s| s.instance == instance)) else {
        return Delivery::Never(chilpa_i18n::t("that server is no longer in the library"));
    };
    if !stored.enabled {
        return Delivery::Later(chilpa_i18n::t("that server is disabled"));
    }
    let Some(kind) = chilpa_servers::Kind::from_provider(&stored.provider) else {
        return Delivery::Never(chilpa_i18n::t("that server is no longer in the library"));
    };
    let (Some(scale), Some(rated)) = (kind.rating_scale().and_then(Scale::of_server), Rated::from_name(&change.kind)) else {
        return Delivery::Never(chilpa_i18n::t("this server keeps no ratings Chilpa can change"));
    };
    let (Some(server), Some(key)) = (net::server(&stored), net::key(keys, &stored)) else {
        return Delivery::Later(chilpa_i18n::t_args("sign in to {} again", &[&stored.label]));
    };
    let value = scale.value(change.stars);
    match chilpa_servers::rating::rate(network, &server, &key, rated, &change.id, value) {
        Ok(()) => Delivery::Sent(value.unwrap_or(0.0)),
        Err(Failure::Gone(why)) => Delivery::Never(why),
        Err(failure) => Delivery::Later(failure.to_string()),
    }
}

/// The settings row value that turns it off or on.
const OFF: &str = "ratings-off";
const ON: &str = "ratings-on";

impl Chilpa {
    /// A server's row in Settings: its menu turns rating sync on or off,
    /// and its description says when it is off or when changes wait.
    pub(crate) fn server_rating_row(&mut self, source: i64, description: &mut String, values: &mut Vec<String>, options: &mut Vec<String>) {
        let Some(lib) = self.library() else { return };
        let Some(stored) = lib.server(source).ok().flatten() else { return };
        if chilpa_servers::Kind::from_provider(&stored.provider).and_then(chilpa_servers::Kind::rating_scale).is_none() {
            return;
        }
        let origin = server_origin(&stored.instance);
        let on = lib.rating_sync_enabled(&origin).unwrap_or(true);
        values.push(if on { OFF } else { ON }.into());
        options.push(if on { chilpa_i18n::t("Stop syncing ratings") } else { chilpa_i18n::t("Sync ratings") });
        if !on {
            description.push(' ');
            description.push_str(&chilpa_i18n::t("Ratings are not synced with it."));
        } else if let Some((count, why)) = self.rating_failures().remove(&origin) {
            let n = count.to_string();
            description.push(' ');
            description.push_str(&chilpa_i18n::tf(
                "{} rating waits to reach it: {}",
                "{} ratings wait to reach it: {}",
                count as i64,
                &[&n, &why],
            ));
        }
    }

    /// Settings: a server's menu turned rating sync on or off. Turned on,
    /// the server is read again, which brings its ratings.
    pub(crate) fn server_ratings_setting(&mut self, key: &str, value: &str) -> bool {
        let Some(source) = key.strip_prefix("sources.server/").and_then(|id| id.parse::<i64>().ok()) else { return false };
        if value != ON && value != OFF {
            return false;
        }
        let Some(stored) = self.library().and_then(|lib| lib.server(source).ok().flatten()) else { return false };
        let origin = server_origin(&stored.instance);
        let kept = self.library().is_some_and(|lib| lib.set_rating_sync(&origin, value == ON).is_ok());
        if !kept {
            self.now.error = chilpa_i18n::t("the library could not be written");
            return true;
        }
        if value == ON {
            self.sync_server(source);
        }
        self.servers.ratings.due_now();
        true
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod live;
