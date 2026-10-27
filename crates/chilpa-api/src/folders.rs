//! What "Open folder" plays: the audio files under a folder, in order.

/// The extensions Chilpa knows how to open today.
const EXTENSIONS: &[&str] =
    &["flac", "wav", "aiff", "aif", "m4a", "mp4", "alac", "aac", "mp3", "ogg", "oga", "dsf", "dff", "iso"];

/// A folder's audio files, its subfolders' included, in order.
///
/// It used to stop at the folder, as if going deeper were only the
/// library's job, and "Open folder" on a box set -- a disc a subfolder --
/// said there was nothing to play. Numbers in names are compared as
/// numbers, so disc 10 comes after disc 9 and not after disc 1. A folder is
/// visited once, however many links lead back to it.
pub fn audio_files_in(folder: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        if !visited.insert(std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone())) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for path in entries.flatten().map(|e| e.path()) {
            if path.is_dir() {
                pending.push(path);
            } else if is_audio(&path) {
                files.push(path);
            }
        }
    }
    files.sort_by(|a, b| in_natural_order(a, b));
    files
}

/// A file Chilpa knows how to open, by its extension.
pub(crate) fn is_audio(path: &std::path::Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
}

/// Two paths compared name by name, with a run of digits compared as the
/// number it is.
fn in_natural_order(a: &std::path::Path, b: &std::path::Path) -> std::cmp::Ordering {
    fn pieces(name: &str) -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in name.chars() {
            let digit = c.is_ascii_digit();
            match out.last_mut() {
                Some((was_digit, piece)) if *was_digit == digit => piece.push(c),
                _ => out.push((digit, c.to_string())),
            }
        }
        out
    }
    let compare = |x: &str, y: &str| {
        for ((digits_x, piece_x), (digits_y, piece_y)) in pieces(x).iter().zip(pieces(y).iter()) {
            let order = match (digits_x, digits_y) {
                (true, true) => {
                    // Leading zeros say nothing about size; they only
                    // break a tie, so "07" and "7" keep an order.
                    let (number_x, number_y) = (piece_x.trim_start_matches('0'), piece_y.trim_start_matches('0'));
                    number_x
                        .len()
                        .cmp(&number_y.len())
                        .then_with(|| number_x.cmp(number_y))
                        .then_with(|| piece_x.len().cmp(&piece_y.len()))
                }
                _ => piece_x.cmp(piece_y),
            };
            if order != std::cmp::Ordering::Equal {
                return order;
            }
        }
        pieces(x).len().cmp(&pieces(y).len())
    };
    let names = |p: &std::path::Path| -> Vec<String> {
        p.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect()
    };
    let (names_a, names_b) = (names(a), names(b));
    for (x, y) in names_a.iter().zip(names_b.iter()) {
        let order = compare(x, y);
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    names_a.len().cmp(&names_b.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_yields_audio_files_only_and_in_order() {
        let mut dir = std::env::temp_dir();
        dir.push("chilpa-api-folder");
        let _ = std::fs::create_dir_all(&dir);
        for name in ["b.flac", "a.wav", "notes.txt", "c.MP3", "d.AAC"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let found: Vec<String> = audio_files_in(&dir)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(found, vec!["a.wav", "b.flac", "c.MP3", "d.AAC"], "without the .txt and in order");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_box_set_folder_yields_every_disc_in_order() {
        // "Open folder" on a box set -- a disc a subfolder -- found nothing to
        // play. Disc 10 goes after disc 9, not after disc 1.
        let dir = std::env::temp_dir().join("chilpa-api-box-set");
        let _ = std::fs::remove_dir_all(&dir);
        for disc in ["Disc 10", "Disc 2", "Disc 1"] {
            std::fs::create_dir_all(dir.join(disc)).unwrap();
            for track in ["02.flac", "01.flac"] {
                std::fs::write(dir.join(disc).join(track), b"x").unwrap();
            }
        }
        std::fs::write(dir.join("cover.jpg"), b"x").unwrap();
        // A link back to the box does not send the listing round in circles.
        #[cfg(unix)]
        std::os::unix::fs::symlink(&dir, dir.join("Disc 2").join("back")).unwrap();
        let found: Vec<String> = audio_files_in(&dir)
            .iter()
            .map(|p| p.strip_prefix(&dir).unwrap().display().to_string())
            .collect();
        assert_eq!(
            found,
            [
                "Disc 1/01.flac",
                "Disc 1/02.flac",
                "Disc 2/01.flac",
                "Disc 2/02.flac",
                "Disc 10/01.flac",
                "Disc 10/02.flac"
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// A folder outside the library, read from the disk as it is: its
/// subfolders, by name, and the audio files that sit in it. Nothing goes into
/// the library; this is for opening a folder, not for adding one.
pub(crate) fn disk_folder(dir: &std::path::Path) -> crate::FolderView {
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            // Hidden folders are the system's, not the music's.
            if !name.starts_with('.') {
                folders.push(crate::FolderItem {
                    name,
                    path: path.display().to_string(),
                    ..crate::FolderItem::default()
                });
            }
        } else if is_audio(&path) {
            files.push(path);
        }
    }
    folders.sort_by(|a, b| in_natural_order(std::path::Path::new(&a.name), std::path::Path::new(&b.name)));
    files.sort_by(|a, b| in_natural_order(a, b));
    let tracks: Vec<crate::TrackRow> = files.iter().flat_map(|path| {
        if chilpa_core::sacd_tracks::is_image(path) {
            image_rows(path)
        } else {
            vec![crate::browse::file_row(&path.display().to_string())]
        }
    }).collect();
    let total = tracks.len();
    crate::FolderView { folders, tracks, in_library: false, total }
}

/// Read an image once for the whole list, including metadata and playable
/// addresses. A virtual track is available when its image can be read.
pub(crate) fn image_rows(path: &std::path::Path) -> Vec<crate::TrackRow> {
    chilpa_core::sacd_tracks::disc_images().tracks(path).unwrap_or_default().into_iter().map(|track| {
        let info = track.info;
        crate::TrackRow {
            id: 0, path: track.path, title: info.tags.title.clone().unwrap_or_default(),
            artist: info.tags.artist_line(), album: info.tags.album.clone().unwrap_or_default(),
            album_id: 0, disc: info.tags.disc_number.unwrap_or(0),
            number: info.tags.track_number.unwrap_or(0), duration_ms: info.duration.as_millis() as u64,
            plays: 0, artwork_path: String::new(), available: true, composer: String::new(),
            disc_subtitle: info.tags.disc_subtitle.unwrap_or_default(), work: info.tags.work.unwrap_or_default(),
            composer_target: String::new(),
            groups: None,
            explicit: info.tags.explicit,
            quality: crate::Quality::of_file(&path.to_string_lossy(), info.bit_depth, info.sample_rate_hz),
        }
    }).collect()
}
