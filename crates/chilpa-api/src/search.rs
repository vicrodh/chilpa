//! Search, joining what is local with whatever the plugins contribute.
//!
//! The rules come from the CHD-0002 spec and are not decoration:
//!
//! - **What is local comes first and at once.** A plugin never makes what is
//!   already on disk wait.
//! - **Each source arrives when it answers**, with its own time limit. One
//!   that does not answer is left out and marked; it does not stop the rest.
//! - **Chilpa orders and groups.** A plugin cannot reorder or hide another's
//!   results: what is its own is appended at the end, in the order it was
//!   registered.
//! - **What is the same song is shown once**, matched by ISRC or MBID,
//!   saying where else it is. Matching by name is not matching, so it is not
//!   done.
//! - **Never a query per keystroke.** It waits for the user to stop typing,
//!   and each source has a minimum rest between queries.

use super::*;
use chilpa_plugin::messages;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How long to wait for the user to stop typing before bothering a plugin.
pub const KEYSTROKE_DEBOUNCE: Duration = Duration::from_millis(350);

/// The least that can pass between two queries to the sources.
pub const SOURCE_COOLDOWN: Duration = Duration::from_millis(800);

/// How many results each source is asked for.
const PER_SOURCE: usize = 50;

/// Where a result already on disk came from.
const LOCAL: &str = "local";

/// Whether a selection of sources, "all" or a comma-separated list of
/// "local" and plugin ids, includes this one.
pub(crate) fn searches_source(selected: &str, candidate: &str) -> bool {
    selected.is_empty() || selected.split(',').any(|chosen| chosen == "all" || chosen == candidate)
}

/// How long a source's answer to a text is reused. A source is slow and its
/// results do not change from one minute to the next.
const ANSWER_FRESH: Duration = Duration::from_secs(600);

/// Sources' answers by source, text and how many were asked for.
fn answers() -> &'static Mutex<HashMap<(String, String, usize), (Instant, messages::Results)>> {
    static ANSWERS: std::sync::OnceLock<Mutex<HashMap<(String, String, usize), (Instant, messages::Results)>>> = std::sync::OnceLock::new();
    ANSWERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A search result, wherever it came from.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Hit {
    /// The path if it is local, or the external id if it is from a source.
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    /// Where it came from: `"local"` or the plugin's id.
    pub source: String,
    /// What that source is called for the user.
    pub source_name: String,
    /// The quality it offers, empty if unknown.
    pub quality: String,
    /// What the analysis found for a local track, empty if it has not been
    /// analysed. There is nothing to analyse on a remote one yet.
    pub analysis: String,
    /// The other sources where the same song is.
    pub also_in: Vec<String>,
    /// True if it can be started playing today.
    pub playable: bool,
}

impl Hit {
    fn local(t: LibraryTrack) -> Self {
        Self {
            id: t.path,
            title: t.title,
            artist: t.artist,
            album: t.album,
            duration_ms: t.duration_ms,
            source: LOCAL.into(),
            source_name: chilpa_i18n::t("My library"),
            quality: String::new(),
            analysis: t.analysis,
            also_in: Vec::new(),
            playable: t.present,
        }
    }

    pub(crate) fn remote(
        t: messages::RemoteTrack,
        source: &str,
        source_name: &str,
        delivers_audio: bool,
    ) -> Self {
        Self {
            id: t.external_id,
            title: t.title,
            artist: t.artist,
            album: t.album,
            duration_ms: t.duration_ms,
            source: source.to_string(),
            source_name: source_name.to_string(),
            quality: t.quality.describe(),
            analysis: String::new(),
            also_in: Vec::new(),
            // It can be attempted, unless the source has already said it
            // delivers no audio. On a click it is asked for
            // `chilpa_resolver` and whatever it answers is played; if it
            // cannot resolve or the link expired, it is said with its
            // reason, just like a local file that went missing.
            playable: delivers_audio,
        }
    }
}

