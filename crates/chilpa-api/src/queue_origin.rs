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
    pub fn set_browse_route(&mut self, route: &Route) {
        self.browse_origin = self.origin_for_route(route);
    }
    pub fn browse_origin(&self) -> Option<QueueOrigin> { self.browse_origin.clone() }
    fn origin_for_route(&self, route: &Route) -> Option<QueueOrigin> {
        if matches!(route.screen, Screen::ListenList | Screen::Settings | Screen::Metadata | Screen::PlaylistManager | Screen::PlaylistImport) { return None; }
        let mut origin = QueueOrigin { screen: route.screen.name().into(), id: route.id, key: route.key.clone(), ..Default::default() };
        if route.id < 0 && matches!(route.screen, Screen::Album | Screen::Playlist) {
            let card = self.catalog.card(route.id)?;
            // Runtime catalog IDs are not navigation identities across installations.
            origin.screen = "source".into(); origin.id = 0;
            origin.key = format!("{} native:{}", card.plugin, card.identity);
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
    pub fn playing_origin_json(&self) -> String {
        self.queue_origins.get(self.now.index).and_then(Option::as_ref)
            .and_then(|origin| serde_json::to_string(origin).ok()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn old_buffers_restore_without_inventing_an_origin() {
        let mut value = serde_json::to_value(crate::listen::Workspace::default()).unwrap();
        value["buffers"][0].as_object_mut().unwrap().remove("origins");
        let restored: crate::listen::Workspace = serde_json::from_value(value).unwrap();
        assert!(restored.buffers[0].origins.is_empty());
    }
}
