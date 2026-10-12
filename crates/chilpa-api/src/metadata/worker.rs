//! The thread that reads files and goes out to the network.
//!
//! One job at a time, in the order they were asked for. It does not touch
//! the library or the facade: what it finds goes back as a `Done`, and the
//! window's thread decides what to do with it.

use super::artwork::{self, Candidate, Waiting};
use super::draft::{HomeState, Read};
use super::save::{self, Plan, Written};
use chilpa_library::sidecar::{load, Home};
use chilpa_library::scan::ContainedTrack;
use chilpa_library::EditTrack;
use chilpa_meta::{discogs, musicbrainz, Failure, Hit, Http, Net, Provider, Release, Reply};
use chilpa_tags::edit::{inspect, placed, snapshot, Layers, Snapshot};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

pub(crate) enum Job {
    Open { session: u64, tracks: Vec<EditTrack> },
    Search { session: u64, turn: u64, provider: Provider, title: String, artist: String, catalog_number: String },
    Load { session: u64, turn: u64, provider: Provider, id: String },
    ArtworkSearch { session: u64, turn: u64, provider: Provider, title: String, artist: String, catalog_number: String },
    ArtworkFetch { session: u64, turn: u64, provider: Provider, url: String },
    ArtworkFile { session: u64, turn: u64, path: PathBuf },
    Save { session: u64, plan: Plan },
}

pub(crate) enum Done {
    Opened { session: u64, tracks: Vec<EditTrack>, reads: Vec<Read>, homes: Vec<HomeState>, layers: Layers },
    Found { session: u64, turn: u64, hits: Result<Vec<Hit>, Failure> },
    Loaded { session: u64, turn: u64, release: Result<Release, Failure> },
    Candidates { session: u64, turn: u64, candidates: Result<Vec<Candidate>, Failure> },
    Cover { session: u64, turn: u64, cover: Result<Waiting, String> },
    Progress { session: u64, done: usize, total: usize },
    Saved { session: u64, written: Written },
    SaveFailed { session: u64, reason: String, written: Written },
}

impl Done {
    pub(crate) fn session(&self) -> u64 {
        match self {
            Done::Opened { session, .. }
            | Done::Found { session, .. }
            | Done::Loaded { session, .. }
            | Done::Candidates { session, .. }
            | Done::Cover { session, .. }
            | Done::Progress { session, .. }
            | Done::Saved { session, .. }
            | Done::SaveFailed { session, .. } => *session,
        }
    }
}

/// The network as it is: `chilpa_core::fetch`, saying who asks.
pub(crate) struct Network {
    agent: String,
}

impl Network {
    pub(crate) fn new() -> Self {
        Self { agent: chilpa_meta::user_agent(env!("CARGO_PKG_VERSION")) }
    }
}

impl Http for Network {
    fn get(&self, url: &str, accept: &str, timeout: Duration, max_bytes: usize) -> Result<Reply, String> {
        let asked = chilpa_core::Fetch { url, user_agent: &self.agent, accept, timeout, max_bytes };
        match chilpa_core::fetch(&asked) {
            Ok(got) => Ok(Reply { status: got.status, headers: got.headers, body: got.body }),
            Err(e) => Err(e.to_string()),
        }
    }
}

pub(crate) struct Worker {
    jobs: Sender<Job>,
    done: Receiver<Done>,
}

impl Worker {
    pub(crate) fn start(network: Arc<dyn Http>) -> Option<Self> {
        let (jobs, asked) = channel::<Job>();
        let (finished, done) = channel::<Done>();
        std::thread::Builder::new()
            .name("chilpa-metadata".into())
            .spawn(move || {
                // Whatever was left in wait belongs to a run that is over.
                artwork::sweep();
                for job in asked {
                    run(job, network.as_ref(), &finished);
                }
            })
            .ok()?;
        Some(Self { jobs, done })
    }

    pub(crate) fn ask(&self, job: Job) -> bool {
        self.jobs.send(job).is_ok()
    }

    /// What has finished since the last time this was asked.
    pub(crate) fn finished(&self) -> Vec<Done> {
        self.done.try_iter().collect()
    }
}

