//! What changes when Chilpa runs in a sandbox (`chilpa_dirs::sandbox`). The
//! desktop's colours come from the Settings portal, since `gsettings` and the
//! configuration files read elsewhere are the sandbox's own; no portal names
//! the desktop's wallpaper, so in a sandbox there is none; a folder chosen
//! through the file chooser is shown by its place on the host; and memory
//! cards are reached through a chosen folder when the sandbox hides them.
//!
//! The portals are read on threads of their own (chilpa-mpris) and what they
//! said is kept here, so the window's thread reads it without waiting on the
//! bus: the appearance as one value and a counter, replaced in place, and the
//! host paths of at most as many folders as the library holds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Condvar, Mutex};
use std::time::Duration;

/// What the desktop asks of Chilpa's colours, as the Settings portal said it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Appearance {
    /// `None` when the desktop states no preference.
    pub dark: Option<bool>,
    pub accent: Option<[u8; 3]>,
}

static SAID: Mutex<Option<Appearance>> = Mutex::new(None);
static ANSWERED: Condvar = Condvar::new();
/// Bumped by each answer of the portal, the first one included.
static REVISION: AtomicU64 = AtomicU64::new(0);

pub(crate) fn sandboxed() -> bool {
    chilpa_dirs::sandbox() != chilpa_dirs::Sandbox::None
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn said(appearance: Appearance) {
    *SAID.lock().unwrap_or_else(|e| e.into_inner()) = Some(appearance);
    REVISION.fetch_add(1, Ordering::AcqRel);
    ANSWERED.notify_all();
}

/// Starts reading the desktop through its portals, in a sandbox only. The
/// connection and its thread end when the handle is dropped.
#[cfg(target_os = "linux")]
pub(crate) fn start_portal() -> Option<chilpa_mpris::portal::Portal> {
    if !sandboxed() {
        return None;
    }
    chilpa_mpris::portal::Portal::start(|appearance| {
        use chilpa_mpris::portal::Scheme;
        let dark = match appearance.scheme {
            Scheme::Dark => Some(true),
            Scheme::Light => Some(false),
            Scheme::NoPreference => None,
        };
        said(Appearance { dark, accent: appearance.accent });
    })
}

/// What the portal last said, without waiting; `None` before it answers.
pub(crate) fn appearance() -> Option<Appearance> {
    *SAID.lock().unwrap_or_else(|e| e.into_inner())
}

/// Waits at most `bound` for the portal's first answer. For a thread that
/// may wait, never the window's.
pub(crate) fn appearance_within(bound: Duration) -> Option<Appearance> {
    let said = SAID.lock().unwrap_or_else(|e| e.into_inner());
    let (said, _) = ANSWERED.wait_timeout_while(said, bound, |said| said.is_none()).unwrap_or_else(|e| e.into_inner());
    *said
}

/// Moves each time the portal says something; the tray follows it.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn appearance_revision() -> u64 {
    REVISION.load(Ordering::Acquire)
}

/// Whether the desktop asks for dark colours: in a sandbox, the portal's
/// last word, where no word and no preference are not dark; elsewhere what
/// GNOME, GTK or KDE say.
pub(crate) fn prefers_dark() -> bool {
    prefers_dark_from(sandboxed(), appearance(), chilpa_dirs::desktop_prefers_dark)
}

fn prefers_dark_from(sandboxed: bool, said: Option<Appearance>, desktop: impl FnOnce() -> bool) -> bool {
    if sandboxed { said.and_then(|said| said.dark).unwrap_or(false) } else { desktop() }
}

/// The colour scheme the auto theme is built from in a sandbox: the
/// portal's dark or light and its accent over the defaults GNOME's reader
/// uses, dark when the desktop states no preference, as that reader does.
pub(crate) fn color_scheme(said: Appearance) -> chilpa_theme::auto::SystemColorScheme {
    let accent = said.accent.map(|[r, g, b]| chilpa_theme::auto::PaletteColor::new(r, g, b));
    chilpa_theme::auto::scheme_from(said.dark.unwrap_or(true), accent)
}

