//! I preserve scan intent before starting and report terminal states explicitly.
use crate::{Chilpa, ScanPhase, ScanStatus};
use crate::scan_state::{self, INTENT};
use std::path::{Path, PathBuf};

impl Chilpa {
    pub(crate) fn scan_error(&mut self, error: String) {
        self.now.error = error.clone();
        self.scan_status.error = error;
        if !self.scan_status.running { self.scan_status.phase = ScanPhase::Failed; }
        self.scan_status.can_resume = !self.scan_status.running && !self.scan_runtime.intent.is_empty();
        self.scan_status.revision = self.scan_status.revision.wrapping_add(1);
    }

    pub fn start_scan(&mut self, folder: &Path) -> bool {
        self.ensure_scan_runtime();
        if self.scan.is_some() { self.scan_error(chilpa_i18n::t("a scan is already running")); return false; }
        if self.metadata.saving() { self.scan_error(chilpa_i18n::t("metadata is being saved; scan when it is done")); return false; }
        if !self.allow_new_scan() { return false; }
        if let Err(error) = scan_state::validate_roots(&[folder.to_path_buf()]) { self.scan_error(error); return false; }
        let registered = self.library().map(|library| {
            let known = library.roots()?.iter().any(|root| root.path == folder);
            let source = library.source_id("local", "default")?;
            library.root_id(source, folder).map(|_| !known)
        });
        match registered {
            Some(Ok(added)) => {
                if added { self.mark_library_changed(); }
                self.start_scan_roots(vec![folder.to_path_buf()])
            }
            Some(Err(error)) => { self.scan_error(error.to_string()); false }
            None => false,
        }
    }

    pub fn rescan(&mut self) -> bool {
        let roots = self.roots();
        self.start_scan_roots(roots)
    }

    fn allow_new_scan(&mut self) -> bool {
        self.ensure_scan_runtime();
        if self.scan.is_none() && !self.scan_runtime.intent.is_empty() {
            self.scan_error(chilpa_i18n::t("Continue or dismiss the interrupted scan before starting another."));
            return false;
        }
        true
    }

    pub(crate) fn start_scan_roots(&mut self, roots: Vec<PathBuf>) -> bool {
        if !self.allow_new_scan() { return false; }
        self.resume_scan_roots(roots)
    }

    fn resume_scan_roots(&mut self, roots: Vec<PathBuf>) -> bool {
        self.start_scan_using(roots, |roots| chilpa_library::ScanJob::try_start_with(
            chilpa_library::Library::default_path(), roots, Some(chilpa_core::artwork_cache()), chilpa_core::sacd_tracks::disc_images()))
    }

    fn start_scan_using(&mut self, roots: Vec<PathBuf>, start: impl FnOnce(Vec<PathBuf>) -> std::io::Result<chilpa_library::ScanJob>) -> bool {
        self.ensure_scan_runtime();
        if self.scan.is_some() { self.scan_error(chilpa_i18n::t("a scan is already running")); return false; }
        if self.metadata.saving() { self.scan_error(chilpa_i18n::t("metadata is being saved; scan when it is done")); return false; }
        if roots.is_empty() { self.scan_error(chilpa_i18n::t("there is no folder in the library yet")); return false; }
        let saved = scan_state::validate_roots(&roots).and_then(|text| chilpa_core::settings::update_value(INTENT, |_| Ok((text, ()))));
        if let Err(error) = saved { self.scan_error(scan_state::storage(error)); return false; }
        let previous_intent = std::mem::replace(&mut self.scan_runtime.intent, roots.clone());
        match start(roots.clone()) {
            Ok(job) => {
                let now = scan_state::now();
                for root in &roots {
                    let path = root.to_string_lossy();
                    self.scan_runtime.startup.remove(path.as_ref());
                    if self.scan_runtime.policies.contains_key(path.as_ref()) {
                        self.scan_runtime.last_attempts.insert(path.into_owned(), now);
                    }
                }
                self.scan = Some(job);
                self.scan_runtime.shown_at = None;
                (self.scan_runtime.written, self.scan_runtime.shown_written) = (0, 0);
                self.scan_status = ScanStatus { running: true, phase: ScanPhase::Discovering,
                    root: if roots.len() == 1 { roots[0].to_string_lossy().into_owned() } else { String::new() },
                    revision: self.scan_status.revision.wrapping_add(1), ..ScanStatus::default() };
                true
            }
            Err(error) => {
                let cleanup = scan_state::validate_roots(&previous_intent).and_then(|text| {
                    chilpa_core::settings::update_value(INTENT, |_| Ok((text, ())))
                });
                if cleanup.is_ok() { self.scan_runtime.intent = previous_intent; }
                let message = chilpa_i18n::t_args("The scan could not start: {}", &[&error.to_string()]);
                self.scan_error(cleanup.err().map_or(message.clone(), |cleanup| format!("{message} {cleanup}")));
                false
            }
        }
    }

