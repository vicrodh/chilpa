//! The cover of the song a station says is on.
//!
//! Each lookup is a request to Chilpa's proxy, which every listener shares.
//! One thread does them, one at a time and at most one every [`GAP`], whatever
//! the stations say and however often. A song that arrives while it waits
//! replaces the one that was waiting, and is looked up when the wait is over.
//! An answer is remembered, so a reconnect gets its cover back without asking.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// The least time between two lookups: 4 a minute, against the proxy's 30.
const GAP: Duration = Duration::from_secs(15);
/// How long "it has no cover" stands before the song may be asked for again.
const NONE_FOR: Duration = Duration::from_secs(10 * 60);
/// Songs remembered: a few hours of a station, and a known limit.
const REMEMBERED: usize = 64;

/// A song to find the cover of.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Song {
    pub(super) url: String,
    /// The whole title the station sent, which is what an answer is matched by.
    pub(super) title: String,
    pub(super) artist: String,
    pub(super) name: String,
}

/// What the thread does next.
#[derive(Debug, PartialEq)]
enum Next {
    Lookup(Song),
    WaitFor(Duration),
    Sleep,
}

/// What the thread knows. Holds no thread and no clock, so it can be tested.
#[derive(Debug)]
struct Lookups {
    gap: Duration,
    pending: Option<Song>,
    last_lookup: Option<Instant>,
    known: VecDeque<(String, Option<String>, Instant)>,
}

impl Lookups {
    fn new(gap: Duration) -> Self {
        Self { gap, pending: None, last_lookup: None, known: VecDeque::new() }
    }

    /// A song is on. The cover when it is already known, or `None` after
    /// leaving the song to be looked up, unless it is known to have none.
    fn want(&mut self, song: Song, now: Instant) -> Option<String> {
        self.pending = None;
        match self.known.iter().find(|(title, ..)| *title == song.title) {
            Some((_, Some(path), _)) => Some(path.clone()),
            Some((_, None, at)) if now.saturating_duration_since(*at) < NONE_FOR => None,
            _ => {
                self.pending = Some(song);
                None
            }
        }
    }

    fn next(&mut self, now: Instant) -> Next {
        if self.pending.is_none() {
            return Next::Sleep;
        }
        if let Some(at) = self.last_lookup {
            let since = now.saturating_duration_since(at);
            if since < self.gap {
                return Next::WaitFor(self.gap - since);
            }
        }
        self.last_lookup = Some(now);
        self.pending.take().map_or(Next::Sleep, Next::Lookup)
    }

    fn answered(&mut self, title: &str, path: Option<String>, now: Instant) {
        self.known.retain(|(known, ..)| known != title);
        self.known.push_back((title.to_string(), path, now));
        while self.known.len() > REMEMBERED {
            self.known.pop_front();
        }
    }
}

/// The cover on disk, from Last.fm through the proxy.
pub(super) fn from_network(song: &Song) -> Option<String> {
    let address = crate::scrobble::lastfm::track_cover(&song.artist, &song.name)?;
    crate::pictures::keep(&address)
}

/// Starts the thread, which ends when the returned sender is dropped. `find`
/// goes to the network; `deliver` receives every cover found or remembered.
pub(super) fn start(
    find: impl Fn(&Song) -> Option<String> + Send + 'static,
    deliver: impl Fn(&Song, String) + Send + 'static,
) -> Option<Sender<Song>> {
    start_with(GAP, find, deliver)
}

fn start_with(
    gap: Duration,
    find: impl Fn(&Song) -> Option<String> + Send + 'static,
    deliver: impl Fn(&Song, String) + Send + 'static,
) -> Option<Sender<Song>> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new().name("chilpa-song-cover".into())
        .spawn(move || run(rx, gap, find, deliver)).ok()?;
    Some(tx)
}

