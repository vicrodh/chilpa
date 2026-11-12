//! Queue entries that cannot be played this run, and what the queue does
//! when one comes up.
//!
//! The player passes over a track that will not open or breaks partway, and
//! says which and why (`PlayerEvent::Skipped`); it fails only on the output,
//! or when nothing it reached could play. The facade does not read the
//! failure's text. Before the player gets there, it judges the entries
//! itself: a local file it looks for, and a server's or a source's item the
//! opener reports when it fails to open it. What is found unavailable, or
//! what the player passed over, is marked for the run and left out of the
//! player's walk in the queue's own order, so shuffle, repeat and the stop
//! mark still hold. A failure once a track of the launch has started, or with
//! something playable where the player was going, is the output's, and stops
//! playback as it always did: an unplugged DAC must not walk the whole queue.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::{Chilpa, NowPlaying, Relaunch, Repeat, TrackRow};

/// Why an entry cannot be played.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Unavailable {
    /// The file, or the disc image it is read from, is not where the queue
    /// says.
    Missing,
    /// It is there and may not be read.
    NoAccess,
    /// Its server or source could not open it, and said why.
    Source(String),
    /// It is there and the player could not read it: it would not open, or
    /// it broke partway. The player passed over it and said why.
    Unreadable(String),
}

impl Unavailable {
    /// For the playback trace.
    fn said(&self) -> &str {
        match self {
            Unavailable::Missing => "missing",
            Unavailable::NoAccess => "no access",
            Unavailable::Source(why) | Unavailable::Unreadable(why) => why,
        }
    }
}

/// What one look at an entry says.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Look {
    Available,
    Unavailable(Unavailable),
    /// It cannot be told without the network or the drive.
    Unknown,
}

/// Looks for an entry's file, the way the player opens it. A server's or a
/// source's item, an address and a disc are not looked at: the network or
/// the drive would have to answer, and the window thread asks neither.
pub(crate) fn look(path: &str) -> Look {
    if path.is_empty() || chilpa_models::RemoteEntry::is_remote(path) || path.contains("://") || path.starts_with("cdda:") {
        return Look::Unknown;
    }
    let file = chilpa_core::sacd_tracks::parse(Path::new(path)).map_or_else(|| PathBuf::from(path), |(image, _)| image);
    let judged = |error: std::io::Error| match error.kind() {
        ErrorKind::NotFound | ErrorKind::NotADirectory => Look::Unavailable(Unavailable::Missing),
        ErrorKind::PermissionDenied => Look::Unavailable(Unavailable::NoAccess),
        _ => Look::Unknown,
    };
    match std::fs::metadata(&file) {
        // Only a plain file is opened: opening a pipe waits for a writer.
        Ok(found) if found.is_file() => std::fs::File::open(&file).map_or_else(judged, |_| Look::Available),
        Ok(_) => Look::Unknown,
        Err(error) => judged(error),
    }
}

/// Where a walk along the queue ended.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Walk {
    /// The first entry worth trying; none when the pass found nothing.
    pub target: Option<usize>,
    /// The unavailable entries passed over, in the order walked.
    pub passed: Vec<usize>,
    /// The stop mark was on one of them: nothing after it plays.
    pub reached_mark: bool,
}

/// Walks `order` from `from`, itself first when `include`, the way the
/// player goes: one pass at most, back to the start only with `wrap`, rows
/// the listener removed passed in silence and unavailable ones collected.
pub(crate) fn walk(
    order: &[usize],
    from: usize,
    include: bool,
    wrap: bool,
    stop_after: Option<usize>,
    removed: impl Fn(usize) -> bool,
    mut unavailable: impl FnMut(usize) -> bool,
) -> Walk {
    let mut walk = Walk::default();
    let Some(at) = order.iter().position(|&index| index == from) else {
        walk.target = Some(from);
        return walk;
    };
    let (count, first) = (order.len(), usize::from(!include));
    for step in first..first + count {
        if at + step >= count && !wrap {
            break;
        }
        let index = order[(at + step) % count];
        if removed(index) {
            continue;
        }
        if !unavailable(index) {
            walk.target = Some(index);
            break;
        }
        walk.passed.push(index);
        if stop_after == Some(index) {
            walk.reached_mark = true;
            break;
        }
    }
    walk
}

