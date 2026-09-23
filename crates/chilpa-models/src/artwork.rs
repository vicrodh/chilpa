//! Referencias a portadas.

use std::path::PathBuf;

/// Dónde está una portada. La UI hace match y la trae como corresponda.
///
/// En QBZ el pipeline de portadas solo sabía de URLs remotas, así que las
/// carátulas locales y las de servidores no llegaban a la UI. Aquí cada
/// origen es una variante explícita.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum ArtworkRef {
    /// No hay portada conocida.
    #[default]
    None,
    /// Un archivo en disco: la carátula junto al álbum, o una ya cacheada.
    File(PathBuf),
    /// Metida en los tags del archivo, en el índice que indica.
    Embedded { path: PathBuf, index: usize },
    /// Una URL que resuelve el source. El plugin ya la dejó lista para pedir.
    Remote(String),
}

impl ArtworkRef {
    pub fn is_none(&self) -> bool {
        matches!(self, ArtworkRef::None)
    }

    /// Una URL que otro proceso pueda abrir, para MPRIS o para el sistema.
    /// Lo embebido no la tiene hasta que se extrae a la caché.
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
        let art = ArtworkRef::File(PathBuf::from("/musica/album/cover.jpg"));
        assert_eq!(art.external_url().as_deref(), Some("file:///musica/album/cover.jpg"));
    }

    #[test]
    fn embedded_artwork_has_no_url_until_it_is_extracted() {
        let art = ArtworkRef::Embedded { path: PathBuf::from("/musica/a.flac"), index: 0 };
        assert_eq!(art.external_url(), None);
    }
}