/// What a source answered to a query.
#[derive(Clone, Debug)]
pub struct SourceReply {
    pub source: String,
    pub source_name: String,
    pub results: messages::Results,
    pub entities: Vec<serde_json::Value>,
    /// Empty if it answered fine.
    pub problem: String,
    /// True if that source can deliver audio. There are services whose API
    /// lets you search but never hands over bytes: what is theirs is shown
    /// greyed out from the start instead of failing on every click.
    pub delivers_audio: bool,
}

impl SourceReply {
    fn failed(source: String, source_name: String, problem: String) -> Self {
        Self {
            source,
            source_name,
            results: messages::Results::default(),
            entities: Vec::new(),
            problem,
            delivers_audio: true,
        }
    }
}

/// A local track's fingerprints: whatever ISRC and MBID it carries,
/// already normalized the way a plugin writes them.
fn fingerprints_of(isrc: Option<String>, mbid: Option<String>) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(f) = mbid.as_deref().and_then(messages::mbid_fingerprint) {
        v.push(f);
    }
    if let Some(f) = isrc.as_deref().and_then(messages::isrc_fingerprint) {
        v.push(f);
    }
    v
}

/// Joins what is local with what the plugins gave, with neither treading on the other.
///
/// `local_fingerprints` says which ISRC or MBID each local result has (by
/// path), so the same song is not shown twice.
pub fn merge(
    local: Vec<LibraryTrack>,
    local_fingerprints: &HashMap<String, Vec<String>>,
    replies: &[SourceReply],
) -> Vec<Hit> {
    let mut out: Vec<Hit> = local.into_iter().map(Hit::local).collect();

    // Where each fingerprint sits within what is already going to be shown.
    let mut at: HashMap<String, usize> = HashMap::new();
    for (i, hit) in out.iter().enumerate() {
        for fp in local_fingerprints.get(&hit.id).into_iter().flatten() {
            at.entry(fp.clone()).or_insert(i);
        }
    }

    for reply in replies {
        for track in &reply.results.tracks {
            match track.fingerprint().and_then(|f| at.get(&f).copied()) {
                // It was already there: it is not repeated, where else it lives is noted.
                Some(i) => {
                    let name = reply.source_name.clone();
                    if !out[i].also_in.contains(&name) {
                        out[i].also_in.push(name);
                    }
                }
                None => {
                    let hit = Hit::remote(
                        track.clone(),
                        &reply.source,
                        &reply.source_name,
                        reply.delivers_audio,
                    );
                    if let Some(f) = track.fingerprint() {
                        at.insert(f, out.len());
                    }
                    out.push(hit);
                }
            }
        }
    }
    out
}

/// A search across the plugins, running on its own thread.
pub struct SearchJob {
    text: String,
    replies: std::sync::mpsc::Receiver<SourceReply>,
    thread: Option<std::thread::JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
}

