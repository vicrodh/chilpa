//! I keep file evidence separate from the album row chosen to present it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use crate::album_folder::disc_folder;

pub(super) fn folded(text: &str) -> String { text.trim().to_lowercase() }

pub(crate) fn location(path: &str) -> String {
    if let Some(image) = path.strip_prefix("sacd:") {
        image.rsplit_once('#').map_or(image, |(file, _)| file).to_owned()
    } else {
        Path::new(path).parent().unwrap_or(Path::new("")).display().to_string()
    }
}

#[derive(Clone)]
pub(super) struct Track {
    pub id: i64,
    pub album: i64,
    pub source: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub credit: String,
    pub disc: Option<u16>,
    pub total: Option<u16>,
    pub present: bool,
    pub year: Option<i64>,
    pub artwork: Option<String>,
}

pub(super) struct Part {
    pub source: i64,
    pub location: String,
    pub title: String,
    pub artist: String,
    pub tracks: Vec<Track>,
    pub container: String,
    pub discs: BTreeSet<u16>,
    pub totals: BTreeSet<u16>,
    pub folder_disc: Option<u16>,
    pub complete: bool,
    credits: BTreeSet<String>,
    confirmed_folder: bool,
    uncredited: bool,
}

impl Part {
    pub fn first_disc(&self) -> u16 { self.discs.first().copied().unwrap_or(u16::MAX) }
    fn folder(&self) -> &Path {
        let path = Path::new(&self.location);
        if self.tracks[0].path.starts_with("sacd:") { path.parent().unwrap_or(path) } else { path }
    }
}

pub(super) fn collect(tracks: Vec<Track>) -> Vec<Part> {
    let mut groups = BTreeMap::<(i64, String, String), Vec<Track>>::new();
    for track in tracks {
        groups.entry((track.source, location(&track.path), track.title.clone())).or_default().push(track);
    }
    let mut parts: Vec<_> = groups.into_iter().map(|((source, location, _), tracks)| {
        let current: Vec<_> = tracks.iter().filter(|t| t.present).collect();
        let credited: Vec<_> = if current.is_empty() { tracks.iter().collect() } else { current.clone() };
        let credits: BTreeMap<_, _> = credited.iter().map(|t| {
            let credit = if t.credit.trim().is_empty() { &t.artist } else { &t.credit };
            (folded(credit), credit.trim().to_owned())
        }).collect();
        let artist = if credits.len() == 1 { credits.values().next().unwrap().clone() }
            else { crate::VARIOUS_ARTISTS.to_owned() };
        Part { source, location, title: tracks[0].title.clone(), artist,
            container: String::new(), discs: current.iter().filter_map(|t| t.disc).collect(),
            totals: current.iter().filter_map(|t| t.total).collect(),
            complete: !current.is_empty() && current.iter().all(|t| t.disc.is_some()),
            uncredited: credited.iter().all(|t| t.credit.trim().is_empty()),
            credits: credits.into_keys().collect(), confirmed_folder: false,
            folder_disc: None, tracks }
    }).collect();
    // I confirm weak folder names against stored sibling parts, never the filesystem.
    let siblings: BTreeSet<(i64, PathBuf, String, String, &'static str, u16)> = parts.iter().filter_map(|p| {
        if !p.tracks.iter().any(|t| t.present) { return None; }
        let folder = p.folder();
        let disc = disc_folder(folder)?;
        Some((p.source, folder.parent()?.to_owned(), folded(&p.title), folded(&p.artist), disc.style, disc.number))
    }).collect();
    for p in &mut parts {
        let folder = p.folder().to_owned();
        let parent = folder.parent().unwrap_or(Path::new(""));
        let confirmed = disc_folder(&folder).filter(|disc| {
            !disc.needs_confirmation || (p.complete && p.discs == BTreeSet::from([disc.number])) ||
                siblings.iter().any(|(source, container, title, _, style, number)|
                    *source == p.source && container == parent && *title == folded(&p.title) &&
                    *style == disc.style && *number != disc.number)
        });
        p.container = if confirmed.is_some() || !p.tracks[0].path.starts_with("sacd:") {
            parent.display().to_string()
        } else { folder.display().to_string() };
        if confirmed.is_some() {
            let mut outer = parent;
            while disc_folder(outer).is_some_and(|disc| !disc.needs_confirmation) {
                let Some(parent) = outer.parent() else { break };
                outer = parent;
            }
            p.container = outer.display().to_string();
        }
        if let Some(disc) = confirmed {
            p.confirmed_folder = true;
            // I retain the raw TOC. A confirmed image folder can supply the set
            // position when independently authored images each report 1/1.
            let image_singleton = p.tracks[0].path.starts_with("sacd:") &&
                p.discs == BTreeSet::from([1]) && p.totals == BTreeSet::from([1]);
            if (!p.complete && p.discs.iter().all(|n| *n == disc.number)) || image_singleton {
                p.discs = BTreeSet::from([disc.number]);
                p.complete = p.tracks.iter().any(|t| t.present);
                p.folder_disc = Some(disc.number);
                if image_singleton { p.totals.clear(); }
            }
        }
    }
    // I carry compilation identity across confirmed disc folders when the
    // mixed-credit disc and the other discs share an actual album credit.
    // A different, unrelated album artist is not compilation evidence.
    let compilations: Vec<_> = parts.iter().filter(|p| p.confirmed_folder && p.credits.len() > 1)
        .map(|p| (p.source, folded(&p.title), p.container.clone(), p.credits.clone(), p.uncredited)).collect();
    for part in &mut parts {
        if part.confirmed_folder && compilations.iter().any(|(source, title, container, credits, uncredited)|
            *source == part.source && *title == folded(&part.title) && *container == part.container &&
            ((*uncredited && part.uncredited) || !credits.is_disjoint(&part.credits))) {
            part.artist = crate::VARIOUS_ARTISTS.to_owned();
        }
    }
    parts
}

/// I reject ambiguous candidates as a whole instead of guessing which disc 1 wins.
pub(super) fn assemble(parts: &[Part]) -> Vec<Vec<usize>> {
    let mut candidates = BTreeMap::<(i64, String, String, String), Vec<usize>>::new();
    let mut releases = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if part.complete && !part.discs.is_empty() && !part.title.is_empty() {
            candidates.entry((part.source, folded(&part.title), folded(&part.artist), part.container.clone())).or_default().push(i);
        } else { releases.push(vec![i]); }
    }
    for ids in candidates.into_values() {
        let totals: BTreeSet<_> = ids.iter().flat_map(|i| parts[*i].totals.iter().copied()).collect();
        let mut discs = BTreeSet::new();
        let compatible = totals.len() <= 1 && ids.iter().all(|i| parts[*i].discs.iter().all(|n|
            *n > 0 && totals.first().is_none_or(|total| n <= total) && discs.insert(*n)));
        if compatible { releases.push(ids); } else { releases.extend(ids.into_iter().map(|i| vec![i])); }
    }
    for ids in &mut releases { ids.sort_by_key(|i| (parts[*i].first_disc(), parts[*i].location.clone())); }
    releases.sort_by_key(|ids| (parts[ids[0]].source, parts[ids[0]].location.clone(), parts[ids[0]].title.clone()));
    releases
}
