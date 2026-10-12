//! What the window is handed. It goes out as JSON, field for field.

use super::artwork::Candidate;
use super::draft::{Cell, Draft};
use super::fields::{self, Field, ALBUM, TRACK};
use super::{Editor, Mode};
use chilpa_meta::{Hit, Release};
use chilpa_tags::edit::{Id3Version, Layers, Refusal};

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct MetadataView {
    /// Moves with every change, for the bindings that read it.
    pub rev: u32,
    /// "closed" | "opening" | "ready" | "saving"
    pub state: &'static str,
    pub session: u64,
    pub album_id: i64,
    /// The row to show first; -1 for none.
    pub focus: i32,
    /// Somebody changed something that a save would write.
    pub dirty: bool,
    /// The last refusal or failure, in the user's language. Empty if none.
    pub error: String,
    /// The last thing that went well. Empty if none.
    pub notice: String,
    pub album: Vec<FieldView>,
    pub tracks: Vec<TrackView>,
    pub cover: CoverView,
    pub save: SaveView,
    pub layers: LayersView,
    pub lookup: LookupView,
    pub artwork: ArtworkView,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct FieldView {
    pub name: &'static str,
    pub value: String,
    pub mixed: bool,
    pub dirty: bool,
    pub overridden: bool,
    pub editable: bool,
    /// Why the value cannot be saved. Empty when it can.
    pub invalid: String,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct TrackView {
    pub id: i64,
    /// The file's name, or what the track goes by inside its disc.
    pub file: String,
    pub inside_a_disc: bool,
    pub fields: Vec<FieldView>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct CoverView {
    /// A file on disk: what is in wait, or what the record has.
    pub path: String,
    pub waiting: bool,
    pub width: u32,
    pub height: u32,
    /// "file" | "musicbrainz" | "discogs" | "" for the one it already had.
    pub from: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct SaveView {
    /// "sidecar" | "files"
    pub mode: &'static str,
    pub files_allowed: bool,
    /// Why not. Empty when allowed.
    pub files_refused: String,
    /// "2.4" | "2.3"
    pub id3: &'static str,
    pub secondary_layers: bool,
    pub done: usize,
    pub total: usize,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct LayerView {
    pub name: &'static str,
    pub files: usize,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct LayersView {
    pub files: usize,
    pub canonical: Vec<LayerView>,
    pub present: Vec<LayerView>,
    pub conflicting: usize,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct LookupView {
    /// "musicbrainz" | "discogs"
    pub provider: &'static str,
    pub searching: bool,
    pub loading: bool,
    pub hits: Vec<Hit>,
    /// The release loaded, ready to be applied.
    pub release: Option<Release>,
    /// Zero applies all discs; otherwise the numbered medium to apply.
    pub selected_disc: u16,
    /// How many of the record's tracks that release has a track for.
    pub matched: usize,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct CandidateView {
    /// Its place in the list, which is what choosing one takes.
    pub index: usize,
    /// A file on disk.
    pub preview: String,
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct ArtworkView {
    pub provider: &'static str,
    pub searching: bool,
    pub loading: bool,
    pub candidates: Vec<CandidateView>,
}

fn field_view(field: &Field, cell: &Cell, of_a_track: bool, editable: bool) -> FieldView {
    FieldView {
        name: field.name,
        value: cell.now.clone(),
        mixed: cell.mixed,
        dirty: cell.dirty(),
        overridden: cell.overridden,
        editable,
        invalid: if cell.mixed { String::new() } else { fields::invalid(field, of_a_track, &cell.now) },
    }
}

/// Why the files cannot be written to, with the first reason that applies.
/// Empty when they can.
pub(crate) fn files_refused(draft: &Draft, layers: &Layers) -> String {
    if draft.rows.iter().any(|row| row.inside_a_disc()) {
        return chilpa_i18n::t("tracks inside a disc image keep their changes in a sidecar");
    }
    let refused = |why: Refusal| layers.refusals.iter().any(|r| *r == Some(why));
    if refused(Refusal::Dsd) {
        return chilpa_i18n::t("DSD files keep their changes in a sidecar");
    }
    if refused(Refusal::Missing) {
        return chilpa_i18n::t("one or more files are not where they were");
    }
    if refused(Refusal::ReadOnly) {
        return chilpa_i18n::t("one or more files cannot be written to");
    }
    if !layers.all_writable() {
        return chilpa_i18n::t("one or more files cannot take tags");
    }
    String::new()
}

fn layers_view(layers: &Layers) -> LayersView {
    LayersView {
        files: layers.files,
        canonical: layers.layers.iter().filter(|l| l.canonical > 0).map(|l| LayerView { name: l.name, files: l.canonical }).collect(),
        present: layers.layers.iter().map(|l| LayerView { name: l.name, files: l.files }).collect(),
        conflicting: layers.conflicting,
    }
}

fn candidate_view(index: usize, candidate: &Candidate) -> CandidateView {
    CandidateView {
        index,
        preview: candidate.preview.to_string_lossy().into_owned(),
        title: candidate.title.clone(),
        detail: candidate.detail.clone(),
    }
}

impl Editor {
    /// Writes down what the window is to see. Called after every change.
    pub(crate) fn refresh(&mut self) {
        let rev = self.view.rev.wrapping_add(1);
        let mut view = MetadataView {
            rev,
            state: self.state.name(),
            session: self.session,
            album_id: self.album_id,
            focus: self.focus,
            // What was said stays said until something else is.
            error: self.error.clone(),
            notice: self.notice.clone(),
            ..MetadataView::default()
        };

        if let Some(draft) = &self.draft {
            view.dirty = self.waiting.is_some()
                || (draft.touched && draft.album.iter().chain(draft.rows.iter().flat_map(|r| r.cells.iter())).any(Cell::dirty));
            view.album = ALBUM.iter().zip(&draft.album).map(|(f, cell)| field_view(f, cell, false, true)).collect();
            view.tracks = draft
                .rows
                .iter()
                .map(|row| TrackView {
                    id: row.track.id,
                    file: row.key.clone(),
                    inside_a_disc: row.inside_a_disc(),
                    fields: TRACK
                        .iter()
                        .zip(&row.cells)
                        .map(|(f, cell)| field_view(f, cell, true, !(f.name == "track_number" && row.inside_a_disc())))
                        .collect(),
                })
                .collect();
            let refused = files_refused(draft, &self.layers);
            view.save = SaveView {
                mode: if self.mode == Mode::Files { "files" } else { "sidecar" },
                files_allowed: refused.is_empty(),
                files_refused: refused,
                id3: if self.id3 == Id3Version::V23 { "2.3" } else { "2.4" },
                secondary_layers: self.secondary_layers,
                done: self.progress.0,
                total: self.progress.1,
            };
            view.lookup.matched = self.release.as_ref().map_or(0, |r| super::apply::pair_for(draft, r, self.selected_disc).iter().flatten().count());
        }
        view.cover = match &self.waiting {
            Some(waiting) => CoverView {
                path: waiting.preview.to_string_lossy().into_owned(),
                waiting: true,
                width: waiting.width,
                height: waiting.height,
                from: waiting.from,
            },
            None => CoverView { path: self.cover.clone(), ..CoverView::default() },
        };
        view.layers = layers_view(&self.layers);
        view.lookup.provider = self.provider.name();
        view.lookup.searching = self.searching;
        view.lookup.loading = self.loading;
        view.lookup.hits = self.hits.clone();
        view.lookup.release = self.release.clone();
        view.lookup.selected_disc = self.selected_disc;
        view.artwork = ArtworkView {
            provider: self.artwork_provider.name(),
            searching: self.artwork_searching,
            loading: self.artwork_loading,
            candidates: self.candidates.iter().enumerate().map(|(i, c)| candidate_view(i, c)).collect(),
        };
        self.view = view;
    }
}
