//! Native Vorbis comments keep encoder provenance during unrelated edits.
//! Optional private fixtures are copied before writing. The fallback creates
//! silent media, and all audio comparisons run through ffmpeg without playback.
use chilpa_tags::edit::{snapshot, stage, Change, FileChanges, Options};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::ogg::tag::VorbisComments;
use lofty::tag::TagExt;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture { directory: PathBuf, path: PathBuf }

impl Drop for Fixture {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.directory); }
}

fn fixture(extension: &str, variant: &str) -> Option<Fixture> {
    let directory = std::env::temp_dir().join(format!("chilpa-vorbis-preservation-{}-{extension}-{variant}",std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("copy.{extension}"));
    let fixture = Fixture { directory, path };
    if let Some(root) = std::env::var_os("CHILPA_TEST_VORBIS_FIXTURES") {
        let source = PathBuf::from(root).join(format!("source.{extension}"));
        std::fs::copy(&source,&fixture.path).expect("copy the requested private fixture");
    } else {
        let codec = if extension == "flac" { "flac" } else { "libvorbis" };
        let made = Command::new("ffmpeg").args(["-nostdin","-v","error","-y","-f","lavfi","-i",
            "anullsrc=r=44100:cl=stereo","-t","0.2","-c:a",codec,"-metadata","title=Preservation fixture",
            "-metadata","genre=Heavy Metal"]).arg(&fixture.path).output();
        if !made.is_ok_and(|output|output.status.success()) {
            eprintln!("ffmpeg cannot generate this fixture; skipped {extension}");
            return None;
        }
    }
    Some(fixture)
}

fn comments(path: &Path) -> VorbisComments {
    let mut reader = std::fs::File::open(path).unwrap();
    if path.extension().is_some_and(|extension|extension == "flac") {
        lofty::flac::FlacFile::read_from(&mut reader,Default::default()).unwrap().remove_vorbis_comments().unwrap()
    } else {
        lofty::ogg::VorbisFile::read_from(&mut reader,Default::default()).unwrap().remove_vorbis_comments()
    }
}

fn other_comments(comments: &VorbisComments) -> BTreeMap<String,Vec<String>> {
    let mut fields = BTreeMap::<String,Vec<String>>::new();
    for (key,value) in comments.items().filter(|(key,_)|!key.eq_ignore_ascii_case("GENRE")) {
        fields.entry(key.to_ascii_lowercase()).or_default().push(value.to_string());
    }
    fields
}

fn audio_hash(path: &Path, copy: bool) -> Vec<u8> {
    let mut command = Command::new("ffmpeg");
    command.args(["-nostdin","-v","error","-i"]).arg(path).args(["-map","0:a:0"]);
    if copy { command.args(["-c:a","copy"]); }
    let result = command.args(["-f","hash","-hash","sha256","-"]).output().expect("ffmpeg audio hash");
    assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
    result.stdout
}

fn pictures(path: &Path) -> Vec<Vec<u8>> {
    let file = lofty::read_from_path(path).unwrap();
    file.primary_tag().into_iter().flat_map(|tag|tag.pictures()).map(|picture|picture.data().to_vec()).collect()
}

fn append_and_check(path: &Path) {
    let before = comments(path);
    let pcm = audio_hash(path,false);
    let bitstream = audio_hash(path,true);
    let artwork = pictures(path);
    let mut genres = snapshot(path).unwrap().genres;
    genres.push("Symphonic Metal".into());
    let mut changes = FileChanges::none(path.to_path_buf());
    changes.genres = Change::Set(genres.clone());
    stage(&[changes],Options::default(),&mut |_,_|{}).unwrap().commit().unwrap();
    let after = comments(path);
    assert_eq!(after.vendor(),before.vendor(),"native vendor survives a genre-only edit");
    assert_eq!(after.items().filter(|(key,_)|key.eq_ignore_ascii_case("ENCODER")).collect::<Vec<_>>(),
        before.items().filter(|(key,_)|key.eq_ignore_ascii_case("ENCODER")).collect::<Vec<_>>(),
        "original ENCODER comments survive a genre-only edit");
    assert_eq!(other_comments(&after),other_comments(&before),"all unselected native comments survive");
    assert_eq!(snapshot(path).unwrap().genres,genres);
    assert_eq!(pictures(path),artwork);
    assert_eq!(audio_hash(path,false),pcm,"decoded audio is preserved");
    assert_eq!(audio_hash(path,true),bitstream,"encoded audio is preserved");
}

#[test]
fn flac_encoder_and_vendor_survive_genre_append() {
    let Some(fixture) = fixture("flac","original") else { return; };
    assert!(comments(&fixture.path).get("ENCODER").is_some(),"the fixture must carry the reported field");
    append_and_check(&fixture.path);
}

#[test]
fn ogg_encoder_and_vendor_survive_genre_append() {
    let Some(fixture) = fixture("ogg","original") else { return; };
    assert!(comments(&fixture.path).get("ENCODER").is_some(),"the fixture must carry the reported field");
    append_and_check(&fixture.path);
}

#[test]
fn encoder_absence_and_multiple_native_values_survive_genre_append() {
    for extension in ["flac","ogg"] {
        for variant in ["absent","multiple"] {
            let Some(fixture) = fixture(extension,variant) else { continue; };
            let mut native = comments(&fixture.path);
            drop(native.remove("ENCODER"));
            if variant == "multiple" {
                native.push("ENCODER".into(),"Original encoder".into());
                native.push("encoder".into(),"Second encoder value".into());
            }
            native.save_to_path(&fixture.path,Default::default()).unwrap();
            append_and_check(&fixture.path);
        }
    }
}