    fn clear_scan_intent(&mut self) -> Result<(), String> {
        chilpa_core::settings::update_value(INTENT, |_| Ok(("[]".into(), ()))).map_err(scan_state::storage)?;
        self.scan_runtime.intent.clear();
        self.scan_status.can_resume = false;
        Ok(())
    }

    pub fn stop_scan(&mut self) {
        if let Some(job) = &self.scan {
            job.stop();
            self.scan_status.phase = ScanPhase::Stopping;
            self.scan_status.revision = self.scan_status.revision.wrapping_add(1);
        }
    }

    pub(crate) fn interrupt_scan_on_shutdown(&mut self) {
        self.analysis_status.waiting = None;
        if self.scan.is_some() { self.scan_status(); }
        if let Some(job) = &self.scan { job.stop(); }
    }

    pub fn resume_scan(&mut self) -> bool {
        self.ensure_scan_runtime();
        let known = self.roots();
        let roots = self.scan_runtime.intent.iter().filter(|root| known.contains(root)).cloned().collect();
        self.resume_scan_roots(roots)
    }

    pub fn dismiss_scan_status(&mut self) {
        if self.scan.is_some() { return; }
        if let Err(error) = self.clear_scan_intent() { self.scan_error(error); return; }
        self.scan_status = ScanStatus { revision: self.scan_status.revision.wrapping_add(1), ..ScanStatus::default() };
    }

    pub fn scan_revision(&self) -> u64 { self.scan_status.revision }

    pub fn scan_status(&mut self) -> &ScanStatus {
        self.ensure_scan_runtime();
        let events = self.scan.as_ref().map(|job| job.poll()).unwrap_or_default();
        for event in events {
            use chilpa_library::ScanProgress;
            match event {
                ScanProgress::Discovered { found } => self.scan_status.found = found,
                ScanProgress::Counted { total } => self.scan_status.total = total,
                ScanProgress::Advanced { done, total } => {
                    self.scan_status.done = done;
                    self.scan_status.total = total.max(done);
                }
                ScanProgress::Committed { written, .. } => {
                    self.scan_runtime.written = self.scan_runtime.written.max(written);
                }
                ScanProgress::Phase { root, stage } => {
                    use chilpa_library::scan_job::ScanStage;
                    self.scan_status.root = root.to_string_lossy().into_owned();
                    if self.scan_status.phase != ScanPhase::Stopping {
                        self.scan_status.phase = match stage {
                            ScanStage::Listing => ScanPhase::Discovering,
                            ScanStage::Reading => ScanPhase::Reading,
                            ScanStage::Finalizing => ScanPhase::Finalizing,
                        };
                    }
                }
                ScanProgress::Done(report) => {
                    let changed = report.changed || report.read > 0 || report.missing > 0;
                    self.scan_status.summary = report.into();
                    self.scan_status.running = false;
                    self.scan_status.done = self.scan_status.total;
                    self.scan_status.phase = ScanPhase::Complete;
                    self.scan_status.error.clear();
                    self.scan = None;
                    if let Err(error) = self.clear_scan_intent() { self.scan_error(error); }
                    if changed { self.mark_library_changed(); }
                    if let Some(folder) = self.analysis_status.waiting.take() { self.start_analysis_under(Some(folder)); }
                }
                ScanProgress::Interrupted(report) => {
                    let changed = report.changed || report.read > 0;
                    self.scan_status.summary = report.into();
                    self.scan_status.running = false;
                    self.scan_status.phase = ScanPhase::Interrupted;
                    self.scan_status.can_resume = !self.scan_runtime.intent.is_empty();
                    self.scan = None;
                    self.analysis_status.waiting = None;
                    if changed { self.mark_library_changed(); }
                }
                ScanProgress::Failed(error) => {
                    self.scan_status.running = false;
                    self.scan = None;
                    self.analysis_status.waiting = None;
                    self.scan_error(chilpa_i18n::t_args("the scan failed: {}", &[&error]));
                    self.mark_library_changed();
                }
            }
            self.scan_status.revision = self.scan_status.revision.wrapping_add(1);
        }
        self.show_scan_writes();
        &self.scan_status
    }

    /// Shows what the running scan wrote as it lands, instead of all at the
    /// end: the first rows at once, then at most every `SHOW_EVERY`, and
    /// only when rows were written since they were last shown. It is looked
    /// at on every poll while a scan runs, so a change held back by the
    /// interval waits for the interval, not for the next batch.
    fn show_scan_writes(&mut self) {
        let runtime = &mut self.scan_runtime;
        if self.scan.is_none() || runtime.written <= runtime.shown_written { return; }
        let now = std::time::Instant::now();
        if runtime.shown_at.is_some_and(|at| now.duration_since(at) < scan_state::SHOW_EVERY) { return; }
        runtime.shown_at = Some(now);
        runtime.shown_written = runtime.written;
        self.mark_library_changed();
    }
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
