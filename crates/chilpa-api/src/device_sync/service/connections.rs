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
}
