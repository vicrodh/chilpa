//! The cover of the song a station says is on.
//!
//! Each lookup is a request to Chilpa's proxy, which every listener shares.
//! One thread does them, one at a time and at most one every [`GAP`], whatever
//! the stations say and however often. A song that arrives while it waits or
//! while a lookup runs replaces the one that was waiting. An answer is
//! remembered, so a reconnect gets its cover back without asking. A lookup that
//! failed says nothing about the song and is tried again, a few times.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::{Duration, Instant};

/// The least time between two lookups: 4 a minute, against the proxy's 30.
const GAP: Duration = Duration::from_secs(15);
/// How long "it has no cover" stands before the song may be asked for again.
const NONE_FOR: Duration = Duration::from_secs(10 * 60);
/// Songs remembered: a few hours of a station, and a known limit.
const REMEMBERED: usize = 64;
/// Lookups of one song before a failing network counts as no cover for now.
const ATTEMPTS: u32 = 3;
/// How long a failed lookup waits for the next attempt.
const RETRY_AFTER: Duration = Duration::from_secs(60);

/// A song to find the cover of.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Song {
    pub(super) url: String,
    /// The whole title the station sent, which is what an answer is matched by.
    pub(super) title: String,
    pub(super) artist: String,
    pub(super) name: String,
}

/// What a lookup gave.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Found {
    Cover(String),
    /// The other side answered, and it has none.
    Nothing,
    /// No answer about the song: no network, a limit, a download that failed.
    Failed,
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
    not_before: Option<Instant>,
    failing: Option<(String, u32)>,
    last_lookup: Option<Instant>,
    known: VecDeque<(String, Option<String>, Instant)>,
}

impl Lookups {
    fn new(gap: Duration) -> Self {
        Self { gap, pending: None, not_before: None, failing: None, last_lookup: None, known: VecDeque::new() }
    }

    /// A song is on. The cover when it is already known, or `None` after
    /// leaving the song to be looked up, unless it is known to have none.
    fn want(&mut self, song: Song, now: Instant) -> Option<String> {
        if self.pending.as_ref().is_some_and(|waiting| *waiting == song) {
            return None;
        }
        self.pending = None;
        self.not_before = None;
        match self.known.iter().find(|(title, ..)| *title == song.title) {
            Some((_, Some(path), _)) => Some(path.clone()),
            Some((_, None, at)) if now.saturating_duration_since(*at) < NONE_FOR => None,
            _ => {
                self.pending = Some(song);
                None
            }
        }
    }

    /// The remembered cover turned out not to be on disk any more.
    fn forget(&mut self, title: &str) {
        self.known.retain(|(known, ..)| known != title);
    }

    fn next(&mut self, now: Instant) -> Next {
        if self.pending.is_none() {
            return Next::Sleep;
        }
        let gap_over = self.last_lookup.map(|at| at + self.gap);
        if let Some(until) = gap_over.into_iter().chain(self.not_before).max().filter(|until| *until > now) {
            return Next::WaitFor(until - now);
        }
        self.last_lookup = Some(now);
        self.not_before = None;
        self.pending.take().map_or(Next::Sleep, Next::Lookup)
    }

    fn answered(&mut self, song: &Song, found: &Found, now: Instant) {
        let path = match found {
            Found::Cover(path) => Some(path.clone()),
            Found::Nothing => None,
            Found::Failed => {
                let attempts = match &self.failing {
                    Some((title, attempts)) if *title == song.title => attempts + 1,
                    _ => 1,
                };
                if attempts < ATTEMPTS {
                    self.failing = Some((song.title.clone(), attempts));
                    if self.pending.is_none() {
                        self.pending = Some(song.clone());
                        self.not_before = Some(now + RETRY_AFTER);
                    }
                    return;
                }
                None
            }
        };
        self.failing = None;
        self.forget(&song.title);
        self.known.push_back((song.title.clone(), path, now));
        while self.known.len() > REMEMBERED {
            self.known.pop_front();
        }
    }
}

