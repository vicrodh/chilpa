//! A genre artwork anchor reads only its card or track and never queues work.

use super::{genres, Catalog};

impl Catalog {
    pub(crate) fn genre_anchor_matches(&self, album: i64, track: i64, matches: impl Fn(&str) -> bool) -> bool {
        let (card, tracks, enrichment) = {
            let inner = self.lock();
            let song = (track < 0).then(|| inner.store.tracks.shared(&track)).flatten();
            if track < 0 && song.is_none() { return false; }
            let card = inner.store.cards.shared(&song.as_ref().map_or(album, |song| song.album_id));
            let tracks = match song {
                Some(song) => vec![song],
                None => card.as_ref().into_iter().flat_map(|card| card.tracks.iter().flatten())
                    .filter_map(|id| inner.store.tracks.shared(id)).collect(),
            };
            (card, tracks, inner.genres.clone())
        };
        let now = super::now();
        let inherited = card.as_ref().map(|card| enrichment.of_card(card, now)).unwrap_or_default();
        if track == 0 && inherited.iter().any(|genre| matches(genre)) { return true; }
        tracks.iter().any(|song| {
            let own = genres::own_genres(&song.metadata, "");
            let genres = if own.is_empty() { &inherited } else { &own };
            genres.iter().any(|genre| matches(genre))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn local_artwork_identity_does_not_wait_for_catalog_access() {
        let catalog = Catalog::new();
        let held = catalog.lock();
        let source = catalog.clone();
        let (send, receive) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let library = chilpa_library::Library::in_memory().unwrap();
            library.connection().execute("INSERT INTO genres(id,name) VALUES (1,'Heavy Metal')", []).unwrap();
            let mut reader = crate::Reader::of(library);
            reader.catalog = Some(source);
            let plain = json!({"kind":"genre","name":"heavy-metal"}).to_string();
            let token = reader.genre_art_identity(&plain).unwrap();
            let anchored = json!({"kind":"genre","name":"short","genreId":1,"genreToken":token}).to_string();
            send.send(reader.genre_art_identity(&anchored)).unwrap();
        });
        let result = receive.recv_timeout(std::time::Duration::from_secs(5));
        drop(held);
        thread.join().unwrap();
        assert!(result.unwrap().is_ok());
    }

    #[test]
    fn remote_artwork_reads_only_the_selected_anchor_and_rejects_missing_tags() {
        let catalog = Catalog::new();
        let (album, track) = {
            let mut inner = catalog.lock();
            let album = inner.store.take_item("fixture", &json!({"kind":"album","ref":"album:1","title":"Album",
                "genres":["Heavy Metal"]}), 1).unwrap();
            let track = inner.store.take_item_track("fixture", &json!({"kind":"track","ref":"track:1","title":"Track",
                "genres":["Personal-Shelf"]}), None, 1).unwrap();
            inner.store.tracks.get_mut(&track).unwrap().album_id = album;
            inner.store.cards.get_mut(&album).unwrap().tracks = Some(vec![track]);
            (album, track)
        };
        assert!(catalog.genre_anchor_matches(album, 0, |name| name == "Heavy Metal"));
        assert!(catalog.genre_anchor_matches(album, 0, |name| name == "Personal-Shelf"));
        assert!(catalog.genre_anchor_matches(0, track, |name| name == "Personal-Shelf"));
        assert!(!catalog.genre_anchor_matches(0, track, |name| name == "Heavy Metal"));
        assert!(!catalog.genre_anchor_matches(-999, 0, |_| true));
        assert!(!catalog.genre_anchor_matches(0, -999, |_| true));
        let mut reader = crate::Reader::of(chilpa_library::Library::in_memory().unwrap());
        reader.catalog = Some(catalog.clone());
        let token = reader.genre_art_identity(&json!({"kind":"genre","name":"personal shelf"}).to_string()).unwrap();
        let mut key = json!({"kind":"genre","name":"short","genreTrack":track,"genreToken":token});
        assert_eq!(reader.genre_art_identity(&key.to_string()).unwrap(), token);
        key["genreToken"] = json!("0".repeat(64));
        assert!(reader.genre_art_identity(&key.to_string()).is_err());
        let inner = catalog.lock();
        assert!(inner.queue.queued.is_empty());
        drop(inner);
        assert!(!*catalog.started.lock().unwrap());
    }
}
