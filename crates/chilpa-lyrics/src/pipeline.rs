//! Who is asked for a track's lyrics, in what order, and who wins.
//!
//! This module has no network. The source and the lyrics service are handed
//! in behind [`Source`] and [`Remote`]; whoever builds the pipeline decides
//! how a call reaches them. So the order and the rules are tested here,
//! with nothing running.
//!
//! The order:
//!
//! 1. **What came with the track** (a file next to it, its tags) wins
//!    outright. Somebody put it there, and nobody else is asked.
//! 2. **The cache**, by identity and, for a source's own track, by its
//!    reference.
//! 3. **The source and the lyrics service at the same time.** A source that
//!    has no lyrics does not add its wait to the service's.
//!
//! Who wins between them: **the source's lyrics, always.** They are for
//! exactly that track. What the service brings only shows when the source
//! has none, fails or does not answer in time, however finely it is timed.
//!
//! The service has already chosen among its providers; what it answers is
//! still judged here against the track ([`crate::matching`]) before it is
//! shown or kept.

use crate::cache::{Cache, Hit, Origin};
use crate::matching::{judge, Track};
use crate::model::Document;
use crate::wire::Candidate;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What was answered about a track's lyrics.
#[derive(Clone, PartialEq, Debug)]
pub enum Answer {
    Found(Vec<Candidate>),
    /// It looked and there are none.
    None,
    /// It could not look. Never remembered.
    Failed { retry_after: Option<Duration>, message: String },
}

/// The lyrics service: asked about any track that has no lyrics of its own.
pub trait Remote: Send + Sync {
    fn id(&self) -> &str;
    /// Shown for a document that does not say who gave it.
    fn name(&self) -> &str;
    fn resolve(&self, track: &Track, language: Option<&str>, timeout: Duration) -> Answer;
}

/// A plugin that plays music and may have lyrics for its own tracks.
pub trait Source: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    /// `language` is the one a translation is wanted in next to the
    /// original, when one is.
    fn lyrics(&self, reference: &str, track: &Track, language: Option<&str>, timeout: Duration) -> Answer;
}

/// How long the source and the service are waited for.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub source: Duration,
    pub remote: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { source: Duration::from_secs(4), remote: Duration::from_secs(6) }
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
    /// The language a translation is wanted in, when the reader wants one.
    pub language: Option<String>,
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

/// The lyrics pipeline: the cache, the service and its pause.
pub struct Pipeline {
    cache: Mutex<Cache>,
    remote: Option<Arc<dyn Remote>>,
    /// The service asked to be left alone, until when.
    cooling: Mutex<Option<Instant>>,
    limits: Limits,
}

