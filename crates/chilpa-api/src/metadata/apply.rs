//! Laying a release over the form.
//!
//! What a release does not say is left alone: a field is filled with what
//! was found, never emptied for what was not.

use super::draft::Draft;
use super::fields;
use super::worker::Job;
use super::State;
use crate::Chilpa;
use chilpa_library::sidecar::join;
use chilpa_meta::{display, Credit, Failure, Provider, Release};

/// What a failure is called, in the user's language.
pub(crate) fn said(provider: Provider, failure: &Failure) -> String {
    let name = match provider {
        Provider::MusicBrainz => "MusicBrainz",
        Provider::Discogs => "Discogs",
    };
    match failure {
        Failure::Busy(seconds) => chilpa_i18n::t_args("{} is busy; try again in {} seconds", &[name, &seconds.to_string()]),
        Failure::Unreachable(_) => chilpa_i18n::t_args("there is no connection to {}", &[name]),
        Failure::Refused(why) => chilpa_i18n::t_args("{} is not available yet: {}", &[name, why]),
        Failure::Unexpected(what) => chilpa_i18n::t_args("{} answered something Chilpa did not expect: {}", &[name, what]),
        Failure::BadId => chilpa_i18n::t_args("that is not something {} can be asked for", &[name]),
    }
}

fn number(text: &str, otherwise: u16) -> u16 {
    text.parse().unwrap_or(otherwise)
}

/// Which track of the release each track of the record is: by disc and
/// number, and in order when none is found that way and both have as many.
pub(crate) fn pair(draft: &Draft, release: &Release) -> Vec<Option<usize>> {
    let (at_number, at_disc) = (fields::track("track_number").unwrap_or(0), fields::track("disc_number").unwrap_or(0));
    let mut taken = vec![false; release.tracks.len()];
    let mut pairs: Vec<Option<usize>> = Vec::with_capacity(draft.rows.len());
    for (i, row) in draft.rows.iter().enumerate() {
        let disc = number(&row.cells[at_disc].now, 1);
        let place = number(&row.cells[at_number].now, u16::try_from(i + 1).unwrap_or(u16::MAX));
        let found = release.tracks.iter().enumerate().position(|(at, t)| !taken[at] && t.disc == disc && t.number == place);
        if let Some(at) = found {
            taken[at] = true;
        }
        pairs.push(found);
    }
    if pairs.iter().all(Option::is_none) && release.tracks.len() == draft.rows.len() {
        return (0..draft.rows.len()).map(Some).collect();
    }
    pairs
}

fn names(credits: &[Credit]) -> String {
    join(&credits.iter().map(|c| c.name.clone()).collect::<Vec<_>>())
}

fn ids(credits: &[Credit]) -> String {
    join(&credits.iter().map(|c| c.id.clone()).filter(|id| !id.is_empty()).collect::<Vec<_>>())
}

/// Lays the release over the form. `spelled` gives a genre the spelling
/// the library already has for it. Returns how many tracks found their
/// pair.
pub(crate) fn apply(draft: &mut Draft, release: &Release, spelled: &dyn Fn(&str) -> String) -> usize {
    let from_musicbrainz = release.provider == Provider::MusicBrainz;
    let genres: Vec<String> = release.genres.iter().map(|g| spelled(g)).collect();
    let mut album: Vec<(&str, String)> = vec![
        ("title", release.title.clone()),
        ("album_artist", display(&release.credits)),
        ("album_artists", names(&release.credits)),
        ("genres", join(&genres)),
        ("catalog_number", release.catalog_number.clone()),
    ];
    if fields::is_date(&release.date) {
        album.push(("date", release.date.clone()));
    }
    if from_musicbrainz {
        album.push(("musicbrainz_release_id", release.id.clone()));
        album.push(("musicbrainz_release_group_id", release.release_group_id.clone()));
        album.push(("musicbrainz_album_artist_ids", ids(&release.credits)));
    } else {
        album.push(("discogs_release_id", release.id.clone()));
    }
    for (name, value) in album {
        // What the release does not say is left as it is.
        if !value.is_empty() {
            draft.set_album(name, &value);
        }
    }

    let pairs = pair(draft, release);
    for (row, found) in pairs.iter().enumerate() {
        let Some(track) = found.and_then(|at| release.tracks.get(at)) else { continue };
        let id = draft.rows[row].track.id;
        let mut filled: Vec<(&str, String)> = vec![("title", track.title.clone())];
        if track.disc > 0 {
            filled.push(("disc_number", track.disc.to_string()));
        }
        if track.number > 0 {
            // Refused for a track inside a disc, which keeps its own.
            filled.push(("track_number", track.number.to_string()));
        }
        // A track nobody is credited on keeps the artist it had.
        if !track.credits.is_empty() {
            filled.push(("artist_credit", display(&track.credits)));
            filled.push(("artists", names(&track.credits)));
            if from_musicbrainz {
                filled.push(("musicbrainz_artist_ids", ids(&track.credits)));
            }
        }
        if from_musicbrainz {
            filled.push(("musicbrainz_recording_id", track.recording_id.clone()));
            filled.push(("musicbrainz_track_id", track.track_id.clone()));
        }
        for (name, value) in filled {
            if !value.is_empty() {
                draft.set_track(id, name, &value);
            }
        }
    }
    pairs.iter().flatten().count()
}

