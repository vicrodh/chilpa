//! Temporary media stays outside the library and owns its listening buffer.
use crate::discs::{block_header, block_of, record_folder, About};
use crate::{Chilpa, TrackEntry, TrackRow};
use chilpa_library::album_folder::without_disc;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

pub struct OpenedMedia {
    pub title: String,
    pub tracks: Vec<TrackRow>,
    /// What each track's file says besides its row, in the tracks' order.
    /// Media built by hand may carry none.
    pub about: Vec<About>,
}

pub(crate) struct Session {
    pub buffer: u64,
    pub media: OpenedMedia,
}

/// The cover in a folder, looked for once per folder.
fn cover_in(folder: PathBuf, seen: &mut BTreeMap<PathBuf, String>) -> String {
    seen.entry(folder).or_insert_with_key(|folder| {
        chilpa_tags::cover_beside(&folder.join("_")).map(|p| p.display().to_string()).unwrap_or_default()
    }).clone()
}

/// What a launch's paths play: a folder gives its audio files, an audio
/// file itself, an SACD image itself. Anything else is not music.
fn launched_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            files.extend(crate::folders::audio_files_in(path));
        } else if crate::folders::is_audio(path) {
            files.push(path.clone());
        }
    }
    files
}

/// Read metadata away from the GUI thread without importing anything.
///
/// The kind `launch` takes one path per line, as a file manager's "Open
/// with" brings them: files and folders, mixed.
pub fn read(kind: &str, path: &str) -> Result<OpenedMedia, String> {
    let launch_paths: Vec<PathBuf> = if kind == "launch" { path.lines().map(PathBuf::from).collect() } else { Vec::new() };
    let path = if kind == "launch" { launch_paths.first().map(PathBuf::as_path).unwrap_or(Path::new("")) } else { Path::new(path) };
    let mut found: Vec<(TrackRow, About)> = if kind == "cd" {
        if path.as_os_str().is_empty() { return Err(chilpa_i18n::t("I did not find a CD drive")); }
        let mut row = crate::browse::file_row("");
        row.path = format!("cdda:{}", path.display());
        row.title = chilpa_i18n::t("Audio CD");
        row.available = true;
        row.quality = crate::Quality::of_file("disc.wav", Some(16), Some(44_100));
        vec![(row, About::default())]
    } else {
        let files = match kind {
            "sacd" => vec![path.to_path_buf()],
            "launch" => launched_files(&launch_paths),
            _ => crate::folders::audio_files_in(path),
        };
        let mut rows = Vec::new();
        let mut covers = BTreeMap::new();
        for file in files {
            let folder = file.parent().unwrap_or(Path::new("")).to_path_buf();
            // A track's cover, from the nearest outwards: its own file, its
            // disc's folder, its record's.
            let art = chilpa_tags::artwork_for(&file, &chilpa_dirs::cache().join("artwork"))
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| cover_in(PathBuf::from(record_folder(&file.to_string_lossy())), &mut covers));
            let mut about = About { own_cover: cover_in(folder, &mut covers), ..About::default() };
            let mut read = if chilpa_core::sacd_tracks::is_image(&file) {
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
                about.album_artist = info.tags.album_artist.clone().unwrap_or_default().trim().to_owned();
                about.year = info.tags.year();
                vec![row]
            };
            for row in &mut read { row.artwork_path = art.clone(); }
            rows.extend(read.into_iter().map(|row| (row, about.clone())));
        }
        rows
    };
    if found.is_empty() { return Err(chilpa_i18n::t("No playable tracks found.")); }
    // By record, then by disc as a number, then as the disc orders them: a
    // box keeps its discs together and in order whatever each is called.
    found.sort_by_cached_key(|(row, _)| {
        let (folder, record, disc) = block_of(row);
        (record, folder, disc, row.number)
    });
    let (tracks, about): (Vec<_>, Vec<_>) = found.into_iter().unzip();
    // The media is a record when every track agrees on which, discs aside.
    let record = without_disc(&tracks[0].album);
    let title = if !record.is_empty() && tracks.iter().all(|t| without_disc(&t.album).to_lowercase() == record.to_lowercase()) { record }
        else if kind == "cd" { chilpa_i18n::t("Audio CD") }
        else { path.file_name().unwrap_or_default().to_string_lossy().into_owned() };
    Ok(OpenedMedia { title, tracks, about })
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
    /// media's own queue. Media of several records, or of a record of
    /// several discs, gets a header before each: its cover, its title and
    /// its artist. A header has no key.
    pub fn media_rows(&self, query: &str) -> (Vec<TrackEntry>, Vec<String>) {
        let Some(session) = &self.media else { return Default::default() };
        let Some(buffer) = self.listen.buffers.iter().find(|b| b.id == session.buffer) else { return Default::default() };
        let (tracks, about) = (&session.media.tracks, &session.media.about);
        let blocks: Vec<_> = tracks.iter().map(block_of).collect();
        // One block is the page's own header; it is not said twice.
        let several = blocks.windows(2).any(|pair| pair[0] != pair[1]);
        let disc_said = tracks.iter().any(|track| track.disc.max(1) != tracks[0].disc.max(1));
        let query = query.trim().to_lowercase();
        let shown: Vec<usize> = (0..tracks.len())
            .filter(|i| format!("{} {} {}", tracks[*i].title, tracks[*i].artist, tracks[*i].album).to_lowercase().contains(&query))
            .collect();
        let (mut entries, mut keys) = (Vec::new(), VecDeque::new());
        for (at, &i) in shown.iter().enumerate() {
            if several && (at == 0 || blocks[shown[at - 1]] != blocks[i]) {
                let count = shown[at..].iter().take_while(|j| blocks[**j] == blocks[i]).count();
                entries.push(TrackEntry::Header(block_header(&tracks[i], about.get(i), count, disc_said)));
                keys.push_back(String::new());
            }
            entries.push(TrackEntry::Track(tracks[i].clone()));
            keys.push_back(buffer.key(i));
        }
        (entries, keys.into())
    }
}