impl Pipeline {
    /// A pipeline over a cache. Without a `remote`, only what came with the
    /// track and what its source has are found.
    pub fn new(cache: Cache, remote: Option<Arc<dyn Remote>>, limits: Limits) -> Pipeline {
        Pipeline { cache: Mutex::new(cache), remote, cooling: Mutex::new(None), limits }
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
        // What a source gave is kept under the language it was asked in: a
        // translation into another one is another answer.
        let kept = |reference: &str| match &request.language {
            Some(language) => format!("{reference}\u{1f}{language}"),
            None => reference.to_string(),
        };

        // What the cache knows, for the source and by identity.
        let mut source_known: Option<Option<Resolved>> = None;
        let mut identity_known: Option<Option<Resolved>> = None;
        let mut stale = false;
        if !fresh {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((source, reference)) = &request.source {
                match cache.get_from_source(source.id(), &kept(reference), track) {
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
        // A track that does not say what song it is cannot be looked up by
        // name: every answer would be a guess.
        let named = !track.title.trim().is_empty() && !track.artist.trim().is_empty();
        // The source's lyrics win: when the cache already has them, the
        // service has nothing to add.
        let source_has = matches!(source_known, Some(Some(_)));
        let need_remote = identity_known.is_none() && named && !source_has && self.remote.is_some();
        if !need_source && !need_remote {
            return Outcome { resolved: choose(source_known.flatten(), identity_known.flatten()), refresh: stale };
        }

        let asked = self.ask(request, need_source, need_remote);

        // Remember what was learned.
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let mut from_source = source_known.flatten();
        if let (Some((source, reference)), true) = (&request.source, need_source) {
            match &asked.source {
                Some(Ok(Some(candidate))) => {
                    let origin = Origin { provider: source.id().to_string(), name: source.name().to_string() };
                    let raw = candidate.raw.as_ref().map(|(f, t)| (f.as_str(), t.as_str()));
                    cache.put_from_source(source.id(), &kept(reference), track, &candidate.document, &origin, raw);
                    from_source = Some(Resolved { document: candidate.document.clone(), origin });
                }
                Some(Ok(None)) => cache.put_none_from_source(source.id(), &kept(reference), track),
                _ => {}
            }
        }
        let mut from_remote = identity_known.flatten();
        match &asked.remote {
            Some(Ok(Some((candidate, origin)))) => {
                let raw = candidate.raw.as_ref().map(|(f, t)| (f.as_str(), t.as_str()));
                cache.put(track, &candidate.claim, &candidate.document, origin, raw);
                from_remote = Some(Resolved { document: candidate.document.clone(), origin: origin.clone() });
            }
            Some(Ok(None)) => cache.put_none(track),
            _ => {}
        }
        Outcome { resolved: choose(from_source, from_remote), refresh: false }
    }

    /// Asks the source and the service at once. Each has its own time
    /// limit, and a source that has lyrics ends the wait: nobody else can
    /// win.
    fn ask(&self, request: &Request, need_source: bool, need_remote: bool) -> Asked {
        enum Message {
            Source(Answer),
            Remote(Answer),
        }
        let remote = self.remote.as_ref().filter(|_| need_remote).filter(|_| {
            // During the pause the service asked for, it is not asked.
            self.cooling.lock().unwrap_or_else(|e| e.into_inner()).is_none_or(|until| until <= Instant::now())
        });

        let (tx, rx) = mpsc::channel::<Message>();
        let started = Instant::now();
        let mut source_until = None;
        if let (true, Some((source, reference))) = (need_source, &request.source) {
            let (tx, source, reference, track, language, limit) = (tx.clone(), source.clone(), reference.clone(), request.track.clone(), request.language.clone(), self.limits.source);
            std::thread::spawn(move || {
                let _ = tx.send(Message::Source(source.lyrics(&reference, &track, language.as_deref(), limit)));
            });
            source_until = Some(started + self.limits.source);
        }
        let mut remote_until = None;
        if let Some(remote) = remote {
            let (tx, remote, track, language, limit) = (tx.clone(), remote.clone(), request.track.clone(), request.language.clone(), self.limits.remote);
            std::thread::spawn(move || {
                let _ = tx.send(Message::Remote(remote.resolve(&track, language.as_deref(), limit)));
            });
            remote_until = Some(started + self.limits.remote);
        }
        drop(tx);

        let mut asked = Asked::default();
        while source_until.is_some() || remote_until.is_some() {
            // Whoever ran out of time is no longer waited for.
            let now = Instant::now();
            if source_until.is_some_and(|until| until <= now) {
                source_until = None;
                asked.source = Some(Err("the source did not answer in time".into()));
            }
            if remote_until.is_some_and(|until| until <= now) {
                remote_until = None;
            }
            let Some(next) = [source_until, remote_until].into_iter().flatten().min() else { break };
            let Ok(message) = rx.recv_timeout(next.saturating_duration_since(now)) else { continue };
            match message {
                Message::Source(answer) => {
                    source_until = None;
                    asked.source = Some(match answer {
                        Answer::Found(mut list) if !list.is_empty() => Ok(Some(list.swap_remove(0))),
                        Answer::Found(_) | Answer::None => Ok(None),
                        Answer::Failed { message, .. } => Err(message),
                    });
                    if matches!(asked.source, Some(Ok(Some(_)))) {
                        break;
                    }
                }
                Message::Remote(answer) => {
                    remote_until = None;
                    let Some(remote) = remote else { continue };
                    asked.remote = Some(match answer {
                        Answer::Found(candidates) => Ok(self.accept(&request.track, remote.as_ref(), candidates)),
                        Answer::None => Ok(None),
                        Answer::Failed { retry_after, message } => {
                            if let Some(wait) = retry_after {
                                *self.cooling.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now() + wait.min(Duration::from_secs(3600)));
                            }
                            Err(message)
                        }
                    });
                }
            }
        }
        asked
    }

    /// The best of what the service sent that is this track's, with who
    /// gave it. Lyrics for another song are not lyrics for this one.
    fn accept(&self, track: &Track, remote: &dyn Remote, candidates: Vec<Candidate>) -> Option<(Candidate, Origin)> {
        let mut best: Option<(Candidate, f32)> = None;
        for candidate in candidates {
            let verdict = judge(track, &candidate.claim, &candidate.document);
            if !verdict.accepted() {
                continue;
            }
            let better = best.as_ref().is_none_or(|(b, score)| (candidate.document.timing, verdict.score()) > (b.document.timing, *score));
            if better {
                best = Some((candidate, verdict.score()));
            }
        }
        let (candidate, _) = best?;
        // A document that says who gave it is shown under that name, not
        // under whoever passed it on.
        let origin = match &candidate.document.source {
            Some(source) => Origin { provider: source.provider.clone(), name: source.name.clone() },
            None => Origin { provider: remote.id().to_string(), name: remote.name().to_string() },
        };
        Some((candidate, origin))
    }
}

/// What the source and the service answered: `Ok(Some)` found, `Ok(None)`
/// none, `Err` could not say. `None` outside means it was not asked, or
/// did not answer in time.
#[derive(Default)]
struct Asked {
    source: Option<Result<Option<Candidate>, String>>,
    remote: Option<Result<Option<(Candidate, Origin)>, String>>,
}

/// The source's lyrics win. The service's show only when the source has none.
fn choose(source: Option<Resolved>, remote: Option<Resolved>) -> Option<Resolved> {
    source.or(remote)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lrc;
    use crate::matching::Claim;
    use crate::model::Timing;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fake {
        id: &'static str,
        delay: Duration,
        answer: Answer,
        calls: AtomicUsize,
    }

    impl Fake {
        fn new(id: &'static str, answer: Answer) -> Arc<Fake> {
            Arc::new(Fake { id, delay: Duration::ZERO, answer, calls: AtomicUsize::new(0) })
        }
        fn slow(id: &'static str, delay: Duration, answer: Answer) -> Arc<Fake> {
            Arc::new(Fake { id, delay, answer, calls: AtomicUsize::new(0) })
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
        fn answer(&self) -> Answer {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(self.delay);
            self.answer.clone()
        }
    }

    impl Remote for Fake {
        fn id(&self) -> &str {
            self.id
        }
        fn name(&self) -> &str {
            self.id
        }
        fn resolve(&self, _: &Track, _: Option<&str>, _: Duration) -> Answer {
            self.answer()
        }
    }

    impl Source for Fake {
        fn id(&self) -> &str {
            self.id
        }
        fn name(&self) -> &str {
            self.id
        }
        fn lyrics(&self, _: &str, _: &Track, _: Option<&str>, _: Duration) -> Answer {
            self.answer()
        }
    }

    fn track() -> Track {
        Track { title: "Song".into(), artist: "Someone".into(), album: None, duration_ms: Some(200_000), isrc: None, mbid: None }
    }

    fn candidate(lrc_text: &str, duration_ms: u32) -> Candidate {
        Candidate {
            claim: Claim { title: Some("Song".into()), artist: Some("Someone".into()), album: None, duration_ms: Some(duration_ms), isrc: None },
            document: lrc::parse(lrc_text),
            raw: Some(("lrc".into(), lrc_text.into())),
        }
    }

    fn found(lrc_text: &str) -> Answer {
        Answer::Found(vec![candidate(lrc_text, 200_000)])
    }

    const LINES: &str = "[00:01.00]Line timed";
    const WORDS: &str = "[00:01.00]<00:01.00>Word <00:01.50>timed";
    const PLAIN: &str = "Just words";

    fn pipeline(remote: Arc<Fake>) -> Pipeline {
        Pipeline::new(Cache::in_memory().unwrap(), Some(remote as Arc<dyn Remote>), Limits { source: Duration::from_millis(400), remote: Duration::from_millis(600) })
    }

    fn request(source: Option<Arc<Fake>>) -> Request {
        Request { track: track(), path: None, source: source.map(|s| (s as Arc<dyn Source>, "track:1".to_string())), language: None }
    }

    fn origin(outcome: &Outcome) -> &str {
        &outcome.resolved.as_ref().unwrap().origin.provider
    }

    #[test]
    fn the_service_answers_for_a_track_with_no_lyrics_of_its_own() {
        let p = pipeline(Fake::new("service", found(WORDS)));
        let outcome = p.resolve(&request(None), false);
        assert_eq!(origin(&outcome), "service");
        assert_eq!(outcome.resolved.unwrap().document.timing, Timing::Word);
    }

    #[test]
    fn what_the_service_sends_is_still_judged_here() {
        let mut wrong = candidate(WORDS, 200_000);
        wrong.claim.title = Some("Something Else Entirely".into());
        let p = pipeline(Fake::new("service", Answer::Found(vec![wrong.clone()])));
        assert!(p.resolve(&request(None), false).resolved.is_none(), "another song is not shown, whoever sent it");

        let p = pipeline(Fake::new("service", Answer::Found(vec![candidate(LINES, 260_000)])));
        assert!(p.resolve(&request(None), false).resolved.is_none(), "nor lyrics timed for another length");

        let p = pipeline(Fake::new("service", Answer::Found(vec![wrong, candidate(LINES, 200_000)])));
        assert_eq!(p.resolve(&request(None), false).resolved.unwrap().document.timing, Timing::Line, "the right one among several is kept");
    }

    #[test]
    fn the_source_wins_whatever_the_service_brings() {
        let source = Fake::new("source", found(LINES));
        let p = pipeline(Fake::new("service", found(WORDS)));
        assert_eq!(origin(&p.resolve(&request(Some(source)), false)), "source", "finer lyrics for the same length still lose to the source's");

        let plain = Fake::new("source", found(PLAIN));
        let p = pipeline(Fake::new("service", found(LINES)));
        assert_eq!(origin(&p.resolve(&request(Some(plain)), false)), "source", "even when the source only has the words");
    }

    #[test]
    fn a_source_with_lyrics_is_not_kept_waiting_for_the_service() {
        let source = Fake::new("source", found(LINES));
        let p = pipeline(Fake::slow("service", Duration::from_millis(500), found(WORDS)));
        let started = Instant::now();
        assert_eq!(origin(&p.resolve(&request(Some(source)), false)), "source");
        assert!(started.elapsed() < Duration::from_millis(300), "the source answered; nobody else can win");
    }

    #[test]
    fn a_source_with_none_falls_back_without_waiting_twice() {
        let source = Fake::slow("source", Duration::from_millis(100), Answer::None);
        let p = pipeline(Fake::slow("service", Duration::from_millis(100), found(LINES)));
        let started = Instant::now();
        assert_eq!(origin(&p.resolve(&request(Some(source)), false)), "service");
        assert!(started.elapsed() < Duration::from_millis(190), "they were asked at the same time");
    }

    #[test]
    fn a_source_that_fails_or_is_late_falls_back() {
        let failing = Fake::new("source", Answer::Failed { retry_after: None, message: "no session".into() });
        let p = pipeline(Fake::new("service", found(LINES)));
        assert_eq!(origin(&p.resolve(&request(Some(failing.clone())), false)), "service");
        p.resolve(&request(Some(failing.clone())), false);
        assert_eq!(failing.calls(), 2, "a source that failed is asked again next time");

        let late = Fake::slow("source", Duration::from_millis(2_000), found(WORDS));
        let p = pipeline(Fake::new("service", found(LINES)));
        let started = Instant::now();
        assert_eq!(origin(&p.resolve(&request(Some(late)), false)), "service");
        let waited = started.elapsed();
        assert!(waited >= Duration::from_millis(400), "the source had its time: {waited:?}");
        assert!(waited < Duration::from_millis(1_000), "and no more than its time: {waited:?}");
    }

    #[test]
    fn a_service_that_is_late_is_not_waited_for_past_its_time() {
        let p = pipeline(Fake::slow("service", Duration::from_millis(2_000), found(LINES)));
        let started = Instant::now();
        assert!(p.resolve(&request(None), false).resolved.is_none());
        assert!(started.elapsed() < Duration::from_millis(1_200));
        // It did not say "none": the next play asks again.
        let p = pipeline(Fake::slow("service", Duration::from_millis(2_000), Answer::None));
        p.resolve(&request(None), false);
        let cache = p.cache.lock().unwrap();
        assert!(cache.get(&track()).is_none(), "nothing was remembered about a service that did not answer");
    }

    #[test]
    fn a_document_that_says_who_gave_it_is_shown_under_that_name() {
        let mut named = candidate(WORDS, 200_000);
        named.document.source = Some(crate::model::Provenance { provider: "somewhere".into(), name: "Somewhere".into(), claim: None });
        let p = pipeline(Fake::new("service", Answer::Found(vec![named])));
        let outcome = p.resolve(&request(None), false);
        let origin = &outcome.resolved.as_ref().unwrap().origin;
        assert_eq!((origin.provider.as_str(), origin.name.as_str()), ("somewhere", "Somewhere"));
    }

    #[test]
    fn answers_are_remembered_and_failures_are_not() {
        let service = Fake::new("service", found(LINES));
        let p = pipeline(service.clone());
        p.resolve(&request(None), false);
        p.resolve(&request(None), false);
        assert_eq!(service.calls(), 1, "the second time comes from the cache");

        let failing = Fake::new("failing", Answer::Failed { retry_after: None, message: "down".into() });
        let p = pipeline(failing.clone());
        assert!(p.resolve(&request(None), false).resolved.is_none());
        p.resolve(&request(None), false);
        assert_eq!(failing.calls(), 2, "a failure is asked again");

        let none = Fake::new("none", Answer::None);
        let p = pipeline(none.clone());
        p.resolve(&request(None), false);
        p.resolve(&request(None), false);
        assert_eq!(none.calls(), 1, "\"none\" is believed for a while");
    }

    #[test]
    fn the_sources_lyrics_are_remembered_by_its_reference() {
        let source = Fake::new("source", found(LINES));
        let service = Fake::new("service", Answer::None);
        let p = pipeline(service.clone());
        p.resolve(&request(Some(source.clone())), false);
        assert_eq!(origin(&p.resolve(&request(Some(source.clone())), false)), "source");
        assert_eq!(source.calls(), 1, "the second time comes from the cache");
        assert!(service.calls() <= 1, "and the service is not asked about a track whose source has lyrics");
    }

    #[test]
    fn a_translation_into_another_language_is_another_answer() {
        let source = Fake::new("source", found(LINES));
        let p = pipeline(Fake::new("service", Answer::None));
        let asked = |language: &str| Request { language: Some(language.into()), ..request(Some(source.clone())) };
        p.resolve(&asked("es"), false);
        p.resolve(&asked("es"), false);
        assert_eq!(source.calls(), 1, "the same language comes from the cache");
        p.resolve(&asked("fr"), false);
        assert_eq!(source.calls(), 2, "another language is asked for");
    }

    #[test]
    fn a_service_that_asks_for_a_pause_gets_it() {
        let busy = Fake::new("busy", Answer::Failed { retry_after: Some(Duration::from_secs(60)), message: "429".into() });
        let p = pipeline(busy.clone());
        p.resolve(&request(None), false);
        p.resolve(&request(None), false);
        assert_eq!(busy.calls(), 1, "it is not asked again during its pause");
    }

    #[test]
    fn a_track_without_names_is_not_looked_up_by_name() {
        let service = Fake::new("service", found(WORDS));
        let p = pipeline(service.clone());
        let mut nameless = request(None);
        nameless.track.artist = String::new();
        assert!(p.resolve(&nameless, false).resolved.is_none());
        assert_eq!(service.calls(), 0);
    }

    #[test]
    fn without_a_service_the_source_still_answers() {
        let p = Pipeline::new(Cache::in_memory().unwrap(), None, Limits::default());
        assert!(p.resolve(&request(None), false).resolved.is_none());
        assert_eq!(origin(&p.resolve(&request(Some(Fake::new("source", found(LINES)))), false)), "source");
    }

    #[test]
    fn what_came_with_the_file_wins_outright() {
        let dir = std::env::temp_dir().join(format!("chilpa-lyrics-pipeline-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("song.flac");
        std::fs::write(&file, b"x").unwrap();
        std::fs::write(dir.join("song.lrc"), "Plain words next door\n").unwrap();
        let service = Fake::new("service", found(WORDS));
        let p = pipeline(service.clone());
        let outcome = p.resolve(&Request { track: track(), path: Some(file), source: None, language: None }, false);
        assert_eq!(origin(&outcome), "file");
        assert_eq!(service.calls(), 0, "nobody is asked when the file has lyrics, however fine the service's are");
        let _ = std::fs::remove_dir_all(dir);
    }
}
