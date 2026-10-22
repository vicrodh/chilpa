//! Navigation provenance belongs to each queue occurrence, not to a file path.
use crate::{Chilpa, navigation::{Route, Screen}};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueOrigin {
    pub screen: String,
    pub id: i64,
    pub key: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub source: String,
}

impl QueueOrigin {
    pub fn route(&self) -> Option<Route> {
        Some(Route { screen: Screen::from_name(&self.screen)?, id: self.id, key: self.key.clone() })
    }
}

impl Chilpa {
    pub fn route_to_origin(&mut self, origin: &QueueOrigin) -> Option<Route> {
        if origin.screen == "source" { self.source_panels(); }
        let plan = self.plan_route_to_origin(origin)?;
        let route = plan.route.clone();
        self.prepare_planned_route(plan);
        Some(route)
    }

    /// I restore a typed origin without opening a source or reading its artwork.
    pub fn plan_route_to_origin(&self, origin: &QueueOrigin) -> Option<crate::navigation::PlannedRoute> {
        if origin.screen == "source" {
            if let Some((plugin, reference)) = origin.key.split_once(" native:") {
                if reference.contains('|') { return origin.route().map(|route| crate::navigation::PlannedRoute { route, media_parent: None }); }
                // I resolve saved record identities back to their record screen,
                // rather than treating album metadata as a page of shelves.
                let id = self.catalog.card_by_identity(plugin, reference)
                    .filter(|card| matches!(card.kind, crate::source_catalog::store::CardKind::Album | crate::source_catalog::store::CardKind::Playlist))
                    .map(|card| card.id).unwrap_or_else(|| self.native_reference_card(plugin, reference));
                if id < 0 { return self.plan_route("album", id); }
            }
        }
        origin.route().map(|route| crate::navigation::PlannedRoute { route, media_parent: None })
    }

    pub fn set_browse_route(&mut self, route: &Route) {
        self.browse_origin = self.origin_for_route(route);
        // What a page shows is read again when the page changes, never while it stays open.
        if self.browse_route.as_ref() != Some(route) {
            self.browse_route = Some(route.clone());
            self.catalog.page_changed();
        }
    }
    pub fn browse_origin(&self) -> Option<QueueOrigin> { self.browse_origin.clone() }
    fn origin_for_route(&self, route: &Route) -> Option<QueueOrigin> {
        if matches!(route.screen, Screen::ListenList | Screen::Settings | Screen::Metadata | Screen::PlaylistManager | Screen::PlaylistImport | Screen::Stats | Screen::ListeningHistory) { return None; }
        // What plays from the results page came from that search.
        if route.screen == Screen::Search {
            return Some(QueueOrigin { screen: "search".into(), query: route.key.clone(), source: self.search.source.clone(), ..Default::default() });
        }
        let mut origin = QueueOrigin { screen: route.screen.name().into(), id: route.id, key: route.key.clone(), ..Default::default() };
        if route.id < 0 && matches!(route.screen, Screen::Album | Screen::Playlist) {
            let (plugin, identity) = self.catalog.identity_of(route.id)?;
            // Runtime catalog IDs are not navigation identities across installations.
            origin.screen = "source".into(); origin.id = 0;
            origin.key = format!("{plugin} native:{identity}");
        }
        Some(origin)
    }
    pub fn with_queue_origin<T>(&mut self, origin: Option<QueueOrigin>, action: impl FnOnce(&mut Self) -> T) -> T {
        let previous = std::mem::replace(&mut self.browse_origin, origin);
        let result = action(self);
        self.browse_origin = previous;
        result
    }
    pub(crate) fn search_origin(&self) -> Option<QueueOrigin> {
        (!self.search.text.is_empty()).then(|| QueueOrigin { screen: "search".into(), query: self.search.text.clone(), source: self.search.source.clone(), ..Default::default() })
    }
    pub fn playing_origin(&self) -> Option<&QueueOrigin> {
        self.queue_origins.get(self.now.index).and_then(Option::as_ref)
    }
    pub fn playing_origin_json(&self) -> String {
        self.playing_origin().and_then(|origin| serde_json::to_string(origin).ok()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playing_origin_tracks_same_path_replacement_and_occurrence_changes() {
        let mut app = Chilpa::new();
        app.now = crate::NowPlaying::default();
        app.queue = vec!["duplicate".into(), "duplicate".into()];
        app.queue_origins = [1, 2].into_iter().map(|id| Some(QueueOrigin { id, ..Default::default() })).collect();
        let first = app.playing_origin().cloned();
        app.now.index = 1;
        assert_ne!(app.playing_origin(), first.as_ref());
        let second = app.playing_origin().cloned();
        app.queue_origins[1].as_mut().unwrap().key = "replacement".into();
        assert_ne!(app.playing_origin(), second.as_ref());
        assert_eq!(serde_json::from_str::<QueueOrigin>(&app.playing_origin_json()).unwrap().key, "replacement");
        app.queue_origins.clear();
        assert!(app.playing_origin().is_none());
        assert!(app.playing_origin_json().is_empty());
    }

    #[test]
    fn saved_record_origins_resolve_to_records_and_keep_other_pages() {
        let mut app = Chilpa::new();
        for kind in ["album", "playlist"] {
            let reference = format!("{kind}:opaque-id");
            let found = crate::source_search::RemoteFound {
                source: "fixture-origin".into(), name: "Fixture".into(), id: reference.clone(),
                title: "Record".into(), detail: String::new(), art: String::new(), duration_ms: 0,
                collection: true, entity: kind.into(),
                item: Some(serde_json::json!({"kind":kind,"ref":reference,"title":"Record"})),
                no_audio: false,
            };
            let id = app.catalog.adopt(&found.source, None, &found);
            let origin = QueueOrigin { screen: "source".into(), key: format!("{} native:{reference}", found.source), ..Default::default() };
            let restored: QueueOrigin = serde_json::from_str(&serde_json::to_string(&origin).unwrap()).unwrap();
            let route = app.route_to_origin(&restored).unwrap();
            assert_eq!(route.screen, Screen::Album);
            assert_eq!(route.id, id);
            app.set_browse_route(&route);
            let browsing = app.browse_origin().unwrap();
            assert_eq!(browsing.screen, restored.screen);
            assert_eq!(browsing.key, restored.key);
            assert_eq!(browsing.id, 0);
        }
        for key in ["fixture-origin native:artist:42", "fixture-origin native:discover:home", "fixture-origin native:album:opaque-id|related"] {
            let origin = QueueOrigin { screen: "source".into(), key: key.into(), ..Default::default() };
            assert_eq!(app.route_to_origin(&origin).unwrap(), origin.route().unwrap());
        }
    }

    #[test]
    fn old_buffers_restore_without_inventing_an_origin() {
        let mut value = serde_json::to_value(crate::listen::Workspace::default()).unwrap();
        value["buffers"][0].as_object_mut().unwrap().remove("origins");
        let restored: crate::listen::Workspace = serde_json::from_value(value).unwrap();
        assert!(restored.buffers[0].origins.is_empty());
    }
}
