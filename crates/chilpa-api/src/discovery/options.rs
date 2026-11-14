//! Small, version-independent browsing preferences; never part of tag data.
#[derive(Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Options {
    pub order: String,
    pub scope: String,
    pub excluded_sources: Option<Vec<String>>,
}
impl Options {
    pub fn parse(value: &str) -> Option<Self> {
        if value.len() > 65536 { return None; }
        serde_json::from_str::<Self>(value).ok().filter(|options| options.excluded_sources.as_ref().is_none_or(|keys| valid_sources(keys)))
    }
    pub fn descending(&self) -> bool { self.order == "name-desc" }
}

pub(super) fn valid_sources(keys: &[String]) -> bool { keys.len() <= 256 && keys.iter().all(|key| key.len() <= 512) }
