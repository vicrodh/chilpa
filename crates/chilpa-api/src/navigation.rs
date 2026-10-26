//! Tabs, each with its own history.
//!
//! A click navigates inside the active tab; only the + button, "Open in new
//! tab" and a middle click open one. Navigating pushes the current route on
//! `back` and clears `forward`; back and forward move between the two. There
//! is always at least one tab, and closing the active one activates the one
//! to its left.

pub(crate) const COMMANDS: &[crate::commands::Command] = &[
    crate::commands::command("go", "nav.go", "album | artist | settings | library"),
    crate::commands::command("q", "nav.closeTab", ""),
    crate::commands::command("qa", "nav.quit", ""),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    Library,
    Artists,
    Albums,
    /// Every track of the library, in one list.
    Tracks,
    Album,
    Artist,
    /// Artists sharing a place and musical vocabulary with the source artist.
    ArtistScene,
    Settings,
    /// A native plugin's download catalogue; the key is its plugin id.
    PluginDownloads,
    /// Settings > Library > Library folders.
    LibraryFolders,
    DeviceSync,
    /// The library by folder, or a folder opened from the disk.
    Folders,
    OpenMedia,
    Playlist,
    PlaylistManager,
    PlaylistImport,
    /// What a search found, whole; the key is what was typed.
    Search,
    /// The metadata editor, on one record.
    Metadata,
    ListenList,
    /// What was listened to, a period at a time, and the history whole.
    Stats,
    ListeningHistory,
    /// A plugin's own tree, browsed. The route's key is the plugin's id.
    Source,
    /// The page of one entry of a source: a station, an episode. The key
    /// is the plugin's id, a space, and the entry's id.
    SourceEntry,
}

impl Screen {
    pub fn name(self) -> &'static str {
        match self {
            Screen::Home => "home",
            Screen::Library => "library",
            Screen::Artists => "artists",
            Screen::Albums => "albums",
            Screen::Tracks => "tracks",
            Screen::Album => "album",
            Screen::Artist => "artist",
            Screen::ArtistScene => "artistScene",
            Screen::Settings => "settings",
            Screen::PluginDownloads => "pluginDownloads",
            Screen::LibraryFolders => "libraryFolders",
            Screen::DeviceSync => "deviceSync",
            Screen::Folders => "folders",
            Screen::OpenMedia => "openMedia",
            Screen::Playlist => "playlist",
            Screen::PlaylistManager => "playlistManager",
            Screen::PlaylistImport => "playlistImport",
            Screen::Metadata => "metadata",
            Screen::ListenList => "listenlist",
            Screen::Stats => "stats",
            Screen::ListeningHistory => "listeningHistory",
            Screen::Search => "search",
            Screen::Source => "source",
            Screen::SourceEntry => "sourceEntry",
        }
    }

    pub fn from_name(name: &str) -> Option<Screen> {
        [Screen::Home, Screen::Library, Screen::Artists, Screen::Albums, Screen::Tracks, Screen::Album, Screen::Artist, Screen::ArtistScene, Screen::Settings, Screen::PluginDownloads, Screen::LibraryFolders, Screen::DeviceSync, Screen::Folders, Screen::OpenMedia, Screen::Playlist, Screen::PlaylistManager, Screen::PlaylistImport, Screen::Metadata, Screen::ListenList, Screen::Stats, Screen::ListeningHistory, Screen::Search, Screen::Source, Screen::SourceEntry].into_iter().find(|s| s.name() == name)
    }
}

/// Where a tab is: a screen and, for a record, which one; for Settings,
/// which section; for Folders, which folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub screen: Screen,
    pub id: i64,
    pub key: String,
}

/// I resolve the destination without opening its media parent before acknowledgement.
pub struct PlannedRoute {
    pub route: Route,
    pub(crate) media_parent: Option<(String, chilpa_plugin::messages::Entry)>,
}

impl PlannedRoute {
    pub fn needs_preparation(&self) -> bool { self.media_parent.is_some() }
}

impl Route {
    pub fn of(screen: Screen) -> Route {
        Route { screen, id: 0, key: String::new() }
    }

