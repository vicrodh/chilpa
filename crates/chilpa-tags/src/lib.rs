//! Leer los tags de un archivo, sin interpretarlos de más.
//!
//! Dos reglas que vienen de la auditoría de QBZ, donde el editor de tags
//! perdía datos:
//!
//! 1. **Los campos multivalor se conservan.** Un disco con tres géneros tiene
//!    tres géneros, no uno con comas. QBZ los colapsaba al guardar.
//! 2. **La fecha se conserva completa.** Si el archivo dice `2019-03-15`, eso
//!    es lo que hay; QBZ la truncaba al año aunque no hubiera cambiado.
//!
//! Este crate solo lee. Escribir tags es otra historia y llega con el editor.

use chilpa_models::{ArtworkRef, Track, TrackRef};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use lofty::tag::Accessor;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug)]
pub enum TagError {
    Open(std::io::Error),
    Unreadable(String),
}

impl std::fmt::Display for TagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagError::Open(e) => write!(f, "no se pudo abrir: {e}"),
            TagError::Unreadable(s) => write!(f, "no se pudieron leer los tags: {s}"),
        }
    }
}

/// Lo que un archivo dice de sí mismo.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Tags {
    pub title: Option<String>,
    /// Todos los artistas que declara, en orden.
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    /// Todos los géneros, sin juntarlos en una sola cadena.
    pub genres: Vec<String>,
    /// La fecha tal como viene: puede ser `2019` o `2019-03-15`.
    pub date: Option<String>,
    pub track_number: Option<u16>,
    pub disc_number: Option<u16>,
    pub isrc: Option<String>,
    pub musicbrainz_recording_id: Option<String>,
    /// Cuántas portadas trae adentro.
    pub pictures: usize,
}

impl Tags {
    /// El año, cuando la fecha lo trae. No modifica la fecha guardada.
    pub fn year(&self) -> Option<u32> {
        self.date.as_ref()?.get(..4)?.parse().ok()
    }

    /// Los artistas en una línea, para las vistas que muestran una sola.
    pub fn artist_line(&self) -> String {
        self.artists.join(", ")
    }
}

/// Lo que se sabe de un archivo: sus tags y cuánto dura.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FileInfo {
    pub tags: Tags,
    pub duration: Duration,
    pub sample_rate_hz: Option<u32>,
    pub bit_depth: Option<u8>,
    pub channels: Option<u8>,
}

/// Lee tags y propiedades de un archivo.
pub fn read(path: &Path) -> Result<FileInfo, TagError> {
    let file = Probe::open(path)
        .map_err(|e| TagError::Unreadable(e.to_string()))?
        .read()
        .map_err(|e| TagError::Unreadable(e.to_string()))?;

    let properties = file.properties();
    let mut info = FileInfo {
        tags: Tags::default(),
        duration: properties.duration(),
        sample_rate_hz: properties.sample_rate(),
        bit_depth: properties.bit_depth(),
        channels: properties.channels(),
    };

    let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) else {
        return Ok(info);
    };

    let several = |clave: ItemKey| -> Vec<String> {
        tag.get_strings(clave).map(str::to_string).collect()
    };

    info.tags = Tags {
        title: tag.title().map(|t| t.to_string()),
        artists: {
            let all = several(ItemKey::TrackArtist);
            if all.is_empty() {
                tag.artist().map(|a| vec![a.to_string()]).unwrap_or_default()
            } else {
                all
            }
        },
        album: tag.album().map(|a| a.to_string()),
        album_artist: tag.get_string(ItemKey::AlbumArtist).map(str::to_string),
        genres: {
            let all = several(ItemKey::Genre);
            if all.is_empty() {
                tag.genre().map(|g| vec![g.to_string()]).unwrap_or_default()
            } else {
                all
            }
        },
        date: tag
            .get_string(ItemKey::RecordingDate)
            .or_else(|| tag.get_string(ItemKey::ReleaseDate))
            .map(str::to_string)
            .or_else(|| tag.get_string(ItemKey::Year).map(str::to_string)),
        track_number: tag.track().map(|n| n as u16),
        disc_number: tag.disk().map(|n| n as u16),
        isrc: tag.get_string(ItemKey::Isrc).map(str::to_string),
        musicbrainz_recording_id: tag
            .get_string(ItemKey::MusicBrainzRecordingId)
            .map(str::to_string),
        pictures: tag.picture_count() as usize,
    };
    Ok(info)
}