/// An entry found unavailable, and the launch that found it.
struct Mark {
    why: Unavailable,
    launch: u64,
}

/// The launch in flight: where it started, whether a track of it has
/// started, and the player's generation for it.
#[derive(Clone)]
struct Launch {
    from: usize,
    started: bool,
    generation: u64,
}

/// The run's unavailable entries and the launch in flight.
#[derive(Default)]
pub(crate) struct State {
    /// By queue entry, so a track queued twice is judged once.
    marked: HashMap<String, Mark>,
    launch: Option<Launch>,
    /// Launches so far. A mark made before the current one was handed to
    /// the player as a removed row, so it cannot be what the player failed on.
    launches: u64,
    /// Grows when the marks change, so the queue's rows are read again.
    revision: u64,
    reports: Reports,
}

impl State {
    pub(crate) fn reports(&self) -> Reports {
        self.reports.clone()
    }
}

/// What the remote opener passed over, as it happens on the player's thread,
/// and how fast it has been failing. Every opener the facade installs shares
/// it.
#[derive(Clone, Default)]
pub(crate) struct Reports(Arc<Mutex<Held>>);

#[derive(Default)]
struct Held {
    entries: Vec<(String, String)>,
    failing: u32,
    last: Option<Instant>,
}

/// At most this many reports wait for the facade.
const KEPT: usize = 256;
/// Failures further apart than this are not one burst.
const BURST: Duration = Duration::from_secs(3);
/// The wait before each remote open once a burst has failed twice: at most
/// five tries a second against a server that keeps saying no.
const PACE: Duration = Duration::from_millis(200);

impl Reports {
    /// The opener could not open `entry`. `online` is false when no request
    /// went out, which is no reason to slow down.
    pub(crate) fn failed(&self, entry: String, why: &str, online: bool) {
        let mut held = self.lock();
        if held.entries.len() < KEPT {
            held.entries.push((entry, why.to_string()));
        }
        if online {
            let now = Instant::now();
            held.failing = if held.last.is_some_and(|at| now.duration_since(at) < BURST) { held.failing.saturating_add(1) } else { 1 };
            held.last = Some(now);
        }
    }

    /// How long the next remote open waits.
    pub(crate) fn pause(&self) -> Duration {
        let held = self.lock();
        pause(held.failing, held.last.map(|at| at.elapsed()))
    }

    fn take(&self) -> Vec<(String, String)> {
        std::mem::take(&mut self.lock().entries)
    }

    fn lock(&self) -> MutexGuard<'_, Held> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Two failures in a burst open freely; from the third on, each open waits.
fn pause(failing: u32, since: Option<Duration>) -> Duration {
    match since {
        Some(since) if failing >= 2 && since < BURST => PACE,
        _ => Duration::ZERO,
    }
}

/// Whether a queue entry is the one the opener reported.
fn same_entry(path: &str, entry: &str) -> bool {
    path == entry || chilpa_models::RemoteEntry::parse(path).is_some_and(|parsed| parsed.to_entry() == entry)
}

impl Chilpa {
    /// Before a launch: a run starts, unless this launch moves the output,
    /// and the start moves past what cannot be played. `None` when nothing
    /// from there can; the queue has then been ended.
    pub(crate) fn playable_start(&mut self, from: usize, why: Relaunch) -> Option<usize> {
        if why == Relaunch::Output {
            return Some(from);
        }
        self.new_unavailable_run();
        let walk = self.walk_unavailable(from, true);
        match walk.target {
            Some(target) if !walk.reached_mark => Some(target),
            _ => {
                self.end_unplayable(from);
                None
            }
        }
    }

