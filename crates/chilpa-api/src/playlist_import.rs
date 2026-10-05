//! I keep importer work outside the window thread; its plugin owns the workflow.
use crate::Chilpa;
use base64::Engine;
use serde_json::{json, Value};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
#[derive(Default)]
pub(crate) struct State {
    selected: String,
    snapshot: Value,
    options: Value,
    choices: Value,
    error: String,
    pending: Option<mpsc::Receiver<(String, Result<Value, String>)>>,
    last_poll: Option<Instant>,
    targets: Vec<String>,
    status_input: Value,
    queued: Option<(String, String)>,
}
impl Chilpa {
    pub fn playlist_import_json(&mut self) -> String {
        if let Some((method, result)) = self
            .playlist_import
            .pending
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            self.playlist_import.pending = None;
            match result {
                Ok(value) => match method.as_str() {
                    "import.options" => self.playlist_import.options = value,
                    "import.status" => self.playlist_import.snapshot = value,
                    "import.choices" => self.playlist_import.choices = value,
                    _ => self.playlist_import.last_poll = None,
                },
                Err(error) => self.playlist_import.error = error,
            }
        }
        if self.playlist_import.pending.is_none() {
            if let Some((action, text)) = self.playlist_import.queued.take() {
                self.playlist_import_command(&action, &text);
            }
        }
        let mut ids = self.natives.ids();
        ids.sort();
        let importers: Vec<_> = ids
            .into_iter()
            .filter(|id| self.natives.imports_playlists(id))
            .collect();
        if !importers.contains(&self.playlist_import.selected) {
            self.natives.broker.revoke(&self.playlist_import.selected);
            self.playlist_import = State::default();
            if let Some(id) = importers.first() {
                self.playlist_import.selected = id.clone();
                self.import_request("import.options", json!({}));
            }
        }
        if !self.playlist_import.selected.is_empty()
            && self.playlist_import.pending.is_none()
            && self
                .playlist_import
                .last_poll
                .is_none_or(|t| t.elapsed() > Duration::from_millis(350))
        {
            self.import_request("import.status", self.playlist_import.status_input.clone());
        }
        let folders: Vec<_> = self
            .playlist_folders()
            .into_iter()
            .map(|f| json!({"id":f.id,"name":f.name}))
            .collect();
        json!({"installed":!importers.is_empty(),"plugin":self.playlist_import.selected,"options":self.playlist_import.options,
            "sources":crate::plugin_broker::sources(&self.natives),"folders":folders,"state":self.playlist_import.snapshot,
            "choices":self.playlist_import.choices,"error":self.playlist_import.error,"pending":self.playlist_import.pending.is_some()}).to_string()
    }
    pub fn playlist_import_command(&mut self, action: &str, text: &str) {
        if action == "cancel" {
            self.natives.broker.revoke(&self.playlist_import.selected);
        }
        if self.playlist_import.pending.is_some() {
            self.playlist_import.queued = Some((action.into(), text.into()));
            return;
        }
        let mut input: Value = serde_json::from_str(text).unwrap_or_else(|_| json!({}));
        self.playlist_import.error.clear();
        let method = match action {
            "preview" => "import.preview",
            "choices" => "import.choices",
            "match" => {
                self.playlist_import.targets = input["targets"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
                "import.match"
            }
            "commit" => {
                let catalogs: Vec<_> = crate::plugin_broker::sources(&self.natives)
                    .iter()
                    .filter_map(|v| v["id"].as_str().map(str::to_owned))
                    .collect();
                input["grant"] = json!(self.natives.broker.grant(
                    &self.playlist_import.selected,
                    &self.playlist_import.targets,
                    &catalogs
                ));
                "import.commit"
            }
            "cancel" | "reset" => {
                self.natives.broker.revoke(&self.playlist_import.selected);
                if action == "cancel" {
                    "import.cancel"
                } else {
                    "import.reset"
                }
            }
            "status" => {
                self.playlist_import.status_input = input.clone();
                "import.status"
            }
            _ => return,
        };
        self.import_request(method, input);
    }
    fn import_request(&mut self, method: &str, mut input: Value) {
        let natives = self.natives.clone();
        let owner = self.playlist_import.selected.clone();
        let method = method.to_string();
        let (tx, rx) = mpsc::channel();
        self.playlist_import.pending = Some(rx);
        self.playlist_import.last_poll = Some(Instant::now());
        std::thread::spawn(move || {
            let result = (|| {
                if let Some(path) = input["file"].as_str() {
                    let path = url::Url::parse(path)
                        .ok()
                        .and_then(|u| u.to_file_path().ok())
                        .ok_or("Choose a local playlist file.")?;
                    use std::io::Read;
                    let file = std::fs::File::open(&path)
                        .map_err(|_| "The selected file could not be opened.")?;
                    let mut bytes = Vec::new();
                    file.take(16 * 1024 * 1024 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|_| "The selected file could not be read.")?;
                    if bytes.len() > 16 * 1024 * 1024 {
                        return Err("Playlist files must be at most 16 MiB.".into());
                    }
                    input["filename"] =
                        json!(path.file_name().unwrap_or_default().to_string_lossy());
                    input["bytes"] = json!(base64::engine::general_purpose::STANDARD.encode(bytes));
                    input.as_object_mut().unwrap().remove("file");
                }
                natives.call(&owner, &method, input, Duration::from_secs(45))
            })();
            let _ = tx.send((method, result));
        });
    }
}
