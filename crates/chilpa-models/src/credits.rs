//! I keep external contributions separate from a file's tags and artists.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contributor {
    /// The provider's stable artist identifier, including groups.
    pub id: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreditRole {
    Producer,
    RecordingEngineer,
    Engineer,
    MixEngineer,
    MasteringEngineer,
    Arranger,
    Writer,
    Composer,
    Lyricist,
    Performer,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreditScope { Recording, Work, Release }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contribution {
    pub contributor: Contributor,
    /// The name printed on this particular credit.
    pub credited_name: String,
    pub role: CreditRole,
    /// I preserve the original relationship, including roles I cannot name.
    pub relation: String,
    pub attributes: Vec<String>,
    pub scope: CreditScope,
    pub entity_id: String,
    pub entity_title: String,
}
