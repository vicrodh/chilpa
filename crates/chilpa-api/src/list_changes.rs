//! I plan small list edits before publishing any of them.
//! Scratch indices live for one refresh: at most 512 rows and 50 structural
//! edits per model. Larger or ambiguous changes use the caller's reset path.

use crate::{Album, GridRow, TrackEntry};

pub const MAX_ROWS: usize = 512;
pub const MAX_STEPS: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Remove(usize),
    Move { from: usize, to: usize },
    Insert(usize),
}

impl Step {
    /// I use destination-child coordinates before removing the source row.
    pub fn destination(self) -> usize {
        match self { Self::Move { from, to } => to + usize::from(from < to), _ => 0 }
    }
    pub fn apply<T: Clone>(self, rows: &mut Vec<T>, new: &[T]) {
        match self {
            Self::Remove(at) => { rows.remove(at); }
            Self::Move { from, to } => { let row = rows.remove(from); rows.insert(to, row); }
            Self::Insert(at) => rows.insert(at, new[at].clone()),
        }
    }
    pub fn remap(self, row: Option<usize>) -> Option<usize> {
        let row = row?;
        match self {
            Self::Remove(at) if row == at => None,
            Self::Remove(at) => Some(row - usize::from(row > at)),
            Self::Insert(at) => Some(row + usize::from(row >= at)),
            Self::Move { from, to } if row == from => Some(to),
            Self::Move { from, to } => { let at = row - usize::from(row > from); Some(at + usize::from(at >= to)) },
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub steps: Vec<Step>,
    /// Final indices of surviving rows whose data needs publishing.
    pub changed: Vec<usize>,
}

impl Changes {
    /// Inserted rows also publish their final backing indices after staging.
    pub fn published(&self) -> impl Iterator<Item = usize> + '_ {
        self.changed.iter().copied().chain(self.steps.iter().filter_map(|step| match step { Step::Insert(at) => Some(*at), _ => None }))
    }
}

/// I remove first, reorder survivors, insert, then publish changed content.
/// Keys must identify occurrences; a track id alone cannot identify a queue row.
pub fn between<K: Eq>(old: &[K], new: &[K], equal: impl Fn(usize, usize) -> bool) -> Option<Changes> {
    if old.len().max(new.len()) > MAX_ROWS { return None; }
    if old.iter().enumerate().any(|(i, key)| old[..i].contains(key))
        || new.iter().enumerate().any(|(i, key)| new[..i].contains(key)) { return None; }
    let origins: Vec<_> = new.iter().map(|key| old.iter().position(|other| other == key)).collect();
    let mut live: Vec<_> = (0..old.len()).collect();
    let mut result = Changes::default();
    for at in (0..old.len()).rev() {
        if !origins.contains(&Some(at)) {
            result.steps.push(Step::Remove(at));
            live.remove(at);
            if result.steps.len() > MAX_STEPS { return None; }
        }
    }
    for (to, origin) in origins.iter().flatten().enumerate() {
        let from = live.iter().position(|row| row == origin)?;
        if from != to {
            result.steps.push(Step::Move { from, to });
            let row = live.remove(from); live.insert(to, row);
            if result.steps.len() > MAX_STEPS { return None; }
        }
    }
    for (at, origin) in origins.into_iter().enumerate() {
        match origin {
            None => result.steps.push(Step::Insert(at)),
            Some(before) if !equal(before, at) => result.changed.push(at),
            _ => {}
        }
        if result.steps.len() > MAX_STEPS { return None; }
    }
    Some(result)
}

#[derive(PartialEq, Eq)]
enum TrackKey<'a> { Queue(&'a str), Track(i64, &'a str), Header(&'a str, bool, bool) }

fn track_key<'a>(row: &'a TrackEntry, key: Option<&'a String>) -> TrackKey<'a> {
    match row {
        TrackEntry::Header(h) => TrackKey::Header(&h.label, h.record, h.work),
        TrackEntry::Track(_) if key.is_some_and(|key| !key.is_empty()) => TrackKey::Queue(key.unwrap()),
        TrackEntry::Track(t) => TrackKey::Track(t.id, &t.path),
    }
}

pub fn tracks(old: &[TrackEntry], new: &[TrackEntry], old_keys: &[String], new_keys: &[String]) -> Option<Changes> {
    if old.len().max(new.len()) > MAX_ROWS { return None; }
    let a: Vec<_> = old.iter().enumerate().map(|(i, row)| track_key(row, old_keys.get(i))).collect();
    let b: Vec<_> = new.iter().enumerate().map(|(i, row)| track_key(row, new_keys.get(i))).collect();
    let (old_numbers, new_numbers) = (numbers(old_keys, false), numbers(new_keys, false));
    between(&a, &b, |i, j| i == j && old[i] == new[j] && old_numbers.get(i) == new_numbers.get(j))
}

pub fn track_anchor(old: &[TrackEntry], new: &[TrackEntry], old_keys: &[String], new_keys: &[String], anchor: Option<usize>) -> Option<usize> {
    let at = anchor?;
    let key = track_key(old.get(at)?, old_keys.get(at));
    new.iter().enumerate().position(|(i, row)| track_key(row, new_keys.get(i)) == key)
}

/// I keep disc numbering and queue numbering consistent after every edit.
pub fn numbers(keys: &[String], discs: bool) -> Vec<i32> {
    let mut number = 0;
    keys.iter().map(|key| {
        if !key.is_empty() { number += 1; } else if discs { number = 0; }
        number
    }).collect()
}

/// A shelf has one model row per card; grids retain their grouped rows.
pub fn album_rows(len: usize, columns: usize, max_rows: usize, groups: &[(usize, String)]) -> Vec<GridRow> {
    if max_rows == 1 { crate::grid_rows(len.min(columns), 1, 0, &[]) }
    else { crate::grid_rows(len, columns, max_rows, groups) }
}

pub fn albums(old: &[Album], new: &[Album], before: &[GridRow], after: &[GridRow]) -> Option<Changes> {
    if old.len().max(new.len()) > MAX_ROWS { return None; }
    let key = |a: &Album| (a.id, if a.id == 0 { a.title.clone() } else { String::new() });
    let keys = |rows: &[GridRow], cards: &[Album]| rows.iter().map(|r|
        (r.label.clone(), cards[r.first..r.first + r.span].iter().map(&key).collect::<Vec<_>>())).collect::<Vec<_>>();
    between(&keys(before, old), &keys(after, new), |i, j| {
        let (a, b) = (&before[i], &after[j]);
        a == b && old[a.first..a.first + a.span] == new[b.first..b.first + b.span]
    })
}

#[cfg(test)]
mod tests;
