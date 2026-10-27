use super::{Root, Service};

impl Service {
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