/// Most folders kept with their place on the host: as many as the library holds.
const MAX_HOST_PATHS: usize = crate::scan_state::MAX_ROOTS;

/// Where the folders the document portal shows live on the host, for
/// showing them. A folder chosen through the file chooser inside a sandbox
/// arrives as `<mount>/<id>/<name>` and is kept, scanned and played by that
/// path, which stays the same while Chilpa keeps its permission; only the
/// portal knows the host's. It is asked on a thread of its own, once per
/// folder and one question at a time, and what it said is kept for at most
/// [`MAX_HOST_PATHS`] folders.
#[derive(Default)]
pub(crate) struct HostPaths {
    /// Each folder asked about, with the host's path, or `None` when the
    /// portal did not say: such a folder is shown as it is.
    known: HashMap<String, Option<String>>,
    asking: Option<mpsc::Receiver<HashMap<PathBuf, PathBuf>>>,
    /// What the question in flight asked about.
    asked: Vec<String>,
}

impl HostPaths {
    /// Each path as it is shown: a document-portal path by its place on the
    /// host once the portal said it, as it is until then. Paths not known
    /// yet are asked about through what `portal` hands back, on a thread of
    /// its own, unless a question is already in flight. Never waits.
    pub(crate) fn shown(&mut self, paths: &[String], portal: impl FnOnce() -> Option<Asker>) -> Vec<String> {
        let unknown: Vec<String> = paths.iter().filter(|path| in_document_portal(path) && !self.known.contains_key(*path)).cloned().collect();
        if !unknown.is_empty() && self.asking.is_none() {
            if let Some(ask) = portal() {
                self.ask(unknown, ask);
            }
        }
        paths.iter().map(|path| self.known.get(path).cloned().flatten().unwrap_or_else(|| path.clone())).collect()
    }

    fn ask(&mut self, paths: Vec<String>, ask: Asker) {
        let (tx, rx) = mpsc::sync_channel(1);
        let asked: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        let started = std::thread::Builder::new().name("chilpa-host-paths".into()).spawn(move || {
            let _ = tx.send(ask(asked));
        });
        if started.is_ok() {
            self.asking = Some(rx);
            self.asked = paths;
        }
    }

    /// Takes the portal's answer, if it came. True when there is one, so
    /// the folders are shown again.
    pub(crate) fn absorb(&mut self) -> bool {
        let Some(rx) = &self.asking else { return false };
        let answer = match rx.try_recv() {
            Ok(answer) => answer,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => HashMap::new(),
        };
        self.asking = None;
        if self.known.len() + self.asked.len() > MAX_HOST_PATHS {
            self.known.clear();
        }
        for path in std::mem::take(&mut self.asked) {
            let host = answer.get(Path::new(&path)).map(|host| host.to_string_lossy().into_owned());
            self.known.insert(path, host);
        }
        true
    }
}

/// What asks the portal: given paths, where they live on the host.
pub(crate) type Asker = Box<dyn FnOnce(Vec<PathBuf>) -> HashMap<PathBuf, PathBuf> + Send>;

fn in_document_portal(path: &str) -> bool {
    #[cfg(target_os = "linux")]
    return chilpa_mpris::portal::in_document_portal(Path::new(path));
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        false
    }
}

/// Whether the sandbox hides where memory cards and USB drives are mounted:
/// none of those places is there.
pub(crate) fn removable_media_hidden() -> bool {
    sandboxed() && !chilpa_dirs::removable_media_roots().iter().any(|root| root.is_dir())
}

/// What the devices screen says while the sandbox hides them, naming the
/// folder the sandbox would have to be given. Empty when nothing is hidden.
pub(crate) fn removable_media_notice() -> String {
    if !removable_media_hidden() {
        return String::new();
    }
    let root = chilpa_dirs::removable_media_roots().into_iter().next().unwrap_or_default();
    chilpa_i18n::t_args(
        "Chilpa's sandbox hides memory cards and USB drives. Choose a folder on one, or allow Chilpa to read {}.",
        &[&root.display().to_string()],
    )
}

