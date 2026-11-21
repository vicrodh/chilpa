//! Field proposals over a stable editor draft. Creating a projection is read-only;
//! selections mutate the form only when Apply is requested, never the files.
use super::{apply, draft::{Cell, Draft}, fields, Editor};
use chilpa_library::sidecar::join;
use chilpa_meta::{display, Provider, Release};

pub(crate) struct Change {
    pub key: String,
    pub field: &'static str,
    pub row: Option<usize>,
    pub value: String,
    pub selected: bool,
    pub restore: bool,
    at: usize,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ChangeView {
    pub key: String,
    pub field: &'static str,
    pub label: String,
    pub track: Option<i64>,
    pub title: String,
    pub current: String,
    pub current_mixed: bool,
    pub proposed: String,
    pub proposed_mixed: bool,
    pub selected: bool,
    pub invalid: String,
}

impl Change {
    fn cell<'a>(&self, draft: &'a Draft) -> &'a Cell {
        self.row.map_or(&draft.album[self.at], |row| &draft.rows[row].cells[self.at])
    }
    pub fn differs(&self, draft: &Draft) -> bool {
        let cell = self.cell(draft);
        let (value, mixed) = if self.restore { cell.initial() } else { (self.value.as_str(), false) };
        let field = if self.row.is_some() { &fields::TRACK[self.at] } else { &fields::ALBUM[self.at] };
        cell.mixed != mixed || !fields::same(field.kind, &cell.now, value)
    }
    pub fn view(&self, draft: &Draft) -> ChangeView {
        let cell = self.cell(draft);
        let (proposed, mixed) = if self.restore { let (value, mixed) = cell.initial(); (value, mixed) } else { (self.value.as_str(), false) };
        let definition = if self.row.is_some() { &fields::TRACK[self.at] } else { &fields::ALBUM[self.at] };
        ChangeView { key:self.key.clone(), field:self.field, label:fields::label(self.field,self.row.is_some()), track:self.row.map(|row| draft.rows[row].track.id),
            title:self.row.map(|row| draft.rows[row].track.title.clone()).unwrap_or_default(), current:cell.now.clone(),
            current_mixed:cell.mixed, proposed:proposed.to_string(), proposed_mixed:mixed, selected:self.selected,
            invalid:if mixed { String::new() } else { fields::invalid(definition, self.row.is_some(), proposed) } }
    }
    pub fn apply(&self, draft: &mut Draft) {
        if self.restore {
            if let Some(row) = self.row { draft.rows[row].cells[self.at].revert(); }
            else { draft.album[self.at].revert(); }
        } else if let Some(row) = self.row {
            draft.set_track(draft.rows[row].track.id, self.field, &self.value);
        } else { draft.set_album(self.field, &self.value); }
    }
}

pub(crate) struct Projection { pub changes:Vec<Change>, pub matched:usize }

impl Projection {
    fn push(&mut self, draft: &Draft, row: Option<usize>, name: &'static str, value: String, restore: bool) {
        if !restore && value.is_empty() { return; }
        let at = if row.is_some() { fields::track(name) } else { fields::album(name) };
        let Some(at) = at else { return };
        if row.is_some_and(|row| name == "track_number" && draft.rows[row].inside_a_disc()) { return; }
        let field = if row.is_some() { &fields::TRACK[at] } else { &fields::ALBUM[at] };
        let value = fields::normalize(field.kind, &value);
        let key = row.map_or_else(|| format!("album:{name}"), |row| format!("track:{}:{name}", draft.rows[row].track.id));
        let mut change = Change { key, field:name, row, value, selected:name != "genres", restore, at };
        let view = change.view(draft);
        if view.current_mixed == view.proposed_mixed && fields::same(field.kind, &view.current, &view.proposed) { return; }
        if !view.invalid.is_empty() { change.selected = false; }
        self.changes.push(change);
    }
}

