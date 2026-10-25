//! I schedule incremental scans from the existing facade poll, without a watcher.
use crate::{Chilpa, ScanPhase, ScanPolicy};
use crate::scan_state::{self, INTENT, POLICIES};
use std::time::{Duration, Instant};

impl Chilpa {
    pub(crate) fn ensure_scan_runtime(&mut self) {
        if self.scan_runtime.initialized { return; }
        self.scan_runtime.initialized = true;
        let loaded = (|| {
            let policies = chilpa_core::settings::read_value(POLICIES).map_err(scan_state::storage)?;
            let intent = chilpa_core::settings::read_value(INTENT).map_err(scan_state::storage)?;
            Ok::<_, String>((scan_state::policies(policies.as_deref())?, scan_state::intent(intent.as_deref())?))
        })();
        let (mut policies, mut intent) = match loaded {
            Ok(value) => value,
            Err(error) => { self.scan_error(error); return; }
        };
        if !policies.is_empty() || !intent.is_empty() {
            let roots = self.library().and_then(|library| library.roots().ok()).unwrap_or_default();
            policies.retain(|path, _| roots.iter().any(|root| root.path.to_string_lossy() == path.as_str()));
            intent.retain(|path| roots.iter().any(|root| &root.path == path));
            for root in roots {
                let path = root.path.to_string_lossy().into_owned();
                if let Some(policy) = policies.get(&path) {
                    self.scan_runtime.last_attempts.insert(path.clone(), root.last_scan.unwrap_or(0));
                    if *policy == ScanPolicy::Startup { self.scan_runtime.startup.insert(path); }
                }
            }
        }
        self.scan_runtime.policies = policies;
        self.scan_runtime.intent = intent;
        if !self.scan_runtime.intent.is_empty() {
            self.scan_status.phase = ScanPhase::Interrupted;
            self.scan_status.can_resume = true;
            self.scan_status.revision = self.scan_status.revision.wrapping_add(1);
        }
    }

    pub fn set_folder_scan_policy(&mut self, path: &str, key: &str) -> bool {
        self.ensure_scan_runtime();
        let Some(policy) = ScanPolicy::parse(key) else {
            self.scan_error(chilpa_i18n::t("The folder scan policy is invalid."));
            return false;
        };
        let roots = self.library().and_then(|library| library.roots().ok()).unwrap_or_default();
        let Some(root) = roots.iter().find(|root| root.path.to_string_lossy() == path) else {
            self.scan_error(chilpa_i18n::t("The folder is no longer in the library."));
            return false;
        };
        if self.scan_runtime.policies.get(path).copied().unwrap_or_default() == policy { return true; }
        let updated = chilpa_core::settings::update_value(POLICIES, |old| {
            let mut policies = scan_state::policies(old)?;
            if policy == ScanPolicy::Manual { policies.remove(path); }
            else { policies.insert(path.to_owned(), policy); }
            let text = serde_json::to_string(&policies).map_err(scan_state::storage)?;
            if policies.len() > scan_state::MAX_ROOTS || text.len() > scan_state::MAX_BYTES { return Err(scan_state::limits()); }
            Ok((text, policies))
        });
        match updated {
            Ok(policies) => {
                self.scan_runtime.policies = policies;
                self.scan_runtime.startup.remove(path);
                if policy == ScanPolicy::Manual { self.scan_runtime.last_attempts.remove(path); }
                else { self.scan_runtime.last_attempts.insert(path.to_owned(), root.last_scan.unwrap_or(0)); }
                self.scan_runtime.next_check = Instant::now();
                self.scan_status.error.clear();
                self.scan_status.revision = self.scan_status.revision.wrapping_add(1);
                true
            }
            Err(error) => { self.scan_error(scan_state::storage(error)); false }
        }
    }

    pub(crate) fn forget_folder_scan_state(&mut self, path: &str) -> bool {
        if !self.set_folder_scan_policy(path, "manual") { return false; }
        let remaining: Vec<_> = self.scan_runtime.intent.iter().filter(|root| root.to_string_lossy() != path).cloned().collect();
        if remaining.len() != self.scan_runtime.intent.len() {
            let saved = scan_state::validate_roots(&remaining).and_then(|text| {
                chilpa_core::settings::update_value(INTENT, |_| Ok((text, ())))
            });
            if let Err(error) = saved { self.scan_error(scan_state::storage(error)); return false; }
            self.scan_runtime.intent = remaining;
            self.scan_status.can_resume = !self.scan_runtime.intent.is_empty();
            if !self.scan_status.can_resume && self.scan_status.phase == ScanPhase::Interrupted { self.scan_status.phase = ScanPhase::Idle; }
        }
        true
    }

    pub fn folder_scan_policy(&mut self, path: &str) -> ScanPolicy {
        self.ensure_scan_runtime();
        self.scan_runtime.policies.get(path).copied().unwrap_or_default()
    }

    pub(crate) fn advance_scan(&mut self) {
        self.scan_status();
        if self.scan.is_some() || !self.scan_runtime.intent.is_empty() || self.scan_runtime.policies.is_empty()
            || Instant::now() < self.scan_runtime.next_check { return; }
        self.scan_runtime.next_check = Instant::now() + Duration::from_secs(60);
        if self.metadata.saving() { return; }
        let now = scan_state::now();
        let due: Vec<_> = self.scan_runtime.policies.iter().filter_map(|(path, policy)| {
            let last = self.scan_runtime.last_attempts.get(path).copied().unwrap_or(0);
            let ready = match policy {
                ScanPolicy::Startup => self.scan_runtime.startup.contains(path),
                _ => policy.interval().is_some_and(|interval| now.saturating_sub(last) >= interval),
            };
            ready.then(|| std::path::PathBuf::from(path))
        }).collect();
        if !due.is_empty() { self.start_scan_roots(due); }
    }
}
