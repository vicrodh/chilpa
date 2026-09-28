//! Sending what is listened to, as it is listened to: to ListenBrainz and
//! to Last.fm.
//!
//! Each service has a thread of its own that does its network, so the
//! window never waits on it: the facade hands both what started playing and
//! what counted as a listen, and reads back a status for Settings. How a
//! listen waits when it cannot go out is the same for both and lives in
//! `queue`; what differs -- how an account is connected, what a request
//! looks like -- is each service's own module.

mod lastfm;
mod listenbrainz;
mod queue;

use queue::{Order, Service};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

/// A track that was listened to, or is playing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listen {
    pub artist: String,
    pub track: String,
    #[serde(default)]
    pub release: String,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub track_number: Option<u16>,
    #[serde(default)]
    pub recording_mbid: String,
    #[serde(default)]
    pub isrc: String,
    /// When it started, in unix seconds: the moment a listen is filed under.
    #[serde(default)]
    pub listened_at: i64,
}

/// Why a request did not go through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// The credential is not valid. Nothing goes until it is replaced.
    Token,
    /// Try again in that many seconds: no network, the server failing, its
    /// rate limit -- or, while an account is being connected, not allowed
    /// yet.
    Later(u64),
    /// The service refused what was sent. Sending it again changes nothing.
    Refused(String),
}

/// What kind of submission a request is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// What is playing now: shown on the profile, never counted.
    PlayingNow,
    /// One listen, as it happens.
    Single,
    /// Several listens kept while they could not be sent.
    Import,
}

/// What Settings shows about a service.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScrobbleStatus {
    pub enabled: bool,
    /// Who the account belongs to; empty when there is none.
    pub user: String,
    /// An account is being connected.
    pub checking: bool,
    /// Where the person allows Chilpa on the service's site, while
    /// connecting an account that asks for it.
    pub link: String,
    /// Listens kept until they can be sent.
    pub pending: usize,
    /// The last thing that went wrong, for the person to read.
    pub error: String,
}

/// One service's thread, from the facade's side.
struct Handle {
    orders: Sender<Order>,
    status: Arc<Mutex<ScrobbleStatus>>,
}

impl Handle {
    fn start(service: Service, dir: PathBuf, secrets: PathBuf) -> Self {
        let (orders, status) = queue::start(service, dir, secrets);
        Self { orders, status }
    }

    fn status(&self) -> ScrobbleStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn send(&self, order: Order) {
        let _ = self.orders.send(order);
    }

    fn connect(&self, credential: String) {
        if let Ok(mut s) = self.status.lock() {
            s.checking = true;
            s.error.clear();
        }
        self.send(Order::Connect(credential));
    }
}

/// The two services, as the facade keeps them.
pub struct Scrobbler {
    listenbrainz: Handle,
    lastfm: Handle,
}

impl Scrobbler {
    fn start(dir: PathBuf, secrets: PathBuf) -> Self {
        Self {
            listenbrainz: Handle::start(listenbrainz::SERVICE, dir.join("listenbrainz"), secrets.clone()),
            lastfm: Handle::start(lastfm::SERVICE, dir.join("lastfm"), secrets),
        }
    }

    fn both(&self, order: impl Fn() -> Order) {
        self.listenbrainz.send(order());
        self.lastfm.send(order());
    }
}

impl crate::Chilpa {
    /// Starts sending listens, to whichever services have an account. The
    /// window calls it once; nothing else does, so a test never opens the
    /// keyring or the network.
    pub fn start_scrobbling(&mut self) {
        if self.scrobbler.is_none() {
            self.scrobbler = Some(Scrobbler::start(chilpa_dirs::data(), Self::plugins_dir()));
        }
    }

    /// Where ListenBrainz stands, for Settings.
    pub fn listenbrainz(&self) -> ScrobbleStatus {
        self.scrobbler.as_ref().map(|s| s.listenbrainz.status()).unwrap_or_default()
    }

