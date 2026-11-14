//! Projections are stored separately from pending file patches until Apply.
use super::{
    fields::{self, Patch},
    model::Row,
    worker::Session,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Targets {
    pub mode: String,
    #[serde(default)]
    pub keys: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct PlanRow {
    pub patches: Vec<Patch>,
    #[serde(default)]
    pub membership: Option<Value>,
    #[serde(default)]
    pub issue: String,
}

impl Session {
    pub fn document(&self, kind: &str) -> Result<Value, String> {
        self.manifest
            .document(kind, "active")
            .map_err(|error| error.to_string())?
            .map(|body| serde_json::from_str(&body).map_err(|error| error.to_string()))
            .unwrap_or(Ok(Value::Null))
    }

    pub fn put_document(&self, kind: &str, value: &Value) -> Result<(), String> {
        let body = serde_json::to_string(value).map_err(|error| error.to_string())?;
        if matches!(kind, "lookup" | "plan") {
            let other_kind = if kind == "lookup" { "plan" } else { "lookup" };
            let other = self
                .manifest
                .document(other_kind, "active")
                .map_err(|error| error.to_string())?;
            if body.len() + other.as_ref().map_or(0, String::len) > 240 * 1024 {
                return Err(chilpa_i18n::t(
                    "This proposal summary exceeds the metadata page limit. Choose fewer groups or fields.",
                ));
            }
        }
        self.manifest
            .put_document(kind, "active", &body)
            .map_err(|error| error.to_string())
    }

    pub fn proposal_row(&self, id: i64, plan: &PlanRow) -> Result<Value, String> {
        let entry = self
            .manifest
            .entry(id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| chilpa_i18n::t("The file is no longer in the current selection."))?;
        let row = Row::read(&entry)?;
        let before = row.proposed();
        let mut changes: Vec<_> = plan.patches.iter().filter_map(|patch| {
            let current = before.get(&patch.field).unwrap_or(&Value::Null);
            let after = patch.result(current);
            (after != *current).then(|| json!({"field":patch.field,"label":fields::label(&patch.field),"before":current,"after":after,"operation":patch.operation}))
        }).collect();
        if let Some(member) = &plan.membership {
            changes.push(json!({"field":"presentation","label":chilpa_i18n::t("Album grouping"),"before":row.text("album"),"after":member["title"],"operation":"set"}));
        }
        Ok(
            json!({"key":format!("{}:{id}",self.number),"title":row.text("title"),"path":row.path,"album":row.text("album"),
            "selected":true,"changes":changes,"status":if plan.issue.is_empty() {"proposal"} else {"unmatched"},"error":plan.issue,"statusLabel":if plan.issue.is_empty() {chilpa_i18n::t("Proposed")} else {chilpa_i18n::t("Unmatched file")}}),
        )
    }

    pub fn apply_plan(&self, id: &str) -> Result<(), String> {
        let plan = self.document("plan")?;
        if plan["id"].as_str() != Some(id)
            || plan["baseDataRevision"].as_u64() != Some(self.revision)
            || plan["canApply"] != true
        {
            return Err(chilpa_i18n::t(
                "Review the current proposal before applying it.",
            ));
        }
        self.manifest.transaction(|| {
            let mut after = 0;
            while let Some((id, body)) = self
                .manifest
                .next_plan_entry(after)
                .map_err(|error| error.to_string())?
            {
                after = id;
                let projection: PlanRow =
                    serde_json::from_str(&body).map_err(|error| error.to_string())?;
                let entry = self
                    .manifest
                    .entry(id)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| {
                        chilpa_i18n::t("The file is no longer in the current selection.")
                    })?;
                let mut row = Row::read(&entry)?;
                for patch in projection.patches {
                    let current = row
                        .proposed()
                        .get(&patch.field)
                        .cloned()
                        .unwrap_or(Value::Null);
                    let after = patch.result(&current);
                    let applied = Patch {
                        field: patch.field,
                        operation: if after.is_null() {
                            "clear".into()
                        } else {
                            "set".into()
                        },
                        value: after,
                        group_key: None,
                    };
                    row.patch(&applied)?;
                }
                if projection.membership.is_some() {
                    if matches!(row.phase.as_str(), "overlay" | "reconcile") {
                        return Err(chilpa_i18n::t(
                            "Finish the pending save before applying this proposal.",
                        ));
                    }
                    row.phase = "tags".into();
                    row.file_written = false;
                    row.overlay_written = false;
                    row.overlay_evidence = None;
                    row.error.clear();
                    row.membership = projection.membership;
                }
                if row.membership.is_some() {
                    row.status = "changed".into();
                }
                self.manifest
                    .update(&row.entry(id)?)
                    .map_err(|error| error.to_string())?;
            }
            self.manifest
                .clear_plan()
                .map_err(|error| error.to_string())
        })
    }

    pub fn plan_key(&self, value: &Value) -> Result<String, String> {
        use sha2::{Digest, Sha256};
        let body = serde_json::to_vec(value).map_err(|error| error.to_string())?;
        Ok(format!("{}:plan:{:x}", self.number, Sha256::digest(body)))
    }

    pub fn check_revision(&self, revision: u64) -> Result<(), String> {
        if revision != self.revision {
            return Err(chilpa_i18n::t(
                "The selection changed. Review the current metadata before preparing this proposal.",
            ));
        }
        Ok(())
    }

    pub fn target_ids(
        &self,
        targets: &Targets,
    ) -> Result<Option<std::collections::BTreeSet<i64>>, String> {
        match targets.mode.as_str() {
            "selected" if targets.keys.is_empty() => Ok(None),
            "keys" if targets.keys.len() <= 10000 => targets
                .keys
                .iter()
                .map(|key| self.item_id(key))
                .collect::<Result<_, _>>()
                .map(Some),
            _ => Err(chilpa_i18n::t(
                "Choose files from the current metadata selection.",
            )),
        }
    }
}