impl SearchJob {
    /// Fires the query only at the selected searchable sources.
    pub fn start_in(plugins: Arc<Mutex<plugins::Plugins>>, text: String, limit: usize, source: String) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stopped = cancelled.clone();
        let query = messages::write(&messages::Query::new(&text, limit));
        let text_asked = text.clone();
        let thread = std::thread::Builder::new()
            .name("chilpa-search".into())
            .spawn(move || {
                // The list is taken first and the lock is released between
                // plugins: a slow one must not leave the window waiting nor
                // block the rest of the facade.
                let Ok(who) = plugins.lock().map(|p| p.searchers()) else { return };
                std::thread::scope(|workers| {
                for (id, name) in who {
                    if stopped.load(Ordering::Relaxed) { return; }
                    if !searches_source(&source, &id) { continue; }
                    let natives = plugins.lock().ok().map(|p| p.natives.clone());
                    if let Some(natives) = natives.filter(|n| n.is_native(&id)) {
                        let (tx, text, stopped) = (tx.clone(), text_asked.clone(), stopped.clone());
                        workers.spawn(move || {
                            if stopped.load(Ordering::Relaxed) { return; }
                            let reply = crate::source_search::native_reply(&natives, &id, &name, &text, limit);
                            if !stopped.load(Ordering::Relaxed) { let _ = tx.send(reply); }
                        });
                        continue;
                    }
                    let delivers =
                        plugins.lock().map(|p| p.delivers_audio(&id)).unwrap_or(true);
                    // What this source answered to this text a moment ago comes back at once.
                    let asked = (id.clone(), text_asked.trim().to_lowercase(), limit);
                    let kept = answers().lock().ok().and_then(|a| a.get(&asked).filter(|(at, _)| at.elapsed() < ANSWER_FRESH).map(|(_, r)| r.clone()));
                    if let Some(results) = kept {
                        let reply = SourceReply { source: id, source_name: name, results, entities: Vec::new(), problem: String::new(), delivers_audio: delivers };
                        if tx.send(reply).is_err() { return; }
                        continue;
                    }
                    let answer = match plugins.lock() {
                        Ok(mut p) => {
                            if stopped.load(Ordering::Relaxed) { return; }
                            p.call_waiting(&id, messages::op::SEARCH, &query)
                        }
                        Err(_) => return,
                    };
                    let reply = match answer {
                        Ok(bytes) => match messages::read::<messages::Results>(&bytes) {
                            Ok(r) => SourceReply {
                                source: id,
                                source_name: name,
                                results: {
                                    let clean = r.clean();
                                    if let Ok(mut kept) = answers().lock() {
                                        kept.retain(|_, (at, _)| at.elapsed() < ANSWER_FRESH);
                                        kept.insert(asked, (Instant::now(), clean.clone()));
                                    }
                                    clean
                                },
                                entities: Vec::new(),
                                problem: String::new(),
                                delivers_audio: delivers,
                            },
                            Err(e) => SourceReply::failed(id, name, e),
                        },
                        Err(e) => SourceReply::failed(id, name, e),
                    };
                    if tx.send(reply).is_err() {
                        // There is nobody listening any more: the search changed.
                        return;
                    }
                }
                });
            })
            .ok();
        Self { text, replies: rx, thread, cancelled }
    }

    /// Which text this search is for.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whatever has arrived, without blocking.
    pub fn collect(&self) -> Vec<SourceReply> {
        let mut v = Vec::new();
        while let Ok(r) = self.replies.try_recv() {
            v.push(r);
        }
        v
    }

    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_some_and(|h| h.is_finished())
    }
}

impl Drop for SearchJob {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        // The thread ends on its own as soon as the channel closes; it is not
        // waited on, because an old search must not hold up the new one.
        self.thread.take();
    }
}

/// What the facade keeps about the search in progress.
#[derive(Default)]
pub(crate) struct SearchState {
    /// What the user has typed now.
    pub text: String,
    pub source: String,
    /// When they last typed.
    pub keystroke: Option<Instant>,
    /// When the sources were last asked.
    pub last_query: Option<Instant>,
    /// The query to the plugins, while it runs.
    pub job: Option<SearchJob>,
    /// What each source answered, one per source.
    pub replies: Vec<SourceReply>,
    /// What came out of the library, unmerged.
    pub local: Vec<LibraryTrack>,
    /// The sources whose results are shown whole, not their first few.
    pub expanded: std::collections::HashSet<String>,
    /// The fingerprints of what is local, by path.
    pub fingerprints: HashMap<String, Vec<String>>,
    /// Local and remote already joined, which is what the view draws.
    pub hits: Vec<Hit>,
    /// The source result that was sent to play, with the URL it resolved
    /// to. A stream's decoder knows nothing but the URL, so the card has
    /// to come from here.
    pub playing: Option<(String, Hit)>,
    /// True from when the results changed until someone picks them up.
    /// Without it the view would have to guess when to look: a query can
    /// start and finish between two ticks, and the last source to answer
    /// would go undrawn.
    pub changed: bool,
}