    /// Checks a user token and, if it is good, keeps it and starts sending.
    pub fn connect_listenbrainz(&mut self, token: &str) {
        self.start_scrobbling();
        if let Some(s) = &self.scrobbler {
            s.listenbrainz.connect(token.trim().to_string());
        }
    }

    pub fn disconnect_listenbrainz(&mut self) {
        if let Some(s) = &self.scrobbler {
            s.listenbrainz.send(Order::Disconnect);
        }
    }

    pub fn enable_listenbrainz(&mut self, on: bool) {
        if let Some(s) = &self.scrobbler {
            s.listenbrainz.send(Order::Enable(on));
        }
    }

    /// Where Last.fm stands, for Settings.
    pub fn lastfm(&self) -> ScrobbleStatus {
        self.scrobbler.as_ref().map(|s| s.lastfm.status()).unwrap_or_default()
    }

    /// Starts connecting a Last.fm account: the status carries the page
    /// where the person allows Chilpa, and the account is kept once they do.
    pub fn connect_lastfm(&mut self) {
        self.start_scrobbling();
        if let Some(s) = &self.scrobbler {
            s.lastfm.connect(String::new());
        }
    }

    pub fn disconnect_lastfm(&mut self) {
        if let Some(s) = &self.scrobbler {
            s.lastfm.send(Order::Disconnect);
        }
    }

    pub fn enable_lastfm(&mut self, on: bool) {
        if let Some(s) = &self.scrobbler {
            s.lastfm.send(Order::Enable(on));
        }
    }

    /// The listen a track that just started would be, told to the services
    /// as what is playing now. None when the tags do not name it.
    pub(crate) fn announce(
        &self,
        tags: Option<&chilpa_tags::Tags>,
        duration_ms: u64,
        started_at: i64,
    ) -> Option<Listen> {
        let t = tags?;
        let listen = Listen {
            artist: t.artist_line(),
            track: self.now.title.clone(),
            release: self.now.album.clone(),
            duration_ms,
            track_number: t.track_number,
            recording_mbid: t.musicbrainz_recording_id.clone().unwrap_or_default(),
            isrc: t.isrc.clone().unwrap_or_default(),
            listened_at: started_at,
        };
        if listen.artist.is_empty() || listen.track.is_empty() {
            return None;
        }
        if let Some(s) = &self.scrobbler {
            s.both(|| Order::Playing(listen.clone()));
        }
        Some(listen)
    }

    /// Sends the playing track's listen once enough of it has been heard:
    /// now, filed under when it started, and not when it ends -- a crash or
    /// a closed window after the mark would lose it otherwise.
    pub(crate) fn scrobble_if_heard(&mut self) {
        let Some(current) = self.in_progress.as_mut() else { return };
        if current.scrobbled || current.sample_rate_hz == 0 {
            return;
        }
        let played_ms = current.heard_ms();
        if counts(played_ms, current.duration_ms) {
            current.scrobbled = true;
            if let (Some(s), Some(listen)) = (&self.scrobbler, &current.listen) {
                s.both(|| Order::Listened(listen.clone()));
            }
        }
    }
}

/// Whether a listen that has played for `played_ms` of `duration_ms` counts:
/// half of it or four minutes, whichever comes first, and never a track of
/// half a minute or less, or of unknown length. Both services' rule.
pub(crate) fn counts(played_ms: u64, duration_ms: u64) -> bool {
    duration_ms > 30_000 && played_ms >= (duration_ms / 2).min(240_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listen_counts_at_half_or_four_minutes() {
        assert!(!counts(89_000, 180_000));
        assert!(counts(90_000, 180_000), "half of three minutes");
        assert!(counts(240_000, 1_200_000), "four minutes of twenty");
        assert!(!counts(239_000, 1_200_000));
        assert!(!counts(30_000, 30_000), "half a minute is not enough to count");
        assert!(!counts(500_000, 0), "a length nobody knows never counts");
    }
}
