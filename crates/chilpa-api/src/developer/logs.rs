//! One process owns at most 5000 lines / 4 MiB, plus two 1 MiB disk files.
//! The stderr reader calls me only when bytes arrive; no publication runs idle.
use super::{redaction, repeat};
use serde::Serialize;
use std::{time::Instant, collections::VecDeque, fs::{File, OpenOptions}, io::{Write, Seek}, path::{Path, PathBuf}, sync::{LazyLock, Mutex}, time::{SystemTime, UNIX_EPOCH}};

const MAX_LINES: usize = 5000;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_LINE: usize = 4096;
const DISK_BYTES: usize = 1024 * 1024;
static STORE: LazyLock<Mutex<Store>> = LazyLock::new(|| Mutex::new(Store::default()));

/// One retained record; `ts` is epoch milliseconds.
#[derive(Clone, Serialize)]
pub(super) struct Entry { pub(super) ts: i64, pub(super) level: &'static str, pub(super) target: String, pub(super) message: String }
#[derive(Default)]
struct Store {
    entries: VecDeque<Entry>, bytes: usize, sink: Option<File>, path: Option<PathBuf>, disk_bytes: usize,
    status: String, redact_continuation: bool, private_key: bool, repeats: repeat::Consecutive,
}

fn regular(path: &Path) -> std::io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(std::io::Error::other("the log destination is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}
fn create_sink(path: &Path) -> std::io::Result<File> {
    let backup = path.with_extension("previous.log");
    if regular(&backup)? { std::fs::remove_file(&backup)?; }
    if regular(path)? { std::fs::rename(path, &backup)?; }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
fn private_directory(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)] {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path)?;
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() { return Err(std::io::Error::other("the log directory is not private")); }
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 { return Err(std::io::Error::other("the log directory is not private")); }
    }
    Ok(())
}
impl Store {
    fn open(&mut self, directory: &Path) {
        let path = directory.join("application.log");
        match private_directory(directory).and_then(|()| create_sink(&path)) {
            Ok(file) => { self.sink = Some(file); self.path = Some(path); self.disk_bytes = 0; }
            Err(_) => self.status = chilpa_i18n::t("The log file could not be opened. Session logs remain available here."),
        }
    }
    fn append(&mut self, text: &str) {
        let oversized = text.len() > MAX_LINE;
        let text = if oversized { "[log entry exceeded the line limit]" } else { text };
        let lower = text.to_ascii_lowercase();
        let private_key = self.private_key || (lower.contains("begin") && lower.contains("private key"));
        self.private_key = private_key && !(lower.contains("end") && lower.contains("private key"));
        let cleaned = redaction::clean(text);
        let message = if oversized || cleaned.len() > MAX_LINE { "[log entry exceeded the line limit]".into() }
            else if private_key || self.redact_continuation { "[redacted sensitive log entry]".into() } else { cleaned.clone() };
        self.redact_continuation = cleaned == "[redacted sensitive log entry]" && text.trim_end().ends_with([':', '=']);
        if message.is_empty() { return; }
        let lower = message.to_ascii_lowercase();
        let level = if lower.starts_with("error:") || lower.starts_with("[error]") { "error" }
            else if lower.starts_with("warning:") || lower.starts_with("[warn]") { "warn" }
            else if lower.starts_with("info:") || lower.starts_with("[info]") { "info" }
            else if lower.starts_with("debug:") || lower.starts_with("[debug]") { "debug" }
            else if lower.starts_with("trace:") || lower.starts_with("[trace]") { "trace" }
            // A panic says what it is; anything else the process wrote is information.
            else if lower.starts_with("thread '") && lower.contains("panicked") { "error" } else { "info" };
        let target = message.split_once(':').map(|(prefix, _)| prefix).filter(|prefix| prefix.len() <= 48 && !prefix.contains(' ')).unwrap_or("application").to_string();
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        for entry in self.repeats.push(Entry { ts, level, target, message }, Instant::now()).into_iter().flatten() { self.record(entry); }
    }
    /// A burst that has not ended yet is summarized before anything reads the ring.
    fn flush(&mut self) {
        if let Some(entry) = self.repeats.flush() { self.record(entry); }
    }
    fn record(&mut self, entry: Entry) {
        let bytes = entry.message.capacity() + entry.target.capacity();
        while self.entries.len() >= MAX_LINES || self.bytes + bytes > MAX_BYTES {
            if let Some(old) = self.entries.pop_front() { self.bytes -= old.message.capacity() + old.target.capacity(); } else { break; }
        }
        self.bytes += bytes;
        if let Some(path) = self.path.clone() {
            let line = format!("{} {} {} {}\n", format_ts(entry.ts), entry.level.to_ascii_uppercase(), entry.target, entry.message);
            if self.disk_bytes + line.len() > DISK_BYTES {
                self.sink.take();
                self.sink = create_sink(&path).ok(); self.disk_bytes = 0;
            }
            if self.sink.as_mut().is_some_and(|file| file.write_all(line.as_bytes()).is_err()) {
                self.sink.take();
                self.status = chilpa_i18n::t("The log file could not be written. Session logs remain available here.");
            }
            self.disk_bytes += line.len();
        }
        self.entries.push_back(entry);
    }
}
/// I start after the real application's instance check, never in a probe child.
pub fn initialize() {
    STORE.lock().unwrap_or_else(|error| error.into_inner()).open(&chilpa_dirs::cache().join("diagnostics"));
}
/// The input is transient; only the redacted result is retained.
pub fn append(text: &str) { STORE.lock().unwrap_or_else(|error| error.into_inner()).append(text); }
/// `YYYY-MM-DD HH:MM:SS.mmm` in UTC from epoch milliseconds, the form the
/// file and the shared bundle carry.
pub(super) fn format_ts(ms: i64) -> String {
    let (days, rest) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    // Civil date from a day count (proleptic Gregorian, era based).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}.{:03}", rest / 3_600_000, rest / 60_000 % 60, rest / 1000 % 60, rest % 1000)
}
fn matches(entry: &Entry, level: &str, search: &str) -> bool {
    (level == "all" || level.eq_ignore_ascii_case(entry.level))
        && (search.is_empty() || entry.message.to_lowercase().contains(search) || entry.target.to_lowercase().contains(search))
}
/// The viewer shows the last rows that match, never more than this.
const VIEW_ROWS: usize = 500;
/// A manual snapshot, filtered and capped before it crosses Qt.
pub fn document(level: &str, search: &str) -> String {
    let mut store = STORE.lock().unwrap_or_else(|error| error.into_inner());
    store.flush();
    let search = search.chars().take(128).collect::<String>().to_lowercase();
    let matching: Vec<_> = store.entries.iter().filter(|entry| matches(entry, level, &search)).collect();
    #[derive(Serialize)]
    struct Document<'a> { rows: &'a [&'a Entry], total: usize, shown: usize, status: &'a str, #[serde(rename="fileAvailable")] file_available: bool }
    let shown = matching.len();
    let rows = &matching[shown.saturating_sub(VIEW_ROWS)..];
    serde_json::to_string(&Document {rows,total:store.entries.len(),shown,status:&store.status,file_available:store.sink.is_some()}).unwrap_or_else(|_| "{}".into())
}
fn line_of(entry: &Entry) -> String {
    format!("{} {} {} {}\n", format_ts(entry.ts), entry.level.to_ascii_uppercase(), entry.target, entry.message)
}
/// Every row that matches, as the text "Copy all" puts on the clipboard.
pub fn filtered_text(level: &str, search: &str) -> String {
    let mut store = STORE.lock().unwrap_or_else(|error| error.into_inner());
    store.flush();
    let search = search.chars().take(128).collect::<String>().to_lowercase();
    store.entries.iter().filter(|entry| matches(entry, level, &search)).map(line_of).collect()
}
pub fn export() -> String {
    let mut store = STORE.lock().unwrap_or_else(|error| error.into_inner());
    store.flush();
    store.entries.iter().map(line_of).collect()
}
/// The last `count` records, oldest first, for the shared bundle.
pub(super) fn tail(count: usize) -> Vec<Entry> {
    let mut store = STORE.lock().unwrap_or_else(|error| error.into_inner());
    store.flush();
    let skip = store.entries.len().saturating_sub(count);
    store.entries.iter().skip(skip).cloned().collect()
}
pub fn file_url() -> String {
    let store = STORE.lock().unwrap_or_else(|error| error.into_inner());
    store.path.as_ref().filter(|_| store.sink.is_some()).and_then(|path| url::Url::from_file_path(path).ok()).map(|url| url.to_string()).unwrap_or_default()
}
pub fn clear() {
    let mut store = STORE.lock().unwrap_or_else(|error| error.into_inner());
    store.entries.clear(); store.bytes = 0; store.repeats = repeat::Consecutive::default();
    if let Some(file) = &mut store.sink { let _ = file.set_len(0).and_then(|()| file.rewind()); }
    store.disk_bytes = 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_ring_and_lines_are_bounded_and_redacted() {
        let mut store = Store::default();
        for i in 0..MAX_LINES * 2 { store.append(&format!("event {i} {}", "x ".repeat(1500))); }
        assert!(store.entries.len() <= MAX_LINES); assert!(store.bytes <= MAX_BYTES);
        store.append("password=fixture-private-value");
        assert!(!store.entries.back().unwrap().message.contains("fixture-private-value"));
        store.append(&"x".repeat(MAX_LINE + 1));
        assert_eq!(store.entries.back().unwrap().message, "[log entry exceeded the line limit]");
        store.append("Authorization:"); store.append("fixture-multiline-value");
        assert!(!store.entries.back().unwrap().message.contains("fixture-multiline-value"));
        for line in ["-----BEGIN PRIVATE KEY-----", "fixture-key-body", "-----END PRIVATE KEY-----"] { store.append(line); }
        assert!(store.entries.iter().all(|entry| !entry.message.contains("fixture-key-body")));
        store.append(&String::from_utf8_lossy(b"info: invalid \xff utf8"));
        assert!(store.entries.back().unwrap().message.contains('\u{fffd}'));
        store.append(&"a@b ".repeat(MAX_LINE / 4));
        assert_eq!(store.entries.back().unwrap().message, "[log entry exceeded the line limit]");
        assert!(store.entries.back().unwrap().message.len() <= MAX_LINE);
    }
    #[test]
    fn a_timestamp_reads_as_a_date_with_milliseconds() {
        assert_eq!(format_ts(0), "1970-01-01 00:00:00.000");
        assert_eq!(format_ts(1_700_000_000_123), "2023-11-14 22:13:20.123");
        assert_eq!(format_ts(951_782_400_000), "2000-02-29 00:00:00.000");
    }
    #[test]
    fn identical_neighbours_collapse_and_flush_reports_them() {
        let mut store = Store::default();
        for _ in 0..4 { store.append("info: tick"); }
        store.append("info: tock");
        let messages: Vec<_> = store.entries.iter().map(|entry| entry.message.as_str()).collect();
        assert_eq!(messages.len(), 3);
        assert!(messages[1].starts_with("[log] previous message repeated 3 additional times"));
        store.flush();
        assert_eq!(store.entries.len(), 3);
    }
    #[test]
    fn debug_and_trace_prefixes_are_levels_of_their_own() {
        let mut store = Store::default();
        store.append("debug: a"); store.append("[trace] b");
        assert_eq!(store.entries[0].level, "debug"); assert_eq!(store.entries[1].level, "trace");
    }
    #[test]
    fn files_rotate_and_never_contain_raw_secrets() {
        let directory = tempfile::tempdir().unwrap();
        let logs = directory.path().join("diagnostics");
        let mut store = Store::default(); store.open(&logs);
        assert!(store.sink.is_some(), "{}", store.status);
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&logs).unwrap().permissions().mode() & 0o777, 0o700);
        }
        for i in 0..800 { store.append(&format!("{i} {}", "x ".repeat(1500))); }
        store.append("Authorization: Bearer fixture-private-value");
        let path=store.path.as_ref().unwrap();
        for path in [path.clone(),path.with_extension("previous.log")] {
            let data=std::fs::read(&path).unwrap(); assert!(data.len() <= DISK_BYTES);
            assert!(!String::from_utf8(data).unwrap().contains("fixture-private-value"));
            #[cfg(unix)] {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
            }
        }
    }
    #[cfg(unix)]
    #[test]
    fn a_symlink_destination_is_rejected() {
        let directory=tempfile::tempdir().unwrap();
        let logs=directory.path().join("diagnostics");
        private_directory(&logs).unwrap();
        let outside=directory.path().join("outside"); std::fs::write(&outside,b"untouched").unwrap();
        let destination=logs.join("application.log");
        std::os::unix::fs::symlink(&outside,&destination).unwrap();
        let mut store=Store::default(); store.open(&logs); store.append("fixture");
        assert!(store.sink.is_none()); assert_eq!(std::fs::read(outside).unwrap(),b"untouched");
        // I verify the same directory accepts a regular sink after removing the link.
        std::fs::remove_file(destination).unwrap();
        store.open(&logs);
        assert!(store.sink.is_some(), "{}", store.status);
    }
}
