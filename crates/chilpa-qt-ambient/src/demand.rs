//! I coalesce cover-color requests and reject answers for retired consumers.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job { generation: u64, pub path: String }

#[derive(Default)]
pub struct Demand {
    consumers: BTreeMap<String, String>,
    wanted: Option<String>,
    completed: Option<String>,
    running: Option<Job>,
    generation: u64,
}

impl Demand {
    pub fn watch(&mut self, consumer: String, path: String, active: bool) {
        if active && !path.is_empty() { self.consumers.insert(consumer, path.clone()); }
        else { self.consumers.remove(&consumer); }
        let wanted = if active && !path.is_empty() { Some(path) } else { self.consumers.values().next().cloned() };
        if self.wanted != wanted {
            self.wanted = wanted;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub fn next(&mut self) -> Option<Job> {
        if self.running.is_some() || self.wanted == self.completed { return None; }
        let job = Job { generation: self.generation, path: self.wanted.clone()? };
        self.running = Some(job.clone());
        Some(job)
    }

    pub fn finish(&mut self, job: &Job) -> bool {
        if self.running.as_ref() != Some(job) { return false; }
        self.running = None;
        let current = job.generation == self.generation && self.wanted.as_ref() == Some(&job.path);
        if current { self.completed = Some(job.path.clone()); }
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slow_cover_coalesces_rapid_changes_and_cannot_overwrite_the_latest() {
        let mut demand = Demand::default();
        demand.watch("main".into(), "first".into(), true);
        let first = demand.next().unwrap();
        for path in ["second", "third", "last"] {
            demand.watch("main".into(), path.into(), true);
            assert!(demand.next().is_none(), "I keep one worker in flight");
        }
        assert!(!demand.finish(&first), "I reject the old colors");
        let last = demand.next().unwrap();
        assert_eq!(last.path, "last");
        assert!(demand.finish(&last));
        assert!(demand.next().is_none(), "I reuse the completed palette");
    }

    #[test]
    fn the_mini_keeps_demand_when_the_main_background_is_off() {
        let mut demand = Demand::default();
        demand.watch("main".into(), "cover".into(), true);
        demand.watch("mini".into(), "cover".into(), true);
        let job = demand.next().unwrap();
        demand.watch("main".into(), String::new(), false);
        assert!(demand.finish(&job));
        demand.watch("mini".into(), String::new(), false);
        assert!(demand.next().is_none());
        demand.watch("immersive".into(), "other".into(), true);
        let job = demand.next().unwrap();
        demand.watch("immersive".into(), String::new(), false);
        assert!(!demand.finish(&job), "I discard results after the last consumer closes");
        assert!(demand.next().is_none());
    }
}
