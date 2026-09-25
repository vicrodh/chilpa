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
/// plays. DST tracks go in too: they are the disc's, and are refused when
/// played until DST can be decoded.
pub struct DiscImages;

impl chilpa_library::scan::Containers for DiscImages {
    fn may_hold(&self, path: &Path) -> bool {
        is_image(path)
    }

    fn tracks(&self, path: &Path) -> Option<Vec<chilpa_library::scan::ContainedTrack>> {
        let disc = chilpa_sacd::open(path).ok()?;
        Some(
            disc.area
                .tracks
                .iter()
                .map(|track| chilpa_library::scan::ContainedTrack {
                    path: path_of(path, track.number).display().to_string(),
                    info: chilpa_tags::FileInfo {
                        tags: tags(&disc.area, track, path),
                        duration: std::time::Duration::from_secs_f64(track.duration_secs()),
                        sample_rate_hz: Some(chilpa_sacd::DSD64_HZ),
                        bit_depth: Some(1),
                        channels: Some(2),
                    },
                })
                .collect(),
        )
    }
}

/// The reader for disc images, for a scan that runs on its own thread.
pub fn disc_images() -> std::sync::Arc<dyn chilpa_library::scan::Containers + Send + Sync> {
    std::sync::Arc::new(DiscImages)
}

/// What the disc says about one of its tracks, as tags, and how long the
/// track runs in DoP frames: sixteen of its one-bit samples each.
pub(crate) fn about(image: &Path, number: u8) -> Option<(chilpa_tags::Tags, u64)> {
    let disc = chilpa_sacd::open(image).ok()?;
    let track = disc.area.tracks.iter().find(|t| t.number == number)?;
    Some((tags(&disc.area, track, image), dop_frames(track)))
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

fn dop_frames(track: &chilpa_sacd::SacdTrack) -> u64 {
    u64::from(track.duration_frames) * u64::from(chilpa_sacd::DSD64_HZ / 75) / 16
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
        ..chilpa_tags::Tags::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