pub(crate) fn project(draft: &Draft, release: &Release, medium: u16, spelled: &dyn Fn(&str) -> String, explicit: Option<&[Option<usize>]>) -> Projection {
    let pairs = explicit.map(|pairs| pairs.to_vec()).unwrap_or_else(|| apply::pair_for(draft, release, medium));
    let mut plan = Projection { changes:Vec::new(), matched:0 };
    let from_musicbrainz = release.provider == Provider::MusicBrainz;
    let at_disc = fields::track("disc_number").unwrap_or(0);
    let local_discs: std::collections::BTreeSet<u16> = draft.rows.iter().filter_map(|row| row.cells[at_disc].now.parse().ok()).collect();
    let whole_box = local_discs.len() > 1;
    if whole_box && release.catalog_numbers.len() > 1 {
        let cell = draft.album_cell("catalog_number");
        if cell.dirty() && release.catalog_numbers.iter().any(|number| number.eq_ignore_ascii_case(&cell.now)) {
            plan.push(draft, None, "catalog_number", String::new(), true);
        }
    }
    let genres: Vec<_> = release.genres.iter().map(|genre| spelled(genre)).collect();
    let mut album = vec![("title",release.title.clone()), ("album_artist",display(&release.credits)),
        ("album_artists",apply::names(&release.credits)), ("genres",join(&genres)), ("label",release.label.clone())];
    let catalog = if whole_box || medium == 0 && release.catalog_numbers.len() > 1 { String::new() }
        else if medium > 0 { release.catalog_numbers.get(usize::from(medium - 1)).cloned().unwrap_or_else(|| release.catalog_number.clone()) }
        else { release.catalog_number.clone() };
    album.push(("catalog_number",catalog));
    if fields::is_date(&release.date) { album.push(("date",release.date.clone())); }
    if from_musicbrainz {
        album.extend([("musicbrainz_release_id",release.id.clone()), ("musicbrainz_release_group_id",release.release_group_id.clone()),
            ("musicbrainz_album_artist_ids",apply::ids(&release.credits))]);
    } else { album.push(("discogs_release_id",release.id.clone())); }
    if medium == 0 || !whole_box { for (name, value) in album { plan.push(draft, None, name, value, false); } }
    for (row, found) in pairs.iter().enumerate().take(draft.rows.len()) {
        let Some(track) = found.and_then(|at| release.tracks.get(at)).filter(|track| medium == 0 || track.disc == medium) else { continue };
        plan.matched += 1;
        let mut values = vec![("title",track.title.clone())];
        if track.disc > 0 {
            values.push(("disc_number",track.disc.to_string()));
            if let Some(title) = release.media_titles.get(usize::from(track.disc - 1)).filter(|title| !title.trim().is_empty()) {
                values.push(("disc_subtitle",title.trim().to_string()));
            }
        }
        if track.number > 0 { values.push(("track_number",track.number.to_string())); }
        if !track.credits.is_empty() {
            values.extend([("artist_credit",display(&track.credits)), ("artists",apply::names(&track.credits))]);
            if from_musicbrainz { values.push(("musicbrainz_artist_ids",apply::ids(&track.credits))); }
        }
        if from_musicbrainz { values.extend([("musicbrainz_recording_id",track.recording_id.clone()), ("musicbrainz_track_id",track.track_id.clone())]); }
        for (name,value) in values { plan.push(draft, Some(row), name, value, false); }
    }
    plan
}

impl Editor {
    pub(crate) fn rebuild_preview(&mut self) {
        self.changes.clear(); self.preview_matched = 0;
        let (Some(draft), Some(release)) = (&self.draft, &self.release) else { return };
        let pairs = self.beets.pairs(draft, release);
        let mut plan = project(draft, release, self.selected_disc, &str::to_string, pairs.as_deref());
        if let Some(candidate) = self.beets.selected_candidate() {
            for mapping in &candidate.mappings {
                let Some(row) = draft.row(mapping.track) else { continue };
                if self.selected_disc > 0 && release.tracks.get(mapping.index).is_none_or(|track| track.disc != self.selected_disc) { continue; }
                plan.push(draft, Some(row), "composers", join(&mapping.composers), false);
                let title = release.tracks.get(mapping.index).map_or("", |track| track.title.as_str());
                plan.push(draft, Some(row), "work", chilpa_library::work_titles::proposed_work(&mapping.work, title).unwrap_or_default(), false);
            }
        }
        self.preview_matched = plan.matched;
        self.changes = plan.changes;
    }
}

impl crate::Chilpa {
    pub fn metadata_choose_change(&mut self, key: &str, selected: bool) -> bool {
        let editor = &mut self.metadata;
        if editor.state != super::State::Ready { return false; }
        let Some(change) = editor.changes.iter_mut().find(|change| change.key == key) else { return false };
        if selected && editor.draft.as_ref().is_none_or(|draft| !change.view(draft).invalid.is_empty()) { return false; }
        change.selected = selected; editor.refresh(); true
    }
}