    /// The launch went out from `from`, as the player's `generation`.
    pub(crate) fn launched_from(&mut self, from: usize, generation: u64) {
        self.unavailable.launches += 1;
        self.unavailable.launch = Some(Launch { from, started: false, generation });
        if self.now.error.is_empty() {
            self.say_skipped();
        }
    }

    /// The rows the player passes without opening: the ones the listener
    /// removed and the ones found unavailable.
    pub(crate) fn removed_for_player(&self) -> Vec<bool> {
        if self.unavailable.marked.is_empty() {
            return self.removed.clone();
        }
        let gone = |index: usize| self.removed.get(index).copied().unwrap_or(false);
        (0..self.queue.len()).map(|index| gone(index) || self.marked_at(index)).collect()
    }

    /// A track started: what comes after it is looked at now, while it
    /// plays, and what cannot be played is taken out of the player's walk.
    /// The output stays open and gapless holds across the gap.
    pub(crate) fn look_past_unavailable(&mut self) {
        if let Some(launch) = &mut self.unavailable.launch {
            launch.started = true;
        }
        let at = self.now.index;
        if self.casting.active() || self.repeat == Repeat::One || self.stop_after == Some(at) {
            return;
        }
        let walk = self.walk_unavailable(at, false);
        self.leave_out(&walk.passed);
        // The listener asked to stop before what follows the mark, and
        // nothing between here and the mark can play.
        if walk.reached_mark {
            self.set_stop_after(Some(at));
        }
        if !walk.passed.is_empty() {
            self.say_skipped();
        }
    }

    /// Marks what the remote opener reported, and takes it out of the
    /// player's walk for the laps that follow.
    pub(crate) fn take_unavailable_reports(&mut self) {
        let reports = self.unavailable.reports.take();
        if reports.is_empty() {
            return;
        }
        let mut marked = false;
        for (entry, why) in reports {
            let positions: Vec<usize> = (0..self.queue.len()).filter(|&index| same_entry(&self.queue[index], &entry)).collect();
            let Some(&first) = positions.first() else { continue };
            let path = self.queue[first].clone();
            if !self.unavailable.marked.contains_key(&path) {
                let launch = self.unavailable.launches;
                self.unavailable.marked.insert(path, Mark { why: Unavailable::Source(why), launch });
                marked = true;
            }
            self.leave_out(&positions);
        }
        if marked {
            self.unavailable.revision += 1;
            self.say_skipped();
        }
    }

    /// The player passed over a queue entry it could not play -- it would not
    /// open, or it broke partway -- and went on. The entry is marked like one
    /// found missing, left out of the laps that follow, and counted.
    pub(crate) fn player_skipped(&mut self, generation: u64, index: usize, path: &Path, why: String) {
        let current = self.unavailable.launch.as_ref().is_some_and(|launch| launch.generation == generation);
        chilpa_core::trace_playback("facade", "unavailable_passed", format_args!("index={} generation={generation} current={current} why={why:?}", self.base + index));
        // Said by a play this launch replaced: the queue it was about is gone,
        // and the same entry in this one may well play.
        if !current {
            return;
        }
        let positions: Vec<usize> = (0..self.queue.len()).filter(|&at| Path::new(&self.queue[at]) == path).collect();
        let Some(&first) = positions.first() else { return };
        let entry = self.queue[first].clone();
        if !self.unavailable.marked.contains_key(&entry) {
            let launch = self.unavailable.launches;
            self.unavailable.marked.insert(entry, Mark { why: Unavailable::Unreadable(why), launch });
            self.unavailable.revision += 1;
        }
        self.leave_out(&positions);
        self.say_skipped();
    }

