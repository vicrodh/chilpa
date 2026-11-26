//! The metadata editor.
//!
//! One record at a time. Opening it takes a picture of its present tracks
//! that does not change while it is open: the window edits by field name
//! and track id, and a path never comes back from it.
//!
//! The form lives here and not in the window (frontend ADR-0004). What
//! reads files or goes out to the network runs on a thread of its own, and
//! what it finds comes back through `metadata_poll`.

mod apply;
pub(crate) use apply::said as failure_message;
mod artwork;
pub(crate) mod beets;
mod draft;
mod drop;
pub(crate) mod fields;
mod hosted;
mod preview;
mod save;
mod substitute;
pub use substitute::ReplaceField;
mod server;
pub mod view;
pub(crate) mod worker;

use crate::Chilpa;
use chilpa_meta::{Hit, Provider, Release};
use chilpa_tags::edit::{Id3Version, Layers};
use draft::Draft;
use std::sync::Arc;
use view::MetadataView;
use worker::{Done, Job, Worker};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum State {
    #[default]
    Closed,
    Opening,
    Ready,
    Saving,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            State::Closed => "closed",
            State::Opening => "opening",
            State::Ready => "ready",
            State::Saving => "saving",
        }
    }
}

/// Where a save writes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Mode {
    #[default]
    Sidecar,
    Files,
    Xml,
    /// A record on a server: to the server.
    Server,
    /// A record on a server that cannot take the changes: nothing until
    /// the listener chooses the sidecar. A sidecar is never made unasked.
    Undecided,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::Sidecar => "sidecar",
            Mode::Files => "files",
            Mode::Xml => "xml",
            Mode::Server => "server",
            Mode::Undecided => "",
        }
    }
}

pub(crate) struct Editor {
    state: State,
    /// Moves every time a record is opened or closed. What comes back from
    /// the worker for another session is dropped.
    session: u64,
    /// The last search or load asked for, and the last thing asked of the
    /// covers. What comes back for an earlier one is dropped.
    lookup_turn: u64,
    artwork_turn: u64,
    album_id: i64,
    focus: i32,
    focus_track: i64,
    album: Option<chilpa_library::StoredAlbum>,
    draft: Option<Draft>,
    layers: Layers,
    mode: Mode,
    /// Protect my library as of the last open or change of the setting, so a
    /// refresh does not read the settings file. Saves read the stored value.
    protection: crate::library_protection::Protection,
    id3: Id3Version,
    secondary_layers: bool,
    provider: Provider,
    beets: beets::BeetsState,
    changes: Vec<preview::Change>,
    preview_matched: usize,
    searching: bool,
    loading: bool,
    hits: Vec<Hit>,
    release: Option<Release>,
    selected_disc: u16,
    artwork_provider: Provider,
    artwork_searching: bool,
    artwork_loading: bool,
    candidates: Vec<artwork::Candidate>,
    waiting: Option<artwork::Waiting>,
    /// The cover the record has, on disk.
    cover: String,
    /// The homes on a server a save is going to write once the worker has
    /// landed the rest.
    pending_hosted: Vec<save::HomePlan>,
    /// The server of a record on one: what it lets be done.
    server: Option<server::ServerState>,
    /// The network and the credential every server is asked with. Tests
    /// only: nothing in one goes out.
    server_access: Option<(Arc<dyn chilpa_servers::Net>, chilpa_servers::Credential)>,
    progress: (usize, usize),
    error: String,
    notice: String,
    view: MetadataView,
    worker: Option<Worker>,
    network: Arc<dyn chilpa_meta::Http>,
    credential: chilpa_meta::discogs::CredentialProvider,
    credential_epoch: Arc<std::sync::atomic::AtomicU64>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            state: State::Closed,
            session: 0,
            lookup_turn: 0,
            artwork_turn: 0,
            album_id: 0,
            focus: -1,
            focus_track: 0,
            album: None,
            draft: None,
            layers: Layers::default(),
            mode: Mode::Sidecar,
            protection: Default::default(),
            id3: Id3Version::V24,
            secondary_layers: false,
            provider: Provider::MusicBrainz,
            beets: beets::BeetsState::default(),
            changes: Vec::new(),
            preview_matched: 0,
            searching: false,
            loading: false,
            hits: Vec::new(),
            release: None,
            selected_disc: 0,
            artwork_provider: Provider::MusicBrainz,
            artwork_searching: false,
            artwork_loading: false,
            candidates: Vec::new(),
            waiting: None,
            cover: String::new(),
            pending_hosted: Vec::new(),
            server: None,
            server_access: None,
            progress: (0, 0),
            error: String::new(),
            notice: String::new(),
            view: MetadataView { state: "closed", focus: -1, ..MetadataView::default() },
            worker: None,
            network: Arc::new(worker::Network::new()),
            credential: Arc::new(||chilpa_meta::discogs::Credential::Anonymous),
            credential_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }
}

