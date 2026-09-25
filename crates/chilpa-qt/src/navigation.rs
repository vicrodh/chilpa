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
        }
    }

    pub fn from_name(name: &str) -> Option<Screen> {
        [
            Screen::Home,
            Screen::Library,
            Screen::Artists,
            Screen::Albums,
            Screen::Album,
            Screen::Artist,
            Screen::Settings,
            Screen::LibraryFolders,
            Screen::Folders,
        ]
            .into_iter()
            .find(|s| s.name() == name)
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