impl crate::Chilpa {
    /// Each library folder as its screens show it (see [`HostPaths`]).
    pub(crate) fn shown_paths(&mut self, paths: &[String]) -> Vec<String> {
        #[cfg(target_os = "linux")]
        let documents = self.desktop.portal.as_ref().map(|portal| portal.documents());
        #[cfg(target_os = "linux")]
        let portal = move || documents.map(|documents| Box::new(move |paths: Vec<PathBuf>| documents.host_paths(&paths)) as Asker);
        #[cfg(not(target_os = "linux"))]
        let portal = || None;
        self.scan_runtime.host_paths.shown(paths, portal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sandbox_asks_only_the_portal_and_nothing_said_is_not_dark() {
        let dark = Appearance { dark: Some(true), accent: None };
        let never = || panic!("a sandbox does not read the desktop's own settings");
        assert!(prefers_dark_from(true, Some(dark), never));
        assert!(!prefers_dark_from(true, Some(Appearance::default()), never));
        assert!(!prefers_dark_from(true, None, never));
        assert!(prefers_dark_from(false, None, || true));
    }

    #[test]
    fn an_answer_wakes_whoever_waits_for_it_and_moves_the_revision() {
        let before = appearance_revision();
        let light = Appearance { dark: Some(false), accent: Some([1, 2, 3]) };
        let waiter = std::thread::spawn(|| appearance_within(Duration::from_secs(5)));
        said(light);
        assert_eq!(waiter.join().unwrap(), Some(light));
        assert_eq!(appearance(), Some(light));
        assert_eq!(appearance_revision(), before + 1);
        assert_eq!(appearance_within(Duration::ZERO), Some(light), "an answer kept is not waited for");
    }

    fn document(name: &str) -> String {
        chilpa_dirs::document_portal_mounts()[0].join("a1").join(name).to_string_lossy().into_owned()
    }

    #[test]
    fn a_portal_folder_shows_by_its_host_path_once_the_portal_says_it() {
        if !cfg!(target_os = "linux") {
            return;
        }
        let (music, other, plain) = (document("Music"), document("Other"), "/srv/music".to_string());
        let paths = [music.clone(), other.clone(), plain.clone()];
        let mut host = HostPaths::default();
        let (tx, rx) = mpsc::channel();
        let answer = music.clone();
        let asked = host.shown(&paths, || Some(Box::new(move |asked: Vec<PathBuf>| {
            tx.send(asked).unwrap();
            HashMap::from([(PathBuf::from(&answer), PathBuf::from("/home/someone/Music"))])
        }) as Asker));
        assert_eq!(asked, paths, "until the portal answers each shows as it is");
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), [PathBuf::from(&music), PathBuf::from(&other)], "only the portal's own");
        let started = std::time::Instant::now();
        while !host.absorb() {
            assert!(started.elapsed() < Duration::from_secs(5), "the answer never came");
            std::thread::yield_now();
        }
        let never = || -> Option<Asker> { panic!("an answered folder is not asked about again") };
        assert_eq!(host.shown(&paths, never), ["/home/someone/Music".to_string(), other, plain]);
        assert!(!host.absorb(), "one answer is taken once");
    }

    #[test]
    fn without_a_portal_nothing_is_asked_and_nothing_waits() {
        let mut host = HostPaths::default();
        let paths = [document("Music")];
        assert_eq!(host.shown(&paths, || None), paths);
        assert!(!host.absorb());
    }

    #[test]
    fn outside_a_sandbox_every_card_is_listed_and_nothing_is_said() {
        assert!(!removable_media_hidden());
        assert!(removable_media_notice().is_empty());
    }

    #[test]
    fn the_portals_scheme_keeps_its_accent_and_reads_no_preference_as_dark() {
        let light = color_scheme(Appearance { dark: Some(false), accent: Some([53, 132, 228]) });
        assert_eq!(light.accent, Some(chilpa_theme::auto::PaletteColor::new(53, 132, 228)));
        assert!(light.window_bg.unwrap().luminance() > 0.5);
        let unsaid = color_scheme(Appearance::default());
        assert!(unsaid.window_bg.unwrap().luminance() < 0.5);
        assert_eq!(unsaid.accent, None);
    }
}
