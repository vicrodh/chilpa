//! The smoke that needs a real library and a person who decided to run it.
//!
//! Ignored by default, because it reads every file under a folder somebody
//! owns. What it answers is what synthetic folders cannot: whether a scan of
//! an actual collection -- tens of thousands of files, on a network share,
//! tagged by whoever ripped them -- finishes, what it records, and whether
//! what it records is true.
//!
//! ```text
//! CHILPA_SMOKE_LIBRARY=/path/to/music \
//!   cargo test -p chilpa-library --test real_library -- --ignored --nocapture
//! ```
//!
//! Nothing under the folder is written: the library goes to a temporary file
//! and covers are not extracted. The rate, the channels and the bit depth the
//! library stores are checked against `ffprobe` on a sample of tracks, because
//! a library that asks Chilpa's own tag reader and then checks it against the
//! same reader has checked nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Extensions that are audio but that the scanner does not list, so the run
/// can say how much of the collection it cannot see.
const UNLISTED: &[&str] = &["iso", "dff", "ape", "wv", "tta", "tak", "opus", "wma", "cue", "mka"];

/// What ffprobe says a file carries.
struct Probe {
    rate: Option<u32>,
    channels: Option<u32>,
    bits: Option<u32>,
    codec: String,
}

fn probe(path: &Path) -> Option<Probe> {
    let out = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries",
               "stream=codec_name,sample_rate,channels,bits_per_raw_sample", "-of", "default=nw=1"])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let field = |name: &str| {
        text.lines().find_map(|l| l.strip_prefix(&format!("{name}="))).map(str::to_string)
    };
    Some(Probe {
        rate: field("sample_rate").and_then(|v| v.parse().ok()),
        channels: field("channels").and_then(|v| v.parse().ok()),
        bits: field("bits_per_raw_sample").and_then(|v| v.parse().ok()).filter(|b| *b > 0),
        codec: field("codec_name").unwrap_or_default(),
    })
}

fn unlisted_on_disk(root: &Path) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext = ext.to_lowercase();
                if UNLISTED.contains(&ext.as_str()) {
                    *counts.entry(ext).or_insert(0) += 1;
                }
            }
        }
    }
    counts
}

#[test]
#[ignore = "reads a whole real library; run it on purpose"]
fn a_real_library_scans_and_what_it_records_is_true() {
    let Some(root) = std::env::var_os("CHILPA_SMOKE_LIBRARY").map(PathBuf::from) else {
        eprintln!("smoke: set CHILPA_SMOKE_LIBRARY to a music folder to run this");
        return;
    };
    let db = std::env::temp_dir().join("chilpa-real-library.db");
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", db.display()));
    }
    let lib = chilpa_library::Library::open(&db).unwrap();

    let started = Instant::now();
    let report = chilpa_library::scan::scan_root_reporting(&lib, &root, None, &|n| {
        if n % 2_000 == 0 && n > 0 {
            eprintln!("smoke: {n} files looked at");
        }
    })
    .unwrap();
    let first = started.elapsed();
    assert!(!report.unreachable, "{} could not be read", root.display());
    eprintln!(
        "smoke: first scan: {} audio files, {} read, {} unreadable, in {:.1} s ({:.2} ms a file)",
        report.seen,
        report.read,
        report.unreadable,
        first.as_secs_f64(),
        first.as_secs_f64() * 1000.0 / report.seen.max(1) as f64
    );

    // What the scanner does not list, so nobody mistakes it for everything.
    for (ext, n) in unlisted_on_disk(&root) {
        eprintln!("smoke: not listed by the scanner: {n} .{ext}");
    }

    let sql = rusqlite::Connection::open(&db).unwrap();
    let count = |q: &str| -> i64 { sql.query_row(q, [], |r| r.get(0)).unwrap() };
    let tracks = count("SELECT COUNT(*) FROM tracks");
    eprintln!("smoke: {tracks} tracks recorded");
    eprintln!("smoke: {} without an album", count("SELECT COUNT(*) FROM tracks WHERE album_id IS NULL"));
    eprintln!("smoke: {} without an artist", count("SELECT COUNT(*) FROM tracks WHERE artist IS NULL OR artist = ''"));
    eprintln!("smoke: {} with no duration (tags unreadable)", count("SELECT COUNT(*) FROM tracks WHERE duration_ms = 0"));
    eprintln!("smoke: {} albums", count("SELECT COUNT(*) FROM albums"));
    assert_eq!(
        tracks as usize,
        report.seen - report.unreadable,
        "every audio file that could be read became a track"
    );

    // The second witness, on a sample spread across the whole library.
    let mut rows: Vec<(String, Option<u32>, Option<u32>, Option<u32>)> = sql
        .prepare("SELECT external_id, sample_rate_hz, channels, bit_depth FROM tracks ORDER BY external_id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let step = (rows.len() / 300).max(1);
    rows = rows.into_iter().step_by(step).collect();
    let mut wrong = Vec::new();
    let mut checked = 0;
    for (path, rate, channels, bits) in &rows {
        let Some(p) = probe(Path::new(path)) else { continue };
        checked += 1;
        let lossless = matches!(p.codec.as_str(), "flac" | "alac") || p.codec.starts_with("pcm_");
        // ffprobe states a DSD rate in bytes per channel, one eighth of the
        // one-bit rate the library keeps: 352,800 for DSD64's 2,822,400.
        let probed_rate = if p.codec.starts_with("dsd_") { p.rate.map(|r| r * 8) } else { p.rate };
        if probed_rate.is_some() && probed_rate != *rate {
            wrong.push(format!("{path}: rate {rate:?}, ffprobe {probed_rate:?}"));
        }
        if p.channels.is_some() && p.channels != *channels {
            wrong.push(format!("{path}: channels {channels:?}, ffprobe {:?}", p.channels));
        }
        if lossless && p.bits.is_some() && p.bits != *bits {
            wrong.push(format!("{path}: {} bits {bits:?}, ffprobe {:?}", p.codec, p.bits));
        }
    }
    eprintln!("smoke: {checked} tracks checked against ffprobe, {} disagree", wrong.len());
    for line in wrong.iter().take(20) {
        eprintln!("smoke:   {line}");
    }

    // The rule from the QBZ audit: a rescan with nothing changed reads nothing.
    let started = Instant::now();
    let again = chilpa_library::scan::scan_root_reporting(&lib, &root, None, &|_| {}).unwrap();
    eprintln!(
        "smoke: rescan: {} read, {} unchanged, {} missing, in {:.1} s",
        again.read,
        again.unchanged,
        again.missing,
        started.elapsed().as_secs_f64()
    );
    assert_eq!(again.read, 0, "a rescan with nothing changed read files again");
    assert_eq!(again.missing, 0, "a rescan with nothing changed marked tracks missing");
    assert!(wrong.is_empty(), "the library records what ffprobe contradicts: {wrong:#?}");
}
