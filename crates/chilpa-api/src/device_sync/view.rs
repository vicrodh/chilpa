//! I prepare bounded presentation data without sending saved track keys to a frontend.
use super::{Root, Service};
use chilpa_i18n as i18n;
use serde_json::{json, Value};

impl Service {
    pub fn catalog_json(&self, query: &str) -> String {
        let query: String = query
            .chars()
            .take(256)
            .flat_map(char::to_lowercase)
            .collect();
        let options: Vec<_> = self
            .options()
            .iter()
            .filter(|row| {
                query.is_empty()
                    || row.name.to_lowercase().contains(&query)
                    || row.description.to_lowercase().contains(&query)
            })
            .collect();
        let profile = self.profile();
        let profiles: Vec<_> = self
            .profiles()
            .iter()
            .map(|p| json!({"id":p.id,"name":p.name}))
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
            "destination_names":destinations.iter().map(|d|&d["name"]).collect::<Vec<_>>(),
            "destination_values":self.destinations().iter().map(|d|&d.id).collect::<Vec<_>>(),
            "destination_details":destinations.iter().map(|d|&d["detail"]).collect::<Vec<_>>(),
            "destination_enabled":self.destinations().iter().map(|d|d.writable).collect::<Vec<_>>(),
            "profiles":profiles,"destinations":destinations,"options":options,"profile_folder":folder,
            "has_profiles":!profiles.is_empty(),"has_connected":self.destinations().iter().any(|d|d.id!="mounted"),
            "selected":i18n::tf("{} selection","{} selections",selected as i64,&[&selected.to_string()]),
            "storage_detail":destinations.iter().zip(self.destinations()).find(|(_,d)|d.id==self.status().destination).map(|(row,_)|&row["detail"]),
            "can_review":self.destinations().iter().any(|d|d.id==self.status().destination&&d.writable) && profile.is_some_and(|p|matches!((&p.root,self.status().destination.as_str()),(Root::Mounted{..},"mounted")) || matches!(&p.root,Root::Mtp{..}) && self.status().destination.contains('/')) && selected>0,
            "change_count":self.change_count(),"page_count":self.change_count().div_ceil(200).max(1)}).to_string()
    }
    pub fn status_json(&self) -> String {
        let status = self.status();
        let mut value = serde_json::to_value(status).unwrap_or(Value::Null);
        let label = match status.phase.as_str() {
            "discovering" => i18n::mark("Looking for players…"),
            "loading" => i18n::mark("Loading device profiles…"),
            "pairing" => i18n::mark("Adding storage…"),
            "saving" => i18n::mark("Saving selection…"),
            "selecting" => i18n::mark("Reading Library selection…"),
            "browsing" => i18n::mark("Loading music…"),
            "preparing" => i18n::mark("Reviewing changes…"),
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
        value["summary"] = json!(i18n::t_args(
            "{} to copy · {} to update · {} to remove · {} unchanged",
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
