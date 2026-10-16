//! References to cover art.

use std::path::PathBuf;

/// Where a cover lives. The UI matches on it and fetches it accordingly.
///
/// In QBZ the artwork pipeline only knew about remote URLs, so local covers
/// and server ones never reached the UI. Here every origin is an explicit
/// variant.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum ArtworkRef {
    /// No cover is known.
    #[default]
    None,
    /// A file on disk: the cover next to the album, or one already cached.
    File(PathBuf),
    /// Embedded in the file's tags, at the index it names.
    Embedded { path: PathBuf, index: usize },
    /// A URL the source resolves. The plugin already left it ready to fetch.
    Remote(String),
}

impl ArtworkRef {
    pub fn is_none(&self) -> bool {
        matches!(self, ArtworkRef::None)
    }

    /// A URL another process can open, for MPRIS or for the system. Embedded
    /// art has none until it is extracted to the cache.
    pub fn external_url(&self) -> Option<String> {
        match self {
            ArtworkRef::File(path) => Some(format!("file://{}", path.display())),
            ArtworkRef::Remote(url) => Some(url.clone()),
            ArtworkRef::None | ArtworkRef::Embedded { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_artwork_by_default() {
        assert!(ArtworkRef::default().is_none());
    }

    #[test]
    fn a_local_file_does_have_an_external_url() {
        let art = ArtworkRef::File(PathBuf::from("/music/album/cover.jpg"));
        assert_eq!(art.external_url().as_deref(), Some("file:///music/album/cover.jpg"));
    }

    #[test]
    fn embedded_artwork_has_no_url_until_it_is_extracted() {
        let art = ArtworkRef::Embedded { path: PathBuf::from("/music/a.flac"), index: 0 };
        assert_eq!(art.external_url(), None);
    }
}