impl Editor {
    pub(crate) fn saving(&self) -> bool {
        self.state == State::Saving
    }

    /// Says no, and why. Always false, so it can be returned.
    fn refuse(&mut self, why: String) -> bool {
        self.error = why;
        self.notice.clear();
        self.refresh();
        false
    }

    /// Hands the worker a job, starting it the first time. False when there
    /// is no worker to hand it to.
    fn ask(&mut self, job: Job) -> bool {
        if self.worker.is_none() {
            self.worker = Worker::start(self.network.clone(),self.credential.clone(),self.credential_epoch.clone());
        }
        match &self.worker {
            Some(worker) => worker.ask(job),
            None => false,
        }
    }

    /// Forgets the record that was open. What it was waiting for comes
    /// back to nobody.
    fn forget(&mut self) {
        self.beets.cancel(); self.changes.clear(); self.preview_matched = 0;
        self.session += 1;
        self.state = State::Closed;
        self.draft = None;
        self.album = None;
        self.layers = Layers::default();
        self.mode = Mode::Sidecar;
        self.id3 = Id3Version::V24;
        self.secondary_layers = false;
        self.searching = false;
        self.loading = false;
        self.hits.clear();
        self.release = None;
        self.selected_disc = 0;
        self.artwork_searching = false;
        self.artwork_loading = false;
        self.candidates.clear();
        self.waiting = None;
        self.cover.clear();
        self.pending_hosted.clear();
        self.server = None;
        self.progress = (0, 0);
        self.focus = -1;
        if let Some(worker) = &self.worker { for done in worker.finished() { artwork::discard(&done); } }
        artwork::sweep();
    }
}

impl Chilpa {
    /// What the window sees of the editor.
    pub fn metadata_view(&self) -> &MetadataView {
        &self.metadata.view
    }

    /// Whether looking records up is turned off in Settings.
    fn metadata_lookups_off(&self) -> bool {
        self.preference("library.metaSrc").as_deref() == Some("embedded")
    }

    /// Opens a record in the editor. `focus_track` is the track to show
    /// first, or zero.
    pub fn metadata_open(&mut self, album_id: i64, focus_track: i64) -> bool {
        if self.metadata.saving() {
            return false;
        }
        let found = self.library().map(|lib| (lib.album(album_id).ok().flatten(), lib.edit_tracks(album_id).unwrap_or_default()));
        let (album, tracks) = match found {
            None => {
                let why = self.now.error.clone();
                return self.metadata.refuse(why);
            }
            Some((Some((album, _)), tracks)) if !tracks.is_empty() => (album, tracks),
            Some(_) => return self.metadata.refuse(chilpa_i18n::t("none of that record's files are here now")),
        };
        // What the worker would read beside a file is read from the library
        // for a record on a server, here, where the library is.
        let hosted = self.library().map(|lib| hosted::homes(lib, &tracks)).unwrap_or_default();
        // And how to ask its server what it says of it.
        let reach = self.metadata_reach(&tracks);
        let chosen = match self.preference("library.metaSrc").as_deref() {
            Some("discogs") => Provider::Discogs,
            _ if !self.musicbrainz_enabled() => Provider::Discogs,
            _ => Provider::MusicBrainz,
        };
        let beets_runtime = self.beets_python();
        let beets_enabled = self.preference("metadata.engine").as_deref() == Some("beets");

        if self.metadata.worker.is_none() {
            (self.metadata.credential,self.metadata.credential_epoch)=self.discogs_provider();
        }
        let editor = &mut self.metadata;
        editor.forget();
        editor.state = State::Opening;
        editor.album_id = album_id;
        editor.focus_track = focus_track;
        editor.cover = album.artwork_path.clone().unwrap_or_default();
        editor.album = Some(album);
        editor.provider = chosen;
        editor.beets.runtime = beets_runtime;
        editor.beets.enabled = beets_enabled;
        if editor.beets.enabled { editor.provider = Provider::MusicBrainz; }
        editor.artwork_provider = chosen;
        editor.error.clear();
        editor.notice.clear();
        let session = editor.session;
        if !editor.ask(Job::Open { session, tracks, hosted, reach }) {
            editor.forget();
            return editor.refuse(chilpa_i18n::t("the editor could not be started"));
        }
        editor.refresh();
        true
    }

