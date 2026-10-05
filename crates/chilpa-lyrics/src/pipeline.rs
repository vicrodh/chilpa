//! Who is asked for a track's lyrics, in what order, and who wins.
//!
//! This module has no network. The plugins are handed in behind
//! [`Provider`] and [`Source`]; whoever builds the pipeline decides how a
//! call reaches them. So the order and the rules are tested here, without
//! a plugin running.
//!
//! The order:
//!
//! 1. **What came with the track** (a file next to it, its tags) wins
//!    outright. Somebody put it there.
//! 2. **The cache**, by identity and, for a source's own track, by its
//!    reference.
//! 3. **The source and the providers at the same time.** A source that has
//!    no lyrics does not add its wait to the providers'.
//!
//! Who wins between them: **the source's lyrics**, because they are for
//! exactly that track. Unless a provider brings a strictly finer timing
//! (words against lines, lines against plain text) for a confirmed match:
//! the same ISRC, or both lengths known and within two seconds.
//!
//! Among providers, candidates that are not the same song are dropped
//! ([`crate::matching`]); the finest timing wins, then the best score.
//! Waiting stops when nothing still pending could change the winner.

use crate::cache::{Cache, Hit, Origin};
use crate::matching::{judge, Claim, Track, Verdict, TIMED_TOLERANCE_MS};
use crate::model::{Document, Timing};
use crate::wire::Candidate;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What a plugin answered.
#[derive(Clone, PartialEq, Debug)]
pub enum Answer {
    Found(Vec<Candidate>),
    /// It looked and there are none.
    None,
    /// It could not look. Never remembered.
    Failed { retry_after: Option<Duration>, message: String },
}

/// A plugin whose job is finding lyrics for any track.
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    /// The finest timing it can bring, as it declares.
    fn best(&self) -> Timing;
    fn find(&self, track: &Track, timeout: Duration) -> Answer;
}

/// A plugin that plays music and may have lyrics for its own tracks.
pub trait Source: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn lyrics(&self, reference: &str, track: &Track, timeout: Duration) -> Answer;
}

/// How long the source and the providers are waited for.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub source: Duration,
    pub providers: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { source: Duration::from_secs(4), providers: Duration::from_secs(6) }
    }
}

/// One track to find lyrics for.
#[derive(Clone)]
pub struct Request {
    pub track: Track,
    /// The file, when the track is one.
    pub path: Option<PathBuf>,
    /// The source and the track's reference in it, when it comes from one.
    pub source: Option<(Arc<dyn Source>, String)>,
}

/// Lyrics found, and who gave them.
#[derive(Clone, PartialEq, Debug)]
pub struct Resolved {
    pub document: Document,
    pub origin: Origin,
}

/// What a lookup ended with.
#[derive(Clone, PartialEq, Debug)]
pub struct Outcome {
    pub resolved: Option<Resolved>,
    /// What was served came from the cache and is old: look again, without
    /// holding up the screen.
    pub refresh: bool,
}

/// The lyrics pipeline: the cache, the providers and their pauses.
pub struct Pipeline {
    cache: Mutex<Cache>,
    providers: Mutex<Vec<Arc<dyn Provider>>>,
    /// Providers that asked to be left alone, until when.
    cooling: Mutex<HashMap<String, Instant>>,
    limits: Limits,
}

impl Pipeline {
    pub fn new(cache: Cache, limits: Limits) -> Pipeline {
        Pipeline { cache: Mutex::new(cache), providers: Mutex::new(Vec::new()), cooling: Mutex::new(HashMap::new()), limits }
    }

    /// Replaces the providers, when plugins are turned on or off.
    pub fn set_providers(&self, providers: Vec<Arc<dyn Provider>>) {
        *self.providers.lock().unwrap_or_else(|e| e.into_inner()) = providers;
    }

