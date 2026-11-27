//! A playlist written to a file another player reads, and what a name may
//! lose on its way to a source.
//!
//! M3U8 lists the tracks that are files on this computer; XSPF and JSON
//! also keep, by title, artist and album, the ones that are not (a server's,
//! a source's), so the importer can find them again elsewhere. The JSON is
//! the importer's own playlist: a name, a description and tracks with title,
//! artist, album, `duration_ms` and ISRC.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::TrackRow;

/// What writing a playlist left: how many tracks went in, how many had no
/// place in the format, and the reason it failed, empty when it did not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Exported {
    pub written: usize,
    pub left_out: usize,
    pub error: String,
}

impl Exported {
    pub fn json(&self) -> String {
        serde_json::json!({"written": self.written, "left_out": self.left_out, "error": self.error}).to_string()
    }
}

/// The file a row plays from, when it is one on this computer: not a
/// server's, not a source's, not a track inside a disc image.
fn file_of(row: &TrackRow) -> Option<PathBuf> {
    let path = Path::new(&row.path);
    let file = path.is_absolute()
        && !chilpa_models::RemoteEntry::is_remote(&row.path)
        && chilpa_core::sacd_tracks::parse(path).is_none();
    file.then(|| path.to_path_buf())
}

/// A file path as M3U writes it: relative to the list's folder when it is
/// inside it, so the list moves with the music.
fn relative(base: &Path, track: &Path) -> PathBuf {
    track.strip_prefix(base).map(Path::to_path_buf).unwrap_or_else(|_| track.to_path_buf())
}

/// The line a player shows for a track: "Artist - Title", or the title.
fn shown(row: &TrackRow) -> String {
    let line = if row.artist.is_empty() { row.title.clone() } else { format!("{} - {}", row.artist, row.title) };
    // A line break would start a line of its own and read as a path.
    line.replace(['\r', '\n'], " ")
}

/// An extended M3U8 list for the folder `base`, and how many rows it left
/// out for not being files here.
pub fn m3u8(name: &str, rows: &[TrackRow], base: &Path) -> (String, usize) {
    let mut text = String::from("#EXTM3U\n");
    if !name.is_empty() {
        let _ = writeln!(text, "#PLAYLIST:{}", name.replace(['\r', '\n'], " "));
    }
    let mut left_out = 0;
    for row in rows {
        let Some(file) = file_of(row) else {
            left_out += 1;
            continue;
        };
        let seconds = if row.duration_ms == 0 { -1 } else { ((row.duration_ms + 500) / 1000) as i64 };
        let _ = writeln!(text, "#EXTINF:{seconds},{}", shown(row));
        let _ = writeln!(text, "{}", relative(base, &file).display());
    }
    (text, left_out)
}

fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 has no place for most control characters.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// A file path as a `file://` URI, each byte outside the unreserved set
/// percent-encoded.
fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            uri.push(byte as char);
        } else {
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri
}

/// An XSPF list: every row, with its file when it is one here.
pub fn xspf(name: &str, description: &str, rows: &[TrackRow]) -> String {
    let mut text = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<playlist version=\"1\" xmlns=\"http://xspf.org/ns/0/\">\n");
    if !name.is_empty() {
        let _ = writeln!(text, "  <title>{}</title>", escaped(name));
    }
    if !description.is_empty() {
        let _ = writeln!(text, "  <annotation>{}</annotation>", escaped(description));
    }
    text.push_str("  <trackList>\n");
    for row in rows {
        text.push_str("    <track>\n");
        if let Some(file) = file_of(row) {
            let _ = writeln!(text, "      <location>{}</location>", escaped(&file_uri(&file)));
        }
        for (tag, value) in [("title", &row.title), ("creator", &row.artist), ("album", &row.album)] {
            if !value.is_empty() {
                let _ = writeln!(text, "      <{tag}>{}</{tag}>", escaped(value));
            }
        }
        if row.duration_ms > 0 {
            let _ = writeln!(text, "      <duration>{}</duration>", row.duration_ms);
        }
        text.push_str("    </track>\n");
    }
    text.push_str("  </trackList>\n</playlist>\n");
    text
}

