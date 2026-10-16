//! Medium navigation names destinations, never the entries currently being browsed.
use super::*;

pub(super) fn media_children(medium: &str) -> Option<Vec<PanelChild>> {
    let sections = match medium {
        "podcasts" => vec![("home","Home","home"),("explore","Explore","search"),("saved","Subscriptions","heart"),("continue","Continue listening","playerTrackNext"),("categories","Categories","layoutGrid")],
        "radio" => vec![("home","Home","home"),("explore","Explore","search"),("saved","My stations","heart"),("genres","Genres","layoutGrid"),("countries","Countries","list")],
        _ => return None,
    };
    Some(sections.into_iter().map(|(id,title,icon)| PanelChild {id:format!("media:{id}"),title:chilpa_i18n::t(title),icon:icon.into(),route:String::new()}).collect())
}

impl Chilpa {
    /// The track badge reuses the source's cached sidebar artwork.
    pub fn track_source_badge(&self, path: &str) -> String {
        let plugin = crate::source_opener::of_plugin(path).map(|(id, _)| id);
        let icon = plugin.as_ref().map(|id| self.sources.panels.iter().find(|p| &p.id == id)
            .map(|p| if p.icon_image.is_empty() { p.icon.clone() } else { p.icon_image.clone() })
            .unwrap_or_else(|| crate::plugin_icons::image_url(&Self::plugins_dir(), id))).unwrap_or_default();
        serde_json::json!({"label":self.track_source_name(path),"icon":icon,"local":plugin.is_none() && !crate::on_a_server(path)}).to_string()
    }

    pub(super) fn open_media_section(&mut self, source: &str, section: &str) {
        if !self.source_panels().iter().any(|p| p.id == source && p.children.iter().any(|c| c.id == format!("media:{section}"))) { return; }
        let medium = self.sources.media_kinds.get(source).cloned().unwrap_or_default();
        let switched = self.sources.current != source;
        let was_root = !switched && self.sources.trail.is_empty() && self.sources.text.is_empty();
        self.sources.current = source.into();
        self.sources.medium = medium;
        self.sources.media_tab = section.into();
        self.sources.music = false;
        self.sources.play_after_load = false;
        self.sources.filter.clear();
        self.sources.text.clear();
        self.sources.trail.clear();
        if switched { self.sources.attribution.clear(); self.sources.columns.clear(); self.sources.picks.clear(); }
        self.read_saved();
        if let Some(id) = match section {"saved"=>Some(SAVED),"continue"=>Some(UNFINISHED),_=>None} {
            self.sources.trail.push(Crumb { id:id.into(),title:chilpa_i18n::t(if section == "saved" { "Saved" } else { "Continue listening" }),entry:None,presentation:None });
            self.load_listing();
        } else if !was_root { self.load_listing(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn medium_menus_are_destinations_not_results() {
        let podcasts=media_children("podcasts").unwrap();
        assert_eq!(podcasts.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),vec!["media:home","media:explore","media:saved","media:continue","media:categories"]);
        let radio=media_children("radio").unwrap();
        assert_eq!(radio.last().unwrap().id,"media:countries");
        assert!(!radio.iter().any(|p| p.id=="media:continue"));
        assert!(media_children("music").is_none());
    }
}