    /// One line: screen, id and key, by tabs. A key never holds a tab or a
    /// line break -- it is a name, a path or a section.
    fn line(&self) -> String {
        format!("{}\t{}\t{}", self.screen.name(), self.id, self.key)
    }

    fn from_line(line: &str) -> Option<Route> {
        let mut parts = line.splitn(3, '\t');
        let screen = Screen::from_name(parts.next()?)?;
        let id = parts.next()?.parse().ok()?;
        Some(Route { screen, id, key: parts.next().unwrap_or_default().to_string() })
    }
}

#[derive(Clone, Debug)]
struct Tab {
    route: Route,
    back: Vec<Route>,
    forward: Vec<Route>,
}

impl Tab {
    fn at(route: Route) -> Tab {
        Tab { route, back: Vec::new(), forward: Vec::new() }
    }
}

#[derive(Clone, Debug)]
pub struct Tabs {
    tabs: Vec<Tab>,
    active: usize,
}

impl Default for Tabs {
    fn default() -> Self {
        Tabs { tabs: vec![Tab::at(Route::of(Screen::Home))], active: 0 }
    }
}

impl Tabs {
    pub fn routes(&self) -> Vec<Route> {
        self.tabs.iter().map(|t| t.route.clone()).collect()
    }

    pub fn active(&self) -> usize {
        self.active
    }

    pub fn step(&mut self, by: i32) {
        self.active = (self.active as i64 + i64::from(by)).rem_euclid(self.tabs.len() as i64) as usize;
    }

    pub fn current(&self) -> Route {
        self.tabs[self.active].route.clone()
    }

    pub fn can_back(&self) -> bool {
        !self.tabs[self.active].back.is_empty()
    }

    pub fn can_forward(&self) -> bool {
        !self.tabs[self.active].forward.is_empty()
    }

    /// Navigates inside the active tab. Going where it already is does not
    /// grow the history.
    pub fn go(&mut self, to: Route) {
        let tab = &mut self.tabs[self.active];
        if tab.route == to {
            return;
        }
        // A new search on the results page takes the page's place, as QBZ:
        // going back leaves the page, not each search typed on it.
        if tab.route.screen == Screen::Search && to.screen == Screen::Search {
            tab.route = to;
            return;
        }
        let was = std::mem::replace(&mut tab.route, to);
        tab.back.push(was);
        tab.forward.clear();
    }

    pub fn back(&mut self) {
        let tab = &mut self.tabs[self.active];
        if let Some(to) = tab.back.pop() {
            let was = std::mem::replace(&mut tab.route, to);
            tab.forward.push(was);
        }
    }

    pub fn forward(&mut self) {
        let tab = &mut self.tabs[self.active];
        if let Some(to) = tab.forward.pop() {
            let was = std::mem::replace(&mut tab.route, to);
            tab.back.push(was);
        }
    }

    /// Opens a tab right after the active one and activates it.
    pub fn open(&mut self, at: Route) {
        self.tabs.insert(self.active + 1, Tab::at(at));
        self.active += 1;
    }

