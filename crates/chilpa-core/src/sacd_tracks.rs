//! The tracks inside a SACD image, each addressed on its own.
//!
//! A queue, a playlist and the library hold one string per track, and an
//! image holds many. A track is `sacd:<image>#<number>`, the form QBZ used,
//! so what one wrote the other can read.

use std::path::{Path, PathBuf};

const PREFIX: &str = "sacd:";

/// The path of track `number` of `image`.
pub fn path_of(image: &Path, number: u8) -> PathBuf {
    PathBuf::from(format!("{PREFIX}{}#{number}", image.display()))
}

/// The image and the track number a path names, when it names one.
pub fn parse(path: &Path) -> Option<(PathBuf, u8)> {
    let rest = path.to_str()?.strip_prefix(PREFIX)?;
    // The last '#': a folder's name can carry one, a track number cannot.
    let (image, number) = rest.rsplit_once('#')?;
    Some((PathBuf::from(image), number.parse().ok()?))
}

/// True if the file is a disc image, by its extension.
pub fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("iso"))
}

/// Paths as the tracks they hold: a SACD image as one path per track of its
/// stereo area, anything else as it is.
///
/// An image that does not open as a SACD stays as it is, and is refused where
/// it is played, with the reason.
pub fn expand(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        match is_image(path).then(|| chilpa_sacd::open(path).ok()).flatten() {
            Some(disc) => out.extend(disc.area.tracks.iter().map(|t| path_of(path, t.number))),
            None => out.push(path.clone()),
        }
    }
    out
}

/// Disc images as the library sees them: files that hold tracks.
///
/// The library does not know what is inside an image and may not depend on
/// what does, so the core hands it this. Each track goes in under its own
/// path, named by the disc, as DSD64 stereo -- the stereo area is what
/// plays, DST compressed or not.
pub struct DiscImages;

impl chilpa_library::scan::Containers for DiscImages {
    fn may_hold(&self, path: &Path) -> bool {
        is_image(path)
    }

    fn evidence_stamp(&self, path: &Path) -> u64 {
        super::sacd_xml::stamp(path)
    }

    fn tracks(&self, path: &Path) -> Option<Vec<chilpa_library::scan::ContainedTrack>> {
        match self.contents(path) {
            chilpa_library::scan::Contents::Tracks(tracks) => Some(tracks),
            _ => None,
        }
    }

    /// An image that could not be read this time, from the disk or the
    /// network, is told apart from one that is not a SACD: the library keeps
    /// what it had from the first.
    fn contents(&self, path: &Path) -> chilpa_library::scan::Contents {
        let disc = match chilpa_sacd::open(path) {
            Ok(disc) => disc,
            Err(chilpa_sacd::SacdError::Open(error)) => return chilpa_library::scan::Contents::Unreadable(error),
            Err(_) => return chilpa_library::scan::Contents::Nothing,
        };
        let expected: Vec<_> = disc.area.tracks.iter().map(|track| track.number).collect();
        let xml = super::sacd_xml::read_for(path, &expected);
        chilpa_library::scan::Contents::Tracks(
            disc.area
                .tracks
                .iter()
                .map(|track| {
                    let mut tags = tags(&disc.area, track, path);
                    if let Some(xml) = &xml { xml.overlay_tags(track.number, &mut tags); }
                    chilpa_library::scan::ContainedTrack {
                    path: path_of(path, track.number).display().to_string(),
                    info: chilpa_tags::FileInfo {
                        tags,
                        duration: std::time::Duration::from_secs_f64(track.duration_secs()),
                        sample_rate_hz: Some(chilpa_sacd::DSD64_HZ),
                        bit_depth: Some(1),
                        channels: Some(2),
                    },
                }})
                .collect(),
        )
    }
}

/// The reader for disc images, for a scan that runs on its own thread.
pub fn disc_images() -> std::sync::Arc<dyn chilpa_library::scan::Containers + Send + Sync> {
    std::sync::Arc::new(DiscImages)
}

/// The external stereo XML paired with this image, if the match is exact.
pub fn xml_for(image: &Path) -> Option<super::sacd_xml::XmlMetadata> {
    let disc = chilpa_sacd::open(image).ok()?;
    let expected: Vec<_> = disc.area.tracks.iter().map(|track| track.number).collect();
    super::sacd_xml::read_for(image, &expected)
}

