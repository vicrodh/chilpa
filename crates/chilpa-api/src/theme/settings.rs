//! The theme's rows of Settings and the options under its picker: the
//! list and its filter, the auto theme's source, and the custom theme's
//! base colours (QBZ 2.1.2, `AppearanceSettings.qml`).

use super::choice::{self, auto, custom, Entry, ACCENT, AUTO_FOLDED, AUTO_IMAGE, AUTO_SOURCE, AUTO_SOURCES, CUSTOM, CUSTOM_FOLDED, FILTER, THEME};
use super::Theme;
use crate::settings_rows::SettingsRow;
use crate::Chilpa;

/// The accents offered (design/05): the theme's own, then four.
const ACCENTS: &[&str] = &["", "#A3161F", "#D8323A", "#3DAEE9", "#5C6166"];

impl Chilpa {
    /// The theme chosen, for a desktop that is dark or not.
    pub fn theme(&self, system_dark: bool) -> Theme {
        choice::resolve(&self.chosen_theme(), system_dark, self.preference(ACCENT).as_deref())
    }

    fn chosen_theme(&self) -> String {
        self.preference(THEME).unwrap_or_else(|| "system".to_string())
    }

    /// The theme's own rows: the list, and the accent, which the custom
    /// theme leaves to its editor.
    pub(crate) fn theme_row(&self, row: &mut SettingsRow) -> bool {
        match row.key.as_str() {
            THEME => {
                for Entry { slug, label, .. } in choice::entries() {
                    row.values.push(slug);
                    row.options.push(label);
                }
                row.value = self.chosen_theme();
            }
            ACCENT => {
                if self.chosen_theme() == CUSTOM {
                    row.control.clear();
                    return true;
                }
                // The theme's own first -- drawn by the window, which knows
                // which theme is on -- then the design's four.
                row.values = ACCENTS.iter().map(|a| a.to_string()).collect();
                row.options = row.values.clone();
                row.value = self.preference(ACCENT).unwrap_or_default();
            }
            _ => return false,
        }
        true
    }

    /// What the options under the theme's picker show, as one document: the
    /// list's filter and which themes are light, the auto theme's source,
    /// and the custom theme's colours.
    pub fn theme_options_json(&self) -> String {
        let chosen = self.chosen_theme();
        let entries = choice::entries();
        let title = entries.iter().find(|e| e.slug == chosen).map(|e| e.label.clone()).unwrap_or_default();
        let light: Vec<&str> = entries.iter().filter(|e| e.light).map(|e| e.slug.as_str()).collect();
        let source = self.preference(AUTO_SOURCE).and_then(|s| AUTO_SOURCES.iter().position(|v| *v == s)).unwrap_or(0);
        let folded = |key: &str| self.preference(key).as_deref() == Some("true");
        serde_json::json!({
            "theme": chosen,
            "title": title,
            "filter": self.preference(FILTER).and_then(|f| f.parse::<u8>().ok()).unwrap_or(0).min(2),
            "light": light,
            "auto": {
                "sources": [chilpa_i18n::t("System Colors"), chilpa_i18n::t("Wallpaper Sync"), chilpa_i18n::t("Custom Image")],
                "source": source,
                "image": self.preference(AUTO_IMAGE).unwrap_or_default(),
                "desktop": auto::desktop(),
                "folded": folded(AUTO_FOLDED),
            },
            "custom": {
                "state": custom::state(),
                "folded": folded(CUSTOM_FOLDED),
            },
        })
        .to_string()
    }

    /// The list's filter: 0 all, 1 dark, 2 light.
    pub fn choose_theme_filter(&mut self, filter: i32) {
        self.remember_preference(FILTER, &filter.clamp(0, 2).to_string());
    }

    /// The auto theme's source, by its place in the list. True when it is
    /// the theme on screen and has to be generated again.
    pub fn choose_auto_theme_source(&mut self, index: i32) -> bool {
        let Some(source) = usize::try_from(index).ok().and_then(|i| AUTO_SOURCES.get(i)) else { return false };
        self.remember_preference(AUTO_SOURCE, source);
        self.chosen_theme() == choice::AUTO
    }

    /// The image the auto theme reads, which makes the image its source.
    /// True when it is the theme on screen and has to be generated again.
    pub fn choose_auto_theme_image(&mut self, path: &str) -> bool {
        if path.trim().is_empty() {
            return false;
        }
        self.remember_preference(AUTO_IMAGE, path.trim());
        self.remember_preference(AUTO_SOURCE, "image");
        self.chosen_theme() == choice::AUTO
    }

    /// Folds or opens the box of the auto or the custom options.
    pub fn fold_theme_options(&mut self, which: &str, folded: bool) {
        let key = if which == CUSTOM { CUSTOM_FOLDED } else { AUTO_FOLDED };
        self.remember_preference(key, if folded { "true" } else { "false" });
    }
}