/// The playlist as the importer reads it back: every row, with the ISRC
/// the library knows for it.
pub fn json(name: &str, description: &str, rows: &[TrackRow], isrcs: &HashMap<i64, String>) -> String {
    let tracks: Vec<serde_json::Value> = rows.iter().map(|row| {
        let mut track = serde_json::json!({"title": row.title, "artist": row.artist});
        if !row.album.is_empty() {
            track["album"] = row.album.clone().into();
        }
        if row.duration_ms > 0 {
            track["duration_ms"] = row.duration_ms.into();
        }
        if let Some(isrc) = isrcs.get(&row.id) {
            track["isrc"] = isrc.clone().into();
        }
        track
    }).collect();
    let mut document = serde_json::json!({"name": name, "tracks": tracks});
    if !description.is_empty() {
        document["description"] = description.into();
    }
    serde_json::to_string_pretty(&document).unwrap_or_default() + "\n"
}

/// The format a file's extension names; M3U8 for any other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    M3u8,
    Xspf,
    Json,
}

fn format_of(path: &Path) -> Format {
    match path.extension().map(|ext| ext.to_ascii_lowercase()) {
        Some(ext) if ext == "xspf" => Format::Xspf,
        Some(ext) if ext == "json" => Format::Json,
        _ => Format::M3u8,
    }
}

/// Writes the text whole or not at all: a list cut in half by a full disk
/// would still open, short.
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let dir = path.parent().filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(text.as_bytes())?;
    file.persist(path).map(|_| ()).map_err(|error| error.error)
}

/// A character a source may refuse in a name: emoji, pictographs and the
/// symbol blocks (arrows, shapes, dingbats, technical and math signs), the
/// joiners and selectors emoji are built with, private-use characters and
/// controls. Letters, marks and punctuation of every script are ordinary.
fn unusual(c: char) -> bool {
    matches!(c as u32,
        0x0000..=0x001F | 0x007F..=0x009F
        | 0x200D | 0x20E3 | 0xFE00..=0xFE0F
        | 0x2190..=0x21FF | 0x2200..=0x22FF | 0x2300..=0x23FF | 0x2460..=0x24FF
        | 0x2500..=0x25FF | 0x2600..=0x27BF | 0x2900..=0x297F | 0x2B00..=0x2BFF
        | 0xE000..=0xF8FF | 0x1F000..=0x1FAFF | 0xE0000..=0xE007F | 0xF0000..=0x10FFFF)
}

/// Whether a name carries a character a source may refuse, so that the
/// name field says it may be cut short there.
pub fn name_is_unusual(name: &str) -> bool {
    name.chars().any(unusual)
}

