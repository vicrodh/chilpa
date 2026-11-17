//! Bounds and progress for synthetic DSDIFF metadata.
use chilpa_tags::read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const RATE: u32 = 2_822_400;
static NEXT_FILE: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        let number = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "chilpa-tags-dff-bounds-{}-{number}.dff", std::process::id()
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn header(id: &[u8; 4], len: u64) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend_from_slice(&len.to_be_bytes());
    out
}

fn chunk(id: &[u8; 4], bytes: &[u8]) -> Vec<u8> {
    let mut out = header(id, bytes.len() as u64);
    out.extend_from_slice(bytes);
    if bytes.len() % 2 == 1 {
        out.push(0);
    }
    out
}

fn properties() -> Vec<u8> {
    let mut prop = b"SND ".to_vec();
    prop.extend(chunk(b"FS  ", &RATE.to_be_bytes()));
    prop.extend(chunk(b"CHNL", b"\0\x02SLFTSRGT"));
    prop.extend(chunk(b"CMPR", b"DSD \0"));
    prop.extend(chunk(b"JUNK", &[]));
    chunk(b"PROP", &prop)
}

fn form(extra: &[u8]) -> Vec<u8> {
    let mut payload = b"DSD ".to_vec();
    payload.extend(properties());
    payload.extend(chunk(b"DSD ", &[0x69; 32]));
    payload.extend_from_slice(extra);
    chunk(b"FRM8", &payload)
}

fn declared_form_len(bytes: &mut [u8], len: u64) {
    bytes[4..12].copy_from_slice(&len.to_be_bytes());
}

// The old release parser can loop forever. Only this disposable child reads
// malformed sizes; the parent reaps it on both completion and timeout.
fn bounded(test: &str, body: impl FnOnce()) {
    if std::env::var("CHILPA_DFF_BOUNDS_CHILD").ok().as_deref() == Some(test) {
        body();
        return;
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env("CHILPA_DFF_BOUNDS_CHILD", test)
        .stdin(Stdio::null())
        .spawn().unwrap();
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "malformed DSDIFF child failed: {status}");
            break;
        }
        if started.elapsed() >= Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("malformed DSDIFF parser did not finish within five seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn wrapping_chunk_size_is_rejected_without_stalling() {
    bounded("wrapping_chunk_size_is_rejected_without_stalling", || {
        let mut bytes = header(b"FRM8", 16);
        bytes.extend_from_slice(b"DSD ");
        bytes.extend(header(b"JUNK", u64::MAX - 11));
        assert_eq!(bytes.len(), 28);
        reject(&bytes, "wrapping payload size");
    });
}

#[test]
fn chunk_and_property_boundaries_are_enforced() {
    bounded("chunk_and_property_boundaries_are_enforced", || {
        let mut short_form = form(&[]);
        declared_form_len(&mut short_form, 3);
        reject(&short_form, "FRM8 shorter than its form type");
        let mut overflow_form = form(&[]);
        declared_form_len(&mut overflow_form, u64::MAX);
        reject(&overflow_form, "overflowing FRM8 end");
        reject(&form(b"JUNK"), "partial outer header");
        reject(&form(&header(b"JUNK", 4)), "payload beyond FRM8");
        let mut no_padding = form(&chunk(b"JUNK", b"x"));
        no_padding.pop();
        let length = no_padding.len() as u64 - 12;
        declared_form_len(&mut no_padding, length);
        reject(&no_padding, "padding beyond FRM8");
        let mut truncated = form(&chunk(b"JUNK", b"metadata"));
        truncated.truncate(truncated.len() - 2);
        reject(&truncated, "metadata beyond the physical file");
        reject(&form(&chunk(b"PROP", b"SND")), "short property type");
        reject(&form(&chunk(b"PROP", b"SND JUNK\0\0")), "partial property header");
        let mut prop = b"SND ".to_vec();
        prop.extend(header(b"JUNK", 4096));
        reject(&form(&chunk(b"PROP", &prop)), "payload beyond PROP");
        let mut prop = b"SND ".to_vec();
        prop.extend(header(b"JUNK", 1));
        prop.push(0);
        reject(&form(&chunk(b"PROP", &prop)), "padding outside PROP");
        for (id, payload) in [(b"FS  ", &b"\0\0"[..]), (b"CHNL", &b"\0"[..])] {
            let mut prop = b"SND ".to_vec();
            prop.extend(chunk(id, payload));
            reject(&form(&chunk(b"PROP", &prop)), "short fixed-width property");
        }
    });
}

#[test]
fn odd_and_empty_chunks_keep_the_following_audio_in_place() {
    let mut extra = chunk(b"JUNK", &[]);
    extra.extend(chunk(b"DIIN", b"x"));
    let file = Fixture::new(&form(&extra));
    check_valid(&file);
}

#[test]
fn chunks_outside_the_form_are_not_scanned() {
    let mut bytes = form(&[]);
    let mut foreign_prop = b"SND ".to_vec();
    foreign_prop.extend(chunk(b"FS  ", &5_644_800u32.to_be_bytes()));
    bytes.extend(chunk(b"PROP", &foreign_prop));
    let file = Fixture::new(&bytes);
    check_valid(&file);
}

fn reject(bytes: &[u8], reason: &str) {
    let file = Fixture::new(bytes);
    assert!(read(&file.0).is_err(), "accepted {reason}");
}

fn check_valid(file: &Fixture) {
    let info = read(&file.0).unwrap();
    assert_eq!(info.sample_rate_hz, Some(RATE));
    assert_eq!(info.channels, Some(2));
    assert_eq!(info.bit_depth, Some(1));
    assert_eq!(info.duration, Duration::from_secs_f64(128.0 / f64::from(RATE)));
}

#[test]
fn unrelated_properties_do_not_replace_sound_properties() {
    let mut unrelated = b"JUNK".to_vec();
    unrelated.extend(chunk(b"FS  ", &5_644_800u32.to_be_bytes()));
    let file = Fixture::new(&form(&chunk(b"PROP", &unrelated)));
    check_valid(&file);
}

#[test]
fn a_short_comment_does_not_consume_the_next_chunk() {
    let mut extra = chunk(b"DIAR", &[]);
    let mut title = 5u32.to_be_bytes().to_vec();
    title.extend_from_slice(b"Title");
    extra.extend(chunk(b"DITI", &title));
    let file = Fixture::new(&form(&extra));
    let info = read(&file.0).unwrap();
    assert!(info.tags.artists.is_empty());
    assert_eq!(info.tags.title.as_deref(), Some("Title"));
}

#[test]
fn a_declared_audio_duration_does_not_wrap() {
    bounded("a_declared_audio_duration_does_not_wrap", || {
        let mut payload = b"DSD ".to_vec();
        payload.extend(properties());
        let audio_length = 1u64 << 62;
        payload.extend(header(b"DSD ", audio_length));
        let mut bytes = header(b"FRM8", payload.len() as u64 + audio_length);
        bytes.extend(payload);
        let file = Fixture::new(&bytes);
        let info = read(&file.0).unwrap();
        assert_eq!(info.duration.as_secs(), ((1u128 << 64) / u128::from(RATE)) as u64);
    });
}

#[test]
fn wrapping_property_size_is_rejected_without_stalling() {
    bounded("wrapping_property_size_is_rejected_without_stalling", || {
        let mut prop = b"SND ".to_vec();
        prop.extend(header(b"JUNK", u64::MAX - 11));
        reject(&form(&chunk(b"PROP", &prop)), "wrapping property payload size");
    });
}
