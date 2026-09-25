//! Every SACD image under a folder, opened for real.
//!
//! Ignored by default: it reads files somebody owns. The synthetic images in
//! `image.rs` say the parser follows the format as it was written down; this
//! says it reads the images people actually have, which were made by
//! whatever ripper was at hand.
//!
//! ```text
//! CHILPA_SMOKE_SACD=/path/to/music \
//!   cargo test -p chilpa-sacd --test real_images -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};

fn images_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("iso")) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
#[ignore = "reads real disc images; run it on purpose"]
fn every_sacd_image_under_a_folder_opens() {
    let Some(root) = std::env::var_os("CHILPA_SMOKE_SACD").map(PathBuf::from) else {
        eprintln!("smoke: set CHILPA_SMOKE_SACD to a folder to run this");
        return;
    };
    let mut failed = Vec::new();
    for path in images_under(&root) {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        match chilpa_sacd::open(&path) {
            Ok(image) => eprintln!(
                "smoke: {:?}, {} tracks | {name}",
                image.area.encoding,
                image.area.tracks.len()
            ),
            // Not every ISO is a SACD; a DVD beside the music is not a failure.
            Err(chilpa_sacd::SacdError::MalformedToc(why)) if why.contains("SACDMTOC") => {
                eprintln!("smoke: not a SACD | {name}")
            }
            Err(e) => {
                eprintln!("smoke: FAILED {e} | {name}");
                failed.push(name);
            }
        }
    }
    assert!(failed.is_empty(), "these SACD images could not be read: {failed:#?}");
}