/// The cover on disk, from Last.fm through the proxy.
pub(super) fn from_network(song: &Song) -> Found {
    match crate::scrobble::lastfm::track_cover(&song.artist, &song.name) {
        Ok(Some(address)) => crate::pictures::keep(&address).map_or(Found::Failed, Found::Cover),
        Ok(None) => Found::Nothing,
        Err(_) => Found::Failed,
    }
}

/// Starts the thread, which ends after the returned sender is dropped, once
/// the lookup it may be in has returned. `find` goes to the network; `deliver`
/// receives every cover found or remembered.
pub(super) fn start(
    find: impl Fn(&Song) -> Found + Send + 'static,
    deliver: impl Fn(&Song, String) + Send + 'static,
) -> Option<Sender<Song>> {
    start_with(GAP, |path| std::path::Path::new(path).is_file(), find, deliver)
}

fn start_with(
    gap: Duration,
    on_disk: impl Fn(&str) -> bool + Send + 'static,
    find: impl Fn(&Song) -> Found + Send + 'static,
    deliver: impl Fn(&Song, String) + Send + 'static,
) -> Option<Sender<Song>> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new().name("chilpa-song-cover".into())
        .spawn(move || run(rx, gap, on_disk, find, deliver)).ok()?;
    Some(tx)
}