    /// Closes the editor without saving. Not while a save is running.
    pub fn metadata_close(&mut self) {
        if self.metadata.saving() {
            return;
        }
        self.metadata.forget();
        self.metadata.error.clear();
        self.metadata.notice.clear();
        self.metadata.refresh();
    }

    pub fn metadata_set_album(&mut self, name: &str, value: &str) -> bool {
        let editor = &mut self.metadata;
        if editor.state != State::Ready {
            return false;
        }
        let set = editor.draft.as_mut().is_some_and(|draft| draft.set_album(name, value));
        if set {
            editor.refresh();
        }
        set
    }

    pub fn metadata_set_track(&mut self, track_id: i64, name: &str, value: &str) -> bool {
        let editor = &mut self.metadata;
        if editor.state != State::Ready {
            return false;
        }
        let Some(draft) = editor.draft.as_mut() else { return false };
        let inside = draft.row(track_id).is_some_and(|row| draft.rows[row].inside_a_disc());
        if name == "track_number" && inside {
            return editor.refuse(chilpa_i18n::t("the track's number is its place inside the disc image"));
        }
        let set = draft.set_track(track_id, name, value);
        if set {
            editor.refresh();
        }
        set
    }

    /// Puts every field back as the record opened, and lets go of the
    /// cover in wait.
    pub fn metadata_revert(&mut self) {
        let editor = &mut self.metadata;
        if editor.state != State::Ready {
            return;
        }
        if let Some(draft) = editor.draft.as_mut() {
            draft.revert();
        }
        editor.artwork_turn += 1;
        editor.artwork_loading = false;
        editor.waiting = None;
        artwork::forget_waiting(editor.session);
        editor.refresh();
    }

    /// "sidecar" | "files" | "xml" | "server"
    pub fn metadata_set_mode(&mut self, mode: &str) -> bool {
        let editor = &mut self.metadata;
        let Some(draft) = editor.draft.as_ref().filter(|_| editor.state == State::Ready) else { return false };
        let mode = match mode {
            "sidecar" => Mode::Sidecar,
            "files" => Mode::Files,
            "xml" => Mode::Xml,
            "server" => Mode::Server,
            _ => return false,
        };
        let protection = crate::library_protection::Protection::kept();
        let blocked = if mode == Mode::Sidecar { protection.sidecar_refused() } else { protection.writes_refused() };
        if !blocked.is_empty() {
            return editor.refuse(blocked);
        }
        if mode == Mode::Server {
            let state = editor.server.clone().unwrap_or_default();
            if !state.writable {
                let why = if state.why.is_empty() { chilpa_i18n::t("Chilpa cannot change this server's metadata") } else { state.why };
                return editor.refuse(why);
            }
        }
        if mode == Mode::Files {
            let refused = view::files_refused(draft, &editor.layers);
            if !refused.is_empty() {
                return editor.refuse(refused);
            }
        }
        if mode == Mode::Xml {
            let refused = view::xml_refused(draft);
            if !refused.is_empty() { return editor.refuse(refused); }
        }
        editor.mode = mode;
        editor.refresh();
        true
    }

