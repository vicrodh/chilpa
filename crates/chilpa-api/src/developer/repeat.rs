//! Consecutive identical records collapse into the first one and, while the
//! burst goes on, a counted summary every few seconds.
use super::logs::Entry;
use std::time::{Duration, Instant};

/// A continuing burst stays visible without printing every duplicate.
const SUMMARY_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Default)]
pub(super) struct Consecutive {
    current: Option<Run>,
}

struct Run {
    entry: Entry,
    period_started: Instant,
    last_seen: Instant,
    repeats: u64,
}

impl Run {
    fn matches(&self, entry: &Entry) -> bool {
        self.entry.level == entry.level && self.entry.target == entry.target && self.entry.message == entry.message
    }

    fn summary(&self) -> Option<Entry> {
        (self.repeats > 0).then(|| Entry {
            ts: self.entry.ts,
            level: self.entry.level,
            target: self.entry.target.clone(),
            message: format!(
                "[log] previous message repeated {} additional times over {} ms: {}",
                self.repeats,
                self.last_seen.saturating_duration_since(self.period_started).as_millis(),
                self.entry.message,
            ),
        })
    }
}

impl Consecutive {
    /// Accepts only visible, already redacted records. The output is at most
    /// the previous summary followed by the new record.
    pub(super) fn push(&mut self, entry: Entry, now: Instant) -> [Option<Entry>; 2] {
        if let Some(run) = self.current.as_mut().filter(|run| run.matches(&entry)) {
            run.entry.ts = entry.ts;
            run.last_seen = now;
            run.repeats = run.repeats.saturating_add(1);
            if now.saturating_duration_since(run.period_started) >= SUMMARY_INTERVAL {
                let summary = run.summary();
                run.repeats = 0;
                run.period_started = now;
                return [summary, None];
            }
            return [None, None];
        }
        let summary = self.flush();
        self.current = Some(Run { entry: entry.clone(), period_started: now, last_seen: now, repeats: 0 });
        [summary, Some(entry)]
    }

    /// Ends the run, so the next record is always visible in full.
    pub(super) fn flush(&mut self) -> Option<Entry> {
        self.current.take().and_then(|run| run.summary())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(message: &str) -> Entry {
        Entry { ts: 0, level: "info", target: "application".into(), message: message.into() }
    }
    fn messages(lines: [Option<Entry>; 2]) -> Vec<String> {
        lines.into_iter().flatten().map(|entry| entry.message).collect()
    }

    #[test]
    fn a_repeat_is_counted_and_not_printed() {
        let (mut run, start) = (Consecutive::default(), Instant::now());
        assert_eq!(messages(run.push(entry("tick"), start)), ["tick"]);
        assert!(messages(run.push(entry("tick"), start + Duration::from_millis(10))).is_empty());
        assert!(messages(run.push(entry("tick"), start + Duration::from_millis(20))).is_empty());
        let out = messages(run.push(entry("tock"), start + Duration::from_millis(30)));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], "[log] previous message repeated 2 additional times over 20 ms: tick");
        assert_eq!(out[1], "tock");
    }

    #[test]
    fn a_long_burst_prints_a_summary_each_interval() {
        let (mut run, start) = (Consecutive::default(), Instant::now());
        run.push(entry("tick"), start);
        run.push(entry("tick"), start + Duration::from_secs(1));
        let out = messages(run.push(entry("tick"), start + SUMMARY_INTERVAL));
        assert_eq!(out, ["[log] previous message repeated 2 additional times over 5000 ms: tick"]);
        assert!(messages(run.push(entry("tick"), start + SUMMARY_INTERVAL + Duration::from_secs(1))).is_empty());
    }

    #[test]
    fn a_level_change_is_not_a_repeat() {
        let (mut run, start) = (Consecutive::default(), Instant::now());
        run.push(entry("tick"), start);
        let mut other = entry("tick");
        other.level = "error";
        assert_eq!(messages(run.push(other, start)), ["tick"]);
    }

    #[test]
    fn flush_reports_what_is_pending_once() {
        let (mut run, start) = (Consecutive::default(), Instant::now());
        run.push(entry("tick"), start);
        run.push(entry("tick"), start);
        assert!(run.flush().is_some());
        assert!(run.flush().is_none());
    }
}
