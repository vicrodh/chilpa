use super::{Root, Service};

impl Service {
    pub(in crate::device_sync) fn selected_storage_matches(&self) -> bool {
        self.profile().is_some_and(|profile| self.destinations.iter().any(|destination|
            destination.id == self.status.destination && destination.writable && destination.profiles.contains(&profile.id)))
    }
    pub(in crate::device_sync) fn storage_notice(&self) -> String {
        let Some(profile) = self.profile() else { return String::new() };
        if self.selected_storage_matches() { return String::new(); }
        match self.destinations.iter().find(|destination| destination.id == self.status.destination) {
            None => chilpa_i18n::t("The saved storage is not connected."),
            Some(destination) if destination.profiles.contains(&profile.id) => chilpa_i18n::t("This storage is read-only."),
            Some(destination) if !destination.profiles.is_empty() => chilpa_i18n::t("This storage belongs to another saved profile."),
            Some(_) => chilpa_i18n::t("Choose the storage paired with this profile."),
        }
    }
    pub(super) fn align_destination(&mut self) {
        let Some(profile) = self.profile() else { return };
        let matches: Vec<_> = self.destinations.iter().filter(|d| d.writable && d.profiles.contains(&profile.id)).collect();
        let connected: Vec<_> = matches.iter().filter(|d| d.id != "mounted").collect();
        self.status.destination = if connected.len() == 1 { connected[0].id.clone() }
            else if connected.len() > 1 { String::new() }
            else if matches.iter().any(|d| d.id == "mounted") || matches!(profile.root, Root::Mounted { .. }) { "mounted".into() }
            else { String::new() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_sync::{Profile, work::Destination};
    fn destination(id: &str, profile: &str) -> Destination {
        Destination { id:id.into(), name:String::new(), storage:String::new(), available:String::new(), capacity:String::new(), writable:true, profiles:if profile.is_empty() {vec![]} else {vec![profile.into()]} }
    }
    fn finish(service: &mut Service, outcome: super::super::Outcome) {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        service.pending = Some(receiver);
        assert!(sender.send(Ok(outcome)).is_ok());
        service.poll();
    }
    #[test]
    fn mtp_reader_and_restart_keep_distinct_profiles_and_wait_for_the_matching_storage() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::device_sync::Store { directory:directory.path().join("profiles") };
        let profiles: Vec<_> = [("a", "b", "Player"), ("c", "d", "Reader")].into_iter().map(|(id, target, name)| Profile {
            id:id.repeat(32), target_id:target.repeat(32), name:name.into(), root:Root::Mtp { folder:"Chilpa".into() },
            picks:vec![], remove_unselected:true, revision:0,
        }).collect();
        store.update(|catalog| { catalog.profiles = profiles.clone(); Ok(()) }).unwrap();
        let mut service = Service::new(directory.path().join("absent-library"), store.directory.clone());
        service.catalog.profiles = profiles.clone();
        service.status.profile = profiles[0].id.clone();
        let reader = destination("volume:reader", &profiles[1].id);
        service.destinations = vec![destination("usb-old/2", &profiles[0].id), reader.clone()];
        service.align_options();
        assert_eq!(service.status.destination, "usb-old/2");
        assert!(service.selected_storage_matches());
        service.select_profile(&profiles[1].id).unwrap();
        assert_eq!(service.status.destination, "volume:reader");
        service.select_profile(&profiles[0].id).unwrap();
        let previous = service.status.clone();
        finish(&mut service, super::super::Outcome::Connections(vec![reader.clone()], None, previous));
        assert_eq!(service.status.profile, profiles[0].id);
        assert!(service.status.destination.is_empty(), "the reader cannot replace an absent MTP player");
        assert!(!service.selected_storage_matches());
        assert!(!service.storage_notice().is_empty());
        service.select_destination("volume:reader").unwrap();
        let view: serde_json::Value = serde_json::from_str(&service.catalog_json("")).unwrap();
        assert_eq!(view["can_review"], false, "a writable card belonging to another profile cannot be reviewed");
        assert!(!view["storage_notice"].as_str().unwrap().is_empty());
        drop(service);
        let mut reopened = Service::new(directory.path().join("absent-library"), store.directory.clone());
        reopened.load().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while reopened.status.busy {
            assert!(std::time::Instant::now() < deadline);
            reopened.poll();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(reopened.status.error.is_none());
        assert_eq!(reopened.status.profile, profiles[0].id);
        assert!(reopened.status.destination.is_empty());
        let previous = reopened.status.clone();
        finish(&mut reopened, super::super::Outcome::Connections(vec![reader, destination("usb-new/7", &profiles[0].id)], None, previous));
        assert_eq!(reopened.status.destination, "usb-new/7");
        assert!(reopened.selected_storage_matches());
        assert!(reopened.storage_notice().is_empty());
        assert_eq!(store.read().unwrap().profiles, profiles, "reconnection never rewrites stored target identities");
    }

    #[test]
    fn unchanged_connections_preserve_a_manual_choice_preview_and_recovery_notice() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = Service::new(directory.path().join("absent-library"), directory.path().join("profiles"));
        service.catalog.profiles.push(Profile { id:"a".repeat(32), target_id:"b".repeat(32), name:"Card".into(), root:Root::Mtp { folder:"Chilpa".into() }, picks:vec![], remove_unselected:false, revision:0 });
        service.status.profile = "a".repeat(32);
        service.status.destination = "other/2".into();
        service.status.phase = "review".into();
        service.status.plan = "current-preview".into();
        service.destinations = vec![destination("paired/1", &"a".repeat(32)), destination("other/2", "")];
        let previous = service.status.clone();
        let mut reordered = service.destinations.clone();
        reordered.reverse();
        let revision = service.revision();
        finish(&mut service, super::super::Outcome::Connections(reordered, None, previous));
        assert_eq!(service.status.destination, "other/2");
        assert_eq!(service.status.plan, "current-preview");
        assert_eq!(service.status.phase, "review");
        assert_eq!(service.destinations[0].id, "paired/1");
        assert_eq!(service.revision(), revision, "unchanged connections do not rebuild catalogs");
        service.status.phase = "error".into();
        service.status.plan.clear();
        service.status.error = Some(crate::device_sync::UiError { message:"Recovery required".into(), detail:String::new(), recovery:true });
        let previous = service.status.clone();
        let destinations = service.destinations.clone();
        finish(&mut service, super::super::Outcome::Connections(destinations, None, previous));
        assert!(service.status.error.as_ref().unwrap().recovery);
    }
    #[test]
    fn reopening_keeps_the_operation_error_when_discovery_succeeds() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = Service::new(directory.path().join("absent-library"), directory.path().join("profiles"));
        service.status.destination = "mounted".into();
        service.status.phase = "error".into();
        service.status.error = Some(crate::device_sync::UiError { message:"Recovery required".into(), detail:String::new(), recovery:true });
        let previous = service.status.clone();
        finish(&mut service, super::super::Outcome::Reopened(vec![], super::super::library::Labels::new(), vec![destination("mounted", "")], None, previous));
        assert_eq!(service.status.phase, "error");
        assert!(service.status.error.as_ref().unwrap().recovery);
    }
    #[test]
    fn last_profile_is_persisted_separately_from_the_compatible_catalog() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::device_sync::Store { directory:directory.path().to_path_buf() };
        let id = "d".repeat(32);
        store.remember_profile(&id).unwrap();
        assert_eq!(store.active_profile(), Some(id));
        assert!(!directory.path().join("profiles.json").exists());
        std::fs::write(directory.path().join("active-profile"), vec![b'x';34]).unwrap();
        assert!(store.active_profile().is_none());
    }
    #[test]
    fn paired_card_follows_its_marker_across_ports_and_ignores_internal_storage() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = Service::new(directory.path().join("absent-library"), directory.path().join("profiles"));
        let profile = Profile { id:"a".repeat(32), target_id:"b".repeat(32), name:"Card".into(), root:Root::Mtp { folder:"Chilpa".into() }, picks:vec![], remove_unselected:false, revision:0 };
        service.catalog.profiles.push(profile.clone());
        service.status.profile = profile.id.clone();
        service.status.destination = "old-port/1".into();
        service.destinations = vec![destination("new-port/1", ""), destination("new-port/2", &profile.id)];
        service.align_destination();
        assert_eq!(service.status.destination, "new-port/2");
        service.destinations = vec![destination("volume:card-reader", &profile.id)];
        service.align_destination();
        assert_eq!(service.status.destination, "volume:card-reader");
        service.destinations.clear();
        service.align_destination();
        assert!(service.status.destination.is_empty());
        service.destinations = vec![destination("port-a/2", &profile.id), destination("port-b/2", &profile.id)];
        service.align_destination();
        assert!(service.status.destination.is_empty(), "duplicate identities require an explicit storage choice");
    }
    #[test]
    fn a_detected_writable_card_can_review_relative_and_legacy_profiles() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = Service::new(directory.path().join("absent-library"), directory.path().join("profiles"));
        let profile = "a".repeat(32);
        service.catalog.profiles.push(Profile { id:profile.clone(), target_id:"b".repeat(32), name:"Card".into(), root:Root::Mtp { folder:"Chilpa".into() }, picks:vec![crate::device_sync::Pick::Playlist { id:"playlist".into(), name:"Evening".into(), key:"local:list".into() }], remove_unselected:false, revision:0 });
        service.status.profile = profile.clone();
        service.destinations = vec![destination("volume:card-reader", &profile)];
        service.align_destination();
        let can_review = |service: &Service| serde_json::from_str::<serde_json::Value>(&service.catalog_json("")).unwrap()["can_review"] == true;
        assert!(can_review(&service), "a detected card does not use an MTP location/storage identifier");
        service.catalog.profiles[0].root = Root::Mounted { path:directory.path().join("Chilpa") };
        assert!(can_review(&service), "a legacy profile can follow its card into a reader");
        service.catalog.profiles[0].picks.clear();
        assert!(can_review(&service), "an empty saved selection can retire previously managed files");
        service.destinations[0].writable = false;
        assert!(!can_review(&service), "read-only cards cannot start a review");
        service.destinations.clear();
        assert!(!can_review(&service), "a disconnected card cannot start a review");
    }
}
