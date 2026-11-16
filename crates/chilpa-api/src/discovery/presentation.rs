//! Project labels only after matching and counting full original spellings.
use super::*;
use sha2::{Digest, Sha256};

pub(super) struct Label {
    pub text: String,
    pub chars: usize,
    pub truncated: bool,
}

impl Label {
    pub fn new(name: &str) -> Self {
        let chars = name.chars().count();
        let truncated = chars > 128;
        // Keep distinguishing endings. The disk index uses the same 95/32 split.
        let text = if truncated {
            let prefix: String = name.chars().take(95).collect();
            let suffix: String = name.chars().rev().take(32).collect::<Vec<_>>().into_iter().rev().collect();
            format!("{prefix}…{suffix}")
        } else { name.to_string() };
        Self { text, chars, truncated }
    }
}

pub(super) fn token(name: &str) -> String {
    format!("{:x}", Sha256::digest(genres::strong_identity(name).as_bytes()))
}

pub(super) fn resolution(record: &genres::Resolution) -> Value {
    let original = Label::new(&record.original);
    json!({"original":original.text,"originalChars":original.chars,"originalTruncated":original.truncated,
        "recognized":record.recognized,"method":record.method,"source":record.source,"alternatives":record.alternatives})
}

impl Reader {
    pub(super) fn genre_entry_key(&self, name: &str, label: &Label, excluded: &[String], index: &crate::library_sources::SourceIndex) -> Result<String,String> {
        let mut route = json!({"kind":"genre","name":label.text});
        if label.truncated {
            route["genreToken"] = json!(token(name));
            if let Some(id) = self.lib.genre_id_for_name(name).map_err(|error|error.to_string())? {
                route["genreId"] = json!(id);
            } else {
                let through = sources::through(excluded);
                let stars = crate::library_sources::Stars::default();
                let item = index.items.iter().find(|item|through.passes(item,&stars) && item.genres.iter().any(|tag|tag==name))
                    .ok_or_else(||chilpa_i18n::t("This genre is no longer available."))?;
                if item.album != 0 { route["album"] = json!(item.album); }
                else if item.track < 0 { route["genreTrack"] = json!(item.track); }
                else { return Err(chilpa_i18n::t("This genre is no longer available.")); }
            }
        }
        Ok(route.to_string())
    }
}
