//! The Offline music page and the menus that lead to it. The window's
//! bridge hands every action over by name without knowing any of them.

use super::{choice, global_directory, manager, worker::ids, ROOT_PREFERENCE};
use crate::Chilpa;
use chilpa_library::Library;
use serde_json::json;

const SELECTIONS: [&str; 3] = ["tracks", "album", "playlist"];

/// What the page acts on besides every download: the selection a menu
/// sent, and a server folder being written.
#[derive(Default)]
pub(crate) struct Page {
    staged: Option<(String, String)>,
    pub(super) manager: manager::Manager,
    folder: Option<std::thread::JoinHandle<Result<(), String>>>,
    said: String,
}

impl Chilpa {
    pub fn server_offline(&mut self, action: &str, kind: &str, selected: &str) -> String {
        match action {
            "info" => self.server_offline_info(kind, selected),
            "start" => self.server_offline_start(kind, selected),
            "cancel" => {
                self.server_offline_cancel();
                self.server_offline_status()
            }
            "removal_info" => self.server_offline_removal_info(kind, selected),
            "dismiss_removal" => {
                self.server_offline_dismiss_removal();
                "{}".into()
            }
            "remove" => self.server_offline_remove(selected),
            "stage" => self.server_offline_stage(kind, selected),
            "selection" => self.server_offline_selection(),
            "manager" => self.server_offline_manager(kind, selected),
            "refresh" => {
                self.servers.offline_page.manager.stale = true;
                "{}".into()
            }
            // The page closed: its listing goes; the next visit reads it again.
            "leave" => {
                self.servers.offline_page.manager = manager::Manager::default();
                "{}".into()
            }
            "server_folder" => self.server_offline_folder(kind.parse().unwrap_or(0), selected),
            "limit" => json!({"ok": self.offline_limit_setting(super::LIMIT, kind)}).to_string(),
            _ => self.server_offline_status(),
        }
    }

    /// A menu's selection, for the page to offer; anything else clears it.
    pub fn server_offline_stage(&mut self, kind: &str, selected: &str) -> String {
        let chosen = ids(selected);
        self.servers.offline_page.staged = (SELECTIONS.contains(&kind) && !chosen.is_empty())
            .then(|| (kind.to_string(), serde_json::to_string(&chosen).unwrap_or_default()));
        self.server_offline_selection()
    }

    /// The staged selection, with what its download would take.
    pub fn server_offline_selection(&mut self) -> String {
        let Some((kind, selected)) = self.servers.offline_page.staged.clone() else {
            return json!({"kind": "downloads", "selected": "[]", "count": 0}).to_string();
        };
        let info: serde_json::Value = serde_json::from_str(&self.server_offline_info(&kind, &selected)).unwrap_or_default();
        json!({"kind": kind, "selected": selected, "count": info["count"], "too_many": info["too_many"]}).to_string()
    }

    /// The page's listing (`manager`). `seen` is the revision the page has.
    pub fn server_offline_manager(&mut self, seen: &str, filters: &str) -> String {
        if self.servers.offline_page.folder.as_ref().is_some_and(|task| task.is_finished()) {
            if let Some(task) = self.servers.offline_page.folder.take() {
                self.servers.offline_page.said = task.join().unwrap_or_else(|_| Err(chilpa_i18n::t("The settings could not be saved."))).err().unwrap_or_default();
                self.servers.offline_page.manager.stale = true;
            }
        }
        let chosen = self.preference(ROOT_PREFERENCE).unwrap_or_default();
        let limit = self.offline_limit();
        let context = move || manager::Context { root: global_directory(Some(chosen.as_str())), chosen, limit };
        let page = &mut self.servers.offline_page;
        let waiting = page.folder.is_some();
        page.manager.request(seen, manager::Filters::parse(filters), &page.said, waiting, context)
    }

    /// Where one server's next copies go: a folder URL, or empty for the
    /// offline folder. Written on a thread; the listing says when it is.
    pub fn server_offline_folder(&mut self, source: i64, folder: &str) -> String {
        let folder = match choice(folder) {
            Ok(folder) => folder,
            Err(error) => return json!({"error": error}).to_string(),
        };
        if source <= 0 || self.servers.offline_page.folder.is_some() {
            return json!({"error": chilpa_i18n::t("The settings could not be saved.")}).to_string();
        }
        self.servers.offline_page.said.clear();
        let task = std::thread::Builder::new().name("chilpa-offline-folder".into()).spawn(move || {
            let saved = Library::open_existing(&Library::default_path()).and_then(|lib| lib.set_server_offline_folder(source, &folder));
            match saved {
                Ok(true) => Ok(()),
                _ => Err(chilpa_i18n::t("The settings could not be saved.")),
            }
        });
        match task {
            Ok(task) => {
                self.servers.offline_page.folder = Some(task);
                json!({"pending": true}).to_string()
            }
            Err(_) => json!({"error": chilpa_i18n::t("The settings could not be saved.")}).to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_selection_is_staged_and_anything_else_clears_it() {
        let mut api = Chilpa::new();
        let staged: serde_json::Value = serde_json::from_str(&api.server_offline("stage", "album", "[17]")).unwrap();
        assert_eq!((staged["kind"].as_str(), staged["selected"].as_str()), (Some("album"), Some("[17]")));
        let again: serde_json::Value = serde_json::from_str(&api.server_offline("selection", "", "")).unwrap();
        assert_eq!(again["kind"], "album", "it stays until something else is staged");
        for (kind, selected) in [("downloads", "[]"), ("album", "[]"), ("elsewhere", "[3]"), ("tracks", "not ids")] {
            api.server_offline("stage", "tracks", "[1,2]");
            let cleared: serde_json::Value = serde_json::from_str(&api.server_offline("stage", kind, selected)).unwrap();
            assert_eq!(cleared["kind"], "downloads", "{kind} {selected}");
        }
    }

    #[test]
    fn the_limit_and_folders_are_taken_only_as_listed() {
        let mut api = Chilpa::new();
        assert_eq!(api.server_offline("limit", "300", ""), r#"{"ok":false}"#);
        assert_eq!(api.server_offline("limit", "unlimited", ""), r#"{"ok":true}"#);
        assert_eq!(api.offline_limit().quota, None);
        let refused: serde_json::Value = serde_json::from_str(&api.server_offline("server_folder", "7", "https://example.invalid/music")).unwrap();
        assert!(refused["error"].is_string());
        let nobody: serde_json::Value = serde_json::from_str(&api.server_offline("server_folder", "0", "")).unwrap();
        assert!(nobody["error"].is_string());
        assert_eq!(api.server_offline("leave", "", ""), "{}");
    }

    #[test]
    fn a_server_folder_is_written_on_a_thread_and_the_page_hears_how_it_went() {
        let mut api = Chilpa::new();
        let root = tempfile::tempdir().unwrap();
        let folder = url::Url::from_directory_path(root.path()).unwrap().to_string();
        assert_eq!(api.server_offline("server_folder", "7", &folder), r#"{"pending":true}"#);
        let busy: serde_json::Value = serde_json::from_str(&api.server_offline("server_folder", "7", &folder)).unwrap();
        assert!(busy["error"].is_string() || busy["pending"] == true, "one write at a time");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut answer = serde_json::Value::Null;
        while std::time::Instant::now() < deadline {
            answer = serde_json::from_str(&api.server_offline("manager", "", "")).unwrap();
            if answer["said"] != "" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(answer["said"], chilpa_i18n::t("The settings could not be saved."), "there is no server 7");
    }
}