impl crate::Chilpa {
    /// Writes a playlist to `path`: XSPF for a `.xspf` file, JSON for a
    /// `.json` one, M3U8 for any other.
    pub fn export_playlist(&mut self, playlist_id: i64, path: &Path) -> Exported {
        let Some(list) = self.playlist(playlist_id) else {
            return Exported { error: chilpa_i18n::t("that list is gone"), ..Default::default() };
        };
        let rows = self.playlist_rows(playlist_id);
        let (text, left_out) = match format_of(path) {
            Format::Xspf => (xspf(&list.name, &list.description, &rows), 0),
            Format::Json => {
                let ids: Vec<i64> = rows.iter().map(|row| row.id).filter(|id| *id > 0).collect();
                let mut isrcs = HashMap::new();
                if let Some(lib) = self.library() {
                    for chunk in ids.chunks(500) {
                        for (id, (isrc, _)) in lib.fingerprints_for(chunk).unwrap_or_default() {
                            if let Some(isrc) = isrc.filter(|isrc| !isrc.is_empty()) {
                                isrcs.insert(id, isrc);
                            }
                        }
                    }
                }
                (json(&list.name, &list.description, &rows, &isrcs), 0)
            }
            Format::M3u8 => m3u8(&list.name, &rows, path.parent().unwrap_or(Path::new("."))),
        };
        match write_whole(path, &text) {
            Ok(()) => Exported { written: rows.len() - left_out, left_out, error: String::new() },
            Err(error) => Exported { error: chilpa_i18n::t_args("the list could not be saved: {}", &[&error.to_string()]), ..Default::default() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(path: &str, title: &str, artist: &str, duration_ms: u64) -> TrackRow {
        let mut row = crate::browse::file_row(path);
        row.title = title.into();
        row.artist = artist.into();
        row.album = "Album & Co".into();
        row.duration_ms = duration_ms;
        row
    }

    #[test]
    fn m3u8_writes_files_relative_to_its_folder_and_leaves_out_the_rest() {
        let rows = [
            row("/music/a/One.flac", "One", "Band", 61_400),
            row("remote:plugin/source/7", "Streamed", "Band", 200_000),
            row("/elsewhere/Two.flac", "Two", "", 0),
        ];
        let (text, left_out) = m3u8("Mix\nnew", &rows, Path::new("/music"));
        assert_eq!(left_out, 1);
        assert_eq!(text, "#EXTM3U\n#PLAYLIST:Mix new\n#EXTINF:61,Band - One\na/One.flac\n#EXTINF:-1,Two\n/elsewhere/Two.flac\n");
    }

    #[test]
    fn xspf_keeps_every_track_escaped_with_its_file_when_it_has_one() {
        let rows = [row("/music/a b/Ünö.flac", "<One>", "Band", 61_400), row("remote:plugin/source/7", "Streamed", "Band & Co", 0)];
        let text = xspf("Mix 🎸", "for \"now\"", &rows);
        assert!(text.contains("<title>Mix 🎸</title>"), "{text}");
        assert!(text.contains("<annotation>for &quot;now&quot;</annotation>"), "{text}");
        assert!(text.contains("<location>file:///music/a%20b/%C3%9Cn%C3%B6.flac</location>"), "{text}");
        assert!(text.contains("<title>&lt;One&gt;</title>"), "{text}");
        assert!(text.contains("<creator>Band &amp; Co</creator>"), "{text}");
        assert_eq!(text.matches("<track>").count(), 2);
        assert_eq!(text.matches("<location>").count(), 1, "the streamed track has no file");
        assert_eq!(text.matches("<duration>").count(), 1, "an unknown length is not written");
    }

    #[test]
    fn a_name_with_emoji_or_symbols_is_unusual_and_any_script_s_words_are_not() {
        for name in ["Road trip", "Zoë’s «best» – vol. 2", "日本の「歌」、夏", "Nr. 5 (live) & more!", "Ñandú’s mix…", "हिन्दी गाने", "Песни №1", "Ελληνικά"] {
            assert!(!name_is_unusual(name), "{name}");
        }
        for name in ["Road trip 🚗", "Love ♥", "★ Favourites", "Chill ✌️", "Up ↑"] {
            assert!(name_is_unusual(name), "{name}");
        }
    }

    #[test]
    fn the_format_follows_the_file_s_extension() {
        assert_eq!(format_of(Path::new("/tmp/list.XSPF")), Format::Xspf);
        assert_eq!(format_of(Path::new("/tmp/list.json")), Format::Json);
        assert_eq!(format_of(Path::new("/tmp/list.m3u8")), Format::M3u8);
        assert_eq!(format_of(Path::new("/tmp/list")), Format::M3u8);
    }

    #[test]
    fn json_is_the_importer_s_playlist_with_every_track_and_its_isrc() {
        let mut one = row("/music/a/One.flac", "One", "Band", 61_400);
        one.id = 12;
        let rows = [one, row("remote:plugin/source/7", "Streamed \"live\"", "Band", 0)];
        let isrcs = HashMap::from([(12, "USRC17607839".to_string())]);
        let text = json("Mix 🎸", "", &rows, &isrcs);
        let document: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(document["name"], "Mix 🎸");
        assert!(document.get("description").is_none(), "an empty description is left out");
        let tracks = document["tracks"].as_array().unwrap();
        assert_eq!(tracks.len(), 2, "a track that is no file here is kept by name");
        assert_eq!(tracks[0], serde_json::json!({"title": "One", "artist": "Band", "album": "Album & Co", "duration_ms": 61_400, "isrc": "USRC17607839"}));
        assert_eq!(tracks[1], serde_json::json!({"title": "Streamed \"live\"", "artist": "Band", "album": "Album & Co"}));
    }

    #[test]
    fn a_list_is_written_whole_where_it_was_asked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mix.m3u8");
        write_whole(&path, "#EXTM3U\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "#EXTM3U\n");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "no temporary file is left");
    }
}