    /// The player failed. When nothing it reached in this launch could be
    /// played, the queue ends as one that ran out; otherwise playback stops
    /// and says why.
    pub(crate) fn player_failed(&mut self, message: String) {
        if self.skip_failure() {
            return;
        }
        self.now.playing = false;
        self.stop_after = None;
        self.sleep_until = None;
        self.now.error = message;
        // A device failure is not a listen, but whatever did play is
        // logged, marked as interrupted.
        self.log_play(true);
    }

    /// Grows when the marks change: the queue's rows are read again.
    pub(crate) fn unavailable_revision(&self) -> u64 {
        self.unavailable.revision
    }

    /// Dims the rows of the playing queue that this run found unavailable.
    pub(crate) fn dim_unavailable(&self, buffer: u64, rows: &mut [TrackRow]) {
        if buffer != self.listen.active || self.unavailable.marked.is_empty() {
            return;
        }
        for row in rows.iter_mut().filter(|row| self.unavailable.marked.contains_key(&row.path)) {
            row.available = false;
        }
    }

    /// Whether a position of the queue was found unavailable this run.
    pub(crate) fn marked_at(&self, index: usize) -> bool {
        self.queue.get(index).is_some_and(|path| self.unavailable.marked.contains_key(path))
    }

    /// Whether this run skipped anything, so its count is still said once
    /// the queue is over.
    pub(crate) fn skipped_any(&self) -> bool {
        !self.unavailable.marked.is_empty()
    }

    fn new_unavailable_run(&mut self) {
        // What the opener said during the last run is not this run's.
        self.unavailable.reports.take();
        if !self.unavailable.marked.is_empty() {
            self.unavailable.marked.clear();
            self.unavailable.revision += 1;
        }
    }

    /// Walks the queue as the player would, marking what cannot be played.
    fn walk_unavailable(&mut self, from: usize, include: bool) -> Walk {
        let order = self.full_queue_order();
        let (wrap, stop_after, launch) = (self.repeat == Repeat::All, self.stop_after, self.unavailable.launches);
        let (queue, removed, marked) = (&self.queue, &self.removed, &mut self.unavailable.marked);
        let mut changed = false;
        let walk = walk(&order, from, include, wrap, stop_after, |index| removed.get(index).copied().unwrap_or(false), |index| {
            let Some(path) = queue.get(index) else { return false };
            if marked.contains_key(path) {
                return true;
            }
            let Look::Unavailable(why) = look(path) else { return false };
            chilpa_core::trace_playback("facade", "unavailable_marked", format_args!("index={index} why={why:?}"));
            marked.insert(path.clone(), Mark { why, launch });
            changed = true;
            true
        });
        if changed {
            self.unavailable.revision += 1;
        }
        walk
    }

    /// Takes positions out of the player's walk; it passes them unopened.
    fn leave_out(&self, positions: &[usize]) {
        let Some(player) = &self.player else { return };
        for &index in positions.iter().filter(|&&index| index >= self.base) {
            player.remove_from_queue(index - self.base);
        }
    }

    /// The run's skips, said as one count. The same text set again is no
    /// change, so the window's toast updates in place instead of stacking.
    fn say_skipped(&mut self) {
        let count = self.unavailable.marked.len();
        if count > 0 {
            self.now.error = chilpa_i18n::tf("Skipped {} unavailable track", "Skipped {} unavailable tracks", count as i64, &[&count.to_string()]);
        }
    }

