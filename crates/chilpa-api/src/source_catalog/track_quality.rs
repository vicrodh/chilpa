//! Display album quality as a fallback without promoting it to a track fact.
use super::store::{CardKind, Store, Track};

impl Store {
    fn track_format<'a>(&'a self, track: &'a Track) -> Option<&'a super::store::Format> {
        track.format.as_ref().or_else(|| self.cards.get(&track.album_id)
            .filter(|card| card.kind == CardKind::Album && card.plugin == track.plugin)
            .and_then(|card| card.format.as_ref()))
    }

    pub(super) fn with_album_quality(&self, mut track: Track) -> Track {
        if track.format.is_none() {
            track.format = self.track_format(&track).cloned();
        }
        track
    }
}

impl super::Catalog {
    /// A badge only borrows stored quality; it never schedules metadata or artwork.
    pub(crate) fn card_quality(&self, id: i64) -> crate::Quality {
        let inner = self.lock();
        let Some(card) = inner.store.cards.get(&id) else { return crate::Quality::default() };
        let known = card.tracks.as_deref().unwrap_or_default().iter()
            .filter_map(|id| inner.store.tracks.get(id))
            .filter_map(|track| inner.store.track_format(track))
            .filter(|format| format.rate.is_some()).map(|format| format.quality())
            .max_by_key(|quality| (quality.tier.rank(), quality.bits, quality.rate_hz));
        known.or_else(|| card.format.as_ref().map(|format| format.quality())).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_catalog::{store::{Card, Format}, Catalog};
    use serde_json::json;

    fn fixture() -> (std::sync::Arc<Catalog>, i64, i64, i64) {
        let catalog = Catalog::new();
        let (album, unknown, different) = {
            let mut inner = catalog.lock();
            inner.loaded = true;
            let album = inner.store.take_item("fixture", &json!({"kind":"album","ref":"album:1","title":"Record",
                "quality":{"codec":"flac","bits":24,"rate":96000}}), 1).unwrap();
            let under = inner.store.cards.get(&album).cloned().unwrap();
            let unknown = inner.store.take_item_track("fixture", &json!({"kind":"track","ref":"track:1","title":"Inherited"}), Some(&under), 1).unwrap();
            let different = inner.store.take_item_track("fixture", &json!({"kind":"track","ref":"track:2","title":"Own",
                "quality":{"codec":"flac","bits":16,"rate":44100}}), Some(&under), 1).unwrap();
            inner.store.cards.get_mut(&album).unwrap().tracks = Some(vec![unknown, different]);
            (album, unknown, different)
        };
        (catalog, album, unknown, different)
    }

    #[test]
    fn album_quality_reaches_rows_without_changing_stored_track_facts() {
        let (catalog, album, unknown, different) = fixture();
        let rows = catalog.tracks(album).unwrap();
        assert_eq!(rows[0].format.as_ref().unwrap().quality().short(), "FLAC 24/96");
        assert_eq!(rows[1].format.as_ref().unwrap().quality().short(), "FLAC 16/44.1");
        let rows = catalog.tracks_by_ids(&[unknown, different]);
        assert_eq!(super::super::view::row_of(&rows[0].0, String::new()).quality.short(), "FLAC 24/96");
        assert_eq!(super::super::view::row_of(&rows[1].0, String::new()).quality.short(), "FLAC 16/44.1");
        let path = catalog.lock().store.tracks.get(&unknown).unwrap().path.clone();
        assert_eq!(catalog.track_by_path(&path).unwrap().0.format.unwrap().quality().short(), "FLAC 24/96");
        let inner = catalog.lock();
        assert!(inner.store.tracks.get(&unknown).unwrap().format.is_none());
        assert!(inner.queue.queued.is_empty(), "Showing a quality fallback must not schedule work");
    }

    #[test]
    fn card_badges_borrow_fallbacks_without_scheduling_or_mutating_tracks() {
        let (catalog, album, unknown, different) = fixture();
        assert_eq!(catalog.card_quality(album).short(), "FLAC 24/96");
        {
            let mut inner = catalog.lock();
            inner.store.tracks.get_mut(&unknown).unwrap().format = Some(Format {
                codec: "dsd".into(), rate: None, ..Default::default()
            });
        }
        assert_eq!(catalog.card_quality(album).short(), "FLAC 16/44.1", "a track without a rate does not displace known quality");
        {
            let mut inner = catalog.lock();
            inner.store.tracks.get_mut(&different).unwrap().format = Some(Format {
                codec: "dsd".into(), rate: Some(2822400), ..Default::default()
            });
        }
        assert_eq!(catalog.card_quality(album).short(), "DSD64");
        {
            let mut inner = catalog.lock();
            inner.store.cards.get_mut(&album).unwrap().tracks = Some(vec![unknown, -999]);
            inner.store.cards.get_mut(&album).unwrap().format = Some(Format {
                codec: "flac".into(), advertised_hires: true, ..Default::default()
            });
        }
        assert_eq!(catalog.card_quality(album).short(), "FLAC Hi-Res", "card hints survive when no member has a known rate");
        assert_eq!(catalog.card_quality(-999), crate::Quality::default());
        let inner = catalog.lock();
        assert_eq!(inner.store.tracks.get(&unknown).unwrap().format.as_ref().unwrap().rate, None);
        assert!(inner.queue.queued.is_empty(), "a badge never schedules artwork or metadata");
    }

    #[test]
    fn measured_track_quality_wins_over_album_fallback_after_refresh() {
        let (catalog, album, unknown, _) = fixture();
        let path = catalog.lock().store.tracks.get(&unknown).unwrap().path.clone();
        catalog.measured(&path, Some("mp3"), None, 44100);
        {
            let mut inner = catalog.lock();
            let under = inner.store.cards.get(&album).cloned().unwrap();
            inner.store.take_item_track("fixture", &json!({"kind":"track","ref":"track:1","title":"Inherited"}), Some(&under), 2).unwrap();
        }
        assert_eq!(catalog.tracks_by_ids(&[unknown])[0].0.format.as_ref().unwrap().codec, "mp3");
        let mut inner = catalog.lock();
        inner.store.cards.get_mut(&album).unwrap().format = Some(Format {codec:"flac".into(), bits:Some(24), rate:Some(192000), ..Default::default()});
        drop(inner);
        assert_eq!(catalog.track_by_path(&path).unwrap().0.format.unwrap().codec, "mp3");
    }

    #[test]
    fn playlist_quality_is_not_an_album_fallback_and_cross_source_cards_do_not_match() {
        let mut store = Store::default();
        store.cards.insert(-1, Card {id:-1,plugin:"fixture".into(),kind:CardKind::Playlist,
            format:Some(Format{codec:"flac".into(),bits:Some(24),rate:Some(192000),..Default::default()}),..Default::default()});
        let track = Track {plugin:"fixture".into(),album_id:-1,..Default::default()};
        assert!(store.with_album_quality(track.clone()).format.is_none());
        let card = store.cards.get_mut(&-1).unwrap();card.kind=CardKind::Album;card.plugin="other".into();
        assert!(store.with_album_quality(track).format.is_none());
    }

    #[test]
    fn a_playlist_revision_changes_when_its_tracks_inherit_new_album_quality() {
        let (catalog, album, unknown, _) = fixture();
        let playlist = {
            let mut inner=catalog.lock();
            let id=inner.store.take_item("fixture",&json!({"kind":"playlist","ref":"playlist:3","title":"List"}),1).unwrap();
            inner.store.cards.get_mut(&id).unwrap().tracks=Some(vec![unknown]);id
        };
        let before=catalog.entity_revision(playlist);
        assert_eq!(catalog.entity_revision(playlist),before);
        catalog.lock().store.cards.get_mut(&album).unwrap().format=Some(Format{codec:"flac".into(),bits:Some(16),rate:Some(44100),..Default::default()});
        let after=catalog.entity_revision(playlist);
        assert_ne!(after.0,before.0);
        assert_eq!(after.1,before.1);
        assert_eq!(catalog.entity_revision(playlist),after);
        assert!(catalog.lock().store.tracks.get(&unknown).unwrap().format.is_none());
    }
}