impl Chilpa {
    /// Searches the library and, if there are plugins, asks them too.
    ///
    /// What is local comes out at once, in the returned value. What the
    /// plugins give arrives later through [`Chilpa::poll`] and is picked up
    /// with [`Chilpa::found`].
    pub fn search_all(&mut self, text: &str, limit: usize) -> Vec<Hit> {
        self.search_in(text, limit, "all")
    }

    /// Selects the query destinations, not merely which replies are drawn.
    /// An absent source never falls back to querying every installed plugin.
    pub fn search_in(&mut self, text: &str, limit: usize, source: &str) -> Vec<Hit> {
        let source = if source.is_empty() { "all" } else { source };
        let text = text.trim().to_string();
        if text.is_empty() {
            self.search = SearchState { source: source.into(), playing: self.search.playing.take(), ..SearchState::default() };
            return Vec::new();
        }
        let local = if searches_source(source, LOCAL) { self.search(&text, limit) } else { Vec::new() };
        let fingerprints = self.fingerprints_of(&local);
        // A new search throws away what the previous one's sources answered:
        // showing results from another query is worse than showing nothing.
        self.search = SearchState {
            text,
            source: source.into(),
            keystroke: Some(Instant::now()),
            last_query: self.search.last_query,
            job: None,
            replies: Vec::new(),
            local,
            expanded: Default::default(),
            fingerprints,
            hits: Vec::new(),
            changed: false,
            // What is already playing goes on playing even if something else
            // is searched for, so its card is kept.
            playing: self.search.playing.take(),
        };
        self.rebuild_hits();
        self.search.hits.clone()
    }

    /// The last thing joined, for the view to draw.
    pub fn found(&self) -> &[Hit] {
        &self.search.hits
    }

    /// Starts the result at that position playing.
    ///
    /// What is local plays at once and queues what follows, as always.
    /// What is a source's has to be resolved first: the plugin answers with
    /// a URL, and from there on it is the same path as a radio.
    ///
    /// It returns false and leaves the reason in `now_playing().error` if it
    /// could not be done. A source that cannot resolve is not a failure of
    /// Chilpa's, but not something to swallow in silence either.
    pub fn play_hit(&mut self, index: usize, device: &str, bit_perfect: bool) -> bool {
        let Some(hit) = self.search.hits.get(index).cloned() else { return false };

        if hit.source == LOCAL {
            // From here down, skipping the remote ones: queueing something not
            // yet resolved would leave the queue waiting.
            let paths: Vec<String> = self.search.hits[index..]
                .iter()
                .filter(|h| h.source == LOCAL)
                .map(|h| h.id.clone())
                .collect();
            self.play_tracks(&paths, device, bit_perfect);
            return true;
        }

        let worker = self.plugins.as_ref().and_then(|plugins| plugins.lock().ok())
            .is_some_and(|plugins| plugins.worker_domains(&hit.source).is_some());
        if worker {
            if !hit.playable { return false; }
            let card = chilpa_library::store::progress::Heard { external_id: hit.id.clone(), title: hit.title.clone(),
                subtitle: hit.artist.clone(), duration_ms: hit.duration_ms, ..Default::default() };
            if !self.library().is_some_and(|library| library.remember_heard(&hit.source, &card).is_ok()) {
                self.now.error = chilpa_i18n::t("it could not be saved");
                return false;
            }
            // Keep the persistent identity in the queue. The remote opener
            // resolves it off the UI thread and checks every media redirect
            // against this worker's approved network domains.
            let entry = crate::source_opener::entry_of(&hit.source, &hit.id);
            self.search.playing = Some((entry.clone(), hit));
            return !self.play_tracks(&[entry], device, bit_perfect).is_empty();
        }

        match self.resolve(&hit) {
            Ok(stream) => {
                // A track in several parts travels as one queue entry, with
                // its parts joined by a space: a URL cannot carry one, so the
                // core splits them back without guessing.
                let url = stream.parts().join(" ");
                self.search.playing = Some((url.clone(), hit));
                self.play_url(&url, device, bit_perfect);
                true
            }
            Err(e) => {
                self.now.error = format!("{}: {e}", hit.source_name);
                false
            }
        }
    }

