//! The genres a plugin's Discover was last filtered by, put back when one
//! of its tabs is opened again without them. A plugin carries its filter
//! from tab to tab, so the filter is the plugin's and not a tab's: chosen on
//! one tab, it is there when the sidebar opens another.

/// Where a plugin keeps the genres its Discover was last filtered by.
pub fn preference(plugin: &str) -> String {
    format!("ui.source.{plugin}.discover.genres")
}

/// Where a tab kept them before they were the plugin's; still read, for a
/// filter chosen then and not chosen again since.
fn tab_preference(plugin: &str, tab: &str) -> String {
    format!("ui.source.{plugin}.discover.{tab}.genres")
}

/// `key` with the remembered genres appended when it names a Discover tab
/// with no genres. Any other key, and a tab opened with genres, stay as they
/// are. `kept` reads a preference.
pub fn remembered(key: &str, kept: impl Fn(&str) -> Option<String>) -> String {
    let Some((plugin, tab)) = key.split_once(" native:discover:") else { return key.to_string() };
    if plugin.is_empty() || tab.is_empty() || tab.contains([':', '|']) {
        return key.to_string();
    }
    let genres = kept(&preference(plugin)).or_else(|| kept(&tab_preference(plugin, tab))).unwrap_or_default();
    let valid = !genres.is_empty() && genres.split(',').all(|id| !id.is_empty() && id.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-')));
    if valid { format!("{key}:{genres}") } else { key.to_string() }
}

impl crate::Chilpa {
    /// The key a route is made with: a Discover tab opened bare comes back
    /// with the genres it was last filtered by.
    pub fn route_key(&self, screen: crate::navigation::Screen, key: &str) -> String {
        if screen == crate::navigation::Screen::Source { remembered(key, |name| self.preference(name)) } else { key.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(key: &str) -> Option<String> {
        (key == "ui.source.fixture.discover.genres").then(|| "12,40".to_string())
    }

    #[test]
    fn a_discover_tab_opened_bare_gets_its_genres_back() {
        assert_eq!(remembered("fixture native:discover:home", kept), "fixture native:discover:home:12,40");
    }

    #[test]
    fn genres_chosen_on_one_tab_come_back_on_another() {
        // Chosen on Editor's Picks; the sidebar opens Home.
        assert_eq!(remembered("fixture native:discover:editors", kept), "fixture native:discover:editors:12,40");
        assert_eq!(remembered("fixture native:discover:home", kept), "fixture native:discover:home:12,40");
    }

    #[test]
    fn a_filter_kept_by_a_tab_before_is_still_read_until_one_is_chosen_again() {
        let old = |key: &str| (key == "ui.source.fixture.discover.home.genres").then(|| "7".to_string());
        assert_eq!(remembered("fixture native:discover:home", old), "fixture native:discover:home:7");
        let cleared = |key: &str| match key {
            "ui.source.fixture.discover.genres" => Some(String::new()),
            "ui.source.fixture.discover.home.genres" => Some("7".to_string()),
            _ => None,
        };
        assert_eq!(remembered("fixture native:discover:home", cleared), "fixture native:discover:home", "a cleared filter stays cleared");
    }

    #[test]
    fn a_key_that_names_genres_or_another_page_is_left_alone() {
        for key in ["fixture native:discover:home:7", "fixture native:discover:home|recent", "fixture native:artist:42", "fixture native:discover:new"] {
            assert_eq!(remembered(key, kept), key);
        }
    }

    #[test]
    fn an_empty_or_odd_preference_changes_nothing() {
        assert_eq!(remembered("fixture native:discover:home", |_| Some(String::new())), "fixture native:discover:home");
        assert_eq!(remembered("fixture native:discover:home", |_| Some("1,,2".into())), "fixture native:discover:home");
        assert_eq!(remembered("fixture native:discover:home", |_| Some("a b".into())), "fixture native:discover:home");
    }
}
