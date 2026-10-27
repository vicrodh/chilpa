//! I retain native Vorbis fields that the generic tag model does not expose.

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType};
use lofty::ogg::{OpusFile, SpeexFile, VorbisFile};
use lofty::ogg::tag::VorbisComments;
use lofty::probe::Probe;
use std::path::Path;

/// I read metadata only, without properties or artwork buffers. The caller
/// drops these comments after its scan or staged write; no cache is retained.
pub(crate) fn comments(path: &Path) -> Result<VorbisComments, String> {
    let probe = Probe::open(path).map_err(|error| error.to_string())?
        .guess_file_type().map_err(|error| error.to_string())?;
    let kind = probe.file_type();
    let mut reader = probe.into_inner();
    let options = ParseOptions::new().read_properties(false).read_cover_art(false);
    match kind {
        Some(FileType::Flac) => lofty::flac::FlacFile::read_from(&mut reader, options)
            .map(|mut file| file.remove_vorbis_comments().unwrap_or_default()),
        Some(FileType::Vorbis) => VorbisFile::read_from(&mut reader, options).map(|mut file| file.remove_vorbis_comments()),
        Some(FileType::Opus) => OpusFile::read_from(&mut reader, options).map(|mut file| file.remove_vorbis_comments()),
        Some(FileType::Speex) => SpeexFile::read_from(&mut reader, options).map(|mut file| file.remove_vorbis_comments()),
        _ => return Err("The file does not contain Vorbis comments".into()),
    }.map_err(|error| error.to_string())
}

pub(crate) fn read(path: &Path) -> Result<Option<bool>, String> {
    Ok(comments(path)?.get("ITUNESADVISORY").and_then(crate::advisory))
}