/// Saca la portada embebida a un archivo, para que la interfaz la muestre.
///
/// Se escribe una sola vez por archivo de origen: el nombre sale de la ruta
/// y de su fecha de modificación, así que cambiar los tags produce otro
/// nombre y la vista no se queda con la portada vieja.
pub fn extract_artwork(path: &Path, cache_dir: &Path) -> Result<Option<PathBuf>, TagError> {
    let file = Probe::open(path)
        .map_err(|e| TagError::Unreadable(e.to_string()))?
        .read()
        .map_err(|e| TagError::Unreadable(e.to_string()))?;
    let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) else {
        return Ok(None);
    };
    let Some(cover) = tag.pictures().first() else {
        return Ok(None);
    };
    let extension = match cover.mime_type().map(|m| m.as_str()) {
        Some("image/png") => "png",
        _ => "jpg",
    };
    let label = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = format!("{:x}-{label}.{extension}", path_hash(path));
    std::fs::create_dir_all(cache_dir).map_err(TagError::Open)?;
    let target = cache_dir.join(name);
    if !target.exists() {
        std::fs::write(&target, cover.data()).map_err(TagError::Open)?;
    }
    Ok(Some(target))
}

/// Un hash corto y estable de la ruta, para nombrar el archivo de caché.
fn path_hash(path: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    h.finish()
}

/// Arma un [`Track`] de la biblioteca local a partir de un archivo.
pub fn track_from_file(path: &Path, id: TrackRef) -> Result<Track, TagError> {
    let info = read(path)?;
    let title = info.tags.title.clone().unwrap_or_else(|| {
        path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    });
    let mut track = Track::new(id, title);
    track.duration = info.duration;
    track.track_number = info.tags.track_number;
    track.isrc = info.tags.isrc.clone();
    track.recording_mbid = info.tags.musicbrainz_recording_id.clone();
    track.artists = info
        .tags
        .artists
        .iter()
        .map(|name| chilpa_models::ArtistRef::named(name))
        .collect();
    track.album = info
        .tags
        .album
        .as_ref()
        .map(|title| chilpa_models::AlbumRef::titled(title));
    track.artwork = if info.tags.pictures > 0 {
        ArtworkRef::Embedded { path: path.to_path_buf(), index: 0 }
    } else {
        ArtworkRef::None
    };
    Ok(track)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_year_comes_from_the_date_without_truncating_it() {
        let tags = Tags { date: Some("2019-03-15".into()), ..Tags::default() };
        assert_eq!(tags.year(), Some(2019));
        assert_eq!(tags.date.as_deref(), Some("2019-03-15"), "la fecha se conserva completa");
    }

    #[test]
    fn several_artists_show_on_one_line_without_getting_lost() {
        let tags = Tags { artists: vec!["Uno".into(), "Dos".into()], ..Tags::default() };
        assert_eq!(tags.artist_line(), "Uno, Dos");
        assert_eq!(tags.artists.len(), 2, "siguen siendo dos, no una cadena");
    }

    #[test]
    fn the_cover_name_changes_when_the_file_changes() {
        // Dos rutas distintas no pueden compartir archivo de caché.
        assert_ne!(path_hash(Path::new("/a.flac")), path_hash(Path::new("/b.flac")));
        assert_eq!(path_hash(Path::new("/a.flac")), path_hash(Path::new("/a.flac")));
    }

    #[test]
    fn a_file_that_does_not_exist_is_reported() {
        assert!(read(Path::new("/no/existe.flac")).is_err());
    }
}