    /// Whether the failure was the player running out of what it could play
    /// in this launch; if so, the queue ended as a finished one. The player
    /// passes over what will not open or breaks partway and says so as it
    /// goes, so it fails on a track only when nothing it reached played: once
    /// a track of the launch has started, or where something playable stands
    /// on its way, the failure is the output's and is not skipped.
    fn skip_failure(&mut self) -> bool {
        self.take_unavailable_reports();
        let Some(launch) = self.unavailable.launch.clone() else { return false };
        if launch.started || self.casting.active() || self.queue.is_empty() {
            return false;
        }
        let walk = self.walk_unavailable(launch.from, true);
        // Only an entry marked during this launch can be what the player
        // failed on: one marked before was handed to it as a removed row.
        let current = self.unavailable.launches;
        let fresh = walk.passed.iter().any(|&index| {
            self.queue.get(index).and_then(|path| self.unavailable.marked.get(path)).is_some_and(|mark| mark.launch == current)
        });
        if !fresh || (walk.target.is_some() && !walk.reached_mark) {
            return false;
        }
        let why: Vec<&str> = walk.passed.iter()
            .filter_map(|&index| self.queue.get(index).and_then(|path| self.unavailable.marked.get(path)))
            .map(|mark| mark.why.said()).collect();
        chilpa_core::trace_playback("facade", "unavailable_skipped", format_args!("passed={:?} why={why:?} mark={}", walk.passed, walk.reached_mark));
        self.say_skipped();
        // The player has returned and closed its output.
        self.now.playing = false;
        self.finish_queue();
        true
    }