fn run(rx: Receiver<Song>, gap: Duration, find: impl Fn(&Song) -> Option<String>, deliver: impl Fn(&Song, String)) {
    let mut lookups = Lookups::new(gap);
    let take = |lookups: &mut Lookups, song: Song| {
        if let Some(path) = lookups.want(song.clone(), Instant::now()) {
            deliver(&song, path);
        }
    };
    loop {
        match lookups.next(Instant::now()) {
            Next::Lookup(song) => {
                let path = find(&song);
                lookups.answered(&song.title, path.clone(), Instant::now());
                if let Some(path) = path {
                    deliver(&song, path);
                }
            }
            Next::WaitFor(wait) => match rx.recv_timeout(wait) {
                Ok(song) => take(&mut lookups, song),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            },
            Next::Sleep => match rx.recv() {
                Ok(song) => take(&mut lookups, song),
                Err(_) => return,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn song(title: &str) -> Song {
        Song { url: "https://radio.example/a".into(), title: title.into(), artist: "A".into(), name: title.into() }
    }

    #[test]
    fn a_title_sent_many_times_a_second_for_an_hour_asks_once() {
        let (mut lookups, start) = (Lookups::new(GAP), Instant::now());
        let mut asked = 0;
        for tick in 0..30 * 60 * 60u64 {
            let now = start + Duration::from_millis(tick * 33);
            if lookups.want(song("A - One"), now).is_none() {
                if let Next::Lookup(found) = lookups.next(now) {
                    asked += 1;
                    lookups.answered(&found.title, Some("cover".into()), now);
                }
            }
        }
        assert_eq!(asked, 1);
    }

    #[test]
    fn a_song_without_a_cover_is_asked_for_six_times_an_hour_at_most() {
        let (mut lookups, start) = (Lookups::new(GAP), Instant::now());
        let mut asked = 0;
        for second in 0..3_600u64 {
            let now = start + Duration::from_secs(second);
            lookups.want(song("A - One"), now);
            if let Next::Lookup(found) = lookups.next(now) {
                asked += 1;
                lookups.answered(&found.title, None, now);
            }
        }
        assert_eq!(asked, 6);
    }

    #[test]
    fn titles_and_stations_changing_every_second_stay_under_the_gap() {
        let (mut lookups, start) = (Lookups::new(GAP), Instant::now());
        let mut asked = 0;
        for second in 0..3_600u64 {
            let now = start + Duration::from_secs(second);
            let mut each = song(&format!("A - {second}"));
            each.url = format!("https://radio.example/{}", second % 2);
            lookups.want(each, now);
            if let Next::Lookup(found) = lookups.next(now) {
                asked += 1;
                lookups.answered(&found.title, None, now);
            }
        }
        assert_eq!(asked, 240);
    }

    #[test]
    fn a_song_that_arrives_during_the_wait_is_looked_up_when_it_ends() {
        let (mut lookups, start) = (Lookups::new(GAP), Instant::now());
        lookups.want(song("A - One"), start);
        assert_eq!(lookups.next(start), Next::Lookup(song("A - One")));
        let soon = start + Duration::from_secs(5);
        lookups.want(song("B - Two"), soon);
        assert_eq!(lookups.next(soon), Next::WaitFor(Duration::from_secs(10)));
        let later = start + GAP;
        assert_eq!(lookups.next(later), Next::Lookup(song("B - Two")));
        assert_eq!(lookups.next(later), Next::Sleep);
    }

    #[test]
    fn only_the_song_on_now_is_looked_up_after_the_wait() {
        let (mut lookups, start) = (Lookups::new(GAP), Instant::now());
        lookups.want(song("A - One"), start);
        lookups.next(start);
        lookups.want(song("B - Two"), start);
        lookups.want(song("C - Three"), start);
        assert_eq!(lookups.next(start + GAP), Next::Lookup(song("C - Three")));
    }

    #[test]
    fn a_known_cover_comes_back_without_asking_and_replaces_a_waiting_song() {
        let (mut lookups, start) = (Lookups::new(GAP), Instant::now());
        lookups.answered("A - One", Some("cover".into()), start);
        lookups.want(song("B - Two"), start);
        assert_eq!(lookups.want(song("A - One"), start).as_deref(), Some("cover"));
        assert_eq!(lookups.next(start + GAP), Next::Sleep);
    }

    #[test]
    fn what_is_remembered_has_a_limit() {
        let (mut lookups, start) = (Lookups::new(GAP), Instant::now());
        for each in 0..REMEMBERED + 10 {
            lookups.answered(&format!("A - {each}"), None, start);
        }
        assert_eq!(lookups.known.len(), REMEMBERED);
    }

    #[test]
    fn the_thread_asks_the_network_once_for_a_flood_and_ends_with_its_sender() {
        let asked = Arc::new(Mutex::new(0));
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let (counter, covers) = (asked.clone(), delivered.clone());
        let tx = start_with(Duration::from_millis(50), move |_| {
            *counter.lock().unwrap() += 1;
            Some("cover".into())
        }, move |song, path| covers.lock().unwrap().push((song.title.clone(), path))).unwrap();
        for _ in 0..1_000 {
            tx.send(song("A - One")).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while delivered.lock().unwrap().len() < 1_000 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(*asked.lock().unwrap(), 1);
        assert_eq!(delivered.lock().unwrap().len(), 1_000, "the known cover is handed back each time");
        drop(tx);
    }
}
