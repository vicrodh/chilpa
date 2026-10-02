//! Evidence used to assess a foreign plugin before enabling it.
//!
//! Adapters supply requirements and observations. The evaluator knows no
//! provider names, endpoints or package formats. A compatibility result is
//! neither a permission grant nor a security verdict.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A revision identifies both sides of an assessment. Old observations must
/// not certify a replacement package or an updated adapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub package_digest: String,
    pub adapter_digest: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Light {
    Green,
    Yellow,
    Red,
}

/// Results recorded by the host, never read from an untrusted repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub revision: Revision,
    pub operation: String,
    pub passed: bool,
}

/// Names are adapter contracts, not service identities. The adapter must
/// report unknown or dynamic requirements as incomplete inspection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Requirements {
    pub inspected: bool,
    pub required: BTreeSet<String>,
    pub optional: BTreeSet<String>,
    pub operations: BTreeSet<String>,
}

/// The host's current implementation, including deployment availability.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Support {
    pub available: bool,
    pub interfaces: BTreeSet<String>,
}

/// Structured reasons let the facade translate the assessment without
/// parsing diagnostic text. Empty required operations cannot earn green.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assessment {
    pub light: Light,
    pub runtime_available: bool,
    pub inspection_complete: bool,
    pub missing_required: Vec<String>,
    pub missing_optional: Vec<String>,
    pub failed_operations: Vec<String>,
    pub untested_operations: Vec<String>,
}

pub fn assess(
    revision: &Revision,
    requirements: &Requirements,
    support: &Support,
    observations: &[Observation],
) -> Assessment {
    let missing_required: Vec<_> =
        requirements.required.difference(&support.interfaces).cloned().collect();
    let missing_optional: Vec<_> =
        requirements.optional.difference(&support.interfaces).cloned().collect();
    let mut failed_operations = Vec::new();
    let mut untested_operations = Vec::new();
    for operation in &requirements.operations {
        // The latest observation for these exact bytes wins. A later failed
        // call must invalidate a successful smoke from an earlier session.
        match observations.iter().rev().find(|o| {
            &o.revision == revision && &o.operation == operation
        }) {
            Some(o) if o.passed => {}
            Some(_) => failed_operations.push(operation.clone()),
            None => untested_operations.push(operation.clone()),
        }
    }
    let light = if !support.available || !missing_required.is_empty() || !failed_operations.is_empty() {
        Light::Red
    } else if !requirements.inspected
        || requirements.operations.is_empty()
        || !missing_optional.is_empty()
        || !untested_operations.is_empty()
        || revision.package_digest.is_empty()
        || revision.adapter_digest.is_empty()
    {
        Light::Yellow
    } else {
        Light::Green
    };
    Assessment {
        light,
        runtime_available: support.available,
        inspection_complete: requirements.inspected,
        missing_required,
        missing_optional,
        failed_operations,
        untested_operations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn revision() -> Revision {
        Revision { package_digest: "package-a".into(), adapter_digest: "adapter-a".into() }
    }

    fn requirements() -> Requirements {
        Requirements {
            inspected: true,
            required: BTreeSet::from(["http".into()]),
            operations: BTreeSet::from(["browse".into(), "resolve".into()]),
            ..Requirements::default()
        }
    }

    fn support() -> Support {
        Support { available: true, interfaces: BTreeSet::from(["http".into()]) }
    }

    fn observations() -> Vec<Observation> {
        ["browse", "resolve"].into_iter().map(|operation| Observation {
            revision: revision(), operation: operation.into(), passed: true,
        }).collect()
    }

    #[test]
    fn a_package_cannot_certify_itself_from_its_manifest() {
        let result = assess(&revision(), &requirements(), &support(), &[]);
        assert_eq!(result.light, Light::Yellow);
        assert_eq!(result.untested_operations, ["browse", "resolve"]);
    }

    #[test]
    fn green_requires_the_current_package_and_adapter() {
        let mut revision = revision();
        assert_eq!(assess(&revision, &requirements(), &support(), &observations()).light, Light::Green);
        revision.package_digest = "package-b".into();
        assert_eq!(assess(&revision, &requirements(), &support(), &observations()).light, Light::Yellow);
        revision.package_digest = "package-a".into();
        revision.adapter_digest = "adapter-b".into();
        assert_eq!(assess(&revision, &requirements(), &support(), &observations()).light, Light::Yellow);
    }

    #[test]
    fn a_placeholder_cannot_earn_green() {
        let requirements = Requirements { inspected: true, ..Requirements::default() };
        assert_eq!(assess(&revision(), &requirements, &support(), &[]).light, Light::Yellow);
    }

    #[test]
    fn unsupported_requirements_and_missing_runtimes_are_red() {
        let mut support = support();
        support.interfaces.clear();
        let result = assess(&revision(), &requirements(), &support, &observations());
        assert_eq!(result.light, Light::Red);
        assert_eq!(result.missing_required, ["http"]);
        support.interfaces.insert("http".into());
        support.available = false;
        assert_eq!(assess(&revision(), &requirements(), &support, &observations()).light, Light::Red);
    }

    #[test]
    fn incomplete_inspection_and_optional_gaps_remain_yellow() {
        let mut requirements = requirements();
        requirements.inspected = false;
        assert_eq!(assess(&revision(), &requirements, &support(), &observations()).light, Light::Yellow);
        requirements.inspected = true;
        requirements.optional.insert("scheduler".into());
        let result = assess(&revision(), &requirements, &support(), &observations());
        assert_eq!(result.light, Light::Yellow);
        assert_eq!(result.missing_optional, ["scheduler"]);
    }

    #[test]
    fn a_later_failure_invalidates_the_smoke_until_retested() {
        let mut observations = observations();
        observations.push(Observation { revision: revision(), operation: "resolve".into(), passed: false });
        assert_eq!(assess(&revision(), &requirements(), &support(), &observations).light, Light::Red);
        observations.push(Observation { revision: revision(), operation: "resolve".into(), passed: true });
        assert_eq!(assess(&revision(), &requirements(), &support(), &observations).light, Light::Green);
    }
}
