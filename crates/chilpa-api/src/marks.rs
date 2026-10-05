//! Stars and favorites, for whatever takes them: one snapshot for the
//! window, asked by the kind of thing and its id.
//!
//! The kinds are "track", "album", "playlist" and "artist". An artist has no
//! stars, and is known by its name.

use crate::Chilpa;
use chilpa_library::store::favorites::{FavoriteTarget, Favorites};
use chilpa_library::store::ratings::{RatingTarget, Ratings};

#[derive(Clone, Debug, Default)]
pub struct Marks {
    ratings: Ratings,
    favorites: Favorites,
}

fn rated(kind: &str, id: i64) -> Option<RatingTarget> {
    match kind {
        "track" => Some(RatingTarget::Track(id)),
        "album" => Some(RatingTarget::Album(id)),
        "playlist" => Some(RatingTarget::Playlist(id)),
        _ => None,
    }
}

fn marked<'a>(kind: &str, key: &'a str) -> Option<FavoriteTarget<'a>> {
    let id = || key.parse::<i64>().ok().filter(|id| *id > 0);
    match kind {
        "track" => id().map(FavoriteTarget::Track),
        "album" => id().map(FavoriteTarget::Album),
        "playlist" => id().map(FavoriteTarget::Playlist),
        "artist" if !key.trim().is_empty() => Some(FavoriteTarget::Artist(key)),
        _ => None,
    }
}

impl Marks {
    pub fn stars(&self, kind: &str, id: i64) -> i32 {
        rated(kind, id).map(|target| self.ratings.get(target).into()).unwrap_or(0)
    }

    pub fn favorite(&self, kind: &str, key: &str) -> bool {
        marked(kind, key).is_some_and(|target| self.favorites.has(target))
    }
}

impl Chilpa {
    pub fn marks(&mut self) -> Option<Marks> {
        let mut ratings = self.ratings()?;
        let external = self.library()?.external_ratings().ok()?;
        for (kind, id, source, identity) in self.catalog.rating_identities() {
            if let Some(stars) = external.get(&(source, kind.clone(), identity)) {
                match kind.as_str() {
                    "track" => { ratings.tracks.insert(id, *stars); }
                    "playlist" => { ratings.playlists.insert(id, *stars); ratings.albums.insert(id, *stars); }
                    _ => { ratings.albums.insert(id, *stars); }
                }
            }
        }
        let favorites = self.library()?.favorites().ok()?;
        Some(Marks { ratings, favorites })
    }

    /// False when the kind takes no stars or they could not be kept.
    pub fn rate_as(&mut self, kind: &str, id: i64, stars: i32) -> bool {
        if id < 0 {
            if !(0..=5).contains(&stars) { return false; }
            let Some((actual, _, source, identity)) = self.catalog.rating_identities().into_iter()
                .find(|(actual, found, _, _)| *found == id && (actual == kind || kind == "album" && actual == "playlist")) else { return false };
            let Some(lib) = self.library() else { return false };
            return match lib.rate_external(&source, &actual, &identity, stars as u8) {
                Ok(()) => true,
                Err(error) => { self.now.error = chilpa_i18n::t_args("the rating could not be saved: {}", &[&error.to_string()]); false }
            };
        }
        rated(kind, id).is_some_and(|target| self.rate(target, stars))
    }

