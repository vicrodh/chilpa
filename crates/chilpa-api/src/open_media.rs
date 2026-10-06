//! Temporary media stays outside the library and owns its listening buffer.
use crate::{Chilpa, TrackEntry, TrackRow};
use std::path::Path;

pub struct OpenedMedia {
    pub title: String,
    pub tracks: Vec<TrackRow>,
}

pub(crate) struct Session {
    pub buffer: u64,
    pub media: OpenedMedia,
}

/// Read metadata away from the GUI thread without importing anything.
pub fn read(kind: &str, path: &str) -> Result<OpenedMedia, String> {
    let path = Path::new(path);
    let mut tracks = if kind == "cd" {
        if path.as_os_str().is_empty() { return Err(chilpa_i18n::t("I did not find a CD drive")); }
        let mut row = crate::browse::file_row("");
        row.path = format!("cdda:{}", path.display());
        row.title = chilpa_i18n::t("Audio CD");
        row.available = true;
        row.quality = crate::Quality::of_file("disc.wav", Some(16), Some(44_100));
        vec![row]
    } else {
        let files = if kind == "sacd" { vec![path.to_path_buf()] } else { crate::folders::audio_files_in(path) };
        let mut rows = Vec::new();
        for file in files {
            let art = chilpa_tags::artwork_for(&file, &chilpa_dirs::cache().join("artwork"))
                .map(|p| p.display().to_string()).unwrap_or_default();
            let mut found = if chilpa_core::sacd_tracks::is_image(&file) {
                crate::folders::image_rows(&file)
            } else {
                let Ok(info) = chilpa_tags::read(&file) else { continue };
                let mut row = crate::browse::file_row(&file.to_string_lossy());
                row.title = info.tags.title.clone().unwrap_or(row.title);
                row.artist = info.tags.artist_line();
                row.album = info.tags.album.clone().unwrap_or_default();
                row.number = info.tags.track_number.unwrap_or(0);
                row.disc = info.tags.disc_number.unwrap_or(0);
                row.duration_ms = info.duration.as_millis() as u64;
                row.quality = crate::Quality::of_file(&file.to_string_lossy(), info.bit_depth, info.sample_rate_hz);
                vec![row]
            };
            for row in &mut found { row.artwork_path = art.clone(); }
            rows.extend(found);
        }
        rows
    };
    if tracks.is_empty() { return Err(chilpa_i18n::t("No playable tracks found.")); }
    tracks.sort_by(|a, b| (&a.album, a.disc, a.number).cmp(&(&b.album, b.disc, b.number)));
    let album = &tracks[0].album;
    let title = if !album.is_empty() && tracks.iter().all(|t| &t.album == album) { album.clone() }
        else if kind == "cd" { chilpa_i18n::t("Audio CD") }
        else { path.file_name().unwrap_or_default().to_string_lossy().into_owned() };
    Ok(OpenedMedia { title, tracks })
}

impl Chilpa {
    pub fn open_media(&mut self, media: OpenedMedia) {
        self.close_media();
        let paths = media.tracks.iter().map(|t| t.path.clone()).collect::<Vec<_>>();
        let title = media.title.clone();
        self.media = Some(Session { buffer: self.listen.next, media });
        self.listen_create(&title, &paths);
    }

    pub fn close_media(&mut self) {
        if let Some(session) = self.media.take() {
            self.listen_close(session.buffer);
            self.listen.undo = None;
        }
    }

    /// A track of the media open now, as it was read when it was opened:
    /// its queue shows what its page shows.
    pub(crate) fn media_row(&self, path: &str) -> Option<TrackRow> {
        self.media.as_ref()?.media.tracks.iter().find(|track| track.path == path).cloned()
    }

    pub fn media_json(&self) -> String {
        let Some(session) = &self.media else { return "{}".into() };
        let tracks = &session.media.tracks;
        let first = &tracks[0];
        serde_json::json!({"title": session.media.title, "artist": first.artist,
            "art": first.artwork_path, "count": tracks.len(), "buffer": session.buffer.to_string(),
            "duration_ms": tracks.iter().map(|t| t.duration_ms).sum::<u64>()}).to_string()
    }

    /// The media's tracks as its page lists them, each with its key in the
    /// media's own queue. A record of several discs gets a header before
    /// each, as its page in the library would; a header has no key.
    pub fn media_rows(&self, query: &str) -> (Vec<TrackEntry>, Vec<String>) {
        let Some(session) = &self.media else { return Default::default() };
        let Some(buffer) = self.listen.buffers.iter().find(|b| b.id == session.buffer) else { return Default::default() };
        let tracks = &session.media.tracks;
        // Loose files of several records are no record: their disc numbers
        // do not group them.
        let record = tracks.iter().all(|track| track.album == tracks[0].album);
        let headers = record && crate::browse::several_discs(tracks);
        let query = query.trim().to_lowercase();
        let (rows, mut keys): (Vec<_>, std::collections::VecDeque<_>) = tracks.iter().enumerate()
            .filter(|(_, row)| format!("{} {} {}", row.title, row.artist, row.album).to_lowercase().contains(&query))
            .map(|(index, row)| (row.clone(), buffer.key(index))).unzip();
        let entries = crate::browse::with_disc_headers(rows, headers);
        let keys = entries.iter().map(|entry| match entry {
            TrackEntry::Header { .. } => String::new(),
            TrackEntry::Track(_) => keys.pop_front().unwrap_or_default(),
        }).collect();
        (entries, keys)
    }
}
