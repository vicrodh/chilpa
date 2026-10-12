//! What every provider is turned into.
//!
//! What a provider does not say is an empty text or a zero, never a
//! missing field: this is what crosses to the window.

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    MusicBrainz,
    Discogs,
}

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Provider::MusicBrainz => "musicbrainz",
            Provider::Discogs => "discogs",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "musicbrainz" => Some(Provider::MusicBrainz),
            "discogs" => Some(Provider::Discogs),
            _ => None,
        }
    }
}

/// One name of a credit, with what joins it to the next.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize)]
pub struct Credit {
    pub name: String,
    /// " feat. ", " & ", or nothing after the last.
    pub join: String,
    /// The provider's own id for the artist.
    pub id: String,
}

/// A row of a search.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize)]
pub struct Hit {
    pub provider: Provider,
    pub id: String,
    pub title: String,
    pub artist: String,
    pub date: String,
    pub tracks: u16,
    pub country: String,
    pub label: String,
    pub catalog_number: String,
    pub format: String,
    /// 0 to 100; zero when the provider gives none.
    pub score: u8,
    /// MusicBrainz only.
    pub release_group_id: String,
}

#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize)]
pub struct ReleaseTrack {
    pub disc: u16,
    pub number: u16,
    pub title: String,
    pub duration_ms: u32,
    /// Empty when the provider credits nobody on this track in particular.
    pub credits: Vec<Credit>,
    pub recording_id: String,
    pub track_id: String,
}

#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize)]
pub struct Release {
    pub provider: Provider,
    pub id: String,
    pub title: String,
    pub credits: Vec<Credit>,
    /// As the provider gives it: a year, or a whole date.
    pub date: String,
    pub genres: Vec<String>,
    pub label: String,
    pub catalog_number: String,
    /// One catalogue number per medium when a numbered run supports that mapping.
    pub catalog_numbers: Vec<String>,
    pub country: String,
    pub barcode: String,
    pub discs: u16,
    /// The names printed on the media, in disc order when available.
    pub media_titles: Vec<String>,
    pub release_group_id: String,
    /// Its page, for somebody to open.
    pub url: String,
    pub tracks: Vec<ReleaseTrack>,
}

/// A credit on one line: names and joins, one after another.
pub fn display(credits: &[Credit]) -> String {
    let mut out = String::new();
    for credit in credits {
        out.push_str(&credit.name);
        out.push_str(&credit.join);
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credit_reads_as_it_is_written_on_the_sleeve() {
        let credits = [
            Credit { name: "Alpha".into(), join: " feat. ".into(), id: "a".into() },
            Credit { name: "Beta".into(), join: String::new(), id: "b".into() },
        ];
        assert_eq!(display(&credits), "Alpha feat. Beta");
        assert_eq!(display(&[]), "");
    }

    #[test]
    fn a_provider_goes_by_its_name() {
        assert_eq!(Provider::from_name("discogs"), Some(Provider::Discogs));
        assert_eq!(Provider::from_name("Discogs"), None);
        assert_eq!(Provider::MusicBrainz.name(), "musicbrainz");
        assert_eq!(serde_json::to_string(&Provider::MusicBrainz).unwrap(), r#""musicbrainz""#);
    }
}
