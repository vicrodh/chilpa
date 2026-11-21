//! Candidate projections reuse the adapter's explicit file-to-release mapping.
use super::{
    fields::{self, Patch},
    lookup,
    model::Row,
    plans::{PlanRow, Targets},
    worker::Session,
};
use chilpa_meta::beets::Candidate;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    candidate_key: String,
    group_key: String,
    data_revision: u64,
    targets: Targets,
    fields: Vec<String>,
    #[serde(default = "add_genres")]
    genres_operation: String,
}
fn add_genres() -> String {
    "append_unique".into()
}
const IDENTITY: [&str; 8] = [
    "musicbrainz_release_id",
    "musicbrainz_release_group_id",
    "musicbrainz_recording_id",
    "musicbrainz_track_id",
    "disc_number",
    "disc_total",
    "track_number",
    "track_total",
];

#[derive(Default)]
struct Choice {
    current: Option<Value>,
    proposed: Option<Value>,
    current_mixed: bool,
    proposed_mixed: bool,
    invalid: String,
}
impl Choice {
    fn observe(&mut self, current: &Value, proposed: &Value, invalid: &str) {
        self.current_mixed |= self.current.as_ref().is_some_and(|held| held != current);
        self.proposed_mixed |= self.proposed.as_ref().is_some_and(|held| held != proposed);
        self.current.get_or_insert_with(|| current.clone());
        self.proposed.get_or_insert_with(|| proposed.clone());
        if !invalid.is_empty() {
            self.invalid = invalid.into();
        }
    }
}
fn display(value: &Value) -> String {
    let text = match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("; "),
        value => value.to_string(),
    };
    lookup::summary_text(&text)
}

fn values(candidate: &Candidate, index: usize) -> Map<String, Value> {
    let release = &candidate.release;
    let track = &release.tracks[index];
    let credits = if track.credits.is_empty() {
        &release.credits
    } else {
        &track.credits
    };
    let line = |credits: &[chilpa_meta::beets::Credit]| {
        credits
            .iter()
            .map(|credit| format!("{}{}", credit.name, credit.join))
            .collect::<String>()
            .trim()
            .to_string()
    };
    let names = |credits: &[chilpa_meta::beets::Credit]| {
        credits
            .iter()
            .map(|credit| credit.name.clone())
            .collect::<Vec<_>>()
    };
    let ids = |credits: &[chilpa_meta::beets::Credit]| {
        credits
            .iter()
            .filter(|credit| !credit.id.is_empty())
            .map(|credit| credit.id.clone())
            .collect::<Vec<_>>()
    };
    let total = release
        .tracks
        .iter()
        .filter(|other| other.disc == track.disc)
        .count();
    let disc_total = release.discs.max(
        release
            .tracks
            .iter()
            .map(|track| track.disc)
            .max()
            .unwrap_or(0),
    );
    json!({"album":release.title,"album_artist":line(&release.credits),"album_artists":names(&release.credits),
        "date":release.date,"genres":release.genres,"label":release.label,"catalog_number":release.catalog_number,
        "title":track.title,"artist_credit":line(credits),"artists":names(credits),"track_number":track.number,"disc_number":track.disc,
        "track_total":total,"disc_total":disc_total,"disc_subtitle":release.media_titles.get(track.disc.saturating_sub(1) as usize),
        "musicbrainz_release_id":release.id,"musicbrainz_release_group_id":release.release_group_id,
        "musicbrainz_album_artist_ids":ids(&release.credits),"musicbrainz_artist_ids":ids(credits),
        "musicbrainz_recording_id":track.recording_id,"musicbrainz_track_id":track.track_id}).as_object().unwrap().clone()
}
fn has_value(value: &Value) -> bool {
    !value.is_null()
        && value.as_str() != Some("")
        && value.as_array().is_none_or(|values| !values.is_empty())
        && value.as_u64() != Some(0)
}

