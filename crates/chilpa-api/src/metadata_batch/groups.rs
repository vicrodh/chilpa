//! Session-scoped group identities and bounded group summaries.
use super::worker::Session;
use serde_json::{Value, json};

impl Session {
    pub fn item_id(&self, key: &str) -> Result<i64, String> {
        key.strip_prefix(&format!("{}:", self.number))
            .and_then(|id| id.parse::<i64>().ok())
            .filter(|id| *id > 0)
            .ok_or_else(|| {
                chilpa_i18n::t("The file selection belongs to another metadata session.")
            })
    }

    pub fn group(&self, key: &str) -> Result<String, String> {
        let group = key
            .strip_prefix(&format!("{}:", self.number))
            .ok_or_else(|| chilpa_i18n::t("The group belongs to another metadata session."))?;
        if !self
            .manifest
            .group_exists(group)
            .map_err(|error| error.to_string())?
        {
            return Err(chilpa_i18n::t(
                "The metadata group is no longer in the current selection.",
            ));
        }
        Ok(group.into())
    }

    pub fn group_view(&self, mut value: Value) -> Result<Value, String> {
        let group = value["key"].as_str().unwrap_or_default();
        let mut discs = self
            .manifest
            .group_discs(group)
            .map_err(|error| error.to_string())?;
        let truncated = discs.len() > 256;
        discs.truncate(256);
        let readable = value["unsupported"].as_u64().unwrap_or(0) == 0;
        let lookup = readable
            && value["selected"]
                .as_u64()
                .is_some_and(|count| count > 0 && count <= 1024);
        value["key"] = json!(format!("{}:{group}", self.number));
        value["location"] = json!(
            std::path::Path::new(value["path"].as_str().unwrap_or_default())
                .parent()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default()
        );
        value["lookup"] = json!({"allowed":lookup,"reason":if lookup { String::new() } else { chilpa_i18n::t("Choose a readable group with at most 1024 files.") }});
        value["join"] = json!({"allowed":readable && !truncated,"reason":""});
        value["split"] = json!({"allowed":readable && discs.len()>1 && !truncated,"reason":""});
        value["discs"] = json!(discs);
        value["discsTruncated"] = json!(truncated);
        if let Some(object) = value.as_object_mut() {
            object.remove("path");
            object.remove("unsupported");
        }
        Ok(value)
    }
}
