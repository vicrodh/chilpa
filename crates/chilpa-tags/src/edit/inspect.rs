//! What tags a record's files carry, and whether they can be written.
//!
//! Nothing here writes. It is what the editor asks before it offers to
//! write into the files, and what the writer asks before it copies the
//! first one.

use lofty::file::{TaggedFile, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::tag::{Accessor, Tag, TagType};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Why a file cannot take tags.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Refusal {
    Missing,
    /// It opens for reading and not for writing.
    ReadOnly,
    /// DSF and DFF: read by this crate, written by nobody yet.
    Dsd,
    Unreadable,
    /// The format's canonical tag cannot be written by this build.
    Format,
}

/// One kind of tag, across the files looked at.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Layer {
    pub name: &'static str,
    /// How many files carry it.
    pub files: usize,
    /// In how many of them it is the canonical one.
    pub canonical: usize,
    /// In how many of them it can be written.
    pub writable: usize,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Layers {
    /// How many different files were looked at.
    pub files: usize,
    /// By name.
    pub layers: Vec<Layer>,
    /// Files where two layers disagree on a field the editor owns.
    pub conflicting: usize,
    /// One per path given, in the same order. `None` takes tags.
    pub refusals: Vec<Option<Refusal>>,
}

impl Layers {
    /// Every file takes tags, and there is at least one.
    pub fn all_writable(&self) -> bool {
        self.files > 0 && self.refusals.iter().all(Option::is_none)
    }
}

pub(crate) fn layer_name(kind: TagType) -> &'static str {
    match kind {
        TagType::Ape => "APEv2",
        TagType::Id3v1 => "ID3v1",
        TagType::Id3v2 => "ID3v2",
        TagType::Mp4Ilst => "MP4 ilst",
        TagType::VorbisComments => "Vorbis comments",
        TagType::RiffInfo => "RIFF INFO",
        TagType::AiffText => "AIFF text",
        _ => "Other",
    }
}

#[derive(Default)]
struct Count {
    files: usize,
    canonical: usize,
    writable: usize,
}

/// Looks at every file. It never fails as a whole: a file that cannot be
/// read is one answer among the others.
pub fn inspect(paths: &[PathBuf]) -> Layers {
    let mut counts: BTreeMap<&'static str, Count> = BTreeMap::new();
    let mut answers: HashMap<&Path, Option<Refusal>> = HashMap::new();
    let mut out = Layers::default();
    for path in paths {
        // The same file twice answers what it answered the first time.
        if let Some(known) = answers.get(path.as_path()) {
            out.refusals.push(*known);
            continue;
        }
        out.files += 1;
        let answer = look(path, &mut counts, &mut out.conflicting);
        answers.insert(path.as_path(), answer);
        out.refusals.push(answer);
    }
    out.layers = counts
        .into_iter()
        .map(|(name, c)| Layer { name, files: c.files, canonical: c.canonical, writable: c.writable })
        .collect();
    out
}

fn look(path: &Path, counts: &mut BTreeMap<&'static str, Count>, conflicting: &mut usize) -> Option<Refusal> {
    if !path.is_file() {
        return Some(Refusal::Missing);
    }
    if crate::dsf::is_dsf(path) || crate::dff::is_dff(path) {
        return Some(Refusal::Dsd);
    }
    let Ok(file) = lofty::read_from_path(path) else {
        return Some(Refusal::Unreadable);
    };
    let primary = file.primary_tag_type();
    for tag in file.tags() {
        let count = counts.entry(layer_name(tag.tag_type())).or_default();
        count.files += 1;
        count.canonical += usize::from(tag.tag_type() == primary);
        count.writable += usize::from(file.tag_support(tag.tag_type()).is_writable());
    }
    *conflicting += usize::from(disagrees(&file));
    if !file.tag_support(primary).is_writable() {
        return Some(Refusal::Format);
    }
    // Opened and closed: nothing is written and nothing is truncated.
    if std::fs::OpenOptions::new().read(true).write(true).open(path).is_err() {
        return Some(Refusal::ReadOnly);
    }
    None
}

/// Whether two of a file's tags say different things about a field the
/// editor owns.
fn disagrees(file: &TaggedFile) -> bool {
    const TEXT: [ItemKey; 8] = [
        ItemKey::TrackTitle,
        ItemKey::TrackArtist,
        ItemKey::AlbumTitle,
        ItemKey::AlbumArtist,
        ItemKey::Genre,
        ItemKey::RecordingDate,
        ItemKey::Year,
        ItemKey::CatalogNumber,
    ];
    let text_differs = |key: ItemKey| {
        let mut said: Vec<String> = Vec::new();
        for tag in file.tags() {
            let Some(value) = tag.get_string(key).map(str::trim).filter(|v| !v.is_empty()) else {
                continue;
            };
            if !said.iter().any(|s| s.eq_ignore_ascii_case(value)) {
                said.push(value.to_string());
            }
        }
        said.len() > 1
    };
    if TEXT.into_iter().any(text_differs) {
        return true;
    }
    let number_differs = |of: fn(&Tag) -> Option<u32>| {
        let mut said: Vec<u32> = file.tags().iter().filter_map(of).collect();
        said.sort_unstable();
        said.dedup();
        said.len() > 1
    };
    number_differs(|tag| tag.track()) || number_differs(|tag| tag.disk())
}