    pub fn activate(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    /// The open tabs, to be kept: the active one's index on the first line,
    /// then each tab's place, one a line. Histories are not kept.
    pub fn save(&self) -> String {
        let mut out = vec![self.active.to_string()];
        out.extend(self.tabs.iter().map(|t| t.route.line()));
        out.join("\n")
    }

    /// Tabs as `save` wrote them; the default when nothing usable is there.
    pub fn restore(saved: &str) -> Tabs {
        let mut lines = saved.lines();
        let active = lines.next().and_then(|l| l.parse::<usize>().ok()).unwrap_or(0);
        let tabs: Vec<Tab> = lines.filter_map(Route::from_line).map(Tab::at).collect();
        if tabs.is_empty() {
            return Tabs::default();
        }
        let active = active.min(tabs.len() - 1);
        Tabs { tabs, active }
    }

    /// Forgets a place that no longer exists -- a deleted playlist -- from
    /// every tab and every history. A tab that was on it goes back to where
    /// it came from, or Home.
    pub fn forget(&mut self, gone: &Route) {
        for tab in &mut self.tabs {
            tab.back.retain(|r| r != gone);
            tab.forward.retain(|r| r != gone);
            if tab.route == *gone {
                tab.route = tab.back.pop().unwrap_or_else(|| Route::of(Screen::Home));
            }
        }
    }

    /// Closes a tab. The last one stays open; closing the active one
    /// activates the one to its left.
    pub fn close(&mut self, index: usize) {
        if self.tabs.len() == 1 || index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        if index < self.active || (index == self.active && self.active > 0) {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len() - 1);
    }
}

/// Where the open tabs are kept between runs.
const TABS_KEY: &str = "ui.tabs";

impl crate::Chilpa {
    /// I perform the destination's source work after its acknowledgement frame.
    pub fn prepare_planned_route(&mut self, plan: PlannedRoute) -> bool {
        if let Some((source, parent)) = plan.media_parent {
            self.open_media_parent(source, parent);
            return true;
        }
        false
    }

    /// A tab's title and the name of its icon. A record's tab is named
    /// after the record, and a source's after its plugin.
    pub fn tab_label(&mut self, r: &Route) -> (String, String) {
        let (title, icon) = match r.screen {
            Screen::Home => (self.t("Home"), "home"),
            Screen::Library => (self.t("Library"), "columns3"),
            Screen::Artists => (self.t("Artists"), "microphone2"),
            Screen::Albums => (self.t("Albums"), "disc"),
            Screen::Tracks => (self.t("Tracks"), "music"),
            Screen::Settings => (self.t("Settings"), "settings"),
            Screen::DeviceSync => (self.t("Device sync"), "deviceMobile"),
            Screen::PluginDownloads => (self.plugins.as_ref().and_then(|plugins| plugins.try_lock().ok())
                .and_then(|plugins| plugins.list().into_iter().find(|plugin| plugin.id == r.key).map(|plugin| plugin.name))
                .unwrap_or_else(|| self.t("Purchases")), "download"),
            Screen::LibraryFolders => (self.t("Library folders"), "folder"),
            Screen::Artist => (r.key.clone(), "user"),
            Screen::ArtistScene => (self.t("Artists from this scene"), "hierarchy2"),
            Screen::Folders => match std::path::Path::new(&r.key).file_name() {
                Some(name) => (name.to_string_lossy().into_owned(), "folder"),
                None => (self.t("Folders"), "folder"),
            },
            Screen::OpenMedia => (self.media.as_ref().map(|m| m.media.title.clone()).unwrap_or_else(|| self.t("Open media")), "folderOpen"),
            Screen::Album => (self.album(r.id).map(|a| a.title).unwrap_or_default(), "disc"),
            Screen::Playlist => (self.playlist(r.id).map(|p| p.name).unwrap_or_default(), "playlist"),
            Screen::PlaylistManager => (self.t("Playlist manager"), "playlist"),
            Screen::PlaylistImport => (self.t("Import playlist"), "playlist"),
            Screen::ListenList => (self.t("Queues"), "listDetails"),
            Screen::Metadata => (self.t("Edit metadata"), "listDetails"),
            Screen::Stats => (self.t("Listening stats"), "chartBar"),
            Screen::ListeningHistory => (self.t("Listening history"), "history"),
            Screen::Search => (r.key.clone(), "search"),
            Screen::Source | Screen::SourceEntry => {
                let (plugin, entry) = r.key.split_once(' ').unwrap_or((r.key.as_str(), ""));
                let panel = self.source_panels().into_iter().find(|p| p.id == plugin).unwrap_or_default();
                let icon = if !panel.icon_image.is_empty() { panel.icon_image } else if panel.icon.is_empty() { "server".to_string() } else { panel.icon };
                let known = if panel.name.is_empty() { self.catalog.name(plugin) } else { Some(panel.name) };
                let name = known.unwrap_or_else(|| plugin.to_string());
                // A listing's tab is named after the listing, an entry's after
                // the entry, when they are known.
                let listing = (r.screen == Screen::Source && !entry.is_empty()).then(|| self.source_listing_title(&r.key)).flatten();
                return (listing.or_else(|| self.source_entry_title(plugin, entry)).unwrap_or(name), icon);
            }
        };
        (title, icon.to_string())
    }

