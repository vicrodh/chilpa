//! I prepare bounded presentation data without sending saved track keys to a frontend.
use super::{Root, Service};
use chilpa_i18n as i18n;
use serde_json::{json, Value};

impl Service {
    pub fn catalog_json(&self, query: &str) -> String {
        self.catalog_page_json(query, "all", 0)
    }
    pub fn catalog_page_json(&self, query: &str, kind: &str, page: usize) -> String {
        let kind = match kind { "playlists" => "playlists", "queries" => "queries", "tracks" => "tracks", "podcasts" => "podcasts", _ => "all" };
        let filter: String = query.chars().take(256).collect();
        let query = filter.to_lowercase();
        let options: Vec<_> = self
            .options()
            .iter()
            .filter(|row| match (&row.pick, kind) {
                (_, "all") | (super::Pick::Playlist { .. }, "playlists")
                | (super::Pick::Query { .. }, "queries") | (super::Pick::Tracks { .. }, "tracks") | (super::Pick::Episode { .. }, "podcasts") => true,
                _ => false,
            })
            .filter(|row| {
                query.is_empty()
                    || row.name.to_lowercase().contains(&query)
                    || row.description.to_lowercase().contains(&query)
            })
            .collect();
        let selection_pages = options.len().div_ceil(200).max(1);
        let selection_page = page.min(selection_pages - 1);
        let selection_total = options.len();
        let options: Vec<_> = options.into_iter().skip(selection_page * 200).take(200).collect();
        let profile = self.profile();
        let profiles: Vec<_> = self
            .profiles()
            .iter()
            .map(|p| json!({"id":p.id,"name":p.name,"connected":self.destinations().iter().any(|d|d.profiles.contains(&p.id))}))
            .collect();
        let destinations: Vec<_> = self.destinations().iter().map(|d| json!({
            "id":d.id,"name":format!("{} · {}",d.name,d.storage),
            "detail":if d.available.is_empty(){String::new()}else{i18n::t_args("{} available of {}", &[&d.available,&d.capacity])},"writable":d.writable
        })).collect();
        let folder = profile
            .map(|p| match &p.root {
                Root::Mtp { folder } => folder.clone(),
                Root::Mounted { path } => path.display().to_string(),
            })
            .unwrap_or_default();
        let selected = self.options().iter().filter(|row| row.checked).count();
        json!({"profile_names":self.profiles().iter().map(|p|&p.name).collect::<Vec<_>>(),
            "profile_values":self.profiles().iter().map(|p|&p.id).collect::<Vec<_>>(),
            "profile_enabled":profiles.iter().map(|p|p["connected"].as_bool().unwrap_or(false)).collect::<Vec<_>>(),
            "destination_names":destinations.iter().map(|d|&d["name"]).collect::<Vec<_>>(),
            "destination_values":self.destinations().iter().map(|d|&d.id).collect::<Vec<_>>(),
            "destination_details":destinations.iter().map(|d|&d["detail"]).collect::<Vec<_>>(),
            "destination_enabled":self.destinations().iter().map(|d|d.writable).collect::<Vec<_>>(),
            "profiles":profiles,"destinations":destinations,"options":options,"profile_folder":folder,"filter":filter,
            "has_profiles":!profiles.is_empty(),"has_connected":self.destinations().iter().any(|d|d.id!="mounted"),
            "selection_kind":kind,"selection_page":selection_page,"selection_pages":selection_pages,"selection_total":selection_total,
            "selected":i18n::tf("{} selection","{} selections",selected as i64,&[&selected.to_string()]),
            "storage_detail":destinations.iter().zip(self.destinations()).find(|(_,d)|d.id==self.status().destination).map(|(row,_)|&row["detail"]),
            "can_review":self.selected_storage_matches(),"storage_notice":self.storage_notice(),
            "change_count":self.change_count(),"page_count":self.change_count().div_ceil(200).max(1)}).to_string()
    }
    pub fn status_json(&self) -> String {
        let status = self.status();
        let mut value = serde_json::to_value(status).unwrap_or(Value::Null);
        let label = match status.phase.as_str() {
            "discovering" | "reconnecting" => i18n::mark("Looking for players…"),
            "loading" => i18n::mark("Loading device profiles…"),
            "pairing" => i18n::mark("Adding storage…"),
            "forgetting" => i18n::mark("Removing saved profile…"),
            "saving" => i18n::mark("Saving selection…"),
            "selecting" => i18n::mark("Reading Library selection…"),
            "browsing" => i18n::mark("Loading music…"),
            "downloading" => i18n::mark("Downloading episodes…"),
            "preparing" => i18n::mark("Reviewing changes…"),
            "transferring" if status.verifying => i18n::mark("Verifying copied files…"),
            "transferring" => i18n::mark("Syncing music…"),
            "cancelling" => i18n::mark("Cancelling…"),
            "done" => i18n::mark("Sync complete"),
            "cancelled" => i18n::mark("Sync cancelled"),
            "review" if self.change_count() == 0 => i18n::mark("Your music is up to date"),
            "review" => i18n::mark("Ready to sync"),
            "error" => i18n::mark("Sync needs attention"),
            _ => i18n::mark("Choose music for this storage"),
        };
        value["label"] = json!(i18n::t(label));
        value["review_note"] = json!(if status.metadata_review {i18n::t("Quick review by size")} else {String::new()});
        value["summary"] = json!(i18n::t_args(
            if status.metadata_review {i18n::mark("{} to copy · {} to update · {} to remove · {} already present")} else {i18n::mark("{} to copy · {} to update · {} to remove · {} unchanged")},
            &[
                &status.copies.to_string(),
                &status.updates.to_string(),
                &status.removals.to_string(),
                &status.unchanged.to_string()
            ]
        ));
        value["progress"] = json!(if status.total == 0 {
            0.0
        } else {
            status.completed as f64 / status.total as f64
        });
        value["progress_text"] = json!(i18n::t_args(
            "{} of {} files · {}",
            &[
                &status.completed.to_string(),
                &status.total.to_string(),
                &status.transferred
            ]
        ));
        value.to_string()
    }
    pub fn changes_json(&self, page: usize) -> String {
        serde_json::to_string(self.change_page(page.saturating_mul(200), 200))
            .unwrap_or_else(|_| "[]".into())
    }
}