    /// Asks the source what one of its ids is played with.
    ///
    /// What it answers is checked before being believed: a URL that is not
    /// http is not attempted, and a link that already expired is said
    /// instead of being opened to fail with a 403 nobody can read.
    fn resolve(&mut self, hit: &Hit) -> Result<messages::Stream, String> {
        let request = messages::write(&messages::Resolve::new(&hit.id));
        let bytes = self.call_plugin_short(&hit.source, messages::op::RESOLVER, &request)?;
        let stream: messages::Stream = messages::read(&bytes)?;
        if !stream.usable() {
            return Err(chilpa_i18n::t("it answered something that is not an http address"));
        }
        if !stream.delivery.playable() {
            return Err(chilpa_i18n::t_args("it delivers {}", &[&messages::describe_delivery(stream.delivery)]));
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if stream.expired(now) {
            return Err(chilpa_i18n::t("the link it gave has already expired"));
        }
        Ok(stream)
    }

    /// Attaches the card the source gave to what is playing, if it is its own.
    pub(crate) fn name_from_source(&mut self, path: &std::path::Path) {
        let path = path.display().to_string();
        let Some((url, hit)) = &self.search.playing else { return };
        if *url != path {
            return;
        }
        self.now.title = hit.title.clone();
        self.now.artist = hit.artist.clone();
        self.now.album = hit.album.clone();
    }

    /// True if the results changed since they were last asked about. Asking
    /// marks them as seen.
    pub fn search_changed(&mut self) -> bool {
        std::mem::take(&mut self.search.changed)
    }

    /// True while some source has still not answered, so the view can say
    /// something is still missing instead of pretending that is all there
    /// is.
    pub fn search_running(&self) -> bool {
        self.search.job.as_ref().is_some_and(|j| !j.finished())
    }

    /// The sources that answered badly, to say so without hiding it.
    pub fn search_problems(&self) -> Vec<String> {
        self.search
            .replies
            .iter()
            .filter(|r| !r.problem.is_empty())
            .map(|r| format!("{}: {}", r.source_name, r.problem))
            .collect()
    }

    /// Moves the search along: it picks up whatever arrived and fires the
    /// query once the user stopped typing. `poll` calls it.
    pub(crate) fn advance_search(&mut self) {
        // What has already arrived, first.
        let arrived: Vec<SourceReply> =
            self.search.job.as_ref().map(|j| j.collect()).unwrap_or_default();
        if !arrived.is_empty() {
            for r in arrived {
                self.search.replies.retain(|other| other.source != r.source);
                self.search.replies.push(r);
            }
            self.rebuild_hits();
        }

        if self.search.text.is_empty() || self.search.source == LOCAL {
            return;
        }
        // They are still typing.
        if !self.search.keystroke.is_some_and(|t| t.elapsed() >= KEYSTROKE_DEBOUNCE) {
            return;
        }
        // This query has already been fired.
        if self.search.job.as_ref().is_some_and(|j| j.text() == self.search.text) {
            return;
        }
        // The sources were just asked: it waits.
        if self.search.last_query.is_some_and(|t| t.elapsed() < SOURCE_COOLDOWN) {
            return;
        }
        let Some(plugins) = self.plugins.clone() else { return };
        let any = plugins.try_lock().map(|p| p.searchers().iter().any(|(id, _)| searches_source(&self.search.source, id))).unwrap_or(false);
        if !any {
            return;
        }
        self.search.last_query = Some(Instant::now());
        self.search.job = Some(SearchJob::start_in(plugins, self.search.text.clone(), PER_SOURCE, self.search.source.clone()));
    }

    fn rebuild_hits(&mut self) {
        self.search.changed = true;
        self.search.hits = merge(
            self.search.local.clone(),
            &self.search.fingerprints,
            &self.search.replies,
        );
    }

    /// Which ISRC or MBID each local result has, in a single query.
    fn fingerprints_of(&mut self, local: &[LibraryTrack]) -> HashMap<String, Vec<String>> {
        if local.is_empty() {
            return HashMap::new();
        }
        let ids: Vec<i64> = local.iter().map(|t| t.id).collect();
        let by_id: HashMap<i64, String> = local.iter().map(|t| (t.id, t.path.clone())).collect();
        let Some(lib) = self.library() else { return HashMap::new() };
        let Ok(found) = lib.fingerprints_for(&ids) else { return HashMap::new() };
        found
            .into_iter()
            .filter_map(|(id, (isrc, mbid))| {
                let fps = fingerprints_of(isrc, mbid);
                if fps.is_empty() {
                    return None;
                }
                Some((by_id.get(&id)?.clone(), fps))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_selection_changes_destinations_without_falling_back_to_all() {
        assert!(searches_source("all", "local"));
        assert!(searches_source("all", "org.example.first"));
        assert!(!searches_source("local", "org.example.first"));
        assert!(!searches_source("org.example.first", "local"));
        assert!(searches_source("org.example.first", "org.example.first"));
        assert!(!searches_source("org.example.first", "org.example.second"));
        assert!(!searches_source("removed", "org.example.first"));
        assert!(searches_source("local,org.example.first", "org.example.first"));
        assert!(searches_source("local,org.example.first", "local"));
        assert!(!searches_source("local,org.example.first", "org.example.second"));
    }

    fn local(id: i64, title: &str) -> LibraryTrack {
        LibraryTrack {
            id,
            path: format!("/music/{title}.flac"),
            title: title.to_string(),
            artist: "Someone".into(),
            album: "An album".into(),
            track_number: None,
            duration_ms: 200_000,
            present: true,
            analysis: String::new(),
        }
    }

    fn remote(id: &str, title: &str, isrc: Option<&str>) -> messages::RemoteTrack {
        messages::RemoteTrack {
            external_id: id.to_string(),
            title: title.to_string(),
            artist: "Someone".into(),
            isrc: isrc.map(str::to_string),
            quality: messages::Quality {
                codec: Some("flac".into()),
                bit_depth: Some(24),
                sample_rate_hz: Some(96_000),
                channels: Some(2),
            },
            ..Default::default()
        }
    }

    fn reply(source: &str, tracks: Vec<messages::RemoteTrack>) -> SourceReply {
        SourceReply {
            source: source.to_string(),
            source_name: format!("Source {source}"),
            entities: Vec::new(),
            results: messages::Results { tracks, ..Default::default() },
            problem: String::new(),
            delivers_audio: true,
        }
    }

    /// One whose API lets you search but never delivers audio.
    fn control_only(source: &str, tracks: Vec<messages::RemoteTrack>) -> SourceReply {
        SourceReply { delivers_audio: false, ..reply(source, tracks) }
    }

    #[test]
    fn what_is_local_always_comes_first() {
        let mine = vec![local(1, "One"), local(2, "Another")];
        let replies = vec![reply("a", vec![remote("x", "De un plugin", None)])];
        let all = merge(mine, &HashMap::new(), &replies);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].source, LOCAL);
        assert_eq!(all[1].source, LOCAL);
        assert_eq!(all[2].source, "a", "what the plugin gave comes after");
    }

    #[test]
    fn the_same_song_shows_once_and_says_where_else_it_is() {
        let mine = vec![local(1, "One")];
        let mut fps = HashMap::new();
        fps.insert("/music/One.flac".to_string(), vec!["isrc:USRC17607839".to_string()]);
        let replies = vec![
            reply("a", vec![remote("x", "One", Some("USRC17607839"))]),
            reply("b", vec![remote("y", "One", Some("usrc17607839"))]),
        ];
        let all = merge(mine, &fps, &replies);
        assert_eq!(all.len(), 1, "it is the same song");
        assert_eq!(all[0].source, LOCAL);
        assert_eq!(all[0].also_in, vec!["Source a", "Source b"], "and where else it is gets said");
    }

    #[test]
    fn without_a_fingerprint_nothing_is_matched() {
        // Same title and same artist, but with no ISRC and no MBID: they are
        // two rows, because matching by name is guessing.
        let mine = vec![local(1, "One")];
        let replies = vec![reply("a", vec![remote("x", "One", None)])];
        assert_eq!(merge(mine, &HashMap::new(), &replies).len(), 2);
    }

    #[test]
    fn two_plugins_with_a_song_that_is_not_local_show_it_once() {
        let replies = vec![
            reply("a", vec![remote("x", "One", Some("USRC1"))]),
            reply("b", vec![remote("y", "One", Some("USRC1"))]),
        ];
        let all = merge(Vec::new(), &HashMap::new(), &replies);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].source, "a", "it keeps the first that answered");
        assert_eq!(all[0].also_in, vec!["Source b"]);
    }

