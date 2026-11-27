//! The order a shuffled queue plays in. Plain, it is any order. With
//! "Keep classical works together", the movements of a work stay together
//! and in the queue's order, and the work moves as one. With "Smooth
//! shuffle", each next track is the closest of a random handful to the one
//! before it: a genre in common, a near year, tempo and loudness, and never
//! the same artist twice in a row while another is left. Both are the
//! facade's: the player takes any order it is given (`Order::set`).
//!
//! Smooth shuffle waits for the library to be read once, a folder scanned
//! or a server synced: before that there is nothing to weigh.

use crate::Chilpa;

/// The toggles under Playback.
pub const WORKS: &str = "playback.shuffleWorks";
pub const SMOOTH: &str = "playback.smoothShuffle";

/// How many of the tracks left are weighed for each next one: enough to
/// find a close one, few enough that a long queue orders at once.
const HANDFUL: usize = 48;
/// A queue longer than this is shuffled plainly: reading what weighs for
/// each of its tracks would hold the window.
const WEIGHED_AT_MOST: usize = 20_000;

/// What the order knows of one track of the queue.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Facts {
    /// The work it is a movement of, as `work_key` names it.
    pub work: Option<String>,
    pub artist: String,
    pub year: Option<i32>,
    pub genres: Vec<i64>,
    pub bpm: Option<f32>,
    pub lufs: Option<f32>,
}

/// A seed from the clock, for a shuffle asked for now.
pub(crate) fn seed_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}

/// A small, fast generator: the order needs variety, not secrets.
struct Random(u64);

impl Random {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15 | 1)
    }

    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}

/// How far apart the end of one stretch and the start of the next sound;
/// lower is closer. What either does not know weighs as a middle distance.
fn distance(a: &Facts, b: &Facts) -> f32 {
    let mut d = 0.0;
    if !a.artist.is_empty() && a.artist.eq_ignore_ascii_case(&b.artist) {
        d += 100.0;
    }
    d += 3.0 * if a.genres.is_empty() || b.genres.is_empty() {
        0.5
    } else {
        let shared = a.genres.iter().filter(|g| b.genres.contains(g)).count() as f32;
        let all = (a.genres.len() + b.genres.len()) as f32 - shared;
        1.0 - shared / all
    };
    d += 2.0 * match (a.year, b.year) {
        (Some(x), Some(y)) => ((x - y).abs() as f32 / 10.0).min(1.0),
        _ => 0.5,
    };
    // Half and double tempo sit together: 70 and 140 share a pulse.
    d += 1.5 * match (a.bpm, b.bpm) {
        (Some(x), Some(y)) if x > 0.0 && y > 0.0 => {
            let ratio = (x / y).ln().abs();
            [ratio, (ratio - 2f32.ln()).abs()].into_iter().fold(f32::MAX, f32::min).min(1.25f32.ln()) / 1.25f32.ln()
        }
        _ => 0.5,
    };
    d += match (a.lufs, b.lufs) {
        (Some(x), Some(y)) => ((x - y).abs() / 6.0).min(1.0),
        _ => 0.5,
    };
    d
}

/// The order of a queue of `facts.len()` tracks. `first` opens it; when it
/// is a movement, the movements after it follow, and the ones before it go
/// on as a work of their own.
pub(crate) fn order(facts: &[Facts], first: Option<usize>, works: bool, smooth: bool, seed: u64) -> Vec<usize> {
    let mut random = Random::new(seed);
    // The stretches that move as one: a work's movements, or one track.
    let mut units: Vec<Vec<usize>> = Vec::new();
    let mut of_work: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (index, fact) in facts.iter().enumerate() {
        match fact.work.as_deref().filter(|_| works) {
            Some(work) => match of_work.get(work) {
                Some(&unit) => units[unit].push(index),
                None => {
                    of_work.insert(work, units.len());
                    units.push(vec![index]);
                }
            },
            None => units.push(vec![index]),
        }
    }
    let mut opening = None;
    if let Some(first) = first.filter(|&f| f < facts.len()) {
        let at = units.iter().position(|unit| unit.contains(&first)).unwrap_or(0);
        let mut unit = units.swap_remove(at);
        let split = unit.iter().position(|&i| i == first).unwrap_or(0);
        let from_first = unit.split_off(split);
        if !unit.is_empty() {
            units.push(unit);
        }
        opening = Some(from_first);
    }
    let mut out = Vec::with_capacity(facts.len());
    let mut last = match opening {
        Some(unit) => {
            out.extend(&unit);
            unit.last().copied()
        }
        None => None,
    };
    if !smooth {
        for i in (1..units.len()).rev() {
            units.swap(i, random.below(i + 1));
        }
        out.extend(units.into_iter().flatten());
        return out;
    }
    while !units.is_empty() {
        let pick = match last {
            None => random.below(units.len()),
            Some(previous) => {
                // A little noise, so the same start does not give the same
                // order; far below what a genre or an artist weighs.
                let weigh = |candidate: usize, random: &mut Random| distance(&facts[previous], &facts[units[candidate][0]]) + random.below(1000) as f32 / 4000.0;
                let mut best = (f32::MAX, 0);
                if units.len() <= HANDFUL {
                    for candidate in 0..units.len() {
                        let d = weigh(candidate, &mut random);
                        if d < best.0 {
                            best = (d, candidate);
                        }
                    }
                } else {
                    for _ in 0..HANDFUL {
                        let candidate = random.below(units.len());
                        let d = weigh(candidate, &mut random);
                        if d < best.0 {
                            best = (d, candidate);
                        }
                    }
                    // The handful held only the same artist: the rest is
                    // looked through for another before giving in.
                    if best.0 >= 100.0 {
                        for candidate in 0..units.len() {
                            let d = weigh(candidate, &mut random);
                            if d < best.0 {
                                best = (d, candidate);
                            }
                        }
                    }
                }
                best.1
            }
        };
        let unit = units.swap_remove(pick);
        last = unit.last().copied();
        out.extend(unit);
    }
    out
}