    /// Nothing from `from` on can be played: whatever played stops, and the
    /// window shows the queue as one that ran out, with the count.
    fn end_unplayable(&mut self, from: usize) {
        if self.player.is_some() && self.now.playing {
            self.stop();
        }
        self.set_stop_after(None);
        self.sleep_until = None;
        self.unavailable.launch = None;
        let lyrics_revision = self.now.lyrics_revision + 1;
        self.now = NowPlaying { index: from, queue_len: self.queue.len(), lyrics_revision, ..NowPlaying::default() };
        self.say_skipped();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unavailable_in(gone: &'static [usize]) -> impl FnMut(usize) -> bool {
        move |index| gone.contains(&index)
    }

    #[test]
    fn a_file_that_is_not_there_is_missing_and_one_that_is_is_available() {
        let folder = tempfile::tempdir().unwrap();
        let here = folder.path().join("here.flac");
        std::fs::write(&here, b"x").unwrap();
        assert_eq!(look(&here.display().to_string()), Look::Available);
        let gone = folder.path().join("gone.flac").display().to_string();
        assert_eq!(look(&gone), Look::Unavailable(Unavailable::Missing));
        let under_a_file = here.join("inner.flac").display().to_string();
        assert_eq!(look(&under_a_file), Look::Unavailable(Unavailable::Missing));
        let image = format!("sacd:{}#2", folder.path().join("gone.iso").display());
        assert_eq!(look(&image), Look::Unavailable(Unavailable::Missing), "a disc image track is its image");
        assert_eq!(look(&folder.path().display().to_string()), Look::Unknown, "only a plain file is opened");
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_may_not_be_read_is_unavailable() {
        use std::os::unix::fs::PermissionsExt;
        let folder = tempfile::tempdir().unwrap();
        let locked = folder.path().join("locked.flac");
        std::fs::write(&locked, b"x").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Whoever can read anything reads it anyway.
        if std::fs::File::open(&locked).is_err() {
            assert_eq!(look(&locked.display().to_string()), Look::Unavailable(Unavailable::NoAccess));
        }
    }

    #[test]
    fn what_needs_the_network_or_a_drive_is_not_judged() {
        let entry = chilpa_models::RemoteEntry::new(chilpa_models::SourceId::new("plugin", "app.example"), "gone").to_entry();
        assert_eq!(look(&entry), Look::Unknown);
        assert_eq!(look("https://radio.example/stream"), Look::Unknown);
        assert_eq!(look("cdda:/dev/sr0"), Look::Unknown);
        assert_eq!(look(""), Look::Unknown);
    }

    #[test]
    fn a_walk_follows_the_order_and_stops_at_the_first_playable() {
        let order = [3, 0, 4, 1, 2];
        let walk = walk(&order, 0, false, false, None, |_| false, unavailable_in(&[4, 1]));
        assert_eq!(walk, Walk { target: Some(2), passed: vec![4, 1], reached_mark: false });
    }

    #[test]
    fn an_inclusive_walk_judges_its_own_start() {
        let walk = walk(&[0, 1, 2], 0, true, false, None, |_| false, unavailable_in(&[0]));
        assert_eq!(walk.target, Some(1));
        assert_eq!(walk.passed, [0]);
    }

    #[test]
    fn removed_rows_are_passed_without_being_judged() {
        let walk = walk(&[0, 1, 2], 0, false, false, None, |index| index == 1, |index| {
            assert_ne!(index, 1, "a removed row is not looked at");
            false
        });
        assert_eq!(walk, Walk { target: Some(2), passed: Vec::new(), reached_mark: false });
    }

    #[test]
    fn the_end_of_the_queue_ends_the_walk_unless_it_wraps() {
        let unwrapped = walk(&[0, 1, 2], 1, false, false, None, |_| false, unavailable_in(&[2]));
        assert_eq!(unwrapped.target, None);
        let wrapped = walk(&[0, 1, 2], 1, false, true, None, |_| false, unavailable_in(&[2]));
        assert_eq!(wrapped.target, Some(0));
        let alone = walk(&[0, 1, 2], 1, false, true, None, |_| false, unavailable_in(&[0, 2]));
        assert_eq!(alone.target, Some(1), "with repeat-all the playing track comes round again");
    }

    #[test]
    fn a_walk_over_a_queue_where_nothing_plays_is_one_pass() {
        let mut looked = 0;
        let walk = walk(&[0, 1, 2, 3], 2, true, true, None, |_| false, |_| {
            looked += 1;
            true
        });
        assert_eq!(walk.target, None);
        assert_eq!(walk.passed, [2, 3, 0, 1]);
        assert_eq!(looked, 4, "each entry once");
    }

    #[test]
    fn the_stop_mark_on_an_unavailable_entry_ends_the_walk() {
        let walk = walk(&[0, 1, 2], 0, false, false, Some(1), |_| false, unavailable_in(&[1]));
        assert_eq!(walk, Walk { target: None, passed: vec![1], reached_mark: true });
    }

    #[test]
    fn an_entry_outside_the_order_is_tried_as_it_is() {
        let walk = walk(&[0, 1], 7, true, false, None, |_| false, |_| panic!("nothing to judge"));
        assert_eq!(walk.target, Some(7));
    }

    #[test]
    fn a_burst_of_failures_slows_the_next_opens_and_a_quiet_spell_resets_it() {
        assert_eq!(pause(0, None), Duration::ZERO);
        assert_eq!(pause(1, Some(Duration::ZERO)), Duration::ZERO, "one failure is no burst");
        assert_eq!(pause(2, Some(Duration::from_millis(50))), PACE);
        assert_eq!(pause(40, Some(BURST)), Duration::ZERO, "the burst is over");
        let reports = Reports::default();
        reports.failed("a".into(), "gone", true);
        assert_eq!(reports.pause(), Duration::ZERO);
        reports.failed("b".into(), "gone", true);
        assert_eq!(reports.pause(), PACE);
        let offline = Reports::default();
        for entry in ["a", "b", "c"] {
            offline.failed(entry.into(), "offline", false);
        }
        assert_eq!(offline.pause(), Duration::ZERO, "nothing went out, nothing to slow down");
        assert_eq!(offline.take().len(), 3);
        assert!(offline.take().is_empty(), "taken once");
    }

    #[test]
    fn reports_are_bounded() {
        let reports = Reports::default();
        for n in 0..KEPT + 10 {
            reports.failed(n.to_string(), "gone", false);
        }
        assert_eq!(reports.take().len(), KEPT);
    }

    #[test]
    fn a_reported_entry_matches_its_queue_row() {
        let entry = chilpa_models::RemoteEntry::new(chilpa_models::SourceId::new("plugin", "app.example"), "track-1").to_entry();
        assert!(same_entry(&entry, &entry));
        assert!(!same_entry("/music/a.flac", &entry));
    }
}