fn run(rx: Receiver<Song>, gap: Duration, on_disk: impl Fn(&str) -> bool, find: impl Fn(&Song) -> Found, deliver: impl Fn(&Song, String)) {
    let mut lookups = Lookups::new(gap);
    let take = |lookups: &mut Lookups, song: Song| {
        if let Some(path) = lookups.want(song.clone(), Instant::now()) {
            if on_disk(&path) {
                deliver(&song, path);
            } else {
                lookups.forget(&song.title);
                lookups.want(song, Instant::now());
            }
        }
    };
    loop {
        // Everything that came during a lookup or a wait, so that what is
        // looked up next is the song on now.
        loop {
            match rx.try_recv() {
                Ok(song) => take(&mut lookups, song),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        match lookups.next(Instant::now()) {
            Next::Lookup(song) => {
                let found = find(&song);
                lookups.answered(&song, &found, Instant::now());
                if let Found::Cover(path) = found {
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
                    lookups.answered(&found, &Found::Cover("cover".into()), now);
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
                lookups.answered(&found, &Found::Nothing, now);
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
                lookups.answered(&found, &Found::Nothing, now);
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
        lookups.answered(&song("A - One"), &Found::Cover("cover".into()), start);
        lookups.want(song("B - Two"), start);
        assert_eq!(lookups.want(song("A - One"), start).as_deref(), Some("cover"));
        assert_eq!(lookups.next(start + GAP), Next::Sleep);
    }

    #[test]
    fn what_is_remembered_has_a_limit() {
        let (mut lookups, start) = (Lookups::new(GAP), Instant::now());
        for each in 0..REMEMBERED + 10 {
            lookups.answered(&song(&format!("A - {each}")), &Found::Nothing, start);
        }
        assert_eq!(lookups.known.len(), REMEMBERED);
    }

    #[test]
    fn a_failed_lookup_is_tried_again_a_few_times_and_stops() {
        let (mut lookups, start) = (Lookups::new(GAP), Instant::now());
        let mut asked = 0;
        for second in 0..3_600u64 {
            let now = start + Duration::from_secs(second);
            if let Next::Lookup(found) = { if second == 0 { lookups.want(song("A - One"), now); } lookups.next(now) } {
                asked += 1;
                lookups.answered(&found, &Found::Failed, now);
            }
        }
        assert_eq!(asked, ATTEMPTS);
        assert_eq!(lookups.want(song("A - One"), start + Duration::from_secs(120)), None);
        assert_eq!(lookups.next(start + Duration::from_secs(120)), Next::Sleep, "it holds as no cover for a while");
    }

    #[test]
    fn a_failed_lookup_does_not_take_the_place_of_the_song_on_now() {
        let (mut lookups, start) = (Lookups::new(GAP), Instant::now());
        lookups.want(song("A - One"), start);
        let Next::Lookup(first) = lookups.next(start) else { panic!("the first lookup is at once") };
        lookups.want(song("B - Two"), start);
        lookups.answered(&first, &Found::Failed, start);
        assert_eq!(lookups.next(start + GAP), Next::Lookup(song("B - Two")));
    }

    #[test]
    fn a_song_sent_again_keeps_the_wait_of_its_retry() {
        let (mut lookups, start) = (Lookups::new(GAP), Instant::now());
        lookups.want(song("A - One"), start);
        let Next::Lookup(first) = lookups.next(start) else { panic!("the first lookup is at once") };
        lookups.answered(&first, &Found::Failed, start);
        lookups.want(song("A - One"), start);
        assert_eq!(lookups.next(start), Next::WaitFor(RETRY_AFTER));
    }

    #[test]
    fn a_remembered_cover_gone_from_disk_is_looked_up_again() {
        let (asked, delivered) = (Arc::new(Mutex::new(0)), Arc::new(Mutex::new(0)));
        let (counter, covers) = (asked.clone(), delivered.clone());
        let on_disk = |path: &str| path == "new";
        let tx = start_with(Duration::ZERO, on_disk, move |_| {
            let mut times = counter.lock().unwrap();
            *times += 1;
            Found::Cover(if *times == 1 { "gone" } else { "new" }.into())
        }, move |_, _| *covers.lock().unwrap() += 1).unwrap();
        tx.send(song("A - One")).unwrap();
        wait_until(|| *delivered.lock().unwrap() == 1);
        tx.send(song("A - One")).unwrap();
        wait_until(|| *delivered.lock().unwrap() == 2);
        assert_eq!(*asked.lock().unwrap(), 2);
    }

    #[test]
    fn after_a_slow_lookup_only_the_song_on_now_is_looked_up() {
        let (entered, release) = (mpsc::channel(), mpsc::channel::<()>());
        let (entered_tx, entered_rx) = entered;
        let (release_tx, release_rx) = release;
        let asked = Arc::new(Mutex::new(Vec::new()));
        let seen = asked.clone();
        let tx = start_with(Duration::ZERO, |_| true, move |song: &Song| {
            seen.lock().unwrap().push(song.title.clone());
            let _ = entered_tx.send(());
            let _ = release_rx.recv();
            Found::Nothing
        }, |_, _| {}).unwrap();
        tx.send(song("A - One")).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        tx.send(song("B - Two")).unwrap();
        tx.send(song("C - Three")).unwrap();
        release_tx.send(()).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        release_tx.send(()).unwrap();
        wait_until(|| asked.lock().unwrap().len() == 2);
        assert_eq!(*asked.lock().unwrap(), ["A - One", "C - Three"]);
    }

    #[test]
    fn the_thread_ends_with_its_sender() {
        let (gone_tx, gone_rx) = mpsc::channel::<()>();
        struct Ends(Sender<()>);
        impl Drop for Ends {
            fn drop(&mut self) { let _ = self.0.send(()); }
        }
        let ends = Ends(gone_tx);
        let tx = start_with(Duration::ZERO, |_| true, move |_| { let _kept = &ends; Found::Nothing }, |_, _| {}).unwrap();
        tx.send(song("A - One")).unwrap();
        drop(tx);
        gone_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    }

    fn wait_until(done: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "the thread did not get there");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn the_thread_asks_the_network_once_for_a_flood() {
        let asked = Arc::new(Mutex::new(0));
        let delivered = Arc::new(Mutex::new(0));
        let (counter, covers) = (asked.clone(), delivered.clone());
        let tx = start_with(Duration::from_millis(50), |_| true, move |_| {
            *counter.lock().unwrap() += 1;
            Found::Cover("cover".into())
        }, move |_, _| *covers.lock().unwrap() += 1).unwrap();
        for _ in 0..1_000 {
            tx.send(song("A - One")).unwrap();
        }
        wait_until(|| *delivered.lock().unwrap() >= 1);
        assert_eq!(*asked.lock().unwrap(), 1);
    }
}
