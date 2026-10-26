//! I keep declarations, individual cases and live observations separate.
//! One inspection owns at most 256 ranges and 32 attributes; no global cache.
use crate::{format::StreamFormat, negotiate::Delivery};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State { Unknown, Untested, Available, Different, Busy, Permission, Disconnected, Unsupported, Stale, Cancelled, Failed }
impl Default for State { fn default() -> Self { Self::Unknown } }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub backend: String,
    pub id: String,
    pub stable_id: String,
    pub strong: bool,
    pub transport: String,
    /// I bind history to the connection and relevant configuration, not its label.
    pub generation: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope { HardwareDeclaration, ConvertibleInput, DriverAcceptance, Opened, Readback, Audibility }

/// I preserve each published tuple/range; separate maxima never make a tuple.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormatRange {
    pub encoding: String,
    pub rate_min: f64,
    pub rate_max: f64,
    pub useful_bits: u16,
    pub container_bits: u16,
    pub channels: u16,
    pub stream: String,
    pub scope: Scope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribute { pub name: String, pub value: String, pub scope: Scope }

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub formats: Vec<FormatRange>,
    pub exclusive: Option<bool>,
    #[serde(default)]
    pub complete: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Inspection {
    pub identity: Identity,
    pub state: State,
    pub method: String,
    pub formats: Vec<FormatRange>,
    pub attributes: Vec<Attribute>,
    pub observation: Observation,
    pub render_available: bool,
}

impl Inspection {
    pub fn convertible(backend: &str, id: &str, generation: String, caps: &crate::negotiate::DeviceCaps) -> Self {
        let mut doc = Self::unavailable(backend,id,generation);
        doc.state = State::Available;
        doc.method = "system_input_contract".into();
        doc.attributes = vec![
            Attribute { name: "sample_rates".into(), value: caps.sample_rates.iter().map(u32::to_string).collect::<Vec<_>>().join(", "), scope: Scope::ConvertibleInput },
            Attribute { name: "sample_formats".into(), value: format!("{:?}",caps.formats), scope: Scope::ConvertibleInput },
            Attribute { name: "channels".into(), value: caps.channels.iter().map(u16::to_string).collect::<Vec<_>>().join(", "), scope: Scope::ConvertibleInput },
        ]; doc
    }
    pub fn unavailable(backend: &str, id: &str, generation: String) -> Self {
        Self {
            identity: Identity { backend: backend.into(), id: id.into(), stable_id: id.into(), strong: false,
                transport: backend.into(), generation },
            state: State::Unknown, method: String::new(), formats: Vec::new(), attributes: Vec::new(),
            observation: Observation::default(), render_available: true,
        }
    }
    pub fn bound(&mut self) {
        self.formats.retain(|f| f.rate_min.is_finite() && f.rate_max.is_finite()
            && f.rate_min > 0.0 && f.rate_max >= f.rate_min && f.channels > 0);
        self.formats.truncate(256);
        self.attributes.truncate(32);
        self.observation.formats.truncate(32);
    }
}

/// I retain the engine's delivery instead of reconstructing it from a badge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub revision: u64,
    pub device: String,
    pub delivery: Delivery,
    /// I retain planned DSP/gain separately from observations at consumption.
    pub planned_processing: Vec<crate::negotiate::Transform>,
    pub exclusive_requested: bool,
    pub observation: Observation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Case {
    pub identity: Identity,
    pub requested: StreamFormat,
    pub exclusive_requested: bool,
    pub opened: Option<StreamFormat>,
    pub observation: Observation,
    pub state: State,
    #[serde(default)]
    pub physical_format: State,
    pub method: String,
    pub timestamp: u64,
    /// I never treat a successful opening as an auditory confirmation.
    pub audible: State,
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declarations_keep_their_combinations_and_scope() {
        let a = FormatRange { encoding: "pcm".into(), rate_min: 44_100.0, rate_max: 44_100.0,
            useful_bits: 16, container_bits: 16, channels: 2, stream: "1".into(), scope: Scope::HardwareDeclaration };
        let b = FormatRange { rate_min: 96_000.0, rate_max: 96_000.0, useful_bits: 24, container_bits: 32, ..a.clone() };
        let mut doc = Inspection::unavailable("test", "endpoint", "1".into());
        doc.formats = vec![a, b];
        doc.bound();
        assert_eq!(doc.formats.len(), 2);
        assert!(!doc.formats.iter().any(|f| f.rate_min == 96_000.0 && f.useful_bits == 16));
    }
    #[test]
    fn same_names_do_not_make_the_same_identity() {
        let a = Inspection::unavailable("test", "uid-a", "1".into());
        let b = Inspection::unavailable("test", "uid-b", "1".into());
        assert_ne!(a.identity, b.identity);
        let mut reconnected = a.identity.clone(); reconnected.generation = "2".into();
        assert_ne!(a.identity, reconnected);
    }
}
