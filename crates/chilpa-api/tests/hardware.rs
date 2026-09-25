//! The smoke that needs a device, a track and a person who decided to run it.
//!
//! Ignored by default and never run in CI, because it opens real hardware and
//! makes real sound. What it exists for is the one question no synthetic test
//! can answer: **did the card open in the track's own format, or did something
//! quietly convert on the way?** Everything else here is measured against a
//! capture that accepts whatever it is handed.
//!
//! On Linux the kernel answers that question directly. While a PCM is open,
//! `/proc/asound/cardN/pcm0p/sub0/hw_params` says what it was opened with --
//! not what Chilpa asked for, not what Chilpa believes. That file is the
//! evidence, and this test reads it while the track plays.
//!
//! ```text
//! CHILPA_SMOKE_DEVICE=alsa:hw:1,0 \
//! CHILPA_SMOKE_TRACK="/path/to/a 24-48 track.flac" \
//! CHILPA_SMOKE_CARD=1 \
//!   cargo test -p chilpa-api --test hardware -- --ignored --nocapture --test-threads=1
//! ```
//!
//! One thread, because both tests open the same card and the second one to
//! ask for it would find it taken.
//!
//! With no variables set it says what it needed and passes, because a smoke
//! that fails for not being asked to run teaches nobody anything.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// What the file carries, according to something that is not Chilpa.
///
/// `ffprobe` and not the decoder, and not because the decoder is unavailable
/// here: a smoke that asks Chilpa what the file is and then checks Chilpa
/// against that answer has checked nothing. The kernel and ffprobe are the
/// two witnesses in this test and neither of them is us.
fn rate_of(track: &Path) -> Option<u32> {
    let out = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries",
               "stream=sample_rate", "-of", "default=nw=1:nk=1"])
        .arg(track)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().lines().next()?.trim().parse().ok()
}

/// How many bits the file carries, according to the same witness.
///
/// `None` when ffprobe does not say, which lossy formats do not: they decode
/// to floats and have no bit depth of their own.
fn bits_of(track: &Path) -> Option<u8> {
    let out = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries",
               "stream=bits_per_raw_sample", "-of", "default=nw=1:nk=1"])
        .arg(track)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().lines().next()?.trim().parse().ok()
}

/// What the kernel says the card is running, while it is running.
fn hw_params(card: &str) -> String {
    let path = format!("/proc/asound/card{card}/pcm0p/sub0/hw_params");
    std::fs::read_to_string(path).unwrap_or_else(|_| "unreadable".to_string())
}

/// Which process has the card open, from the kernel's own status file.
///
/// `hw_params` says what the card is running, not who is running it. Read
/// while another program has the card -- a sound server playing somebody's
/// music -- it describes that program's stream, and this smoke passed on one
/// while Chilpa could not open the card at all.
fn owner_of(card: &str) -> Option<u32> {
    let path = format!("/proc/asound/card{card}/pcm0p/sub0/status");
    let text = std::fs::read_to_string(path).ok()?;
    let line = text.lines().find(|l| l.trim_start().starts_with("owner_pid"))?;
    line.split(':').nth(1)?.trim().parse().ok()
}

/// Whether a process id the kernel names is one of this process's threads.
///
/// The kernel records the thread that opened the card -- the player's, here
/// -- not the process. Comparing it with the process id never matched, and
/// with the card free every run failed saying the card never opened while
/// Chilpa was playing through it.
fn is_this_process(pid: u32) -> bool {
    Path::new(&format!("/proc/self/task/{pid}")).exists()
}

/// Fails before anything is opened if somebody else has the card.
///
/// Opening it then would fail anyway, and what the kernel says about it
/// would be the other program's stream. Stopping here also means the smoke
/// never takes the card from under whoever is listening.
fn free(card: &str) {
    if let Some(pid) = owner_of(card) {
        assert!(
            is_this_process(pid),
            "card {card} is in use by process {pid}; free it before running the smoke"
        );
    }
}

/// What the kernel says the card is running, and only if this process is
/// the one running it.
fn ours(card: &str) -> Option<String> {
    let text = hw_params(card);
    (text.contains("rate:") && owner_of(card).is_some_and(is_this_process)).then_some(text)
}

/// One field of it, as `rate` or `format`.
fn field(text: &str, name: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix(&format!("{name}: ")))
        .map(|v| v.trim().to_string())
}

/// Points the library somewhere disposable.
///
/// This smoke needs the real device and nothing else. Without this it opened
/// the real library and wrote plays into it, which is the one thing a test
/// must never do to the machine it runs on: every other facade test here
/// isolates, and this one did not until it had already put rows in mine.
fn other_library() {
    let dir = std::env::temp_dir().join("chilpa-hardware-smoke");
    let _ = std::fs::create_dir_all(&dir);
    std::env::set_var("XDG_DATA_HOME", dir);
}