    /// True when the tabs are kept and brought back (Settings > Navigation).
    fn restores_tabs(&self) -> bool {
        self.preference("navigation.restoreTabs").as_deref() != Some("false")
    }

    /// The tabs of the last run, unless keeping them was turned off.
    pub fn restored_tabs(&self) -> Option<Tabs> {
        // A start died before its window was shown: the remembered tabs are
        // a suspect, so Home opens and they stay kept (the crash chain).
        if crate::startup_chain::level() >= 2 {
            eprintln!("startup: the remembered tabs are bypassed (crash chain); Home opens");
            return None;
        }
        // The preferences file is one line per key, so the lines of the
        // tabs are kept as "\n".
        let saved = self.preference(TABS_KEY).filter(|_| self.restores_tabs())?;
        Some(Tabs::restore(&saved.replace("\\n", "\n")))
    }

    /// I protect live routes without reading labels or writing preferences.
    pub fn protect_tabs(&mut self, tabs: &Tabs) {
        let routes = tabs.routes();
        let ids: Vec<_> = routes.iter().filter(|route| route.screen == Screen::Playlist && route.id > 0).map(|route| route.id).collect();
        let mut playlists = std::collections::HashMap::new();
        if !ids.is_empty() {
            // I read only member identities before the catalog worker can prune them.
            let reader = if self.library.is_none() { chilpa_library::Library::open_read_only(&chilpa_library::Library::default_path()).ok() } else { None };
            let library = if let Some(library) = reader.as_ref() { Some(library) } else { self.library() };
            if let Some(library) = library {
                for id in ids { if let Some(paths) = playlist_paths(library, id) { playlists.insert(id, paths); } }
            }
        }
        self.catalog.protect_navigation_with_tracks(&routes, playlists);
    }

    pub(crate) fn refresh_playlist_roots(&mut self, id: i64) {
        if !self.catalog.open_playlists().contains(&id) { return; }
        if let Some(library) = self.library() {
            if let Some(paths) = playlist_paths(library, id) { self.catalog.protect_playlist_tracks(id, paths); }
        }
    }