    /// False when the kind is unknown or the mark could not be kept.
    pub fn set_favorite(&mut self, kind: &str, key: &str, on: bool) -> bool {
        let Some(target) = marked(kind, key) else { return false };
        let Some(lib) = self.library() else { return false };
        match lib.set_favorite(target, on) {
            Ok(()) => true,
            Err(error) => {
                self.now.error = chilpa_i18n::t_args("the favorite could not be saved: {}", &[&error.to_string()]);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api() -> Chilpa {
        let lib = chilpa_library::Library::in_memory().unwrap();
        let local = lib.source_id("local", "default").unwrap();
        lib.connection()
            .execute_batch(&format!(
                "INSERT INTO albums(id, title, album_artist, version_key) VALUES (1, 'Set', 'Band', '1:first');
                 INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at) VALUES (7, {local}, 'a.flac', 'One', 1, 0);
                 INSERT INTO playlists(id, source_id, name, created_at, updated_at) VALUES (3, {local}, 'Mine', 0, 0);"
            ))
            .unwrap();
        let mut api = Chilpa::new();
        api.library = Some(lib);
        api
    }

    fn remote(api: &Chilpa, source: &str, kind: &str, id: &str) -> i64 {
        api.catalog.adopt(source, None, &crate::source_search::RemoteFound {
            source: source.into(), name: "Catalog".into(), id: id.into(), title: "Item".into(),
            detail: String::new(), art: String::new(), duration_ms: 0,
            collection: kind != "track", entity: kind.into(), item: None,
        })
    }

    #[test]
    fn external_stars_survive_catalog_eviction_and_never_rate_another_source() {
        let mut api = api();
        let first = remote(&api, "test.catalog.one", "album", "7");
        let other = remote(&api, "test.catalog.two", "album", "7");
        assert!(first < 0 && other < 0 && first != other);
        assert!(api.rate_as("album", first, 4));
        assert_eq!(api.marks().unwrap().stars("album", other), 0);
        api.catalog.forget_plugin("test.catalog.one");
        let restored = remote(&api, "test.catalog.one", "album", "7");
        assert_eq!(api.marks().unwrap().stars("album", restored), 4);
        assert!(api.rate_as("album", restored, 0));
        assert_eq!(api.marks().unwrap().stars("album", restored), 0);
        let playlist = remote(&api, "test.catalog.one", "playlist", "8");
        assert!(api.rate_as("album", playlist, 3));
        assert_eq!(api.marks().unwrap().stars("playlist", playlist), 3);
        assert!(api.same_artist_albums(playlist, 8).is_empty());
        let track = remote(&api, "test.catalog.one", "track", "9");
        assert!(api.rate_as("track", track, 5));
        assert_eq!(api.marks().unwrap().stars("track", track), 5);
    }

    #[test]
    fn playlist_occurrences_share_the_song_rating_without_sharing_row_identity() {
        let mut api = api();
        let adopt = |reference: &str| api.catalog.adopt("test.catalog.one", None, &crate::source_search::RemoteFound {
            source: "test.catalog.one".into(), name: "Catalog".into(), id: reference.into(), title: "Song".into(),
            detail: String::new(), art: String::new(), duration_ms: 0, collection: false, entity: "track".into(),
            item: Some(serde_json::json!({"kind":"track", "ref":reference, "title":"Song"})),
        });
        let song = adopt("track:9");
        let row = adopt("track:9@playlist:3@entry:2");
        assert_ne!(song, row);
        assert!(api.rate_as("track", row, 4));
        let marks = api.marks().unwrap();
        assert_eq!(marks.stars("track", song), 4);
        assert_eq!(marks.stars("track", row), 4);
    }

    #[test]
    fn every_kind_is_marked_by_its_name() {
        let mut api = api();
        for (kind, key) in [("track", "7"), ("album", "1"), ("playlist", "3"), ("artist", "Band")] {
            assert!(!api.marks().unwrap().favorite(kind, key));
            assert!(api.set_favorite(kind, key, true), "{kind} is marked");
            assert!(api.marks().unwrap().favorite(kind, key));
            assert!(api.set_favorite(kind, key, false));
            assert!(!api.marks().unwrap().favorite(kind, key));
        }
    }

    #[test]
    fn a_playlist_takes_stars_and_an_artist_does_not() {
        let mut api = api();
        assert!(api.rate_as("playlist", 3, 4));
        assert_eq!(api.marks().unwrap().stars("playlist", 3), 4);
        assert!(!api.rate_as("artist", 1, 4));
        assert_eq!(api.marks().unwrap().stars("artist", 1), 0);
    }

    #[test]
    fn what_is_not_a_thing_is_not_marked() {
        let mut api = api();
        assert!(!api.set_favorite("track", "0", true));
        assert!(!api.set_favorite("track", "none", true));
        assert!(!api.set_favorite("artist", "  ", true));
        assert!(!api.set_favorite("folder", "1", true));
    }
}