impl Session {
    pub fn candidate(&self, request: &Request) -> Result<(), String> {
        self.check_revision(request.data_revision)?;
        let group = self.group(&request.group_key)?;
        if request.fields.len() > 64
            || !matches!(request.genres_operation.as_str(), "set" | "append_unique")
        {
            return Err(chilpa_i18n::t(
                "Choose supported fields and a genre operation.",
            ));
        }
        let fields: BTreeSet<_> = request.fields.iter().map(String::as_str).collect();
        if fields
            .iter()
            .any(|field| fields::definition(field).is_none())
        {
            return Err(chilpa_i18n::t(
                "The candidate includes an unsupported metadata field.",
            ));
        }
        let body = self
            .manifest
            .document("candidate", &request.candidate_key)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                chilpa_i18n::t("Look up this group again to choose a current candidate.")
            })?;
        let body: Value = serde_json::from_str(&body).map_err(|error| error.to_string())?;
        if body["group"] != group {
            return Err(chilpa_i18n::t(
                "The candidate belongs to another metadata group.",
            ));
        }
        let candidate: Candidate =
            serde_json::from_value(body["candidate"].clone()).map_err(|error| error.to_string())?;
        let evidence = body["evidence"]
            .as_object()
            .ok_or_else(|| chilpa_i18n::t("The metadata lookup has no saved selection."))?;
        let targets = self.target_ids(&request.targets)?;
        if targets
            .as_ref()
            .is_some_and(|ids| ids.iter().any(|id| !evidence.contains_key(&id.to_string())))
        {
            return Err(chilpa_i18n::t(
                "Some selected files were outside this lookup. Look up the selected group again.",
            ));
        }
        let mappings: HashMap<_, _> = candidate
            .mappings
            .iter()
            .map(|mapping| (mapping.track, mapping))
            .collect();
        let mut choices = BTreeMap::<String, Choice>::new();
        let mut required = BTreeSet::new();
        let mut files = 0;
        let mut changed = 0;
        let mut unmatched = 0;
        let mut errors = BTreeSet::new();
        let operation = || {
            self.manifest
                .clear_plan()
                .map_err(|error| error.to_string())?;
            for (key, expected) in evidence {
                let id = key.parse::<i64>().map_err(|error| error.to_string())?;
                let entry = self
                    .manifest
                    .entry(id)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| {
                        chilpa_i18n::t("A lookup file is no longer in the current selection.")
                    })?;
                let row = Row::read(&entry)?;
                if lookup::evidence(&row)? != expected.as_str().unwrap_or_default() {
                    return Err(chilpa_i18n::t(
                        "Metadata changed after lookup. Look up this group again.",
                    ));
                }
                if !targets
                    .as_ref()
                    .map_or(entry.selected, |ids| ids.contains(&id))
                {
                    continue;
                }
                files += 1;
                let Some(mapping) = mappings
                    .get(&id)
                    .filter(|mapping| mapping.index < candidate.release.tracks.len())
                else {
                    unmatched += 1;
                    let missing = PlanRow {
                        patches: Vec::new(),
                        membership: None,
                        issue: chilpa_i18n::t(
                            "This candidate has no matching track for this file.",
                        ),
                    };
                    self.manifest
                        .put_plan_entry(
                            id,
                            &serde_json::to_string(&missing).map_err(|error| error.to_string())?,
                        )
                        .map_err(|error| error.to_string())?;
                    continue;
                };
                let before = row.proposed();
                let mut proposed = values(&candidate, mapping.index);
                if !mapping.composers.is_empty() {
                    proposed.insert("composers".into(), json!(mapping.composers));
                }
                let title = candidate.release.tracks[mapping.index].title.as_str();
                if let Some(work) = chilpa_library::work_titles::proposed_work(&mapping.work, title) {
                    proposed.insert("work".into(), json!(work));
                }
                let identity_requested = fields
                    .iter()
                    .any(|field| field.starts_with("musicbrainz_") && IDENTITY.contains(field));
                let identified = before
                    .get("musicbrainz_release_id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !id.is_empty());
                let positions = fields.iter().any(|field| IDENTITY.contains(field));
                if identity_requested || identified && positions {
                    let mut positions = std::collections::HashSet::new();
                    if candidate.release.tracks.iter().any(|track| {
                        track.disc == 0
                            || track.number == 0
                            || track.disc > candidate.release.discs
                            || !positions.insert((track.disc, track.number))
                    }) {
                        errors.insert(chilpa_i18n::t(
                            "This candidate contains inconsistent disc or track positions.",
                        ));
                    }
                    for field in IDENTITY {
                        let value = proposed.get(field).unwrap_or(&Value::Null);
                        let validation = Patch {
                            field: field.into(),
                            operation: "set".into(),
                            value: value.clone(),
                            group_key: None,
                        }
                        .validate();
                        if !has_value(value) || validation.is_err() {
                            errors.insert(chilpa_i18n::t("This candidate does not contain a complete release identity and disc mapping."));
                        }
                        if before.get(field).unwrap_or(&Value::Null) != value
                            && !fields.contains(field)
                        {
                            required.insert(field);
                        }
                    }
                }
                let mut patches = Vec::new();
                for (field, value) in proposed.into_iter().filter(|(_, value)| has_value(value)) {
                    let patch = Patch {
                        field: field.clone(),
                        operation: if field == "genres" {
                            request.genres_operation.clone()
                        } else {
                            "set".into()
                        },
                        value,
                        group_key: None,
                    };
                    let current = before.get(&field).unwrap_or(&Value::Null);
                    let after = patch.result(current);
                    if after == *current {
                        continue;
                    }
                    let invalid = patch.validate().err().unwrap_or_else(|| {
                        if !row.placed.contains(&field) {
                            chilpa_i18n::t("This field cannot be written to every selected file.")
                        } else {
                            String::new()
                        }
                    });
                    choices
                        .entry(field.clone())
                        .or_default()
                        .observe(current, &after, &invalid);
                    if fields.contains(field.as_str()) {
                        if !invalid.is_empty() {
                            errors.insert(invalid);
                        }
                        patches.push(patch);
                    }
                }
                if !patches.is_empty() {
                    changed += 1;
                    let plan = PlanRow {
                        patches,
                        membership: None,
                        issue: String::new(),
                    };
                    self.manifest
                        .put_plan_entry(
                            id,
                            &serde_json::to_string(&plan).map_err(|error| error.to_string())?,
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
            if files == 0 {
                return Err(chilpa_i18n::t(
                    "Select files from the captured lookup before preparing this candidate.",
                ));
            }
            if unmatched > 0 {
                errors.insert(chilpa_i18n::t("Some selected files have no mapping in this candidate. Adjust the selection before applying it."));
            }
            if !required.is_empty() {
                errors.insert(chilpa_i18n::t(
                    "Select the required release identity fields together.",
                ));
            }
            let field_choices:Vec<_>=choices.into_iter().map(|(field,choice)| json!({"key":field,"field":field,"label":fields::label(&field),"track":null,"title":"",
                "current":display(choice.current.as_ref().unwrap_or(&Value::Null)),"current_mixed":choice.current_mixed,
                "proposed":display(choice.proposed.as_ref().unwrap_or(&Value::Null)),"proposed_mixed":choice.proposed_mixed,
                "selected":fields.contains(field.as_str()),"invalid":choice.invalid})).collect();
            let mut plan = json!({"kind":"candidate","baseDataRevision":self.revision,"candidateKey":request.candidate_key,"groupKey":request.group_key,
                "title":lookup::summary_text(&candidate.release.title),"genresOperation":request.genres_operation,"canApply":changed>0 && errors.is_empty(),
                "error":errors.into_iter().collect::<Vec<_>>().join(" "),"counts":{"files":files,"previewRows":self.manifest.plan_count().map_err(|error|error.to_string())?,"changed":changed,"unmatched":unmatched,"groupsBefore":1,"groupsAfter":1},
                "requiredFields":required,"fieldChoices":field_choices});
            plan["id"] = json!(self.plan_key(&plan)?);
            self.put_document("plan", &plan)
        };
        self.manifest.transaction(operation)
    }
}
