//! What changes when Chilpa runs in a sandbox (`chilpa_dirs::sandbox`). The
//! desktop's colours come from the Settings portal, since `gsettings` and the
//! configuration files read elsewhere are the sandbox's own; and no portal
//! names the desktop's wallpaper, so in a sandbox there is none.
//!
//! The portal is read on a thread of its own (chilpa-mpris) and what it said
//! is kept here, so the window's thread reads it without waiting on the bus.
//! One value and a counter, replaced in place: nothing here grows.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
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