impl Chilpa {
    /// "musicbrainz" | "discogs"
    pub fn metadata_set_provider(&mut self, provider: &str) -> bool {
        let Some(provider) = Provider::from_name(provider) else { return false };
        let editor = &mut self.metadata;
        editor.provider = provider;
        editor.lookup_turn += 1;
        editor.searching = false;
        editor.loading = false;
        editor.hits.clear();
        editor.release = None;
        editor.refresh();
        true
    }

    /// Searches with what the form holds: the record's title, its artist
    /// and its catalogue number.
    pub fn metadata_search(&mut self) -> bool {
        let off = self.metadata_lookups_off();
        let editor = &mut self.metadata;
        let Some(draft) = editor.draft.as_ref().filter(|_| editor.state == State::Ready) else { return false };
        if off {
            return editor.refuse(chilpa_i18n::t("looking records up is turned off in Settings"));
        }
        let (title, artist, catalog_number) = (
            draft.album_cell("title").now.clone(),
            draft.album_cell("album_artist").now.clone(),
            draft.album_cell("catalog_number").now.clone(),
        );
        if title.is_empty() && artist.is_empty() && catalog_number.is_empty() {
            return editor.refuse(chilpa_i18n::t("type a title, an artist or a catalog number to search"));
        }
        editor.lookup_turn += 1;
        editor.searching = true;
        editor.loading = false;
        editor.hits.clear();
        editor.release = None;
        editor.error.clear();
        editor.notice.clear();
        let job = Job::Search { session: editor.session, turn: editor.lookup_turn, provider: editor.provider, title, artist, catalog_number };
        editor.ask(job);
        editor.refresh();
        true
    }

    /// Loads one of the rows of the last search, whole.
    pub fn metadata_load(&mut self, hit: usize) -> bool {
        let off = self.metadata_lookups_off();
        let editor = &mut self.metadata;
        if editor.state != State::Ready {
            return false;
        }
        if off {
            return editor.refuse(chilpa_i18n::t("looking records up is turned off in Settings"));
        }
        let Some(found) = editor.hits.get(hit) else { return false };
        let (provider, id) = (found.provider, found.id.clone());
        editor.lookup_turn += 1;
        editor.loading = true;
        editor.release = None;
        editor.error.clear();
        editor.notice.clear();
        let job = Job::Load { session: editor.session, turn: editor.lookup_turn, provider, id };
        editor.ask(job);
        editor.refresh();
        true
    }

    /// Lays the release loaded over the form. Returns how many tracks
    /// found their pair.
    pub fn metadata_apply(&mut self) -> usize {
        if self.metadata.state != State::Ready {
            return 0;
        }
        let Some(release) = self.metadata.release.clone() else { return 0 };
        // The library is asked first and the form is filled after: both
        // live in `self`.
        let spellings: Vec<(String, String)> = match self.library() {
            Some(lib) => release
                .genres
                .iter()
                .map(|g| (g.clone(), lib.genre_spelling(g).ok().flatten().unwrap_or_else(|| g.clone())))
                .collect(),
            None => Vec::new(),
        };
        let spelled = |genre: &str| spellings.iter().find(|(from, _)| from == genre).map_or_else(|| genre.to_string(), |(_, to)| to.clone());
        let editor = &mut self.metadata;
        let Some(draft) = editor.draft.as_mut() else { return 0 };
        let matched = apply(draft, &release, &spelled);
        editor.refresh();
        matched
    }
}

