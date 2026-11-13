//! Tracks that live on a server of mine (ADR-017).
//!
//! The queue names them with a `chilpa_models::RemoteEntry`. How to fetch
//! one -- the address, the credential -- belongs to the facade, which knows
//! the servers and holds the secrets. It installs an opener once, and the
//! player asks it as each track comes up. The player never sees a
//! credential; it gets a request and reads the bytes.

use chilpa_models::RemoteEntry;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

/// Where a track is read from.
#[derive(Clone, Copy, Debug)]
pub struct Normalization {
    pub gain_db: f32,
    pub peak: Option<f32>,
}
impl Normalization {
    pub fn valid(self) -> Option<Self> {
        self.gain_db.is_finite().then(|| Self { gain_db: self.gain_db, peak: self.peak.filter(|p| p.is_finite() && *p > 0.0) })
    }
}

pub enum Media {
    Remote(chilpa_stream::RangedRequest),
    /// An open handle pins the exact verified bytes, even across a rename.
    File {file:std::fs::File,lease:Arc<dyn Send+Sync>},
}

pub struct Located {
    /// Provider gain replaces local normalization metadata, including a zero gain.
    pub normalization: Option<Normalization>,
    pub media: Media,
    /// The file's own extension ("flac", "m4a"). The address has none.
    pub extension: Option<String>,
    /// The server's name, as Settings shows it: an error names it.
    pub server: String,
    /// An observer bound to this exact resource, including preloaded resources.
    pub playback: Option<Arc<dyn crate::playback_events::Observer>>,
}

/// What the window shows while it plays. The file's own tags are on the
/// server; what the library knows of it stands in for them.
#[derive(Clone, Debug, Default)]
pub struct Described {
    pub duration: std::time::Duration,
    pub tags: chilpa_tags::Tags,
    pub artwork: Option<PathBuf>,
}

pub trait RemoteOpener: Send + Sync {
    fn locate(&self, entry: &RemoteEntry) -> Result<Located, String>;
    fn describe(&self, entry: &RemoteEntry) -> Described;
    /// The track could not be opened. The facade decides whether its server
    /// stopped answering. `play` is the player's generation of the play it
    /// was opened for, when a player asked: a failure that comes late, from a
    /// play that was replaced meanwhile, says nothing of the queue that
    /// replaced it.
    fn failed(&self, _entry: &RemoteEntry, _why: &str, _play: Option<u64>) {}
}

thread_local! {
    /// The play this thread opens tracks for.
    static PLAY: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// Says which play this thread opens tracks for from now on: the player's
/// thread as it takes a play, and the thread that opens a track ahead for it.
pub(crate) fn opening_for(play: Option<u64>) {
    PLAY.set(play);
}

/// The play this thread opens tracks for.
pub(crate) fn opening_play() -> Option<u64> {
    PLAY.get()
}

static OPENER: RwLock<Option<Arc<dyn RemoteOpener>>> = RwLock::new(None);

/// The frames of the remote tracks opened last, for the event that
/// announces each: the decoder knows them exactly and the server only to the
/// second. A few, not one: two players (a test, a relaunch on another
/// output) may open one each before either announces.
static OPENED: Mutex<Vec<(PathBuf, u64)>> = Mutex::new(Vec::new());
/// And the codec each was decoded with, which its address does not say.
static CODECS: Mutex<Vec<(PathBuf, &'static str)>> = Mutex::new(Vec::new());
const REMEMBERED: usize = 8;

/// Sets who fetches remote tracks. The facade calls it once, at startup.
pub fn install(opener: Arc<dyn RemoteOpener>) {
    *OPENER.write().unwrap_or_else(|e| e.into_inner()) = Some(opener);
}

fn opener() -> Option<Arc<dyn RemoteOpener>> {
    OPENER.read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub(crate) fn entry(path: &Path) -> Option<RemoteEntry> {
    path.to_str().and_then(RemoteEntry::parse)
}

pub(crate) type Opened = (chilpa_decode::Decoded, Option<Arc<dyn crate::playback_events::Observer>>, Option<Normalization>);

/// Opens a remote track for the decoder.
pub(crate) fn open(path: &Path, entry: &RemoteEntry) -> Result<Opened, String> {
    let Some(opener) = opener() else {
        return Err(chilpa_i18n::t("this track is on a server, and servers are not connected"));
    };
    let opened = opener.locate(entry).and_then(|located| {
        let server = located.server;
        let named = |e: String| if server.is_empty() { e } else { chilpa_i18n::t_args("the track on {} could not be read: {}", &[&server, &e]) };
        let decoded = match located.media {
            Media::Remote(request) => {
                let file = chilpa_stream::Ranged::open(request).map_err(|e| named(e.to_string()))?;
                chilpa_decode::Decoded::open_remote(file, located.extension.as_deref())
            }
            Media::File {file,lease} => chilpa_decode::Decoded::open_file(file, located.extension.as_deref())
                .map(|mut decoded| {decoded.pin(lease);decoded}),
        };
        decoded.map(|decoded| (decoded, located.playback, located.normalization.and_then(Normalization::valid))).map_err(|e| named(e.to_string()))
    });
    match opened {
        Ok((decoded, playback, normalization)) => {
            if !decoded.codec().is_empty() {
                let mut codecs = CODECS.lock().unwrap_or_else(|e| e.into_inner());
                codecs.retain(|(p, _)| p != path);
                codecs.push((path.to_path_buf(), decoded.codec()));
                if codecs.len() > REMEMBERED {
                    codecs.remove(0);
                }
            }
            if let Some(frames) = decoded.frame_count() {
                let mut opened = OPENED.lock().unwrap_or_else(|e| e.into_inner());
                opened.retain(|(p, _)| p != path);
                opened.push((path.to_path_buf(), frames));
                if opened.len() > REMEMBERED {
                    opened.remove(0);
                }
            }
            Ok((decoded, playback, normalization))
        }
        Err(why) => {
            opener.failed(entry, &why, opening_play());
            Err(why)
        }
    }
}

/// The codec a remote track that was opened is decoded with.
pub fn opened_codec(path: &str) -> Option<&'static str> {
    CODECS.lock().unwrap_or_else(|e| e.into_inner()).iter().rev().find(|(p, _)| p.to_str() == Some(path)).map(|(_, c)| *c)
}

/// What to announce for a remote track that just started.
pub(crate) fn describe(path: &Path, entry: &RemoteEntry) -> (Described, Option<u64>) {
    let described = opener().map(|o| o.describe(entry)).unwrap_or_default();
    let frames = OPENED.lock().unwrap_or_else(|e| e.into_inner()).iter().rev().find(|(p, _)| p == path).map(|(_, f)| *f);
    (described, frames)
}

/// A remote track's name, as the library knows it: what a queue shows
/// before the track plays. `None` for anything that is not one, or when
/// nobody can say.
pub fn title(path: &Path) -> Option<String> {
    let entry = entry(path)?;
    opener()?.describe(&entry).tags.title
}

/// I resolve an original external asset with the same opener as playback.
/// Only the facade's network worker calls this; credentials stay there.
pub fn external(entry: &RemoteEntry) -> Result<(Located, Described), String> {
    let opener = opener().ok_or_else(|| chilpa_i18n::t("this track is on a server, and servers are not connected"))?;
    Ok((opener.locate(entry)?, opener.describe(entry)))
}

/// Validation runs on the download worker and never opens an audio output.
pub fn validate_copy(file: std::fs::File, extension: Option<&str>, cancelled:impl Fn()->bool) -> Result<String,String> {
    chilpa_decode::verification::file(file,extension,cancelled).map_err(|error| error.to_string())
}
