//! One-second silent originals exercise full-file verification per container.

#[test]
fn complete_originals_verify_across_supported_lossless_and_lossy_formats() {
    let mut failures = Vec::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/offline");
    for (name, extension, codec) in [
        ("silence.wav", "wav", "pcm"),
        ("silence.aiff", "aiff", "pcm"),
        ("silence.flac", "flac", "flac"),
        ("silence.alac.m4a", "m4a", "alac"),
        ("silence.aac.m4a", "m4a", "aac"),
        ("silence-scaled.aac.m4a", "m4a", "aac"),
        ("silence-scaled.alac.m4a", "m4a", "alac"),
        ("silence.mp3", "mp3", "mp3"),
        ("silence-vbr.mp3", "mp3", "mp3"),
        ("silence.ogg", "ogg", "vorbis"),
        ("silence.opus", "opus", "opus"),
    ] {
        let file = std::fs::File::open(root.join(name)).unwrap();
        let result = chilpa_decode::verification::file(file, Some(extension), || false);
        match result {
            Ok(observed) => assert_eq!(observed, codec, "{name}"),
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn truncated_originals_never_become_verified_copies() {
    use std::io::Write;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/offline");
    for (name, extension) in [
        ("silence.wav", "wav"),
        ("silence.aiff", "aiff"),
        ("silence.flac", "flac"),
        ("silence.alac.m4a", "m4a"),
        ("silence.aac.m4a", "m4a"),
        ("silence.mp3", "mp3"),
        ("silence-vbr.mp3", "mp3"),
        ("silence.ogg", "ogg"),
        ("silence.opus", "opus"),
    ] {
        let mut bytes = std::fs::read(root.join(name)).unwrap();
        bytes.truncate(bytes.len() - 32.max(bytes.len() / 10));
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&bytes).unwrap();
        file.flush().unwrap();
        assert!(
            chilpa_decode::verification::file(file.reopen().unwrap(), Some(extension), || false)
                .is_err(),
            "{name} lost its tail"
        );
    }
}