/// What a track's container says of it, as what a file would have said.
fn held(track: &EditTrack, inside: Option<&[ContainedTrack]>, xml: Option<&chilpa_core::sacd_xml::XmlMetadata>) -> Option<Snapshot> {
    let tags = inside?.iter().find(|t| t.path == track.external_id)?.info.tags.clone();
    let mut snapshot = Snapshot {
        title: tags.title,
        artist_credit: tags.artists,
        album: tags.album,
        album_artist: tags.album_artist,
        date: tags.date,
        genres: tags.genres,
        track_number: tags.track_number.map(u32::from),
        disc_number: tags.disc_number.map(u32::from),
        musicbrainz_recording_id: tags.musicbrainz_recording_id,
        ..Snapshot::default()
    };
    if let Some((_, number)) = chilpa_core::sacd_tracks::parse(Path::new(&track.external_id)) {
        if let Some(xml) = xml { xml.overlay_snapshot(number, &mut snapshot); }
    }
    Some(snapshot)
}

fn open(session: u64, tracks: Vec<EditTrack>) -> Done {
    let mut reads = Vec::with_capacity(tracks.len());
    let mut homes: Vec<HomeState> = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut containers: HashMap<String, (Option<Vec<ContainedTrack>>, Option<chilpa_core::sacd_xml::XmlMetadata>)> = HashMap::new();
    for track in &tracks {
        let read = match &track.container {
            Some(container) => {
                let (inside, xml) = containers.entry(container.clone()).or_insert_with(|| {
                    let inside = chilpa_core::sacd_tracks::disc_images().tracks(Path::new(container));
                    let expected: Vec<_> = inside.as_ref().into_iter().flatten()
                        .filter_map(|track| chilpa_core::sacd_tracks::parse(Path::new(&track.path)).map(|(_, number)| number))
                        .collect();
                    let xml = chilpa_core::sacd_xml::read_for(Path::new(container), &expected);
                    (inside, xml)
                });
                match held(track, inside.as_deref(), xml.as_ref()) {
                    Some(said) => Read { said, known: true, placed: Vec::new() },
                    None => Read::default(),
                }
            }
            None => {
                let path = Path::new(&track.external_id);
                files.push(path.to_path_buf());
                match snapshot(path) {
                    Ok(said) => Read { said, known: true, placed: placed(path).unwrap_or_default() },
                    Err(_) => Read::default(),
                }
            }
        };
        reads.push(read);

        let home = Home::of(&track.external_id, track.container.as_deref());
        if !homes.iter().any(|h| h.home == home) {
            let (sidecar, unreadable) = match load(&home, &track.external_id) {
                Ok(sidecar) => (sidecar, String::new()),
                Err(e) => (None, e.to_string()),
            };
            let xml = track.container.as_ref().and_then(|container| containers.get(container))
                .and_then(|(_, xml)| xml.as_ref()).map(|xml| xml.path.clone());
            homes.push(HomeState { home, of: track.external_id.clone(), sidecar, unreadable, xml });
        }
    }
    Done::Opened { session, tracks, reads, homes, layers: inspect(&files) }
}

fn run(job: Job, http: &dyn Http, finished: &Sender<Done>) {
    let done = match job {
        Job::Open { session, tracks } => open(session, tracks),
        Job::Search { session, turn, provider, title, artist, catalog_number } => {
            let hits = match provider {
                Provider::MusicBrainz => musicbrainz::search(&Net::musicbrainz(http), &title, &artist, &catalog_number),
                Provider::Discogs => discogs::search(&Net::proxy(http), &artist, &title, &catalog_number),
            };
            Done::Found { session, turn, hits }
        }
        Job::Load { session, turn, provider, id } => {
            let release = match provider {
                Provider::MusicBrainz => musicbrainz::release(&Net::musicbrainz(http), &id),
                Provider::Discogs => discogs::release(&Net::proxy(http), &id),
            };
            Done::Loaded { session, turn, release }
        }
        Job::ArtworkSearch { session, turn, provider, title, artist, catalog_number } => {
            Done::Candidates { session, turn, candidates: artwork::find(http, session, turn, provider, &title, &artist, &catalog_number) }
        }
        Job::ArtworkFetch { session, turn, provider, url } => Done::Cover { session, turn, cover: artwork::fetch(http, session, provider, &url) },
        Job::ArtworkFile { session, turn, path } => Done::Cover { session, turn, cover: artwork::from_file(session, &path) },
        Job::Save { session, plan } => save::run(session, plan, finished),
    };
    let _ = finished.send(done);
}
