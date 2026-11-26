//! Deleting what a record's sidecars say of it. The entries of its tracks
//! leave each sidecar, and so does what it says of the record when no other
//! file is named in it; an empty sidecar is removed with its covers. The
//! library then reads the files again, as a scan would.

use super::draft::Draft;
use super::worker::{Done, Job};
use super::State;
use crate::Chilpa;
use chilpa_library::sidecar::{stage, Home, Sidecar, SidecarError, VERSION};
use std::path::{Path, PathBuf};

/// One sidecar as it is left once the record is taken out of it.
pub(crate) struct Dropped {
    pub home: Home,
    pub sidecar: Sidecar,
}

/// What the worker is asked to do.
pub(crate) struct DropPlan {
    pub homes: Vec<Dropped>,
    /// The record's files, read again once their sidecars are gone.
    pub files: Vec<(i64, String)>,
    /// A record inside an image is read again by a scan: its tracks are not
    /// files of their own.
    pub contained: bool,
    pub library: PathBuf,
    pub now: i64,
}

/// The record's sidecars, each without the record. Homes on a server keep
/// their words in the library and are not touched here.
pub(crate) fn dropped(draft: &Draft, now: i64) -> Vec<Dropped> {
    draft.homes.iter().enumerate().filter(|(_, state)| !state.home.is_hosted()).filter_map(|(at, state)| {
        let mut sidecar = state.sidecar.clone()?;
        for row in draft.rows.iter().filter(|row| row.home == at) {
            sidecar.tracks.remove(&row.key);
        }
        // Nothing else is named in it: what it says of the record goes too.
        if sidecar.tracks.is_empty() {
            sidecar = Sidecar::new(now);
        }
        Some(Dropped { home: state.home.clone(), sidecar })
    }).collect()
}

/// Does it. Runs on the worker.
pub(crate) fn run(session: u64, plan: DropPlan) -> Done {
    let failed = |reason: String| Done::Dropped { session, failed: Some(reason), contained: plan.contained };
    for dropped in &plan.homes {
        let landed = stage(&dropped.home, dropped.sidecar.clone(), None, plan.now).and_then(|staged| staged.land());
        if let Err(error) = landed {
            return failed(match error {
                SidecarError::Newer(_) => chilpa_i18n::t("a newer Chilpa wrote this sidecar; update Chilpa to change it"),
                other => chilpa_i18n::t_args("the sidecar could not be deleted: {}", &[&other.to_string()]),
            });
        }
    }
    if !plan.files.is_empty() {
        let library = match chilpa_library::Library::open_existing(&plan.library) {
            Ok(library) => library,
            Err(error) => return failed(chilpa_i18n::t_args("the sidecar was deleted, and the library could not read the files again: {}; rescan to see them", &[&error.to_string()])),
        };
        for (id, file) in &plan.files {
            let read = chilpa_library::scan::reconcile_metadata_file(&library, *id, file, Path::new(file))
                .and_then(|()| chilpa_library::scan::reclaim_cover(&library, *id, Path::new(file), &chilpa_core::artwork_cache()).map_err(|error| error.to_string()));
            if let Err(error) = read {
                return failed(chilpa_i18n::t_args("the sidecar was deleted, and the library could not read the files again: {}; rescan to see them", &[&error]));
            }
        }
    }
    Done::Dropped { session, failed: None, contained: plan.contained }
}