    /// "2.4" | "2.3"
    pub fn metadata_set_id3(&mut self, version: &str) -> bool {
        let version = match version {
            "2.4" => Id3Version::V24,
            "2.3" => Id3Version::V23,
            _ => return false,
        };
        self.metadata.id3 = version;
        self.metadata.refresh();
        true
    }

    pub fn metadata_set_secondary_layers(&mut self, on: bool) {
        self.metadata.secondary_layers = on;
        self.metadata.refresh();
    }

    /// Takes what the worker has finished, and says what the window sees.
    pub fn metadata_poll(&mut self) -> &MetadataView {
        let finished: Vec<Done> = self.metadata.worker.as_ref().map(Worker::finished).unwrap_or_default();
        for done in finished {
            self.metadata_take(done);
        }
        if self.metadata.view.lookup.beets_busy != self.metadata.beets.busy() { self.metadata.refresh(); }
        &self.metadata.view
    }

    fn metadata_take(&mut self, done: Done) {
        if done.session() != self.metadata.session {
            artwork::discard(&done);
            return;
        }
        let current_account=match &done {
            Done::Found {turn,account_epoch,hits,..} if *turn==self.metadata.lookup_turn && self.metadata.provider==Provider::Discogs=>self.discogs_result(*account_epoch,hits.as_ref().err()),
            Done::Loaded {turn,account_epoch,release,..} if *turn==self.metadata.lookup_turn && self.metadata.provider==Provider::Discogs=>self.discogs_result(*account_epoch,release.as_ref().err()),
            Done::Candidates {turn,account_epoch,candidates,..} if *turn==self.metadata.artwork_turn && self.metadata.artwork_provider==Provider::Discogs=>self.discogs_result(*account_epoch,candidates.as_ref().err()),
            _=>true,
        };
        if !current_account {
            match &done {Done::Found {..}=>self.metadata.searching=false,Done::Loaded {..}=>self.metadata.loading=false,
                Done::Candidates {..}=>self.metadata.artwork_searching=false,_=>{}}
            artwork::discard(&done);
            self.metadata.refuse(chilpa_i18n::t("The Discogs account changed. Search again."));
            return;
        }
        let editor = &mut self.metadata;
        match done {
            Done::Opened { reads, homes, layers, tracks, server, .. } => {
                let Some(album) = editor.album.as_ref() else { return };
                let draft = Draft::open(album, tracks, reads, homes);
                editor.focus = draft.row(editor.focus_track).map_or(-1, |row| row as i32);
                // A record on a server saves to the server when it takes the
                // changes; when it does not, nothing is chosen for the
                // listener: a sidecar is made only when asked for. A
                // protected library sends nothing to the server, and keeps
                // a local record's edits in a sidecar only when allowed.
                let protection = crate::library_protection::Protection::kept();
                editor.protection = protection;
                editor.mode = match &server {
                    Some(state) if state.writable && !protection.on => Mode::Server,
                    Some(_) => Mode::Undecided,
                    None if protection.sidecar_refused().is_empty() => Mode::Sidecar,
                    None => Mode::Undecided,
                };
                editor.server = server;
                editor.draft = Some(draft);
                editor.layers = layers;
                editor.state = State::Ready;
            }
            Done::Progress { done, total, .. } => editor.progress = (done, total),
            Done::Found { turn, hits, .. } if turn == editor.lookup_turn => {
                editor.searching = false;
                match hits {
                    Ok(hits) if hits.is_empty() => editor.error = chilpa_i18n::t("nothing was found"),
                    Ok(hits) => editor.hits = hits,
                    Err(failure) => editor.error = apply::said(editor.provider, &failure),
                }
            }
            Done::Loaded { turn, release, .. } if turn == editor.lookup_turn => {
                editor.loading = false;
                match release {
                    Ok(release) => {
                        editor.selected_disc = editor.draft.as_ref().map_or(0, |draft| {
                            let catalog = &draft.album_cell("catalog_number").now;
                            let by_catalog = release.catalog_numbers.iter().position(|number| number.eq_ignore_ascii_case(catalog))
                                .and_then(|at| u16::try_from(at + 1).ok());
                            let at_disc = fields::track("disc_number").unwrap_or(0);
                            let discs: std::collections::BTreeSet<u16> = draft.rows.iter()
                                .filter_map(|row| row.cells[at_disc].now.parse().ok()).collect();
                            by_catalog.or_else(|| (discs.len() == 1).then(|| *discs.first().unwrap_or(&0)).filter(|n| *n > 1 && *n <= release.discs))
                                .unwrap_or(0)
                        });
                        let (pairs, by_order) = editor.draft.as_ref().map_or((Vec::new(), false), |d| apply::pairing(d, &release, editor.selected_disc));
                        let (matched, total) = (pairs.iter().flatten().count(), editor.draft.as_ref().map_or(0, |d| d.rows.len()));
                        editor.notice = if by_order {
                            chilpa_i18n::t_args("{} of {} tracks were paired by their order: the release numbers them differently. Check the titles before applying.", &[&matched.to_string(), &total.to_string()])
                        } else {
                            chilpa_i18n::t_args("{} of {} tracks match this release", &[&matched.to_string(), &total.to_string()])
                        };
                        editor.release = Some(release);
                        editor.rebuild_preview();
                    }
                    Err(failure) => editor.error = apply::said(editor.provider, &failure),
                }
            }
            Done::Beets { turn, slot, .. } if turn == editor.lookup_turn => {
                editor.searching = false;
                let probing = editor.beets.probing;
                editor.beets.probing = false;
                if let Some(result) = slot.take() {
                    match result {
                        Ok(response) => {
                            editor.beets.accept(response);
                            editor.hits = editor.beets.candidates.iter().map(beets::hit).collect();
                            if probing {
                                editor.notice = if editor.beets.version == chilpa_meta::beets::BEETS_VERSION { chilpa_i18n::t_args("beets {} is ready.", &[&editor.beets.version]) }
                                    else { chilpa_i18n::t_args("beets {} is ready, but Chilpa was tested with beets {}; lookups may not work.", &[&editor.beets.version, chilpa_meta::beets::BEETS_VERSION]) };
                            }
                            else if editor.hits.is_empty() { editor.error = chilpa_i18n::t("nothing was found"); }
                        }
                        Err(error) => editor.error = beets::said(error),
                    }
                }
            }
            Done::Candidates { turn, candidates, .. } if turn == editor.artwork_turn => {
                editor.artwork_searching = false;
                match candidates {
                    Ok(found) if found.is_empty() => editor.error = chilpa_i18n::t("no covers were found"),
                    Ok(found) => editor.candidates = found,
                    Err(failure) => editor.error = apply::said(editor.artwork_provider, &failure),
                }
            }
            Done::Cover { turn, cover, .. } if turn == editor.artwork_turn => {
                editor.artwork_loading = false;
                match cover {
                    Ok(waiting) => editor.waiting = Some(waiting),
                    Err(why) => editor.error = why,
                }
            }
            Done::Saved { written, .. } => return self.metadata_saved(written, None),
            Done::SaveFailed { reason, written, .. } => return self.metadata_saved(written, Some(reason)),
            Done::ServerSaved { outcome, .. } => return self.metadata_server_saved(outcome),
            Done::Dropped { failed, contained, .. } => return self.metadata_dropped(failed, contained),
            // For a search, a load or a cover that is no longer the last asked for.
            other => artwork::discard(&other),
        }
        self.metadata.refresh();
    }

    /// Hands the editor another network. For tests: nothing in one goes out.
    #[doc(hidden)]
    pub fn metadata_network(&mut self, network: Arc<dyn chilpa_meta::Http>) {
        self.metadata.network = network;
        self.metadata.worker = None;
    }

    /// Hands the editor the network and the credential every server is
    /// asked with. For tests: nothing in one goes out.
    #[doc(hidden)]
    pub fn metadata_server_network(&mut self, network: Arc<dyn chilpa_servers::Net>, credential: chilpa_servers::Credential) {
        self.metadata.server_access = Some((network, credential));
    }
}