    /// I protect live routes before optionally saving the open tabs for the next run.
    pub fn keep_tabs(&mut self, tabs: &Tabs) {
        self.protect_tabs(tabs);
        if self.restores_tabs() {
            self.remember_preference(TABS_KEY, &tabs.save().replace('\n', "\\n"));
        }
    }
}

fn playlist_paths(library: &chilpa_library::Library, id: i64) -> Option<std::collections::HashSet<String>> {
    let entries = library.playlist_entries(id).ok()?;
    let ids: Vec<_> = entries.iter().filter_map(|entry| entry.track_id).collect();
    let known = library.track_paths(&ids).ok()?;
    Some(entries.into_iter().map(|entry| entry.track_id.and_then(|id| known.get(&id).cloned()).unwrap_or(entry.external_id))
        .filter(|path| crate::source_opener::of_plugin(path).is_some()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn album(id: i64) -> Route {
        Route { screen: Screen::Album, id, key: String::new() }
    }

    #[test]
    fn cycling_tabs_wraps_without_changing_their_histories() {
        let mut tabs = Tabs::default();
        tabs.go(album(7)); tabs.open(album(8)); tabs.go(album(9));
        tabs.step(1); assert_eq!(tabs.current(), album(7));
        tabs.step(-1); assert_eq!(tabs.current(), album(9));
        tabs.back(); assert_eq!(tabs.current(), album(8));
        tabs.step(i32::MIN); assert!(tabs.active() < 2);
        let mut only = Tabs::default(); only.step(-1); assert_eq!(only.active(), 0);
    }

    #[test]
    fn tabs_kept_come_back_as_they_were() {
        let mut t = Tabs::default();
        t.go(album(7));
        t.open(Route { screen: Screen::Artist, id: 0, key: "Mora Vale".into() });
        let back = Tabs::restore(&t.save());
        assert_eq!(back.routes(), t.routes());
        assert_eq!(back.active(), 1);
        assert_eq!(Tabs::restore("").routes(), [Route::of(Screen::Home)], "nothing kept, Home");
        assert_eq!(Tabs::restore("9\nnowhere\t0\t").routes(), [Route::of(Screen::Home)], "nothing usable, Home");
    }

    #[test]
    fn a_new_search_takes_the_results_page_s_place() {
        let search = |text: &str| Route { screen: Screen::Search, id: 0, key: text.into() };
        assert_eq!(Screen::from_name("search"), Some(Screen::Search));
        let mut tabs = Tabs::default();
        tabs.go(album(7));
        tabs.go(search("mora"));
        tabs.go(search("mora vale"));
        assert_eq!(tabs.current(), search("mora vale"));
        tabs.back();
        assert_eq!(tabs.current(), album(7), "back leaves the page, not each search typed on it");
        tabs.forward();
        assert_eq!(tabs.current(), search("mora vale"));
    }

    #[test]
    fn the_artist_scene_route_opens_and_survives_tab_restore() {
        assert_eq!(Screen::from_name("artistScene"), Some(Screen::ArtistScene));
        let mut tabs = Tabs::default();
        tabs.go(Route { screen: Screen::ArtistScene, id: 0, key: "David Bowie".into() });
        assert_eq!(tabs.current().screen.name(), "artistScene");
        assert_eq!(Tabs::restore(&tabs.save()).current().key, "David Bowie");
    }

    #[test]
    fn a_place_that_is_gone_leaves_every_tab_and_history() {
        let mut t = Tabs::default();
        let gone = Route { screen: Screen::Playlist, id: 3, key: String::new() };
        t.go(Route::of(Screen::Library));
        t.go(gone.clone());
        t.open(gone.clone());
        t.forget(&gone);
        assert_eq!(t.routes(), [Route::of(Screen::Library), Route::of(Screen::Home)]);
        t.activate(0);
        assert!(t.can_back(), "Home is still behind Library");
        assert!(!t.can_forward());
    }

    #[test]
    fn a_click_navigates_in_place_and_back_returns() {
        let mut t = Tabs::default();
        t.go(Route::of(Screen::Library));
        t.go(album(7));
        assert_eq!(t.routes().len(), 1, "no tab was opened");
        t.back();
        assert_eq!(t.current(), Route::of(Screen::Library));
        t.forward();
        assert_eq!(t.current(), album(7));
        assert!(!t.can_forward());
    }

    #[test]
    fn navigating_after_going_back_forgets_the_forward_history() {
        let mut t = Tabs::default();
        t.go(album(1));
        t.back();
        t.go(album(2));
        assert!(!t.can_forward());
        t.back();
        assert_eq!(t.current(), Route::of(Screen::Home));
    }

    #[test]
    fn going_where_it_already_is_does_not_grow_the_history() {
        let mut t = Tabs::default();
        t.go(Route::of(Screen::Home));
        assert!(!t.can_back());
    }

    #[test]
    fn each_tab_keeps_its_own_history() {
        let mut t = Tabs::default();
        t.go(album(1));
        t.open(Route::of(Screen::Library));
        assert!(!t.can_back(), "a new tab starts with no history");
        t.activate(0);
        assert!(t.can_back());
    }

    #[test]
    fn closing_the_active_tab_activates_its_left_neighbor_and_one_always_stays() {
        let mut t = Tabs::default();
        t.open(Route::of(Screen::Library));
        t.open(Route::of(Screen::Albums));
        t.close(2);
        assert_eq!((t.active(), t.current()), (1, Route::of(Screen::Library)));
        t.close(0);
        assert_eq!((t.active(), t.current()), (0, Route::of(Screen::Library)));
        t.close(0);
        assert_eq!(t.routes().len(), 1, "the last tab stays");
    }
}
