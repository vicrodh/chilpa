//! Explicit disc assignments use the on-disk manifest and retain real identities.
use super::{
    fields::Patch,
    lookup::summary_text,
    model::Row,
    plans::{PlanRow, Targets},
    worker::Session,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Disc {
    #[serde(default)]
    group_key: Option<String>,
    #[serde(default)]
    source_disc: Option<u32>,
    #[serde(default)]
    keys: Vec<String>,
    number: u32,
    #[serde(default)]
    title: String,
    #[serde(default)]
    album: Option<String>,
    #[serde(default = "keep")]
    track_number_mode: String,
    #[serde(default)]
    track_numbers: BTreeMap<String, u32>,
}
fn keep() -> String {
    "keep".into()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    operation: String,
    data_revision: u64,
    groups: Vec<String>,
    targets: Targets,
    #[serde(default)]
    album: String,
    #[serde(default)]
    album_artist: String,
    #[serde(default)]
    renumber_discs: bool,
    discs: Vec<Disc>,
}
fn text(values: &serde_json::Map<String, Value>, field: &str) -> String {
    values
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into()
}
fn number(values: &serde_json::Map<String, Value>, field: &str) -> u32 {
    values
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
}
fn valid_title(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 512
}

impl Session {
    pub fn group_plan(&self, request: &Request, cancel: &AtomicBool) -> Result<(), String> {
        self.check_revision(request.data_revision)?;
        let joining = request.operation == "join";
        if !matches!(request.operation.as_str(), "join" | "split")
            || request.groups.is_empty()
            || request.groups.len() > 256
            || request.discs.is_empty()
            || request.discs.len() > 256
            || joining && !valid_title(&request.album)
            || request.album_artist.chars().count() > 512
        {
            return Err(chilpa_i18n::t(
                "Choose albums, disc assignments and a resulting album title.",
            ));
        }
        let groups = request
            .groups
            .iter()
            .map(|key| self.group(key))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let targets = self.target_ids(&request.targets)?;
        let mut selectors = Vec::new();
        let mut target_discs = BTreeSet::new();
        for disc in &request.discs {
            if disc.number == 0
                || disc.number > 9999
                || disc.title.chars().count() > 512
                || !matches!(disc.track_number_mode.as_str(), "keep" | "sequence")
                || disc.keys.len() > 10000
                || disc.track_numbers.len() > 10000
                || disc.album.as_ref().is_some_and(|title| !valid_title(title))
            {
                return Err(chilpa_i18n::t(
                    "This disc assignment contains invalid titles or positions.",
                ));
            }
            if joining && !target_discs.insert(disc.number) {
                return Err(chilpa_i18n::t(
                    "Assign a distinct target disc number to each joined disc.",
                ));
            }
            let group = disc
                .group_key
                .as_deref()
                .map(|key| self.group(key))
                .transpose()?;
            let keys = disc
                .keys
                .iter()
                .map(|key| self.item_id(key))
                .collect::<Result<BTreeSet<_>, _>>()?;
            if keys.is_empty() && (group.is_none() || disc.source_disc.is_none())
                || !keys.is_empty() && (group.is_some() || disc.source_disc.is_some())
                || group.as_ref().is_some_and(|group| !groups.contains(group))
            {
                return Err(chilpa_i18n::t(
                    "Assign each disc using its group and source disc, or explicit files.",
                ));
            }
            for (key, position) in &disc.track_numbers {
                if !keys.contains(&self.item_id(key)?) || *position == 0 || *position > 9999 {
                    return Err(chilpa_i18n::t(
                        "Explicit track numbers must belong to this disc's selected files.",
                    ));
                }
            }
            selectors.push((group, keys));
        }
        let operation = || {
            self.manifest
                .clear_plan()
                .map_err(|error| error.to_string())?;
            let mut after = 0;
            let mut files = 0;
            let mut known_totals = vec![(0u32, 0u32); request.discs.len()];
            let mut known_disc_total = 0u32;
            let mut assigned_keys = BTreeSet::new();
            let mut releases = BTreeSet::new();
            let mut release_groups = BTreeSet::new();
            let mut errors = BTreeSet::new();
            let mut input_hash = sha2::Sha256::default();
            use sha2::Digest;
            while let Some(entry) = self
                .manifest
                .next(after)
                .map_err(|error| error.to_string())?
            {
                after = entry.id;
                if cancel.load(Ordering::Acquire) {
                    return Err(chilpa_i18n::t("The disc proposal was cancelled."));
                }
                if !groups.contains(&entry.group)
                    || !targets
                        .as_ref()
                        .map_or(entry.selected, |ids| ids.contains(&entry.id))
                {
                    continue;
                }
                if entry.state == "unsupported" {
                    return Err(chilpa_i18n::t("Choose readable files for the disc plan."));
                }
                let row = Row::read(&entry)?;
                if matches!(row.phase.as_str(), "overlay" | "reconcile") {
                    return Err(chilpa_i18n::t(
                        "Finish pending saves before preparing a disc plan.",
                    ));
                }
                let values = row.proposed();
                let mut matched = None;
                for (index, (group, keys)) in selectors.iter().enumerate() {
                    if keys.contains(&entry.id)
                        || group.as_ref() == Some(&entry.group)
                            && request.discs[index].source_disc
                                == Some(number(&values, "disc_number"))
                    {
                        if matched.replace(index).is_some() {
                            return Err(chilpa_i18n::t(
                                "A file appears in more than one disc assignment.",
                            ));
                        }
                    }
                }
                let index = matched.ok_or_else(|| {
                    chilpa_i18n::t("Assign every selected file to one resulting disc.")
                })?;
                let total = number(&values, "track_total");
                if total > 0 {
                    let (minimum, maximum) = &mut known_totals[index];
                    *minimum = if *minimum == 0 {
                        total
                    } else {
                        (*minimum).min(total)
                    };
                    *maximum = (*maximum).max(total);
                }
                known_disc_total = known_disc_total.max(number(&values, "disc_total"));
                let position = number(&values, "track_number");
                self.manifest
                    .assign_disc(
                        entry.id,
                        index,
                        if position == 0 {
                            i64::MAX as u64
                        } else {
                            position as u64
                        },
                        &row.path,
                    )
                    .map_err(|error| error.to_string())?;
                if targets.is_some() || !request.discs[index].keys.is_empty() {
                    assigned_keys.insert(entry.id);
                }
                for (field, set) in [
                    ("musicbrainz_release_id", &mut releases),
                    ("musicbrainz_release_group_id", &mut release_groups),
                ] {
                    let value = text(&values, field);
                    if !value.is_empty() && set.len() < 2 {
                        set.insert(value);
                    }
                }
                input_hash.update(row.path.as_bytes());
                input_hash.update(super::lookup::evidence(&row)?.as_bytes());
                files += 1;
            }
            if files == 0 {
                return Err(chilpa_i18n::t("Select files for the disc plan."));
            }
            if targets
                .as_ref()
                .is_some_and(|keys| !keys.is_subset(&assigned_keys))
                || selectors
                    .iter()
                    .any(|(_, keys)| !keys.is_subset(&assigned_keys))
            {
                return Err(chilpa_i18n::t(
                    "Some explicit files are outside the selected disc plan.",
                ));
            }
            if joining && (releases.len() > 1 || release_groups.len() > 1) {
                errors.insert(chilpa_i18n::t("These files identify different releases. Choose a real release match before joining them."));
            }
            self.manifest
                .order_assignments()
                .map_err(|error| error.to_string())?;
            let digest = format!("{:x}", input_hash.finalize());
            let identity=self.plan_key(&json!({"operation":request.operation,"input":digest,"discs":request.discs,"album":request.album,"artist":request.album_artist}))?;
            let identity = identity.split(":plan:").nth(1).unwrap_or_default();
            let remaining = self
                .manifest
                .assigned_remaining_groups()
                .map_err(|error| error.to_string())?;
            let total_discs = request
                .discs
                .iter()
                .map(|disc| disc.number)
                .max()
                .unwrap_or(1)
                .max(if remaining.is_empty() {
                    0
                } else {
                    known_disc_total
                });
            let mut results = Vec::new();
            let mut changed = 0;
            for (index, disc) in request.discs.iter().enumerate() {
                let (count, max_position) = self
                    .manifest
                    .assignment_counts(index)
                    .map_err(|error| error.to_string())?;
                if count == 0 {
                    return Err(chilpa_i18n::t("A disc assignment has no selected files."));
                }
                let sequence = disc.track_number_mode == "sequence";
                let track_total = if sequence {
                    count
                } else {
                    max_position.max(count).max(known_totals[index].1 as i64)
                }
                .max(disc.track_numbers.values().copied().max().unwrap_or(0) as i64);
                if !sequence && known_totals[index].0 != known_totals[index].1 {
                    errors.insert(chilpa_i18n::t("The selected files disagree on this disc's track total. Prepare a consistent total or use sequential numbering."));
                }
                if track_total > 9999 {
                    return Err(chilpa_i18n::t(
                        "A disc has too many track positions for this plan.",
                    ));
                }
                let mut after = 0;
                let mut positions = BTreeSet::new();
                let mut display_title = String::new();
                while let Some((id, ordinal)) = self
                    .manifest
                    .next_assignment(index, after)
                    .map_err(|error| error.to_string())?
                {
                    after = id;
                    let entry = self
                        .manifest
                        .entry(id)
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| chilpa_i18n::t("A selected file is missing."))?;
                    let row = Row::read(&entry)?;
                    let before = row.proposed();
                    let title = if joining {
                        request.album.clone()
                    } else {
                        disc.album.clone().unwrap_or_else(|| text(&before, "album"))
                    };
                    if !valid_title(&title) {
                        return Err(chilpa_i18n::t(
                            "Choose a resulting album title for each disc.",
                        ));
                    }
                    display_title = title.clone();
                    let artist = if joining && !request.album_artist.is_empty() {
                        request.album_artist.clone()
                    } else {
                        text(&before, "album_artist")
                    };
                    let old_disc = number(&before, "disc_number");
                    let old_track = number(&before, "track_number");
                    let new_disc = if !joining && request.renumber_discs {
                        1
                    } else {
                        disc.number
                    };
                    let new_track = disc
                        .track_numbers
                        .get(&format!("{}:{id}", self.number))
                        .copied()
                        .unwrap_or(if sequence { ordinal } else { old_track });
                    if new_track == 0 || !positions.insert(new_track) {
                        errors.insert(chilpa_i18n::t("Track positions are missing or duplicated. Choose sequential numbering or explicit positions."));
                    }
                    let has_release = !text(&before, "musicbrainz_release_id").is_empty();
                    if has_release && (new_disc != old_disc || new_track != old_track) {
                        errors.insert(chilpa_i18n::t("Changing the positions of an identified release requires a real release match."));
                    }
                    let mut desired = BTreeMap::new();
                    desired.insert("album", json!(title));
                    if !artist.is_empty() {
                        desired.insert("album_artist", json!(artist));
                    }
                    desired.insert("disc_number", json!(new_disc));
                    if new_track > 0 {
                        desired.insert("track_number", json!(new_track));
                    }
                    if !has_release {
                        desired.insert(
                            "disc_total",
                            json!(if joining {
                                total_discs
                            } else if request.renumber_discs {
                                1
                            } else {
                                number(&before, "disc_total").max(new_disc)
                            }),
                        );
                        desired.insert("track_total", json!(track_total));
                    }
                    if !disc.title.is_empty() {
                        desired.insert("disc_subtitle", json!(disc.title));
                    }
                    let mut patches = Vec::new();
                    for (field, value) in desired {
                        if before.get(field) == Some(&value) {
                            continue;
                        }
                        let patch = Patch {
                            field: field.into(),
                            operation: "set".into(),
                            value,
                            group_key: None,
                        };
                        patch.validate()?;
                        if !row.placed.contains(&field.to_string()) {
                            errors.insert(chilpa_i18n::t(
                                "A selected file cannot store one of the proposed fields.",
                            ));
                        }
                        patches.push(patch);
                    }
                    let member = json!({"key":format!("local:{identity}:{}",if joining {0} else {index+1}),"title":title,"artist":artist});
                    let projection = PlanRow {
                        patches,
                        membership: Some(member),
                        issue: String::new(),
                    };
                    self.manifest
                        .put_plan_entry(
                            id,
                            &serde_json::to_string(&projection)
                                .map_err(|error| error.to_string())?,
                        )
                        .map_err(|error| error.to_string())?;
                    changed += 1;
                }
                results.push(json!({"title":summary_text(&display_title),"disc":if !joining && request.renumber_discs {1} else {disc.number},"tracks":count,"trackTotal":track_total}));
            }
            let result_groups = if joining {
                vec![
                    json!({"title":summary_text(&request.album),"artist":summary_text(&request.album_artist),"tracks":files,"discs":results}),
                ]
            } else {
                results.clone()
            };
            let mut plan = json!({"kind":"grouping","operation":request.operation,"baseDataRevision":self.revision,
                "title":if joining {summary_text(&request.album)} else {chilpa_i18n::t("Separate by disc")},"canApply":errors.is_empty(),
                "error":errors.into_iter().collect::<Vec<_>>().join(" "),"counts":{"files":files,"previewRows":changed,"changed":changed,"groupsBefore":groups.len(),"groupsAfter":result_groups.len()+remaining.len()},
                "resultGroups":result_groups,"remainingGroups":remaining,"discs":results,"fieldChoices":[],"requiredFields":[]});
            plan["id"] = json!(self.plan_key(&plan)?);
            self.put_document("plan", &plan)
        };
        self.manifest.transaction(operation)
    }
}
