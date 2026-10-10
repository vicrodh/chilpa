//! The crash chain of the restore paths, ported from QBZ (`nav_qt.rs`,
//! `arm_startup_probe`).
//!
//! A restore that crashes is a trap: the app dies at start, and every retry
//! restores the same thing and dies again, with no way out from inside the
//! window. A probe file counts the starts that never reached a shown window,
//! and each level bypasses more of what is restored. It never deletes
//! state, it only goes around it:
//!
//! - level 1 (the previous start reached the window): restore everything;
//! - level 2 (one start died first): the remembered tabs are left, Home opens;
//! - level 3 or more: the kept queue is not restored either, for this start
//!   only; its file stays on disk and is not written over with an empty one.

use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};

static LEVEL: AtomicU8 = AtomicU8::new(0);

fn probe() -> std::path::PathBuf {
    chilpa_dirs::data().join("startup_probe")
}

fn arm_at(path: &Path) -> u8 {
    let before: u8 = std::fs::read_to_string(path).ok().and_then(|t| t.trim().parse().ok()).unwrap_or(0);
    let level = before.saturating_add(1);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, level.to_string());
    level
}

/// Reads and raises the probe. Once, early in the start, before anything is
/// restored.
pub fn arm() {
    let level = arm_at(&probe());
    LEVEL.store(level, Ordering::Relaxed);
    if level > 1 {
        eprintln!("startup: level {level}; {} start(s) before this one died before the window was shown, so less is restored", level - 1);
    }
}

/// The window showed what it restored: the next start restores everything.
pub fn mark_healthy() {
    let _ = std::fs::write(probe(), "0");
}

/// This start's level; 1 when the previous one reached the window.
pub fn level() -> u8 {
    LEVEL.load(Ordering::Relaxed).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_start_that_dies_raises_the_level_and_a_shown_window_clears_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data").join("startup_probe");
        assert_eq!(arm_at(&path), 1, "a first start restores everything");
        assert_eq!(arm_at(&path), 2, "one start died: the tabs are left");
        assert_eq!(arm_at(&path), 3, "two died: the queue is left too");
        std::fs::write(&path, "0").unwrap();
        assert_eq!(arm_at(&path), 1, "after a shown window, back to one");
        std::fs::write(&path, "nonsense").unwrap();
        assert_eq!(arm_at(&path), 1, "an unreadable probe counts as clean");
        std::fs::write(&path, "255").unwrap();
        assert_eq!(arm_at(&path), 255, "it never wraps around to clean");
    }

    #[test]
    fn a_start_that_never_armed_is_level_one() {
        assert!(level() >= 1);
    }
}
