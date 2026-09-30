use super::*;

fn file() -> std::path::PathBuf { chilpa_dirs::data().join("listenlist.json") }
impl Chilpa {
    pub(crate) fn persist_buffers(&self) {
        if !self.keep_queue() { return; }
        let mut saved = self.listen.clone();
        if let Some(b) = self.buffer_snapshot(saved.active) {
            if let Some(at) = saved.buffers.iter_mut().find(|v| v.id == b.id) { *at = b; }
        }
        let Ok(bytes) = serde_json::to_vec(&saved) else { return };
        let path = file();
        let Some(parent) = path.parent() else { return };
        if std::fs::create_dir_all(parent).is_err() { return; }
        let temporary = path.with_extension("tmp");
        if std::fs::write(&temporary, bytes).is_ok() {
            // I retain the previous snapshot if replacement fails.
            if let Err(error) = std::fs::rename(&temporary, &path) {
                chilpa_core::trace_playback("queue", "save_failed", format_args!("{error}"));
            }
        }
    }
    pub(crate) fn forget_buffers(&self) { let _ = std::fs::remove_file(file()); }
    pub(crate) fn restore_buffers(&mut self) -> bool {
        if !self.keep_queue() { return false; }
        let Ok(bytes) = std::fs::read(file()) else { return false };
        let Ok(saved) = serde_json::from_slice::<Workspace>(&bytes) else { return false };
        if !valid(&saved) { return false; }
        self.listen = saved;
        for b in &mut self.listen.buffers {
            if b.name == format!("Buffer {}", b.id) { b.name = chilpa_i18n::t_args("Queue {}", &[&b.id.to_string()]); }
        }
        if let Some(b) = self.listen.buffers.iter().find(|b| b.id == self.listen.active).cloned() {
            self.install_buffer(&b);
        }
        true
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