// Every field named above has to be one. A name that is not would be
// dropped without a word.
#[cfg(test)]
mod tests {
    use super::super::draft::testing::*;
    use super::super::fields::{ALBUM, TRACK};
    use super::*;
    use chilpa_meta::ReleaseTrack;

    fn credit(name: &str, join: &str, id: &str) -> Credit {
        Credit { name: name.into(), join: join.into(), id: id.into() }
    }

    fn track(disc: u16, number: u16, title: &str) -> ReleaseTrack {
        ReleaseTrack { disc, number, title: title.into(), duration_ms: 0, credits: Vec::new(), recording_id: String::new(), track_id: String::new() }
    }

    fn release(provider: Provider, tracks: Vec<ReleaseTrack>) -> Release {
        Release {
            provider,
            id: if provider == Provider::Discogs { "1001".into() } else { "3d3207f9-c4c2-425c-a364-f359b4320805".into() },
            title: "Found".into(),
            credits: vec![credit("Alpha", " & ", "11"), credit("Beta", "", "12")],
            date: String::new(),
            genres: Vec::new(),
            label: String::new(),
            catalog_number: String::new(),
            country: String::new(),
            barcode: String::new(),
            discs: 1,
            release_group_id: String::new(),
            url: String::new(),
            tracks,
        }
    }

    fn as_is(genre: &str) -> String {
        genre.to_string()
    }

    #[test]
    fn what_the_release_does_not_say_is_left_alone() {
        let mut draft = plain();
        draft.set_album("catalog_number", "CAT-1");
        apply(&mut draft, &release(Provider::MusicBrainz, Vec::new()), &as_is);
        assert_eq!(draft.album_cell("title").now, "Found");
        assert_eq!(draft.album_cell("album_artist").now, "Alpha & Beta");
        assert_eq!(draft.album_cell("album_artists").now, "Alpha; Beta");
        assert_eq!(draft.album_cell("date").now, "1990-05-01", "the release had no date");
        assert_eq!(draft.album_cell("genres").now, "Rock", "nor genres");
        assert_eq!(draft.album_cell("catalog_number").now, "CAT-1", "nor a number");
    }

    #[test]
    fn tracks_pair_by_disc_and_number() {
        let draft = plain();
        let found = release(Provider::MusicBrainz, vec![track(1, 2, "Second"), track(2, 1, "Elsewhere"), track(1, 1, "First")]);
        assert_eq!(pair(&draft, &found), [Some(2), Some(0)]);
    }

    #[test]
    fn a_track_with_no_number_counts_from_where_it_stands() {
        let mut draft = plain();
        draft.set_track(2, "track_number", "");
        let found = release(Provider::MusicBrainz, vec![track(1, 1, "First"), track(1, 2, "Second"), track(1, 3, "Third")]);
        assert_eq!(pair(&draft, &found), [Some(0), Some(1)]);
    }

    #[test]
    fn nothing_pairing_and_the_same_count_pairs_in_order() {
        let draft = plain();
        let found = release(Provider::Discogs, vec![track(3, 7, "First"), track(3, 8, "Second")]);
        assert_eq!(pair(&draft, &found), [Some(0), Some(1)]);
        let more = release(Provider::Discogs, vec![track(3, 7, "a"), track(3, 8, "b"), track(3, 9, "c")]);
        assert_eq!(pair(&draft, &more), [None, None], "and another count pairs nothing");
    }

    #[test]
    fn a_release_track_is_given_to_one_track_only() {
        let mut draft = plain();
        draft.set_track(2, "track_number", "1");
        let found = release(Provider::MusicBrainz, vec![track(1, 1, "Only"), track(1, 9, "Far")]);
        assert_eq!(pair(&draft, &found), [Some(0), None]);
    }

    #[test]
    fn discogs_never_fills_a_musicbrainz_id() {
        let mut draft = plain();
        let mut first = track(1, 1, "First");
        first.credits = vec![credit("Gamma", "", "77")];
        assert_eq!(apply(&mut draft, &release(Provider::Discogs, vec![first, track(1, 2, "Second")]), &as_is), 2);
        assert_eq!(draft.album_cell("discogs_release_id").now, "1001");
        assert_eq!(draft.album_cell("musicbrainz_release_id").now, "");
        assert_eq!(draft.album_cell("musicbrainz_album_artist_ids").now, "");
        assert_eq!(draft.rows[0].cells[fields::track("musicbrainz_artist_ids").unwrap()].now, "");
        assert_eq!(draft.rows[0].cells[1].now, "Gamma");
        assert_eq!(draft.first_invalid(), None, "what was filled in can be saved");
    }

