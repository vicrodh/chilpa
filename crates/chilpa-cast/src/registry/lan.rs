//! The real network behind the registry. Every call here only asks who is
//! there: a description GET, an SSDP search, an mDNS browse.
use super::Network;
use crate::{discovery::{self, Sighting}, dlna, Device, Result};
use std::{sync::atomic::AtomicBool, time::Duration};

pub struct Lan;
impl Network for Lan {
    fn describe(&self, location: &str, limit: Duration) -> Result<Device> { dlna::describe_within(location, limit) }
    fn search(&self, window: Duration, running: &AtomicBool) -> Result<Vec<String>> { discovery::renderers(window, running) }
    fn browse(&self, window: Duration, running: &AtomicBool, found: &mut dyn FnMut(Sighting) -> bool) -> Result<()> {
        discovery::browse(window, running, found)
    }
}
