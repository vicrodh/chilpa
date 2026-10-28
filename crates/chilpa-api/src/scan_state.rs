//! I keep scan state and bounded per-folder schedules outside the shell.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Instant;
use serde::{Deserialize, Serialize};

pub(crate) const POLICIES: &str = "library.scanPolicies";
pub(crate) const INTENT: &str = "library.scanIntent";
pub(crate) const MAX_ROOTS: usize = chilpa_library::scan_job::MAX_ROOTS;
pub(crate) const MAX_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScanPolicy {
    #[default]
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "startup")]
    Startup,
    #[serde(rename = "12h")]
    Every12Hours,
    #[serde(rename = "24h")]
    Every24Hours,
    #[serde(rename = "weekly")]
    Weekly,
}
impl ScanPolicy {
    pub fn key(self) -> &'static str {
        match self { Self::Manual => "manual", Self::Startup => "startup", Self::Every12Hours => "12h", Self::Every24Hours => "24h", Self::Weekly => "weekly" }
    }
    pub fn parse(key: &str) -> Option<Self> {
        match key { "manual" => Some(Self::Manual), "startup" => Some(Self::Startup), "12h" => Some(Self::Every12Hours), "24h" => Some(Self::Every24Hours), "weekly" => Some(Self::Weekly), _ => None }
    }
    pub(crate) fn interval(self) -> Option<i64> {
        match self { Self::Every12Hours => Some(43_200), Self::Every24Hours => Some(86_400), Self::Weekly => Some(604_800), _ => None }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScanPhase { #[default] Idle, Discovering, Reading, Finalizing, Stopping, Complete, Interrupted, Failed }
impl ScanPhase {
    pub fn key(self) -> &'static str {
        match self { Self::Idle => "idle", Self::Discovering => "discovering", Self::Reading => "reading", Self::Finalizing => "finalizing", Self::Stopping => "stopping", Self::Complete => "complete", Self::Interrupted => "interrupted", Self::Failed => "failed" }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LibraryFolder {
    pub path: String,
    pub last_scan: Option<i64>,
    pub reachable: bool,
    pub scan_policy: ScanPolicy,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ScanStatus {
    pub running: bool,
    /// Files found so far while the folders are still being listed.
    pub found: usize,
    pub done: usize,
    pub total: usize,
    pub summary: ScanSummary,
    pub phase: ScanPhase,
    pub root: String,
    pub error: String,
    pub can_resume: bool,
    pub revision: u64,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ScanSummary {
    pub seen: usize,
    pub read: usize,
    pub unchanged: usize,
    pub missing: usize,
    pub unreadable: usize,
    pub unreachable: bool,
}
impl From<chilpa_library::ScanReport> for ScanSummary {
    fn from(report: chilpa_library::ScanReport) -> Self {
        Self { seen: report.seen, read: report.read, unchanged: report.unchanged,
            missing: report.missing, unreadable: report.unreadable, unreachable: report.unreachable }
    }
}

// Each map/set is bounded by MAX_ROOTS; serialized policies and intent each
// stay below MAX_BYTES. I release them with the facade, and never queue jobs.
pub(crate) struct Runtime {
    pub initialized: bool,
    pub policies: BTreeMap<String, ScanPolicy>,
    pub last_attempts: BTreeMap<String, i64>,
    pub startup: BTreeSet<String>,
    pub intent: Vec<PathBuf>,
    pub next_check: Instant,
    /// When the screens were last told about records the running scan
    /// wrote, so that the first batch shows at once and the rest no more
    /// often than `SHOW_EVERY`.
    pub shown_at: Option<Instant>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self { initialized: false, policies: BTreeMap::new(), last_attempts: BTreeMap::new(),
            startup: BTreeSet::new(), intent: Vec::new(), next_check: Instant::now(), shown_at: None }
    }
}

/// How often the screens are refreshed with what a running scan wrote.
pub(crate) const SHOW_EVERY: std::time::Duration = std::time::Duration::from_secs(3);

pub(crate) fn limits() -> String { chilpa_i18n::t("A scan supports up to 256 folders and 64 KiB of saved paths.") }
pub(crate) fn storage(error: impl std::fmt::Display) -> String {
    chilpa_i18n::t_args("Scan settings could not be saved or read: {}", &[&error.to_string()])
}
pub(crate) fn policies(text: Option<&str>) -> Result<BTreeMap<String, ScanPolicy>, String> {
    let Some(text) = text.filter(|text| !text.is_empty()) else { return Ok(BTreeMap::new()) };
    if text.len() > MAX_BYTES { return Err(limits()); }
    let value: BTreeMap<String, ScanPolicy> = serde_json::from_str(text).map_err(|_| chilpa_i18n::t("The saved folder scan policy is invalid."))?;
    if value.len() > MAX_ROOTS || value.keys().any(|key| key.is_empty()) { return Err(limits()); }
    Ok(value)
}
pub(crate) fn intent(text: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let Some(text) = text.filter(|text| !text.is_empty()) else { return Ok(Vec::new()) };
    if text.len() > MAX_BYTES { return Err(limits()); }
    let roots: Vec<PathBuf> = serde_json::from_str(text).map_err(|_| chilpa_i18n::t("The saved scan could not be restored."))?;
    validate_roots(&roots)?;
    Ok(roots)
}
pub(crate) fn validate_roots(roots: &[PathBuf]) -> Result<String, String> {
    let text = serde_json::to_string(roots).map_err(storage)?;
    if roots.len() > MAX_ROOTS || text.len() > MAX_BYTES || roots.iter().any(|root| root.as_os_str().is_empty()) { return Err(limits()); }
    Ok(text)
}
pub(crate) fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|time| time.as_secs() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schedules_and_intents_have_explicit_size_limits() {
        assert_eq!(ScanPolicy::Every12Hours.interval(), Some(43_200));
        assert_eq!(ScanPolicy::Every24Hours.interval(), Some(86_400));
        assert_eq!(ScanPolicy::Weekly.interval(), Some(604_800));
        assert_eq!(ScanPolicy::Manual.interval(), None);
        assert_eq!(ScanPolicy::Startup.interval(), None);
        let entries: BTreeMap<_, _> = (0..=MAX_ROOTS).map(|index| (format!("folder-{index}"), ScanPolicy::Weekly)).collect();
        assert!(policies(Some(&serde_json::to_string(&entries).unwrap())).is_err());
        assert!(policies(Some(&" ".repeat(MAX_BYTES + 1))).is_err());
        assert!(intent(Some(&serde_json::to_string(&vec!["folder"; MAX_ROOTS + 1]).unwrap())).is_err());
        assert!(intent(Some("[\"\"]")).is_err());
        assert!(policies(Some("{\"folder\":\"unknown\"}")).is_err());
    }
}