impl Chilpa {
    /// Deletes what the open record's sidecars say of it. False when it is
    /// refused; why is in the editor's error.
    pub fn metadata_drop_sidecar(&mut self) -> bool {
        if self.metadata.state != State::Ready {
            return false;
        }
        if self.metadata.beets.busy() { self.metadata_cancel_lookup(); }
        let scanning = self.scan.is_some();
        let library = chilpa_library::Library::default_path();
        let editor = &mut self.metadata;
        let Some(draft) = editor.draft.as_ref() else { return false };
        if scanning {
            return editor.refuse(chilpa_i18n::t("a scan is running; save when it is done"));
        }
        let now = crate::now_unix();
        let homes = dropped(draft, now);
        if homes.is_empty() {
            return editor.refuse(chilpa_i18n::t("this record has no sidecar"));
        }
        if draft.homes.iter().any(|state| !state.home.is_hosted() && state.sidecar.as_ref().is_some_and(|s| s.version > VERSION)) {
            return editor.refuse(chilpa_i18n::t("a newer Chilpa wrote this sidecar; update Chilpa to change it"));
        }
        let within: Vec<usize> = homes.iter().filter_map(|dropped| draft.homes.iter().position(|state| state.home == dropped.home)).collect();
        let rows = draft.rows.iter().filter(|row| within.contains(&row.home));
        let contained = rows.clone().any(|row| row.track.container.is_some());
        let files = rows.filter(|row| row.track.container.is_none() && !row.hosted())
            .map(|row| (row.track.id, row.track.external_id.clone())).collect();
        let plan = DropPlan { homes, files, contained, library, now };

        editor.state = State::Saving;
        editor.progress = (0, 0);
        editor.error.clear();
        editor.notice.clear();
        editor.lookup_turn += 1;
        editor.artwork_turn += 1;
        editor.searching = false;
        editor.loading = false;
        editor.artwork_searching = false;
        editor.artwork_loading = false;
        let session = editor.session;
        if !editor.ask(Job::Drop { session, plan }) {
            editor.state = State::Ready;
            return editor.refuse(chilpa_i18n::t("the editor could not be started"));
        }
        editor.refresh();
        true
    }

    /// What deleting the sidecars left: the library is told, and the
    /// editor lets the record go, as after a save.
    pub(super) fn metadata_dropped(&mut self, failed: Option<String>, contained: bool) {
        self.mark_library_changed();
        // An image's tracks are read again by the scan, which knows them.
        if contained && failed.is_none() {
            self.rescan();
        }
        let album_id = self.metadata.album_id;
        let editor = &mut self.metadata;
        editor.forget();
        editor.album_id = album_id;
        editor.notice = if failed.is_none() { chilpa_i18n::t("the sidecar was deleted") } else { String::new() };
        editor.error = failed.unwrap_or_default();
        editor.refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::super::draft::testing::*;
    use super::*;
    use chilpa_library::sidecar::Override;

    #[test]
    fn the_record_leaves_its_sidecar_and_an_empty_one_is_empty() {
        let mut sidecar = Sidecar::new(0);
        sidecar.album.catalog_number = Override::Value("CAT-1".into());
        sidecar.track_mut("01.flac").title = Override::Value("Laid over".into());
        sidecar.track_mut("02.flac").title = Override::Value("Also laid over".into());
        let draft = Draft::open(&album(), vec![track(1, 1), track(2, 2)], vec![read(1), read(2)], vec![home(Some(sidecar))]);
        let dropped = dropped(&draft, 7);
        assert_eq!(dropped.len(), 1);
        assert!(dropped[0].sidecar.is_empty(), "nothing else was named in it");
    }

    #[test]
    fn another_record_s_files_keep_what_the_sidecar_says_of_them() {
        let mut sidecar = Sidecar::new(0);
        sidecar.album.catalog_number = Override::Value("CAT-1".into());
        sidecar.track_mut("01.flac").title = Override::Value("Laid over".into());
        sidecar.track_mut("other.flac").title = Override::Value("Not this record".into());
        let draft = Draft::open(&album(), vec![track(1, 1)], vec![read(1)], vec![home(Some(sidecar))]);
        let dropped = dropped(&draft, 7);
        assert!(!dropped[0].sidecar.tracks.contains_key("01.flac"));
        assert_eq!(dropped[0].sidecar.tracks["other.flac"].title, Override::Value("Not this record".into()));
        assert_eq!(dropped[0].sidecar.album.catalog_number, Override::Value("CAT-1".into()), "it still speaks for the folder");
    }

    #[test]
    fn a_record_without_a_sidecar_has_nothing_to_delete() {
        let draft = Draft::open(&album(), vec![track(1, 1)], vec![read(1)], vec![home(None)]);
        assert!(dropped(&draft, 7).is_empty());
    }
}