    #[test]
    fn a_track_nobody_is_credited_on_keeps_its_artist() {
        let mut draft = plain();
        apply(&mut draft, &release(Provider::Discogs, vec![track(1, 1, "First"), track(1, 2, "Second")]), &as_is);
        assert_eq!(draft.rows[0].cells[0].now, "First");
        assert_eq!(draft.rows[0].cells[1].now, "Someone", "the record's artists are not this track's");
        assert!(!draft.rows[0].cells[1].dirty());
    }

    #[test]
    fn musicbrainz_fills_its_ids() {
        let mut draft = plain();
        let mut first = track(1, 1, "First");
        first.credits = vec![credit("Gamma", "", "ca891d65-d9b0-4258-89f7-e6ba29d83767")];
        first.recording_id = "d81a4632-2845-49e1-b3d2-6132dccd5a46".into();
        first.track_id = "07133f3b-eeef-42c4-85c4-ee5a62638e3a".into();
        let mut found = release(Provider::MusicBrainz, vec![first]);
        found.credits = vec![credit("Alpha", "", "ca891d65-d9b0-4258-89f7-e6ba29d83767")];
        found.release_group_id = "60a20bc8-acac-3cbb-99f9-ce458317233a".into();
        found.date = "1984-09-03".into();
        assert_eq!(apply(&mut draft, &found, &as_is), 1);
        assert_eq!(draft.album_cell("musicbrainz_release_id").now, "3d3207f9-c4c2-425c-a364-f359b4320805");
        assert_eq!(draft.album_cell("musicbrainz_release_group_id").now, "60a20bc8-acac-3cbb-99f9-ce458317233a");
        assert_eq!(draft.album_cell("date").now, "1984-09-03");
        let cell = |name: &str| draft.rows[0].cells[fields::track(name).unwrap()].now.clone();
        assert_eq!(cell("musicbrainz_recording_id"), "d81a4632-2845-49e1-b3d2-6132dccd5a46");
        assert_eq!(cell("musicbrainz_track_id"), "07133f3b-eeef-42c4-85c4-ee5a62638e3a");
        assert_eq!(cell("musicbrainz_artist_ids"), "ca891d65-d9b0-4258-89f7-e6ba29d83767");
        assert_eq!(draft.rows[1].cells[0].now, "Track 2", "the track with no pair is as it was");
        assert_eq!(draft.first_invalid(), None);
    }

    #[test]
    fn a_date_that_is_not_one_is_not_applied() {
        let mut draft = plain();
        let mut found = release(Provider::Discogs, Vec::new());
        found.date = "sometime in 1991".into();
        apply(&mut draft, &found, &as_is);
        assert_eq!(draft.album_cell("date").now, "1990-05-01");
    }

    #[test]
    fn a_genre_takes_the_library_s_spelling() {
        let mut draft = plain();
        let mut found = release(Provider::MusicBrainz, Vec::new());
        found.genres = vec!["heavy metal".into(), "zydeco".into()];
        let spelled = |genre: &str| if genre == "heavy metal" { "Heavy Metal".to_string() } else { genre.to_string() };
        apply(&mut draft, &found, &spelled);
        assert_eq!(draft.album_cell("genres").now, "Heavy Metal; zydeco");
    }

    #[test]
    fn applying_what_is_already_there_dirties_nothing() {
        let mut draft = plain();
        let mut found = release(Provider::Discogs, vec![track(1, 1, "Track 1"), track(1, 2, "Track 2")]);
        found.title = "A record".into();
        found.credits = vec![credit("Someone", "", "")];
        found.id = String::new();
        apply(&mut draft, &found, &as_is);
        let changed: Vec<&str> = ALBUM.iter().zip(&draft.album).filter(|(_, c)| c.dirty()).map(|(f, _)| f.name).collect();
        assert_eq!(changed, ["album_artists"], "only the names, which the files did not have apart");
        assert!(draft.rows.iter().all(|row| TRACK.iter().zip(&row.cells).all(|(f, c)| !c.dirty() || f.name == "disc_number")));
    }
}