    /// Forgets what a plugin gave, when it is removed.
    pub fn forget(&self, plugin: &str) {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).forget_provider(plugin);
    }

    /// Finds lyrics. `fresh` skips the cache, for a refresh.
    pub fn resolve(&self, request: &Request, fresh: bool) -> Outcome {
        if let Some(path) = request.path.as_deref() {
            if let Some(found) = crate::local::find(path) {
                return Outcome { resolved: Some(Resolved { document: found.document, origin: found.origin }), refresh: false };
            }
        }
        let track = &request.track;

        // What the cache knows, for the source and by identity.
        let mut source_known: Option<Option<Resolved>> = None;
        let mut identity_known: Option<Option<Resolved>> = None;
        let mut stale = false;
        if !fresh {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((source, reference)) = &request.source {
                match cache.get_from_source(source.id(), reference, track) {
                    Some(Hit::Found { document, origin, stale: old }) => {
                        stale |= old;
                        source_known = Some(Some(Resolved { document, origin }));
                    }
                    Some(Hit::None) => source_known = Some(None),
                    None => {}
                }
            }
            match cache.get(track) {
                Some(Hit::Found { document, origin, stale: old }) => {
                    stale |= old;
                    identity_known = Some(Some(Resolved { document, origin }));
                }
                Some(Hit::None) => identity_known = Some(None),
                None => {}
            }
        }
        let need_source = request.source.is_some() && source_known.is_none();
        let need_providers = identity_known.is_none();
        if !need_source && !need_providers {
            let from_source = source_known.flatten();
            let from_providers = identity_known.flatten();
            return Outcome { resolved: choose(track, from_source, from_providers.map(|r| (r, true))), refresh: stale };
        }

        let asked = self.ask(request, need_source, need_providers);

        // Remember what was learned.
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let mut from_source = source_known.flatten();
        if let (Some((source, reference)), true) = (&request.source, need_source) {
            match &asked.source {
                Some(Ok(Some(candidate))) => {
                    let origin = Origin { provider: source.id().to_string(), name: source.name().to_string() };
                    let raw = candidate.raw.as_ref().map(|(f, t)| (f.as_str(), t.as_str()));
                    cache.put_from_source(source.id(), reference, track, &candidate.document, &origin, raw);
                    from_source = Some(Resolved { document: candidate.document.clone(), origin });
                }
                Some(Ok(None)) => cache.put_none_from_source(source.id(), reference, track),
                _ => {}
            }
        }
        let mut from_providers = identity_known.flatten().map(|r| (r, true));
        if need_providers {
            match &asked.best {
                Some(best) => {
                    let raw = best.candidate.raw.as_ref().map(|(f, t)| (f.as_str(), t.as_str()));
                    cache.put(track, &best.candidate.claim, &best.candidate.document, &best.origin, raw);
                    from_providers = Some((
                        Resolved { document: best.candidate.document.clone(), origin: best.origin.clone() },
                        confirmed(track, &best.candidate.claim, best.verdict),
                    ));
                }
                None if asked.all_answered && asked.providers > 0 => cache.put_none(track),
                None => {}
            }
        }
        Outcome { resolved: choose(track, from_source, from_providers), refresh: false }
    }

    /// Asks the source and the providers at once, and waits as long as an
    /// answer still pending could change the result.
    fn ask(&self, request: &Request, need_source: bool, need_providers: bool) -> Asked {
        enum Message {
            Source(Answer),
            Provider(usize, Answer),
        }
        let providers: Vec<Arc<dyn Provider>> = if need_providers {
            let now = Instant::now();
            let cooling = self.cooling.lock().unwrap_or_else(|e| e.into_inner());
            self.providers
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .filter(|p| cooling.get(p.id()).is_none_or(|until| *until <= now))
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let skipped = need_providers && providers.len() < self.providers.lock().unwrap_or_else(|e| e.into_inner()).len();

        let (tx, rx) = mpsc::channel::<Message>();
        if let (true, Some((source, reference))) = (need_source, &request.source) {
            let (tx, source, reference, track, limit) =
                (tx.clone(), source.clone(), reference.clone(), request.track.clone(), self.limits.source);
            std::thread::spawn(move || {
                let _ = tx.send(Message::Source(source.lyrics(&reference, &track, limit)));
            });
        }
        for (i, provider) in providers.iter().enumerate() {
            let (tx, provider, track, limit) = (tx.clone(), provider.clone(), request.track.clone(), self.limits.providers);
            std::thread::spawn(move || {
                let _ = tx.send(Message::Provider(i, provider.find(&track, limit)));
            });
        }
        drop(tx);

        let started = Instant::now();
        let deadline = started + self.limits.source.max(self.limits.providers) + Duration::from_millis(250);
        let mut source_pending = need_source && request.source.is_some();
        let mut pending: Vec<bool> = vec![true; providers.len()];
        let mut asked = Asked { providers: providers.len(), all_answered: !skipped, ..Asked::default() };
        while source_pending || pending.iter().any(|p| *p) {
            // Nothing pending can change the winner: stop waiting.
            if !source_pending {
                if let Some(best) = &asked.best {
                    let beatable = providers.iter().zip(&pending).any(|(p, waiting)| *waiting && p.best() > best.candidate.document.timing);
                    if !beatable {
                        break;
                    }
                }
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else { break };
            let Ok(message) = rx.recv_timeout(left) else { break };
            match message {
                Message::Source(answer) => {
                    source_pending = false;
                    asked.source = Some(match answer {
                        Answer::Found(mut list) if !list.is_empty() => Ok(Some(list.swap_remove(0))),
                        Answer::Found(_) | Answer::None => Ok(None),
                        Answer::Failed { message, .. } => Err(message),
                    });
                }
                Message::Provider(i, answer) => {
                    pending[i] = false;
                    let provider = &providers[i];
                    match answer {
                        Answer::Found(candidates) => {
                            for candidate in candidates {
                                let verdict = judge(&request.track, &candidate.claim, &candidate.document);
                                if !verdict.accepted() {
                                    continue;
                                }
                                let origin = Origin { provider: provider.id().to_string(), name: provider.name().to_string() };
                                let better = asked.best.as_ref().is_none_or(|b| {
                                    (candidate.document.timing, verdict.score()) > (b.candidate.document.timing, b.verdict.score())
                                });
                                if better {
                                    asked.best = Some(Best { candidate, verdict, origin });
                                }
                            }
                        }
                        Answer::None => {}
                        Answer::Failed { retry_after, .. } => {
                            asked.all_answered = false;
                            if let Some(wait) = retry_after {
                                self.cooling.lock().unwrap_or_else(|e| e.into_inner()).insert(provider.id().to_string(), Instant::now() + wait.min(Duration::from_secs(3600)));
                            }
                        }
                    }
                }
            }
        }
        if pending.iter().any(|p| *p) && asked.best.is_none() {
            asked.all_answered = false;
        }
        asked
    }
}

#[derive(Default)]
struct Asked {
    source: Option<Result<Option<Candidate>, String>>,
    best: Option<Best>,
    providers: usize,
    all_answered: bool,
}

struct Best {
    candidate: Candidate,
    verdict: Verdict,
    origin: Origin,
}

/// A provider's lyrics are confirmed for the track: the same recording by
/// ISRC, or both lengths known and within two seconds.
fn confirmed(track: &Track, claim: &Claim, verdict: Verdict) -> bool {
    verdict == Verdict::Exact
        || track.duration_ms.zip(claim.duration_ms).is_some_and(|(a, b)| a.abs_diff(b) <= TIMED_TOLERANCE_MS)
}

/// The source's lyrics win, unless the provider's are strictly finer and
/// confirmed for the track.
fn choose(_track: &Track, source: Option<Resolved>, provider: Option<(Resolved, bool)>) -> Option<Resolved> {
    match (source, provider) {
        (Some(s), Some((p, true))) if p.document.timing > s.document.timing => Some(p),
        (Some(s), _) => Some(s),
        (None, Some((p, _))) => Some(p),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lrc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        id: &'static str,
        best: Timing,
        delay: Duration,
        answer: Answer,
        calls: AtomicUsize,
    }

    impl Fake {
        fn new(id: &'static str, best: Timing, answer: Answer) -> Arc<Fake> {
            Arc::new(Fake { id, best, delay: Duration::ZERO, answer, calls: AtomicUsize::new(0) })
        }
        fn slow(id: &'static str, best: Timing, delay: Duration, answer: Answer) -> Arc<Fake> {
            Arc::new(Fake { id, best, delay, answer, calls: AtomicUsize::new(0) })
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl Provider for Fake {
        fn id(&self) -> &str {
            self.id
        }
        fn name(&self) -> &str {
            self.id
        }
        fn best(&self) -> Timing {
            self.best
        }
        fn find(&self, _: &Track, _: Duration) -> Answer {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(self.delay);
            self.answer.clone()
        }
    }

    impl Source for Fake {
        fn id(&self) -> &str {
            self.id
        }
        fn name(&self) -> &str {
            self.id
        }
        fn lyrics(&self, _: &str, _: &Track, _: Duration) -> Answer {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(self.delay);
            self.answer.clone()
        }
    }

    fn track() -> Track {
        Track { title: "Song".into(), artist: "Someone".into(), album: None, duration_ms: Some(200_000), isrc: None }
    }

    fn candidate(lrc_text: &str, duration_ms: u32) -> Candidate {
        Candidate {
            claim: Claim { title: Some("Song".into()), artist: Some("Someone".into()), album: None, duration_ms: Some(duration_ms), isrc: None },
            document: lrc::parse(lrc_text),
            raw: Some(("lrc".into(), lrc_text.into())),
        }
    }

    const LINES: &str = "[00:01.00]Line timed";
    const WORDS: &str = "[00:01.00]<00:01.00>Word <00:01.50>timed";
    const PLAIN: &str = "Just words";

    fn pipeline(providers: Vec<Arc<dyn Provider>>) -> Pipeline {
        let p = Pipeline::new(Cache::in_memory().unwrap(), Limits { source: Duration::from_millis(400), providers: Duration::from_millis(600) });
        p.set_providers(providers);
        p
    }

    fn request(source: Option<Arc<dyn Source>>) -> Request {
        Request { track: track(), path: None, source: source.map(|s| (s, "track:1".to_string())) }
    }

    fn origin(outcome: &Outcome) -> &str {
        &outcome.resolved.as_ref().unwrap().origin.provider
    }

    #[test]
    fn the_finest_timing_wins_among_providers() {
        let a = Fake::new("lines", Timing::Line, Answer::Found(vec![candidate(LINES, 200_000)]));
        let b = Fake::new("words", Timing::Word, Answer::Found(vec![candidate(WORDS, 200_500)]));
        let p = pipeline(vec![a, b]);
        assert_eq!(origin(&p.resolve(&request(None), false)), "words");
    }

    #[test]
    fn another_song_never_wins_however_well_timed() {
        let mut wrong = candidate(WORDS, 200_000);
        wrong.claim.title = Some("Something Else Entirely".into());
        let a = Fake::new("wrong", Timing::Word, Answer::Found(vec![wrong]));
        let b = Fake::new("right", Timing::Line, Answer::Found(vec![candidate(LINES, 200_000)]));
        let p = pipeline(vec![a, b]);
        assert_eq!(origin(&p.resolve(&request(None), false)), "right");
    }

    #[test]
    fn the_source_wins_ties_and_finer_lyrics_beat_it_only_when_confirmed() {
        let source = Fake::new("source", Timing::Line, Answer::Found(vec![candidate(LINES, 200_000)]));
        let words = Fake::new("words", Timing::Word, Answer::Found(vec![candidate(WORDS, 200_300)]));
        let p = pipeline(vec![words]);
        assert_eq!(origin(&p.resolve(&request(Some(source.clone())), false)), "words", "words beat lines when the length agrees");

        let lines = Fake::new("lines", Timing::Line, Answer::Found(vec![candidate(LINES, 200_000)]));
        let p = pipeline(vec![lines]);
        assert_eq!(origin(&p.resolve(&request(Some(source.clone())), false)), "source", "a tie goes to the source");

        let mut unknown_length = candidate(WORDS, 0);
        unknown_length.claim.duration_ms = None;
        let unconfirmed = Fake::new("unconfirmed", Timing::Word, Answer::Found(vec![unknown_length]));
        let p = pipeline(vec![unconfirmed]);
        assert_eq!(origin(&p.resolve(&request(Some(source)), false)), "source", "finer, but not confirmed for this track");
    }

    #[test]
    fn plain_lyrics_from_the_source_lose_to_confirmed_timed_ones() {
        let source = Fake::new("source", Timing::Unsynced, Answer::Found(vec![candidate(PLAIN, 200_000)]));
        let lines = Fake::new("lines", Timing::Line, Answer::Found(vec![candidate(LINES, 200_000)]));
        let p = pipeline(vec![lines]);
        assert_eq!(origin(&p.resolve(&request(Some(source)), false)), "lines");
    }

    #[test]
    fn a_source_with_none_falls_back_without_waiting_twice() {
        let source = Fake::new("source", Timing::Word, Answer::None);
        let lines = Fake::slow("lines", Timing::Line, Duration::from_millis(100), Answer::Found(vec![candidate(LINES, 200_000)]));
        let p = pipeline(vec![lines]);
        let started = Instant::now();
        assert_eq!(origin(&p.resolve(&request(Some(source)), false)), "lines");
        assert!(started.elapsed() < Duration::from_millis(350), "they were asked at the same time");
    }

    #[test]
    fn waiting_stops_when_nothing_pending_can_win() {
        let words = Fake::new("words", Timing::Word, Answer::Found(vec![candidate(WORDS, 200_000)]));
        let slow_lines = Fake::slow("slow", Timing::Line, Duration::from_millis(500), Answer::None);
        let p = pipeline(vec![words, slow_lines]);
        let started = Instant::now();
        p.resolve(&request(None), false);
        assert!(started.elapsed() < Duration::from_millis(300), "a provider of lines cannot beat words");
    }

    #[test]
    fn answers_are_remembered_and_failures_are_not() {
        let lines = Fake::new("lines", Timing::Line, Answer::Found(vec![candidate(LINES, 200_000)]));
        let p = pipeline(vec![lines.clone()]);
        p.resolve(&request(None), false);
        p.resolve(&request(None), false);
        assert_eq!(lines.calls(), 1, "the second time comes from the cache");

        let failing = Fake::new("failing", Timing::Line, Answer::Failed { retry_after: None, message: "down".into() });
        let p = pipeline(vec![failing.clone()]);
        assert!(p.resolve(&request(None), false).resolved.is_none());
        p.resolve(&request(None), false);
        assert_eq!(failing.calls(), 2, "a failure is asked again");

        let none = Fake::new("none", Timing::Line, Answer::None);
        let p = pipeline(vec![none.clone()]);
        p.resolve(&request(None), false);
        p.resolve(&request(None), false);
        assert_eq!(none.calls(), 1, "\"none\" is believed for a while");
    }

    #[test]
    fn a_provider_that_asks_for_a_pause_gets_it() {
        let busy = Fake::new("busy", Timing::Line, Answer::Failed { retry_after: Some(Duration::from_secs(60)), message: "429".into() });
        let p = pipeline(vec![busy.clone()]);
        p.resolve(&request(None), false);
        p.resolve(&request(None), false);
        assert_eq!(busy.calls(), 1, "it is not asked again during its pause");
    }

    #[test]
    fn what_came_with_the_file_wins_outright() {
        let dir = std::env::temp_dir().join(format!("chilpa-lyrics-pipeline-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("song.flac");
        std::fs::write(&file, b"x").unwrap();
        std::fs::write(dir.join("song.lrc"), "Plain words next door\n").unwrap();
        let words = Fake::new("words", Timing::Word, Answer::Found(vec![candidate(WORDS, 200_000)]));
        let p = pipeline(vec![words.clone()]);
        let outcome = p.resolve(&Request { track: track(), path: Some(file), source: None }, false);
        assert_eq!(origin(&outcome), "file");
        assert_eq!(words.calls(), 0, "nobody is asked when the file has lyrics");
        let _ = std::fs::remove_dir_all(dir);
    }
}
