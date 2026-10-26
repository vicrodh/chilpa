//! I keep browsing independent of selection writes, preview tokens and transport sessions.
use super::{Browser, Outcome, Service, UiError};
use crate::device_sync::failure;
use serde_json::json;

impl Service {
    pub fn browse(&mut self, source: &str) -> Result<(), UiError> {
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
            };
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
        if let Some(view) = &mut self.browser {
            view.open(id)?;
        }
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
            // I only read local metadata in this job; dropping its receiver also drops any queued snapshot.
            self.cancel.cancel();
            self.pending = None;
            self.status = previous;
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
