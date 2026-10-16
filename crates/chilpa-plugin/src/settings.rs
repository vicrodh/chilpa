//! A shared form contract for adapters. Guest markup never reaches QML.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_FIELDS: usize = 128;
pub const MAX_VALUE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub label: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Control {
    Text { max_length: usize },
    Secret { max_length: usize },
    Toggle,
    Number { integer: bool, min: Option<f64>, max: Option<f64> },
    Select { choices: Vec<Choice> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub key: String,
    pub label: String,
    pub description: String,
    pub required: bool,
    pub control: Control,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub key: String,
    pub label: String,
    /// Only these fields may accompany this action. An authentication
    /// action cannot accidentally receive unrelated account settings.
    pub fields: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Form {
    pub fields: Vec<Field>,
    pub actions: Vec<Action>,
    /// The adapter preserves things it could not translate. They prevent a
    /// complete assessment instead of silently disappearing from the form.
    pub unsupported: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormError {
    TooLarge,
    InvalidDefinition(String),
    UnknownAction,
    UnknownField(String),
    MissingField(String),
    InvalidValue(String),
}

impl Form {
    /// Checks untrusted definitions before exposing a form or accepting a
    /// submission. Errors carry field names, never submitted secret values.
    pub fn validate(&self) -> Result<(), FormError> {
        if self.fields.len() > MAX_FIELDS || self.actions.len() > MAX_FIELDS {
            return Err(FormError::TooLarge);
        }
        let mut fields = BTreeSet::new();
        for field in &self.fields {
            if !key_valid(&field.key) || !fields.insert(field.key.clone()) {
                return Err(FormError::InvalidDefinition(field.key.clone()));
            }
            let valid = match &field.control {
                Control::Text { max_length } | Control::Secret { max_length } => {
                    *max_length > 0 && *max_length <= MAX_VALUE_BYTES
                }
                Control::Number { min, max, .. } => {
                    min.is_none_or(f64::is_finite) && max.is_none_or(f64::is_finite)
                        && !matches!((min, max), (Some(a), Some(b)) if a > b)
                }
                Control::Select { choices } => {
                    !choices.is_empty() && choices.len() <= MAX_FIELDS
                        && choices.iter().enumerate().all(|(index, choice)| {
                            !choice.value.is_array() && !choice.value.is_object()
                                && !choices[..index].iter().any(|other| other.value == choice.value)
                        })
                }
                Control::Toggle => true,
            };
            if !valid {
                return Err(FormError::InvalidDefinition(field.key.clone()));
            }
        }
        let mut actions = BTreeSet::new();
        for action in &self.actions {
            if !key_valid(&action.key) || !actions.insert(&action.key)
                || !action.fields.is_subset(&fields)
            {
                return Err(FormError::InvalidDefinition(action.key.clone()));
            }
        }
        Ok(())
    }

    /// The plugin performs its own semantic validation after this structural
    /// check. This method neither persists values nor starts authentication.
    pub fn validate_submission(
        &self,
        action: &str,
        values: &BTreeMap<String, Value>,
    ) -> Result<(), FormError> {
        self.validate()?;
        let action = self.actions.iter().find(|item| item.key == action)
            .ok_or(FormError::UnknownAction)?;
        if serde_json::to_vec(values).map_err(|_| FormError::TooLarge)?.len() > MAX_VALUE_BYTES {
            return Err(FormError::TooLarge);
        }
        for key in values.keys() {
            if !action.fields.contains(key) {
                return Err(FormError::UnknownField(key.clone()));
            }
        }
        for field in self.fields.iter().filter(|field| action.fields.contains(&field.key)) {
            let Some(value) = values.get(&field.key) else {
                if field.required {
                    return Err(FormError::MissingField(field.key.clone()));
                }
                continue;
            };
            if !field.accepts(value) {
                return Err(FormError::InvalidValue(field.key.clone()));
            }
        }
        Ok(())
    }

    /// Secret values never round trip through a UI snapshot, including
    /// after an unsuccessful submit.
    pub fn public_values(&self, values: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
        self.fields.iter().filter_map(|field| {
            if matches!(field.control, Control::Secret { .. }) {
                return None;
            }
            values.get(&field.key).map(|value| (field.key.clone(), value.clone()))
        }).collect()
    }
}

impl Field {
    fn accepts(&self, value: &Value) -> bool {
        match &self.control {
            Control::Text { max_length } | Control::Secret { max_length } => {
                value.as_str().is_some_and(|s| {
                    s.len() <= *max_length && (!self.required || !s.is_empty())
                })
            }
            Control::Toggle => value.is_boolean(),
            Control::Number { integer, min, max } => value.as_f64().is_some_and(|n| {
                n.is_finite() && (!integer || value.is_i64() || value.is_u64())
                    && min.is_none_or(|bound| n >= bound)
                    && max.is_none_or(|bound| n <= bound)
            }),
            Control::Select { choices } => choices.iter().any(|choice| &choice.value == value),
        }
    }
}

fn key_valid(key: &str) -> bool {
    !key.is_empty() && key.len() <= 256 && !key.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn form() -> Form {
        Form {
            fields: vec![Field {
                key: "password".into(), label: "Password".into(), description: String::new(),
                required: true, control: Control::Secret { max_length: 256 },
            }],
            actions: vec![Action { key: "connect".into(), label: "Connect".into(), fields: BTreeSet::from(["password".into()]) }],
            ..Form::default()
        }
    }

    #[test]
    fn credentials_are_accepted_for_the_action_but_never_echoed() {
        let values = BTreeMap::from([("password".into(), json!("private value"))]);
        let form = form();
        assert!(form.validate_submission("connect", &values).is_ok());
        assert!(form.public_values(&values).is_empty());
        let mut invalid = values;
        invalid.insert("password".into(), json!(["private value"]));
        let error = form.validate_submission("connect", &invalid).unwrap_err();
        assert_eq!(error, FormError::InvalidValue("password".into()));
        assert!(!format!("{error:?}").contains("private value"));
    }

    #[test]
    fn submissions_cannot_add_fields_or_invent_actions() {
        let form = form();
        let values = BTreeMap::from([("other".into(), json!("value"))]);
        assert_eq!(form.validate_submission("connect", &values), Err(FormError::UnknownField("other".into())));
        assert_eq!(form.validate_submission("execute", &values), Err(FormError::UnknownAction));
        assert_eq!(form.validate_submission("connect", &BTreeMap::new()), Err(FormError::MissingField("password".into())));
    }

    #[test]
    fn duplicate_fields_and_unbound_actions_are_invalid() {
        let mut form = form();
        form.fields.push(form.fields[0].clone());
        assert!(form.validate().is_err());
        form.fields.pop();
        form.actions[0].fields.insert("missing".into());
        assert!(form.validate().is_err());
    }

    #[test]
    fn a_selection_preserves_the_original_value_type() {
        let field = Field {
            key: "quality".into(), label: String::new(), description: String::new(), required: true,
            control: Control::Select { choices: vec![Choice { label: "High".into(), value: json!(27) }] },
        };
        assert!(field.accepts(&json!(27)));
        assert!(!field.accepts(&json!("27")));
        assert!(!field.accepts(&json!(999)));
    }
}
