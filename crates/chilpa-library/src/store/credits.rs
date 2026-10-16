//! The artists a joined credit names.
//!
//! "A, B" on a track is two artists as often as it is one band, and nothing
//! in the text says which. So it is never split on what it looks like. A
//! credit is split in two cases: its source gave the artists apart, or
//! every part of it is an artist the library already has on its own. The
//! credit itself stays as its tag wrote it, and is what is shown.

use super::Library;
use rusqlite::params;
use std::collections::{HashMap, HashSet};

/// What joins artists for certain when it joins them at all.
const STRONG: &[&str] = &[",", ";", "\u{3001}", " feat. ", " feat ", " ft. ", " ft ", " featuring "];

/// What joins artists as often as it is part of a name.
const WEAK: &[&str] = &[" & ", " / ", "/", " and ", " with ", " vs. ", " vs ", " x ", " + "];

/// The artists `credit` names, when every one of them is `known`, and
/// nothing when any is not. A part that names several in turn is taken
/// apart too.
pub fn split_credit(credit: &str, known: &dyn Fn(&str) -> bool) -> Option<Vec<String>> {
    for separators in [STRONG, WEAK] {
        let parts = parts_at(credit, separators);
        if parts.len() < 2 {
            continue;
        }
        let mut names: Vec<String> = Vec::new();
        let every = parts.iter().all(|part| {
            let found = split_credit(part, known).or_else(|| known(part).then(|| vec![part.clone()]));
            found.is_some_and(|found| {
                for name in found {
                    if !names.iter().any(|n| key(n) == key(&name)) {
                        names.push(name);
                    }
                }
                true
            })
        });
        if every && names.len() > 1 {
            return Some(names);
        }
    }
    None
}

/// `text` cut at every one of `separators`, whatever their case, without
/// the empty pieces.
fn parts_at(text: &str, separators: &[&str]) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut rest = text;
    'scan: while !rest.is_empty() {
        for separator in separators {
            if rest.get(..separator.len()).is_some_and(|head| head.eq_ignore_ascii_case(separator)) {
                parts.push(String::new());
                rest = &rest[separator.len()..];
                continue 'scan;
            }
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            parts.last_mut().expect("never empty").push(c);
        }
        rest = chars.as_str();
    }
    parts.into_iter().map(|p| p.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|p| !p.is_empty()).collect()
}

/// A name as names are compared: without its case or the spaces around it.
fn key(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

impl Library {
    /// A mark that moves whenever the library is written to, by this
    /// connection or by another. Two equal marks: nothing changed between.
    pub fn written(&self) -> (i64, u64) {
        let others = self.db.query_row("PRAGMA data_version", [], |r| r.get(0)).unwrap_or(0);
        (others, self.db.total_changes())
    }

    /// The artists a credit names: its parts, or itself when it is one.
    pub fn credit_names(&self, credit: &str) -> rusqlite::Result<Vec<String>> {
        let credit = credit.trim();
        let mut stmt = self.db.prepare_cached("SELECT name FROM credit_parts WHERE credit = ?1 ORDER BY position")?;
        let names: Vec<String> = stmt.query_map(params![credit], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        Ok(if names.is_empty() && !credit.is_empty() { vec![credit.to_string()] } else { names })
    }

    /// Keeps the artists a source gave apart for a credit it joined. What
    /// a source states wins over what was matched here.
    pub fn state_credit(&self, credit: &str, names: &[String]) -> rusqlite::Result<()> {
        let credit = credit.trim();
        let names: Vec<String> = names.iter().map(|n| n.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|n| !n.is_empty()).collect();
        if credit.is_empty() || names.len() < 2 {
            return Ok(());
        }
        let stored: Vec<(String, String)> = {
            let mut stmt = self.db.prepare_cached("SELECT name, origin FROM credit_parts WHERE credit = ?1 ORDER BY position")?;
            let rows = stmt.query_map(params![credit], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let same = stored.len() == names.len() && stored.iter().zip(&names).all(|((name, origin), new)| origin == "stated" && name == new);
        if same {
            return Ok(());
        }
        let tx = self.own_transaction()?;
        self.put_parts(credit, &names, "stated")?;
        tx.finish()
    }

    /// Finds the credits whose every part is an artist the library has on
    /// its own, and files their tracks under those artists. It also takes
    /// back a match that no longer holds. Returns how many credits changed.
    ///
    /// It reads first and writes only what changed, so a library where
    /// nothing did is not written to.
    pub fn match_credits(&self) -> rusqlite::Result<usize> {
        let credits: Vec<String> = {
            let mut stmt = self.db.prepare(
                "SELECT DISTINCT trim(artist) FROM tracks WHERE present = 1 AND trim(artist) <> ''
                 UNION SELECT DISTINCT trim(album_credit) FROM tracks WHERE present = 1 AND trim(album_credit) <> ''
                 UNION SELECT DISTINCT trim(album_artist) FROM albums WHERE trim(album_artist) <> ''",
            )?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut stated: HashSet<String> = HashSet::new();
        let mut matched: HashMap<String, Vec<String>> = HashMap::new();
        let mut known: HashSet<String> = credits.iter().map(|c| key(c)).collect();
        {
            let mut stmt = self.db.prepare("SELECT credit, name, origin FROM credit_parts ORDER BY credit, position")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
            for row in rows {
                let (credit, name, origin) = row?;
                if origin == "stated" {
                    known.insert(key(&name));
                    stated.insert(key(&credit));
                } else {
                    matched.entry(key(&credit)).or_default().push(name);
                }
            }
        }
        let is_known = |name: &str| known.contains(&key(name));
        let mut changes: Vec<(String, Option<Vec<String>>)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for credit in &credits {
            let k = key(credit);
            if stated.contains(&k) || !seen.insert(k.clone()) {
                continue;
            }
            let found = split_credit(credit, &is_known);
            if found.as_ref() != matched.get(&k) {
                changes.push((credit.clone(), found));
            }
        }
        // A match whose credit is on no track any more.
        for gone in matched.keys().filter(|k| !seen.contains(*k)) {
            changes.push((gone.clone(), None));
        }
        if changes.is_empty() {
            return Ok(0);
        }
        let tx = self.own_transaction()?;
        for (credit, names) in &changes {
            match names {
                Some(names) => self.put_parts(credit, names, "matched")?,
                None => {
                    self.db.execute("DELETE FROM credit_parts WHERE credit = ?1 AND origin = 'matched'", params![credit])?;
                    self.file_again(credit)?;
                }
            }
        }
        tx.finish()?;
        Ok(changes.len())
    }

    fn put_parts(&self, credit: &str, names: &[String], origin: &str) -> rusqlite::Result<()> {
        self.db.execute("DELETE FROM credit_parts WHERE credit = ?1", params![credit])?;
        let mut insert = self.db.prepare_cached("INSERT INTO credit_parts (credit, position, name, origin) VALUES (?1, ?2, ?3, ?4)")?;
        for (position, name) in names.iter().enumerate() {
            insert.execute(params![credit, position as i64, name, origin])?;
        }
        self.file_again(credit)
    }

    /// Files again what carries that credit. Its tracks and records are
    /// written as they were, which is what makes the library file them.
    fn file_again(&self, credit: &str) -> rusqlite::Result<()> {
        self.db.execute(
            "UPDATE tracks SET album_credit = album_credit
             WHERE trim(artist) = ?1 COLLATE NOCASE OR trim(album_credit) = ?1 COLLATE NOCASE",
            params![credit],
        )?;
        self.db.execute("UPDATE albums SET album_artist = album_artist WHERE trim(album_artist) = ?1 COLLATE NOCASE", params![credit])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
