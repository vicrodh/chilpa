use crate::{invalid, Result};
use serde::{Deserialize, Deserializer, Serialize};

/// A portable, slash-separated path relative to the selected sync root.
/// I validate on deserialization as well as at the transport boundary.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct RelativePath(String);

impl RelativePath {
    pub fn new(text: impl Into<String>) -> Result<Self> {
        let text = text.into();
        if text.is_empty() || text.len() > 1024 || text.split('/').any(|s| !valid_component(s)) {
            return Err(invalid("invalid portable device path"));
        }
        Ok(Self(text))
    }
    pub fn as_str(&self) -> &str { &self.0 }
    pub fn parent(&self) -> Option<Self> { self.0.rsplit_once('/').map(|(p, _)| Self(p.into())) }
    pub fn filename(&self) -> &str { self.0.rsplit('/').next().unwrap() }
}
impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D: Deserializer<'de>>(de: D) -> std::result::Result<Self, D::Error> {
        Self::new(String::deserialize(de)?).map_err(serde::de::Error::custom)
    }
}
impl std::fmt::Display for RelativePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}

fn reserved(text: &str) -> bool {
    let stem = text.split('.').next().unwrap_or("").to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|p| stem.strip_prefix(p).is_some_and(|s| s.len() == 1 && matches!(s.as_bytes()[0], b'1'..=b'9')))
}
fn valid_component(text: &str) -> bool {
    !text.is_empty() && text != "." && text != ".." && text.len() <= 240
        && !text.ends_with(['.', ' ']) && !reserved(text)
        && !text.chars().any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
}

/// I reserve room for an identity suffix instead of depending on title uniqueness.
pub fn component(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let c = if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c };
        if out.len() + c.len_utf8() > 120 { break; }
        out.push(c);
    }
    let out = out.trim().trim_end_matches('.');
    if out.is_empty() || out == "." || out == ".." { return "Unknown".into(); }
    if reserved(out) { format!("_{out}") } else { out.into() }
}
