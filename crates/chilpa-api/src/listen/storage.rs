use super::*;

fn file() -> std::path::PathBuf { chilpa_dirs::data().join("listenlist.json") }
impl Chilpa {
    pub(crate) fn persist_buffers(&self) {
        self.protect_queue_catalog();
        self.write_buffer_snapshot();
    }
    /// I protect the live queue and every inactive buffer on edits, even
    /// when session persistence is disabled. Local files need no catalog pin.
    pub(crate) fn protect_queue_catalog(&self) {
        self.catalog.protect_tracks(catalog_paths(&self.queue, &self.listen));
    }
    fn write_buffer_snapshot(&self) {
        if !self.keep_queue() { return; }
        // A start that bypassed the restore (the crash chain) and built no
        // queue of its own leaves the good copy on disk alone.
        if crate::startup_chain::level() >= 3 && self.listen.buffers.iter().all(|b| b.paths.is_empty()) { return; }
        let fallback = Buffer::empty(self.listen.next);
        let saved = self.workspace_view(&fallback);
        if let Err(error) = write_snapshot(&file(), &saved) {
            chilpa_core::trace_playback("queue", "save_failed", format_args!("{error}"));
        }
    }

    /// Writes the session down while it plays, not only on a clean close: a
    /// killed process (a closed terminal, a logout, a crash) never reaches
    /// `Drop`, and came back to whatever the last queue edit had saved.
    /// A track that starts and a pause are written at once; the position
    /// in between, at most every ten seconds.
    pub(crate) fn persist_progress(&mut self, now: bool) {
        const EVERY: std::time::Duration = std::time::Duration::from_secs(10);
        if self.listen.pending.is_some() || self.listen.active == 0 { return; }
        let due = self.listen.saved_at.is_none_or(|at| at.elapsed() >= EVERY);
        if now || (self.now.playing && !self.now.paused && due) {
            self.listen.saved_at = Some(std::time::Instant::now());
            self.write_buffer_snapshot();
        }
    }
    pub(crate) fn forget_buffers(&self) { let _ = std::fs::remove_file(file()); }
    pub(crate) fn restore_buffers(&mut self) -> bool {
        if !self.keep_queue() { return false; }
        let Ok(bytes) = std::fs::read(file()) else { return false };
        let Ok(saved) = serde_json::from_slice::<Workspace>(&bytes) else { return false };
        if !valid(&saved) { return false; }
        let legacy = serde_json::from_slice::<serde_json::Value>(&bytes).ok();
        self.listen = saved;
        for b in &mut self.listen.buffers {
            let explicit = legacy.as_ref().and_then(|v| v["buffers"].as_array())
                .and_then(|rows| rows.iter().find(|row| row["id"].as_u64() == Some(b.id)))
                .is_some_and(|row| row.get("renamed").is_some());
            if !explicit {
                let automatic = ["Queue", "Cola", "Buffer"].iter().any(|prefix| b.name == *prefix
                    || b.name.strip_prefix(&format!("{prefix} ")).is_some_and(|n| n.parse::<u64>().is_ok()));
                b.renamed = !automatic;
            }
            if b.name == format!("Buffer {}", b.id) { b.name = chilpa_i18n::t_args("Queue {}", &[&b.id.to_string()]); }
        }
        if let Some(b) = self.listen.buffers.iter().find(|b| b.id == self.listen.active).cloned() {
            self.install_buffer(&b);
        }
        self.protect_queue_catalog();
        true
    }
}

fn write_snapshot(path: &std::path::Path, saved: &impl Serialize) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    let temporary = path.with_extension("tmp");
    let mut writer = std::io::BufWriter::new(std::fs::File::create(&temporary)?);
    serde_json::to_writer(&mut writer, saved).map_err(std::io::Error::other)?;
    writer.flush()?;
    drop(writer);
    // I replace the session only after the entire temporary snapshot is written.
    std::fs::rename(temporary, path)
}

fn catalog_paths(queue: &[String], workspace: &Workspace) -> std::collections::HashSet<String> {
    queue.iter().chain(workspace.buffers.iter().filter(|buffer| buffer.id != workspace.active)
        .flat_map(|buffer| buffer.paths.iter()))
        .filter(|path| chilpa_models::RemoteEntry::is_remote(path)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_session_save_replaces_only_a_complete_snapshot() {
        struct Broken;
        impl Serialize for Broken {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeSeq;
                let mut sequence = serializer.serialize_seq(None)?;
                sequence.serialize_element("partial")?;
                Err(serde::ser::Error::custom("interrupted snapshot"))
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.json");
        std::fs::write(&path, b"previous session").unwrap();
        assert!(write_snapshot(&path, &Broken).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"previous session");
        let workspace = Workspace::default();
        write_snapshot(&path, &workspace).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), serde_json::to_vec(&workspace).unwrap());
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn catalog_protection_uses_the_live_queue_and_every_inactive_buffer() {
        let path = |id| crate::source_opener::entry_of("guest", id);
        let mut workspace = Workspace::default();
        workspace.buffers[0].paths = vec![path("stale")];
        let mut other = Buffer::empty(2);
        other.paths = vec![path("other"), "/local.flac".into()];
        workspace.buffers.push(other);
        let live = vec![path("live"), "/local.wav".into(), path("live")];
        assert_eq!(catalog_paths(&live, &workspace), [path("live"), path("other")].into());
        workspace.buffers.pop();
        assert_eq!(catalog_paths(&live, &workspace), [path("live")].into());
        assert!(catalog_paths(&[], &workspace).is_empty(), "clearing the active queue releases its protection");
    }
}
fn valid(work: &Workspace) -> bool {
    let ids: std::collections::HashSet<_> = work.buffers.iter().map(|b| b.id).collect();
    work.version == 1 && !ids.is_empty() && ids.len() == work.buffers.len()
        && ids.contains(&work.selected) && (work.active == 0 || ids.contains(&work.active))
        && ids.iter().all(|&id| id > 0 && id < work.next)
        && work.buffers.iter().map(|b| b.paths.len()).sum::<usize>() <= 100_000
        && work.buffers.iter().all(|b| {
            let mut order = b.order.clone(); order.sort_unstable();
            order == (0..b.paths.len()).collect::<Vec<_>>()
                && b.removed.len() == b.paths.len() && b.placement.len() == b.paths.len()
                && b.seconds.is_finite() && b.seconds >= 0.0
                && b.paths.iter().all(|p| !p.is_empty())
                && (b.paths.is_empty() || b.cursor < b.paths.len())
                && b.groups.iter().all(|g| g.tracks.iter().all(|&i| i < b.paths.len()))
        })
}
