//! Small, version-independent browsing preferences; never part of tag data.
#[derive(Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Options {
    pub order: String,
    pub scope: String,
}
impl Options {
    pub fn parse(value: &str) -> Self {
        if value.len() > 512 { return Self::default(); }
        serde_json::from_str(value).unwrap_or_default()
    }
    pub fn descending(&self) -> bool { self.order == "name-desc" }
}