/// What the disc says about one of its tracks, as tags, and how long the
/// track runs in one-bit samples per channel at DSD64. How many frames that
/// is depends on what opened -- DoP, native or PCM -- and the player counts
/// it from there (`dsd_playback::frames_opened`).
pub(crate) fn about(image: &Path, number: u8) -> Option<(chilpa_tags::Tags, u64)> {
    let disc = chilpa_sacd::open(image).ok()?;
    let track = disc.area.tracks.iter().find(|t| t.number == number)?;
    let mut tags = tags(&disc.area, track, image);
    let expected: Vec<_> = disc.area.tracks.iter().map(|track| track.number).collect();
    if let Some(xml) = super::sacd_xml::read_for(image, &expected) { xml.overlay_tags(number, &mut tags); }
    Some((tags, one_bit_samples(track)))
}

/// A track's title as its disc gives it, when the path names a track of an
/// image.
pub fn title(path: &Path) -> Option<String> {
    let (image, number) = parse(path)?;
    about(&image, number)?.0.title
}

/// Opens track `number` of `image` for reading.
pub(crate) fn open(image: &Path, number: u8) -> Result<chilpa_sacd::SacdTrackReader, String> {
    let disc = chilpa_sacd::open(image).map_err(|e| e.to_string())?;
    let track = disc.area.tracks.iter().find(|t| t.number == number).cloned().ok_or_else(|| {
        chilpa_i18n::t_args("the image has no track {}", &[&number.to_string()])
    })?;
    chilpa_sacd::SacdTrackReader::open(disc, &track).map_err(|e| e.to_string())
}

/// A disc counts time in frames of 1/75 s.
fn one_bit_samples(track: &chilpa_sacd::SacdTrack) -> u64 {
    u64::from(track.duration_frames) * u64::from(chilpa_sacd::DSD64_HZ / 75)
}

/// The disc's names for a track. Where the disc says nothing, the track is
/// numbered and the album takes the image's file name, as it always did.
fn tags(area: &chilpa_sacd::SacdArea, track: &chilpa_sacd::SacdTrack, image: &Path) -> chilpa_tags::Tags {
    chilpa_tags::Tags {
        title: track
            .title
            .clone()
            .or_else(|| Some(chilpa_i18n::t_args("Track {}", &[&track.number.to_string()]))),
        artists: track.performer.clone().or_else(|| area.artist.clone()).into_iter().collect(),
        album: area
            .album
            .clone()
            .or_else(|| image.file_stem().map(|s| s.to_string_lossy().to_string())),
        album_artist: area.artist.clone(),
        track_number: Some(u16::from(track.number)),
        disc_number: area.disc_number,
        disc_total: area.disc_total,
        ..chilpa_tags::Tags::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_tracks_retain_the_master_set_evidence() {
        let track = chilpa_sacd::SacdTrack { number: 1, start_lsn: 550, length_lsn: 10,
            start_frame: 0, duration_frames: 75, encoding: chilpa_sacd::Encoding::Dsd3In14,
            title: None, performer: None };
        let mut area = chilpa_sacd::SacdArea { tracks: vec![track.clone()], album: None,
            artist: None, disc_number: Some(2), disc_total: Some(3),
            encoding: chilpa_sacd::Encoding::Dsd3In14, audio_start_lsn: 550, audio_end_lsn: 559 };
        for (number, total) in [(Some(2), Some(3)), (Some(1), Some(1)), (None, None)] {
            area.disc_number = number;
            area.disc_total = total;
            let read = tags(&area, &track, Path::new("disc.iso"));
            assert_eq!((read.disc_number, read.disc_total), (number, total));
        }
    }

    #[test]
    fn an_image_that_cannot_be_read_is_told_apart_from_one_that_is_no_disc() {
        use chilpa_library::scan::{Containers, Contents};
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("dvd.iso");
        std::fs::write(&other, vec![0u8; 4096]).unwrap();
        assert!(matches!(DiscImages.contents(&other), Contents::Nothing), "no SACD: holds nothing");
        assert!(matches!(DiscImages.contents(&dir.path().join("gone.iso")), Contents::Unreadable(_)), "could not be opened");
        assert!(DiscImages.tracks(&other).is_none());
    }

    #[test]
    fn a_track_path_goes_out_and_comes_back() {
        let image = Path::new("/music/Pink Floyd #1/Wish You Were Here.iso");
        let path = path_of(image, 3);
        assert_eq!(path, PathBuf::from("sacd:/music/Pink Floyd #1/Wish You Were Here.iso#3"));
        assert_eq!(parse(&path), Some((image.to_path_buf(), 3)), "a '#' in a folder is not the track's");
        assert_eq!(parse(Path::new("/music/a.flac")), None);
        assert_eq!(parse(Path::new("sacd:/music/a.iso#x")), None);
    }
}
