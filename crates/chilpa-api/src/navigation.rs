//! Tabs, each with its own history.
//!
//! A click navigates inside the active tab; only the + button, "Open in new
//! tab" and a middle click open one. Navigating pushes the current route on
//! `back` and clears `forward`; back and forward move between the two. There
//! is always at least one tab, and closing the active one activates the one
//! to its left.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    Library,
    Artists,
    Albums,
    Album,
    Artist,
    Settings,
    /// Settings > Library > Library folders.
    LibraryFolders,
    /// The library by folder, or a folder opened from the disk.
    Folders,
    Playlist,
    PlaylistManager,
}

impl Screen {
    pub fn name(self) -> &'static str {
        match self {
            Screen::Home => "home",
            Screen::Library => "library",
            Screen::Artists => "artists",
            Screen::Albums => "albums",
            Screen::Album => "album",
            Screen::Artist => "artist",
            Screen::Settings => "settings",
            Screen::LibraryFolders => "libraryFolders",
            Screen::Folders => "folders",
            Screen::Playlist => "playlist",
            Screen::PlaylistManager => "playlistManager",
        }
    }

    pub fn from_name(name: &str) -> Option<Screen> {
        [Screen::Home, Screen::Library, Screen::Artists, Screen::Albums, Screen::Album, Screen::Artist, Screen::Settings, Screen::LibraryFolders, Screen::Folders, Screen::Playlist, Screen::PlaylistManager].into_iter().find(|s| s.name() == name)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn album(id: i64) -> Route {
        Route { screen: Screen::Album, id, key: String::new() }
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