impl Chilpa {
    /// Whether a work's movements stay together in a shuffle. On unless
    /// turned off: it changes nothing for a queue without works.
    pub fn shuffle_works(&self) -> bool {
        self.preference(WORKS).as_deref() != Some("false")
    }

    /// Whether a shuffle picks each next track close to the one before it.
    pub fn smooth_shuffle(&self) -> bool {
        self.preference(SMOOTH).is_some_and(|v| v == "true")
    }

    /// The shuffled order of the queue, `first` first.
    pub(crate) fn shuffle_order(&mut self, first: Option<usize>, seed: u64) -> Vec<usize> {
        let len = self.queue.len();
        let (works, smooth) = (self.shuffle_works(), self.smooth_shuffle() && self.library_read_once());
        if (!works && !smooth) || len > WEIGHED_AT_MOST {
            let mut order = chilpa_core::queue_playback::shuffled(len, seed);
            if let Some(at) = first.and_then(|first| order.iter().position(|&index| index == first)) {
                order.swap(0, at);
            }
            return order;
        }
        let facts = self.queue_facts(smooth);
        order(&facts, first, works, smooth, seed)
    }

    /// Whether a folder was scanned or a server synced, once.
    fn library_read_once(&mut self) -> bool {
        let Some(lib) = self.library() else { return false };
        lib.roots().is_ok_and(|roots| roots.iter().any(|root| root.last_scan.is_some()))
            || lib.servers().is_ok_and(|servers| servers.iter().any(|server| server.synced_at.is_some()))
    }

    /// What the order knows of each track of the queue; a track the library
    /// does not hold has no facts and goes on its own.
    fn queue_facts(&mut self, smooth: bool) -> Vec<Facts> {
        let paths = self.queue.clone();
        let Some(lib) = self.library() else { return vec![Facts::default(); paths.len()] };
        let found = lib.tracks_by_paths(&paths).unwrap_or_else(|_| vec![None; paths.len()]);
        let ids: Vec<i64> = found.iter().flatten().map(|track| track.id).collect();
        let close = if smooth { lib.closeness_for(&ids, chilpa_core::analysis_job::ANALYZER).unwrap_or_default() } else { Default::default() };
        found
            .iter()
            .map(|track| {
                let Some(track) = track else { return Facts::default() };
                let composers: Vec<String> = track.composers.split(", ").filter(|c| !c.is_empty()).map(str::to_string).collect();
                let work = track.work.clone().filter(|w| !w.trim().is_empty())
                    .or_else(|| chilpa_library::work_titles::work_from_title(&track.title, &composers));
                let unknown = serde_json::json!(track.album_id.unwrap_or(-track.id));
                let close = close.get(&track.id).cloned().unwrap_or_default();
                Facts {
                    work: work.map(|work| chilpa_library::store::compositions::work_key(&work, &composers, unknown)),
                    artist: track.artist.clone(),
                    year: close.year,
                    genres: close.genres,
                    bpm: close.bpm,
                    lufs: close.loudness_lufs,
                }
            })
            .collect()
    }

