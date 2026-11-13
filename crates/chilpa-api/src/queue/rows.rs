//! The rows of the queue in front of the window, and the two ways they
//! change.
//!
//! A play names its tracks by position: what it says is about the rows it
//! was launched on. Rows put in the place of others make every position
//! another track's, so that is done in one place, which counts it and takes
//! the play in front of the window off it; and rows added behind the ones
//! that stand, which move nothing, are added in another.

use crate::plays::{Fate, Replaced};
use crate::Chilpa;

impl Chilpa {
    /// Puts other rows in the place of the queue's. The play in front of
    /// the window, if there is one, was launched on the rows that go: it
    /// leaves the window and is told to stop, and nothing is playing until a
    /// launch on the new rows says so.
    pub(crate) fn swap_rows(&mut self, rows: Vec<String>) {
        self.queue = rows;
        self.rows_epoch = self.rows_epoch.wrapping_add(1);
        if let Some(play) = self.plays.retire(Fate::Replaced(Replaced::Track)) {
            chilpa_core::trace_playback("facade", "rows_swapped_under_play", format_args!("play={play} epoch={}", self.rows_epoch));
            // As for a hand-off: a launch on the new rows most often follows.
            if let Some(player) = &self.player {
                player.stop_for_handoff();
            }
            self.now.playing = false;
        }
    }

    /// Adds rows behind the ones that stand. No position changes, so the
    /// play in front of the window goes on.
    pub(crate) fn extend_rows(&mut self, added: impl IntoIterator<Item = String>) {
        self.queue.extend(added);
    }

    /// Makes `rows` the queue's when they are the rows that stand with more
    /// behind them, which is all an edit of the playing queue may do. Rows
    /// that are not are swapped in instead, and what plays stops.
    pub(crate) fn grow_rows_to(&mut self, rows: &[String]) {
        if rows.starts_with(&self.queue) {
            let standing = self.queue.len();
            self.extend_rows(rows[standing..].iter().cloned());
        } else {
            debug_assert!(false, "an edit of the playing queue moved rows that stand");
            chilpa_core::trace_playback("facade", "rows_not_a_prefix", format_args!("standing={} given={}", self.queue.len(), rows.len()));
            self.swap_rows(rows.to_vec());
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::plays::{Fate, Replaced};
    use crate::{Chilpa, Relaunch};

    fn playing(play: u64, rows: &[&str]) -> Chilpa {
        let mut api = Chilpa::with_listed(Vec::new(), Vec::new());
        api.swap_rows(rows.iter().map(|row| row.to_string()).collect());
        api.launched(play, 0, Relaunch::Track);
        api.now.playing = true;
        api
    }

    #[test]
    fn swapping_rows_counts_it_and_takes_the_play_off_the_window() {
        let mut api = playing(1, &["/a", "/b"]);
        let epoch = api.rows_epoch;
        api.swap_rows(vec!["/c".into()]);
        assert_eq!(api.rows_epoch, epoch + 1);
        assert_eq!(api.plays.get(1).map(|play| play.fate), Some(Fate::Replaced(Replaced::Track)));
        assert!(api.plays.current().is_none());
        assert!(!api.now.playing, "nothing plays until a launch says so");
    }

    #[test]
    fn adding_rows_behind_keeps_the_play_and_the_count() {
        let mut api = playing(2, &["/a", "/b"]);
        let epoch = api.rows_epoch;
        api.extend_rows(["/c".to_string()]);
        api.grow_rows_to(&["/a".into(), "/b".into(), "/c".into(), "/d".into()]);
        assert_eq!(api.queue, ["/a", "/b", "/c", "/d"]);
        assert_eq!(api.rows_epoch, epoch, "no position changed");
        assert_eq!(api.plays.current().map(|play| play.id), Some(2));
        assert!(api.now.playing);
    }

    #[test]
    fn a_play_launched_on_rows_that_went_is_not_in_front_of_the_window() {
        let mut api = playing(3, &["/a"]);
        // Its rows change without it being retired, which only this file can do.
        api.rows_epoch += 1;
        api.apply_player_event(3, chilpa_core::player::PlayerEvent::Progress { frames: 9, played: 9 }, &mut false);
        assert_eq!(api.now.frames, 0, "what it says is about rows that are gone");
    }

    /// The queue's rows are written in this file and nowhere else.
    #[test]
    fn nothing_else_writes_the_rows() {
        let writes = ["self.queue = ", "self.queue.clear()", "self.queue.extend", "self.queue.push", "self.queue.insert", "self.queue.remove", "self.queue.truncate", "self.queue.drain", "self.queue.retain", "self.queue.swap", "mem::take(&mut self.queue)", "&mut self.queue)", "&mut self.queue,", "&mut self.queue;"];
        // Files whose `queue` is another thing: a list of jobs of their own.
        let other = ["now_about/memory.rs", "servers/offline/worker/queue.rs", "queue/rows.rs"];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut pending = vec![root.clone()];
        let mut found = Vec::new();
        while let Some(folder) = pending.pop() {
            for entry in std::fs::read_dir(&folder).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                let name = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
                if !name.ends_with(".rs") || name.ends_with("_tests.rs") || name.ends_with("/tests.rs") || other.contains(&name.as_str()) {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                // What a file's own tests set up is not the facade writing.
                let code = text.split("#[cfg(test)]").next().unwrap_or_default();
                for (number, line) in code.lines().enumerate() {
                    if writes.iter().any(|write| line.contains(write)) {
                        found.push(format!("{name}:{}", number + 1));
                    }
                }
            }
        }
        assert!(found.is_empty(), "rows written outside queue/rows.rs: {found:?}");
    }
}