fn asked_for() -> Option<(String, PathBuf, String)> {
    let device = std::env::var("CHILPA_SMOKE_DEVICE").ok()?;
    let track = std::env::var("CHILPA_SMOKE_TRACK").ok()?;
    let card = std::env::var("CHILPA_SMOKE_CARD").unwrap_or_else(|_| "1".to_string());
    Some((device, PathBuf::from(track), card))
}

#[test]
#[ignore = "opens a real device and makes a real sound; run it on purpose"]
fn the_card_opens_in_the_track_s_own_format() {
    let Some((device, track, card)) = asked_for() else {
        eprintln!(
            "smoke: set CHILPA_SMOKE_DEVICE, CHILPA_SMOKE_TRACK and CHILPA_SMOKE_CARD to run this"
        );
        return;
    };
    assert!(track.exists(), "{} is not there", track.display());
    free(&card);
    other_library();

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_file(&track, &device, true);

    // Wait for the card to be open, not for Chilpa to say it is -- and then
    // for Chilpa to have said it, which is not the same moment. The kernel
    // opens the PCM before the `Started` event has been drained, so reading
    // the verdict as soon as `hw_params` fills in catches the state before
    // anything was reported and makes it look like the player claimed
    // nothing. That is the test racing, not the player lying.
    let mut opened = String::new();
    let limit = Instant::now() + Duration::from_secs(10);
    while Instant::now() < limit {
        if opened.is_empty() {
            if let Some(kernel) = ours(&card) {
                opened = kernel;
            }
        }
        let now = chilpa.poll();
        if !opened.is_empty() && !now.format.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let now = chilpa.poll().clone();
    assert!(
        !opened.is_empty(),
        "the card never opened. What Chilpa says: playing={}, error={:?}",
        now.playing,
        now.error,
    );

    eprintln!("smoke: the kernel says the card opened with:\n{opened}");
    eprintln!("smoke: Chilpa says: {} · bit-perfect {}", now.format, now.bit_perfect);

    let rate: u32 = field(&opened, "rate")
        .and_then(|r| r.split_whitespace().next().map(str::to_string))
        .and_then(|r| r.parse().ok())
        .expect("the kernel states a rate");
    let format = field(&opened, "format").expect("the kernel states a format");

    match rate_of(&track) {
        Some(source_rate) => {
            assert_eq!(
                rate, source_rate,
                "the card opened at {rate} Hz for a {source_rate} Hz track: something resampled",
            );
            eprintln!("smoke: {source_rate} Hz in, {rate} Hz on the wire, format {format}");
        }
        // Without the second witness the rate is reported and not asserted.
        // Saying it matched would be saying it on Chilpa's word alone.
        None => eprintln!(
            "smoke: no ffprobe, so {rate} Hz and {format} are reported, not checked"
        ),
    }

    // The word on the wire can be wider than the track -- a DAC that only
    // takes 32 bit words gets a CD rip widened -- and that is still
    // bit-perfect. What the window says is the music's depth, not the word's.
    if let Some(source_bits) = bits_of(&track).filter(|_| now.bit_perfect) {
        assert!(
            now.format.contains(&format!(" {source_bits} bits ")),
            "a {source_bits} bit track, and the window says: {}",
            now.format,
        );
        eprintln!("smoke: {source_bits} bits in the file, and the window says so");
    }

    chilpa.stop();
}

#[test]
#[ignore = "opens a real device and makes a real sound; run it on purpose"]
fn what_the_window_would_claim_matches_what_the_kernel_did() {
    // The invariant, checked against the one witness that cannot be argued
    // with. If Chilpa says bit-perfect, the card has to be running at the
    // track's own rate. The reverse is allowed: an exact rate through a mixer
    // is not bit-perfect, and the stack is what knows that.
    let Some((device, track, card)) = asked_for() else {
        eprintln!("smoke: set the three variables to run this");
        return;
    };
    free(&card);
    other_library();
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.play_file(&track, &device, true);

    let limit = Instant::now() + Duration::from_secs(10);
    let mut opened = String::new();
    while Instant::now() < limit {
        if opened.is_empty() {
            if let Some(kernel) = ours(&card) {
                opened = kernel;
            }
        }
        let now = chilpa.poll();
        if !opened.is_empty() && !now.format.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let now = chilpa.poll().clone();
    if opened.is_empty() {
        eprintln!("smoke: the card never opened; error={:?}", now.error);
        chilpa.stop();
        return;
    }

    if now.bit_perfect {
        let rate: u32 = field(&opened, "rate")
            .and_then(|r| r.split_whitespace().next().map(str::to_string))
            .and_then(|r| r.parse().ok())
            .expect("the kernel states a rate");
        if let Some(source_rate) = rate_of(&track) {
            assert_eq!(
                rate, source_rate,
                "Chilpa claimed bit-perfect while the card ran at {rate} Hz for {source_rate} Hz",
            );
        }
    }
    chilpa.stop();
}