    #[test]
    fn a_source_that_did_not_answer_does_not_erase_the_others() {
        let broken = SourceReply::failed(
            "broken".into(),
            "Source broken".into(),
            "it went over the allowed compute".into(),
        );
        let replies = vec![broken, reply("b", vec![remote("y", "One", None)])];
        let all = merge(vec![local(1, "Local")], &HashMap::new(), &replies);
        assert_eq!(all.len(), 2, "what is local and what the one that answered gave");
    }

    #[test]
    fn the_quality_of_a_source_shows_and_the_local_one_is_not_invented() {
        let replies = vec![reply("a", vec![remote("x", "One", None)])];
        let all = merge(vec![local(1, "Local")], &HashMap::new(), &replies);
        assert_eq!(all[0].quality, "", "no quality is invented here for what is local");
        assert_eq!(all[1].quality, "FLAC · 24 bits a 96000 Hz");
    }

    #[test]
    fn a_source_that_cannot_deliver_audio_says_so_from_the_start() {
        // There are services whose API lets you search and browse but never
        // hands bytes to a third party. Their results come out greyed from
        // the start: letting every click fail would be worse.
        let replies = vec![control_only("a", vec![remote("x", "One", None)])];
        let all = merge(Vec::new(), &HashMap::new(), &replies);
        assert!(!all[0].playable);
        assert_eq!(all[0].source_name, "Source a", "and whose it is, is known");
    }

    #[test]
    fn what_a_plugin_offers_can_be_tried() {
        // It is marked playable because on a click the source is asked for
        // `chilpa_resolver`. Whether it answers is its own business, and if
        // it cannot it is said with its name; greying out everything remote
        // just in case would be a list that does nothing.
        let replies = vec![reply("a", vec![remote("x", "One", None)])];
        let all = merge(Vec::new(), &HashMap::new(), &replies);
        assert!(all[0].playable);
    }

    #[test]
    fn the_mbid_wins_over_the_isrc_and_both_sides_write_it_the_same() {
        // The local library and a plugin have to produce exactly the same
        // string, or matching matches nothing.
        let mbid = "B1A9C0E9-D987-4042-AE91-78D6A3267D69";
        let mut t = remote("x", "One", Some("usrc1"));
        t.musicbrainz_recording_id = Some(mbid.to_string());
        let local = fingerprints_of(Some("usrc1".into()), Some(mbid.to_ascii_lowercase()));
        assert_eq!(t.fingerprint().unwrap(), local[0]);
        assert_eq!(local.len(), 2, "the local side exposes both, in case the other shore carries one");
    }
}
