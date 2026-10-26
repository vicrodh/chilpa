//! I keep browsing independent of selection writes and preview tokens.
use super::{Browser, Outcome, Service, UiError};
use crate::device_sync::failure;
use serde_json::json;

impl Service {
    pub fn browse(&mut self, source: &str) -> Result<(), UiError> {
        self.browse_at(source, None)
    }
    pub fn reload_browser(&mut self) -> Result<(), UiError> {
        self.idle()?;
        if let Some(id) = self.browser.as_ref().and_then(Browser::focused_playlist) {
            self.browser_back();
            return self.open_browser_album(&id);
        }
        let bookmark = self.browser.as_ref().map(Browser::bookmark);
        self.browse_at(&self.browse_source.clone(), bookmark)
    }
    fn browse_at(&mut self, source: &str, bookmark: Option<crate::device_sync::browser::Bookmark>) -> Result<(), UiError> {
        self.idle()?;
        let pick = if source == "recorded" {
            None
        } else {
            Some(
                self.options
                    .iter()
                    .find(|row| row.id == source)
                    .map(|r| r.pick.clone())
                    .ok_or_else(|| {
                        failure(
                            chilpa_i18n::mark("This Library selection is no longer available."),
                            "",
                        )
                    })?,
            )
        };
        let profile =
            if source == "recorded" {
                Some(self.profile().cloned().ok_or_else(|| {
                    failure(chilpa_i18n::mark("Choose a saved device profile."), "")
                })?)
            } else {
                None
            };
        self.close_browser();
        let title = pick
            .as_ref()
            .map(|p| p.name().to_owned())
            .or_else(|| profile.as_ref().map(|p| p.name.clone()))
            .unwrap_or_default();
        let previous = self.status.clone();
        self.launch("browsing", move |store, library, cancel| {
            let result = if let Some(pick) = pick {
                Browser::selection(&library, &pick, &cancel)
            } else {
                Browser::recorded(&store, &profile.unwrap(), &cancel)
            }.map(|mut view| { if let Some(bookmark) = bookmark { view.restore(bookmark); } view });
            Ok(Outcome::Browsed(result))
        })?;
        self.browse_previous = Some(previous);
        self.browse_source = source.into();
        self.browse_title = title;
        self.revision += 1;
        Ok(())
    }
    pub fn open_browser_album(&mut self, id: &str) -> Result<(), UiError> {
        self.idle()?;
        if let Some(path) = self.browser.as_ref().and_then(|view| view.playlist_path(id)) {
            let profile = self.profile().cloned().ok_or_else(|| failure(chilpa_i18n::mark("Choose a saved device profile."), ""))?;
            let destination = self.status.destination.clone();
            let mut view = self.browser.take().unwrap();
            let id = id.to_owned();
            let previous = self.status.clone();
            let landing = self.playlist_result.clone();
            self.launch("browsing", move |store, _, cancel| {
                let result = super::work::read_playlist(&store, &profile, &destination, &path, &cancel)
                    .and_then(|text| view.open_playlist(&id, &text));
                let mut landing = landing.lock().unwrap_or_else(|e| e.into_inner());
                if cancel.check().is_ok() { *landing = Some(view); }
                Ok(Outcome::PlaylistRead(result.err()))
            })?;
            self.browse_previous = Some(previous);
            self.reading_playlist = true;
            self.browser_error = None;
            self.revision += 1;
            return Ok(());
        }
        if let Some(view) = &mut self.browser {
            view.open(id)?;
        }
        self.revision += 1;
        Ok(())
    }
    pub fn browser_view(&mut self, view: &str) -> Result<(), UiError> {
        self.idle()?;
        if let Some(browser) = &mut self.browser { browser.set_view(view)?; }
        self.browser_error = None;
        self.revision += 1;
        Ok(())
    }
    pub fn browser_back(&mut self) {
        if self.browse_previous.is_some() {
            return;
        }
        if !self.browser.as_mut().is_some_and(Browser::back) {
            self.close_browser();
        }
        self.revision += 1;
    }
    pub fn filter_browser(&mut self, text: &str) {
        if let Some(view) = &mut self.browser {
            view.filter(text);
        }
        self.revision += 1;
    }
    pub fn browser_page(&mut self, page: usize) {
        if let Some(view) = &mut self.browser {
            view.change_page(page);
        }
        self.revision += 1;
    }
    pub fn close_browser(&mut self) {
        if let Some(previous) = self.browse_previous.take() {
            self.cancel.cancel();
            self.playlist_result.lock().unwrap_or_else(|e| e.into_inner()).take();
            if self.reading_playlist {
                // I keep the session slot until the transport closes. A cancelled reader drops
                // its snapshot before replying, so an unseen result cannot retain its catalog.
                self.status = previous.clone();
                self.status.busy = true;
                self.status.phase = "cancelling".into();
                self.browse_previous = Some(previous);
            } else {
                self.pending = None;
                self.status = previous;
            }
        }
        if !self.browse_source.is_empty() {
            self.revision += 1;
        }
        self.browser = None;
        self.browse_source.clear();
        self.browse_title.clear();
        self.browser_error = None;
    }
    pub fn browser_json(&self) -> String {
        if self.browse_source.is_empty() {
            return "{}".into();
        }
        let mut document = self.browser.as_ref().map(Browser::document).unwrap_or_else(|| json!({
            "source":self.browse_source,"title":self.browse_title,"recorded":self.browse_source == "recorded",
            "rows":[],"page":0,"pages":1,"can_back":self.browse_source != "recorded","level":"albums"}));
        document["loading"] = json!(self.browse_previous.is_some());
        document["error"] = json!(self.browser_error);
        document.to_string()
    }
}