    pub(crate) fn shuffle_setting(&mut self, key: &str, value: &str) -> bool {
        if key != WORKS && key != SMOOTH {
            return false;
        }
        if matches!(value, "true" | "false") {
            self.remember_preference(key, value);
        }
        true
    }

    pub(crate) fn shuffle_row(&self, row: &mut crate::SettingsRow) -> bool {
        match row.key.as_str() {
            WORKS => row.value = self.shuffle_works().to_string(),
            SMOOTH => row.value = self.smooth_shuffle().to_string(),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(work: Option<&str>, artist: &str) -> Facts {
        Facts { work: work.map(str::to_string), artist: artist.into(), ..Default::default() }
    }

    fn is_permutation(order: &[usize], len: usize) -> bool {
        let mut sorted = order.to_vec();
        sorted.sort();
        sorted == (0..len).collect::<Vec<_>>()
    }

    #[test]
    fn a_work_s_movements_stay_together_and_in_order() {
        // Two works of four and three movements among loose tracks.
        let facts: Vec<Facts> = [None, Some("a"), Some("a"), None, Some("a"), Some("b"), Some("a"), Some("b"), Some("b"), None]
            .into_iter().map(|w| track(w, "x")).collect();
        for seed in 1..40 {
            let got = order(&facts, None, true, false, seed);
            assert!(is_permutation(&got, facts.len()));
            let a = got.iter().position(|&i| i == 1).unwrap();
            assert_eq!(&got[a..a + 4], &[1, 2, 4, 6], "seed {seed}");
            let b = got.iter().position(|&i| i == 5).unwrap();
            assert_eq!(&got[b..b + 3], &[5, 7, 8], "seed {seed}");
        }
        let apart = order(&facts, None, false, false, 3);
        assert!(is_permutation(&apart, facts.len()));
        assert_ne!((0..40).map(|seed| order(&facts, None, false, false, seed)).filter(|o| o.windows(4).any(|w| w == [1, 2, 4, 6])).count(), 40, "off, a work is shuffled like the rest");
    }

    #[test]
    fn the_track_that_plays_opens_and_its_work_goes_on_from_it() {
        let facts: Vec<Facts> = [Some("a"), Some("a"), Some("a"), Some("a"), None, None].into_iter().map(|w| track(w, "x")).collect();
        for seed in 1..20 {
            let got = order(&facts, Some(2), true, false, seed);
            assert!(is_permutation(&got, facts.len()));
            assert_eq!(&got[..2], &[2, 3], "the third movement, then the fourth");
            let before = got.iter().position(|&i| i == 0).unwrap();
            assert_eq!(got[before + 1], 1, "the first two go on as a work of their own");
            assert_eq!(order(&facts, Some(4), false, true, seed)[0], 4);
        }
    }

    #[test]
    fn a_smooth_shuffle_goes_to_a_close_track_and_never_repeats_an_artist_while_others_are_left() {
        let rock = Facts { artist: "R".into(), genres: vec![1], year: Some(1975), bpm: Some(120.0), lufs: Some(-9.0), ..Default::default() };
        let jazz = Facts { artist: "J".into(), genres: vec![2], year: Some(1959), bpm: Some(80.0), lufs: Some(-18.0), ..Default::default() };
        // Five rock tracks by different artists and five jazz tracks.
        let facts: Vec<Facts> = (0..10).map(|i| {
            let base = if i < 5 { &rock } else { &jazz };
            Facts { artist: format!("{}{}", base.artist, i), ..base.clone() }
        }).collect();
        for seed in 1..30 {
            let got = order(&facts, Some(0), false, true, seed);
            assert!(is_permutation(&got, facts.len()));
            let switches = got.windows(2).filter(|w| (w[0] < 5) != (w[1] < 5)).count();
            assert_eq!(switches, 1, "rock stays with rock, then jazz with jazz; seed {seed}: {got:?}");
        }
        let same: Vec<Facts> = (0..6).map(|i| Facts { artist: if i % 2 == 0 { "A".into() } else { "B".into() }, ..rock.clone() }).collect();
        for seed in 1..30 {
            let got = order(&same, Some(0), false, true, seed);
            assert!(got.windows(2).all(|w| same[w[0]].artist != same[w[1]].artist), "seed {seed}: {got:?}");
        }
    }

    #[test]
    fn half_and_double_tempo_are_close() {
        let at = |bpm| Facts { bpm: Some(bpm), ..Default::default() };
        assert!(distance(&at(70.0), &at(140.0)) < distance(&at(70.0), &at(100.0)));
        assert!(distance(&at(120.0), &at(122.0)) < distance(&at(120.0), &at(90.0)));
    }
}
