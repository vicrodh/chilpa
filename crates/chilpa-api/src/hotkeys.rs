//! Keyboard shortcuts: what each key does, and how the listener changes it.
//!
//! Every shortcut is an action with a key in each keymap. The listener
//! picks a keymap and may give any action a key of their own, which sits
//! over whichever keymap is chosen. A key means one action: taking a key
//! another action had by default leaves that one without a key, and a key
//! another action already answers to is refused.
//!
//! The window hands over every key it gets, as Qt names it, and is told
//! which action to run. It runs it itself: this decides, it does not act.

pub(crate) const COMMANDS: &[crate::commands::Command] = &[
    crate::commands::command("like", "playback.favorite", ""),
];

use crate::Chilpa;
use chilpa_i18n as i18n;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Qt's names for the keys and modifiers this reads.
mod qt {
    pub const SPACE: i32 = 0x20;
    pub const ESCAPE: i32 = 0x0100_0000;
    pub const TAB: i32 = 0x0100_0001;
    pub const BACKTAB: i32 = 0x0100_0002;
    pub const BACKSPACE: i32 = 0x0100_0003;
    pub const RETURN: i32 = 0x0100_0004;
    pub const ENTER: i32 = 0x0100_0005;
    pub const DELETE: i32 = 0x0100_0007;
    pub const LEFT: i32 = 0x0100_0012;
    pub const UP: i32 = 0x0100_0013;
    pub const RIGHT: i32 = 0x0100_0014;
    pub const DOWN: i32 = 0x0100_0015;
    pub const SHIFT: i32 = 0x0200_0000;
    pub const CONTROL: i32 = 0x0400_0000;
    pub const ALT: i32 = 0x0800_0000;
    pub const META: i32 = 0x1000_0000;
}

/// The two sets of keys. The second one puts the common actions on single
/// letters, the way modal editors do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Keymap {
    #[default]
    Default,
    Letters,
    Vi,
}

impl Keymap {
    /// An unknown name is the default: a typo must not leave the window
    /// without keys.
    fn named(name: &str) -> Self {
        match name { "vi" => Keymap::Vi, "letters" => Keymap::Letters, _ => Keymap::Default }
    }

    fn name(self) -> &'static str {
        match self {
            Keymap::Default => "default",
            Keymap::Letters => "letters",
            Keymap::Vi => "vi",
        }
    }
}

/// Where a key is pressed: the window, or the mini player's own window.
/// An action answers in one of them, or in both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Window,
    Mini,
    /// The immersive view over the window: it answers the window's keys
    /// and a few of its own.
    Immersive,
    Both,
}

impl Scope {
    /// The scope a window names: "mini", "immersive", or the window.
    pub fn named(name: &str) -> Scope {
        match name {
            "mini" => Scope::Mini,
            "immersive" => Scope::Immersive,
            _ => Scope::Window,
        }
    }
}

#[derive(Debug)]
struct Action {
    id: &'static str,
    label: &'static str,
    group: &'static str,
    default: &'static str,
    /// Its key in the letters keymap, when it differs.
    letters: &'static str,
    scope: Scope,
}

impl Action {
    fn base(&self, keymap: Keymap) -> &'static str {
        if self.group == VI { return if keymap == Keymap::Vi { self.letters } else { "" }; }
        if keymap == Keymap::Vi {
            match self.id {
                "ui.focusMode" => return "z",
                "nav.nextTab" => return "gt", "nav.previousTab" => return "gT",
                "nav.back" => return "h", "nav.forward" => return "l",
                "nav.search" => return "/", "playback.favorite" | "playback.seekBack" | "playback.seekForward" | "playback.next" => return self.default,
                _ => {}
            }
        }
        if keymap != Keymap::Default && !self.letters.is_empty() { self.letters } else { self.default }
    }
}

const fn action(id: &'static str, label: &'static str, group: &'static str, default: &'static str, letters: &'static str) -> Action {
    Action { id, label, group, default, letters, scope: Scope::Window }
}

/// The same action, answering somewhere else than the window alone.
const fn reaching(scope: Scope, a: Action) -> Action {
    Action { scope, ..a }
}

const PLAYBACK: &str = i18n::mark("Playback");
const NAVIGATION: &str = i18n::mark("Navigation");
const INTERFACE: &str = i18n::mark("Interface");
const MINI: &str = i18n::mark("Mini player");
const IMMERSIVE: &str = i18n::mark("Immersive view");
/// The groups, in the order the sheet shows them.
const VI: &str = "Vi";
const GROUPS: [&str; 6] = [PLAYBACK, NAVIGATION, INTERFACE, MINI, IMMERSIVE, VI];

const ACTIONS: &[Action] = &[
    action("commands.open", i18n::mark("Command line"), INTERFACE, ":", ""),
    action("list.down", i18n::mark("Cursor down"), VI, "", "j"),
    action("list.up", i18n::mark("Cursor up"), VI, "", "k"),
    action("list.first", i18n::mark("First track"), VI, "", "gg"),
    action("list.last", i18n::mark("Last track"), VI, "", "Shift+G"),
    action("list.halfDown", i18n::mark("Half page down"), VI, "", "Ctrl+d"),
    action("list.halfUp", i18n::mark("Half page up"), VI, "", "Ctrl+u"),
    action("list.play", i18n::mark("Play cursor track"), VI, "", "Enter"),
    action("list.menu", i18n::mark("Track menu"), VI, "", "o"),
    action("list.queue", i18n::mark("Queue cursor track"), VI, "", "a"),
    action("list.favorite", i18n::mark("Like cursor track"), VI, "", "f"),
    action("search.next", i18n::mark("Next search result"), VI, "", "n"),
    action("search.previous", i18n::mark("Previous search result"), VI, "", "Shift+N"),
    action("playback.play", i18n::mark("Play"), PLAYBACK, "", ""),
    action("playback.pause", i18n::mark("Pause"), PLAYBACK, "", ""),
    action("playback.seek", i18n::mark("Seek"), PLAYBACK, "", ""),
    action("playback.volume", i18n::mark("Volume"), PLAYBACK, "", ""),
    action("playback.sleep", i18n::mark("Sleep timer"), PLAYBACK, "", ""),
    action("playback.loop", i18n::mark("A-B loop"), PLAYBACK, "", ""),
    action("nav.go", i18n::mark("Go to"), PLAYBACK, "", ""),
    action("settings.set", i18n::mark("Set preference"), PLAYBACK, "", ""),
    action("library.filter", i18n::mark("Filter the library"), PLAYBACK, "", ""),

    reaching(Scope::Both, action("playback.toggle", i18n::mark("Play / Pause"), PLAYBACK, "Space", "")),
    reaching(Scope::Both, action("playback.next", i18n::mark("Next Track"), PLAYBACK, "Ctrl+ArrowRight", "n")),
    reaching(Scope::Both, action("playback.prev", i18n::mark("Previous Track"), PLAYBACK, "Ctrl+ArrowLeft", "p")),
    reaching(Scope::Both, action("playback.volumeUp", i18n::mark("Volume Up"), PLAYBACK, "Ctrl+ArrowUp", "Shift+K")),
    reaching(Scope::Both, action("playback.volumeDown", i18n::mark("Volume Down"), PLAYBACK, "Ctrl+ArrowDown", "Shift+J")),
    reaching(Scope::Both, action("playback.mute", i18n::mark("Mute / Unmute"), PLAYBACK, "m", "")),
    reaching(Scope::Both, action("playback.shuffle", i18n::mark("Toggle Shuffle"), PLAYBACK, "Ctrl+s", "s")),
    reaching(Scope::Both, action("playback.repeat", i18n::mark("Cycle Repeat"), PLAYBACK, "Ctrl+r", "r")),
    reaching(Scope::Both, action("playback.favorite", i18n::mark("Like Track"), PLAYBACK, "Alt+Shift+B", "f")),
    reaching(Scope::Both, action("playback.seekForward", i18n::mark("Seek Forward"), PLAYBACK, "Ctrl+Shift+ArrowRight", "l")),
    reaching(Scope::Both, action("playback.seekBack", i18n::mark("Seek Back"), PLAYBACK, "Ctrl+Shift+ArrowLeft", "h")),
    action("nav.back", i18n::mark("Go Back"), NAVIGATION, "Alt+ArrowLeft", "Ctrl+o"),
    action("nav.forward", i18n::mark("Go Forward"), NAVIGATION, "Alt+ArrowRight", "Ctrl+i"),
    action("nav.search", i18n::mark("Search"), NAVIGATION, "Ctrl+k", "/"),
    action("nav.settings", i18n::mark("Settings"), NAVIGATION, "Ctrl+,", ""),
    action("nav.newTab", i18n::mark("New Tab"), NAVIGATION, "Ctrl+t", ""),
    action("nav.closeTab", i18n::mark("Close Tab"), NAVIGATION, "Ctrl+w", ""),
    action("nav.nextTab", i18n::mark("Next tab"), NAVIGATION, "Ctrl+Tab", ""),
    action("nav.previousTab", i18n::mark("Previous tab"), NAVIGATION, "Ctrl+Shift+Tab", ""),
    action("queue.next", i18n::mark("Next queue"), NAVIGATION, "Ctrl+Alt+ArrowRight", ""),
    action("queue.previous", i18n::mark("Previous queue"), NAVIGATION, "Ctrl+Alt+ArrowLeft", ""),
    action("nav.quit", i18n::mark("Quit"), NAVIGATION, "Ctrl+q", ""),
    action("ui.sidebar", i18n::mark("Toggle Sidebar"), INTERFACE, "Shift+S", ""),
    action("ui.queue", i18n::mark("Queue"), INTERFACE, "q", ""),
    action("ui.lyrics", i18n::mark("Lyrics"), INTERFACE, "Shift+L", ""),
    reaching(Scope::Both, action("ui.escape", i18n::mark("Close / Dismiss"), INTERFACE, "Escape", "")),
    action("ui.showShortcuts", i18n::mark("Show Shortcuts"), INTERFACE, "?", ""),
    action("ui.openLink", i18n::mark("Open Music Link"), INTERFACE, "Ctrl+l", ""),
    reaching(Scope::Both, action("ui.miniPlayer", i18n::mark("Toggle Mini Player"), INTERFACE, "Shift+M", "")),
    // The mini player's own window: its five surfaces on the digits.
    reaching(Scope::Mini, action("mini.micro", i18n::mark("Micro view"), MINI, "1", "")),
    reaching(Scope::Mini, action("mini.compact", i18n::mark("Compact view"), MINI, "2", "")),
    reaching(Scope::Mini, action("mini.artwork", i18n::mark("Artwork view"), MINI, "3", "")),
    reaching(Scope::Mini, action("mini.queue", i18n::mark("Queue view"), MINI, "4", "")),
    reaching(Scope::Mini, action("mini.lyrics", i18n::mark("Lyrics view"), MINI, "5", "")),
    reaching(Scope::Both, action("ui.focusMode", i18n::mark("Toggle Immersive View"), INTERFACE, "Shift+I", "")),
    // The immersive view's own: the arrows seek while it is open.
    reaching(Scope::Immersive, action("focus.seekForward", i18n::mark("Seek Forward 5 s"), IMMERSIVE, "ArrowRight", "")),
    reaching(Scope::Immersive, action("focus.seekBack", i18n::mark("Seek Back 5 s"), IMMERSIVE, "ArrowLeft", "")),
    reaching(Scope::Immersive, action("focus.seekForwardLong", i18n::mark("Seek Forward 10 s"), IMMERSIVE, "Shift+ArrowRight", "")),
    reaching(Scope::Immersive, action("focus.seekBackLong", i18n::mark("Seek Back 10 s"), IMMERSIVE, "Shift+ArrowLeft", "")),
];

/// The actions a key still reaches while a text field has the keyboard,
/// and only with Ctrl held.
const THROUGH_TEXT: [&str; 7] = ["nav.search", "nav.settings", "ui.openLink", "nav.nextTab", "nav.previousTab", "queue.next", "queue.previous"];

pub(crate) fn has_action(id: &str) -> bool { known(id).is_some() }
pub(crate) fn command_label(id: &str) -> String {
    known(id).map(|action| i18n::t(action.label)).unwrap_or_else(|| i18n::t("Command help"))
}

fn known(id: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.id == id)
}

/// The key of an event as a shortcut is written: `[Ctrl+][Alt+][Shift+]Key`.
/// `None` for a modifier pressed alone and for keys that cannot be written.
pub fn shortcut(key: i32, modifiers: i32, text: &str) -> Option<String> {
    let ctrl = modifiers & (qt::CONTROL | qt::META) != 0;
    let (alt, shift) = (modifiers & qt::ALT != 0, modifiers & qt::SHIFT != 0 || key == qt::BACKTAB);
    let named = match key {
        qt::SPACE => Some("Space"),
        qt::LEFT => Some("ArrowLeft"),
        qt::RIGHT => Some("ArrowRight"),
        qt::UP => Some("ArrowUp"),
        qt::DOWN => Some("ArrowDown"),
        qt::ESCAPE => Some("Escape"),
        qt::RETURN | qt::ENTER => Some("Enter"),
        qt::TAB | qt::BACKTAB => Some("Tab"),
        qt::BACKSPACE => Some("Backspace"),
        qt::DELETE => Some("Delete"),
        _ => None,
    };
    let printable = || {
        let mut letters = text.chars();
        letters.next().filter(|c| !c.is_control() && letters.next().is_none())
    };
    // Whether Shift is written: it is for what Shift does not turn into
    // another sign.
    let (token, keeps_shift) = if let Some(name) = named {
        (name.to_owned(), true)
    } else if printable().is_some_and(|c| !c.is_alphanumeric()) {
        (printable()?.to_string(), false)
    } else if (0x41..=0x5a).contains(&key) {
        let letter = key as u8 as char;
        (if shift { letter.to_string() } else { letter.to_ascii_lowercase().to_string() }, true)
    } else if (0x30..=0x39).contains(&key) && (!shift || ctrl) {
        ((key as u8 as char).to_string(), true)
    } else if (0x21..=0x7e).contains(&key) && printable().is_none() && (!ctrl || !shift) {
        ((key as u8 as char).to_string(), false)
    } else {
        (printable()?.to_string(), false)
    };
    let mut written = String::new();
    for (held, name) in [(ctrl, "Ctrl+"), (alt, "Alt+"), (shift && keeps_shift, "Shift+")] {
        if held {
            written.push_str(name);
        }
    }
    written.push_str(&token);
    Some(written)
}

/// A shortcut as the sheet shows it: "Ctrl + ▶".
pub fn shown(shortcut: &str) -> String {
    let Some(key) = shortcut.rsplit('+').next().filter(|k| !k.is_empty()).or_else(|| shortcut.ends_with('+').then_some("+")) else {
        return String::new();
    };
    let modifiers = &shortcut[..shortcut.len() - key.len()];
    let key = match key {
        "ArrowLeft" => "◀".to_owned(),
        "ArrowRight" => "▶".to_owned(),
        "ArrowUp" => "▲".to_owned(),
        "ArrowDown" => "▼".to_owned(),
        "Escape" => "Esc".to_owned(),
        "Enter" => "↵".to_owned(),
        "Backspace" => "⌫".to_owned(),
        "Delete" => "Del".to_owned(),
        other => if other.chars().count() == 1 { other.to_uppercase() } else { other.to_owned() },
    };
    modifiers.split('+').filter(|m| !m.is_empty()).map(str::to_owned).chain([key]).collect::<Vec<_>>().join(" + ")
}

/// What a key pressed in the window comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pressed {
    /// Not a shortcut: the key goes on to whoever wants it.
    Pass,
    /// Taken, with nothing for the window to do.
    Taken,
    /// Taken, and the window runs this action.
    Run(&'static str),
}

/// The keys as the listener has them, and a change of key in progress.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Keys {
    keymap: Keymap,
    /// The listener's own keys, by action, over the keymap's.
    own: BTreeMap<String, String>,
    /// The action whose key is being changed, the key last tried for it
    /// and the action that key already belongs to.
    recording: String,
    tried: String,
    taken_by: &'static str,
    sequence: super::vi_keys::Sequence,
    repeat: usize,
}

impl Keys {
    fn read(keymap: Option<String>, own: Option<String>) -> Self {
        let own: BTreeMap<String, Value> = own.and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
        Keys {
            keymap: Keymap::named(keymap.as_deref().unwrap_or_default()),
            own: own.into_iter().filter_map(|(id, key)| Some((id, key.as_str()?.to_owned()))).collect(),
            ..Keys::default()
        }
    }

    fn own_json(&self) -> String {
        serde_json::to_string(&self.own).unwrap_or_default()
    }

    /// The key each action answers to now. An action whose keymap key the
    /// listener gave to another one is left without a key.
    fn active(&self) -> Vec<(&'static Action, String)> {
        let own = |a: &Action| self.own.get(a.id).filter(|key| !key.is_empty());
        ACTIONS.iter().map(|a| {
            let key = match own(a) {
                Some(key) => key.clone(),
                None => {
                    let base = a.base(self.keymap);
                    let claimed = ACTIONS.iter().any(|other| other.id != a.id && own(other).is_some_and(|key| key == base));
                    if claimed { String::new() } else { base.to_owned() }
                }
            };
            (a, key)
        }).collect()
    }

    fn action_for(&self, shortcut: &str) -> Option<&'static Action> {
        self.active().into_iter().find(|(_, key)| key == shortcut).map(|(a, _)| a)
    }

    fn modified(&self, a: &Action) -> bool {
        self.own.get(a.id).is_some_and(|key| key != a.base(self.keymap))
    }

    /// Gives an action a key. Refused when another action answers to it:
    /// that action is returned.
    fn bind(&mut self, id: &str, shortcut: &str) -> Result<(), &'static Action> {
        let Some(a) = known(id) else { return Ok(()) };
        if let Some(other) = self.action_for(shortcut).filter(|other| other.id != id) {
            return Err(other);
        }
        if shortcut == a.base(self.keymap) {
            self.own.remove(id);
        } else {
            self.own.insert(id.to_owned(), shortcut.to_owned());
        }
        Ok(())
    }

    /// A key while a change of key is in progress. It is always taken.
    fn capture(&mut self, key: i32, modifiers: i32, text: &str) -> bool {
        if key == qt::ESCAPE {
            self.stop_recording();
            return false;
        }
        let Some(shortcut) = shortcut(key, modifiers, text) else { return false };
        let id = self.recording.clone();
        match self.bind(&id, &shortcut) {
            Ok(()) => {
                self.stop_recording();
                true
            }
            Err(other) => {
                self.tried = shortcut;
                self.taken_by = other.label;
                false
            }
        }
    }

    fn stop_recording(&mut self) {
        self.recording.clear();
        self.tried.clear();
        self.taken_by = "";
    }

    /// What a key comes to where it was pressed. `typing` says a text
    /// field has the keyboard. An action that answers elsewhere passes, so
    /// the window never runs the mini player's keys and the mini player
    /// never runs the window's.
    fn pressed_in(&self, scope: Scope, key: i32, modifiers: i32, text: &str, typing: bool) -> Pressed {
        let ctrl = modifiers & (qt::CONTROL | qt::META) != 0;
        if typing && (!ctrl || matches!(key, 0x41 | 0x43 | 0x56 | 0x58 | 0x59 | 0x5a)) {
            return Pressed::Pass;
        }
        let Some(a) = shortcut(key, modifiers, text).and_then(|shortcut| self.action_for(&shortcut)) else { return Pressed::Pass };
        if typing && !THROUGH_TEXT.contains(&a.id) {
            return Pressed::Pass;
        }
        // The immersive view sits over the window and answers its keys too.
        let reaches = a.scope == Scope::Both || a.scope == scope || (scope == Scope::Immersive && a.scope == Scope::Window);
        if !reaches {
            return Pressed::Pass;
        }
        Pressed::Run(a.id)
    }

    /// The sheet: the groups in three columns, each action with its key,
    /// and what the editor shows of a change in progress.
    fn document(&self) -> Value {
        let active = self.active();
        let groups: Vec<Value> = GROUPS.iter().map(|group| {
            let rows: Vec<Value> = active.iter().filter(|(a, _)| a.group == *group).map(|(a, key)| json!({
                "command": crate::commands::for_action(a.id), "id": a.id, "label": i18n::t(a.label), "shortcut": shown(key), "modified": self.modified(a),
            })).collect();
            json!({"label": i18n::t(group), "rows": rows})
        }).collect();
        // One after another, not dealt out: the first columns fill first.
        let per = groups.len().div_ceil(3).max(1);
        let mut columns = groups.chunks(per).map(<[Value]>::to_vec);
        json!({
            "col1": columns.next().unwrap_or_default(), "col2": columns.next().unwrap_or_default(), "col3": columns.next().unwrap_or_default(),
            "keymap": self.keymap.name(),
            "modified": ACTIONS.iter().filter(|a| self.modified(a)).count(),
            "recording": self.recording, "tried": shown(&self.tried),
            "takenBy": if self.taken_by.is_empty() { String::new() } else { i18n::t(self.taken_by) },
        })
    }
}

impl Chilpa {
    fn keys(&mut self) -> &mut Keys {
        if self.keys.is_none() {
            self.keys = Some(Keys::read(self.preference("ui.keys.keymap"), self.preference("ui.keys.own")));
        }
        self.keys.as_mut().expect("just read")
    }

    fn keep_keys(&mut self) {
        let (keymap, own) = (self.keys().keymap.name(), self.keys().own_json());
        self.remember_preference("ui.keys.keymap", keymap);
        self.remember_preference("ui.keys.own", &own);
    }

    /// A key pressed in the window, as Qt names it. While a key is being
    /// changed every key belongs to that change.
    pub fn key_pressed(&mut self, key: i32, modifiers: i32, text: &str, typing: bool) -> Pressed {
        self.key_pressed_in(Scope::Window, key, modifiers, text, typing)
    }

    /// A key pressed in one of the windows.
    pub fn key_repeat(&mut self) -> usize { self.keys().repeat.max(1) }

    pub fn key_pressed_in(&mut self, scope: Scope, key: i32, modifiers: i32, text: &str, typing: bool) -> Pressed {
        if !self.keys().recording.is_empty() {
            if self.keys().capture(key, modifiers, text) {
                self.keep_keys();
            }
            return Pressed::Taken;
        }
        self.keys().repeat = 1;
        if self.keys().keymap == Keymap::Vi && scope != Scope::Mini {
            let Some(token) = shortcut(key, modifiers, text) else { return Pressed::Pass; };
            let keys = self.keys();
            if typing { keys.sequence.clear(); }
            else {
                let input = if keys.own.values().any(|value| value == &token) {
                    keys.sequence.clear(); super::vi_keys::Input::Key(token, 1)
                } else { keys.sequence.press(&token, std::time::Instant::now()) };
                match input {
                    super::vi_keys::Input::Pending => return Pressed::Taken,
                    super::vi_keys::Input::Key(token, count) => {
                        keys.repeat = count;
                        if let Some(a) = keys.action_for(&token) {
                            if a.scope == Scope::Both || a.scope == scope || (scope == Scope::Immersive && a.scope == Scope::Window) { return Pressed::Run(a.id); }
                        }
                        return Pressed::Pass;
                    }
                }
            }
        }
        self.keys().pressed_in(scope, key, modifiers, text, typing)
    }

    /// The shortcuts as the sheet and the editor show them.
    pub fn keys_json(&mut self) -> String {
        self.keys().document().to_string()
    }

    /// One action's shortcut as the sheet shows it, for a menu's hint:
    /// empty when it has none.
    pub fn shortcut_shown(&mut self, id: &str) -> String {
        self.keys().active().into_iter().find(|(action, _)| action.id == id).map(|(_, key)| shown(&key)).unwrap_or_default()
    }

    /// What the editor asks: "record" an action's key, "cancel" that,
    /// "reset" one action, "reset-all", or choose a "keymap".
    pub fn keys_command(&mut self, command: &str, argument: &str) {
        let keys = self.keys();
        match command {
            "record" if known(argument).is_some() => {
                keys.stop_recording();
                keys.recording = argument.to_owned();
                return;
            }
            "cancel" => {
                keys.stop_recording();
                return;
            }
            "reset" => {
                keys.own.remove(argument);
            }
            "reset-all" => keys.own.clear(),
            "keymap" => { keys.keymap = Keymap::named(argument); keys.sequence.clear(); },
            _ => return,
        }
        self.keep_keys();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: i32 = 0x53;

    #[test]
    fn a_key_is_written_the_same_way_however_it_was_typed() {
        assert_eq!(shortcut(qt::SPACE, 0, " ").as_deref(), Some("Space"));
        assert_eq!(shortcut(S, qt::CONTROL, "\u{13}").as_deref(), Some("Ctrl+s"), "the letter, not the control character");
        assert_eq!(shortcut(S, qt::SHIFT, "S").as_deref(), Some("Shift+S"));
        assert_eq!(shortcut(S, qt::META, "").as_deref(), Some("Ctrl+s"), "the command key is Ctrl");
        assert_eq!(shortcut(0x42, qt::ALT | qt::SHIFT, "B").as_deref(), Some("Alt+Shift+B"));
        assert_eq!(shortcut(qt::RIGHT, qt::CONTROL | qt::SHIFT, "").as_deref(), Some("Ctrl+Shift+ArrowRight"));
        assert_eq!(shortcut(0x3f, qt::SHIFT, "?").as_deref(), Some("?"), "Shift made the sign: it is not written");
        assert_eq!(shortcut(0x2c, qt::CONTROL, "").as_deref(), Some("Ctrl+,"));
        assert_eq!(shortcut(0x31, 0, "1").as_deref(), Some("1"));
        assert_eq!(shortcut(0x0100_0020, qt::SHIFT, ""), None, "Shift alone is no key");
        assert_eq!(shortcut(0x0100_0030, 0, ""), None, "nor is F1");
    }

    #[test]
    fn a_shortcut_is_shown_with_its_signs() {
        assert_eq!(shown("Ctrl+ArrowRight"), "Ctrl + ▶");
        assert_eq!(shown("Shift+S"), "Shift + S");
        assert_eq!(shown("m"), "M");
        assert_eq!(shown("?"), "?");
        assert_eq!(shown("Ctrl+,"), "Ctrl + ,");
        assert_eq!(shown("Escape"), "Esc");
        assert_eq!(shown(""), "");
    }

    #[test]
    fn the_command_line_answers_colon_in_every_keymap_and_keyboard_layout() {
        for keymap in [Keymap::Default, Keymap::Letters, Keymap::Vi] {
            let keys = Keys { keymap, ..Keys::default() };
            for (key, modifiers, text) in [(0x3a, qt::SHIFT, ":"), (0x3b, qt::SHIFT, ":"), (0x4d, qt::SHIFT, ":"), (0x3a, 0, "")] {
                assert_eq!(shortcut(key, modifiers, text).as_deref(), Some(":"));
                assert_eq!(keys.pressed_in(Scope::Window, key, modifiers, text, false), Pressed::Run("commands.open"));
                assert_eq!(keys.pressed_in(Scope::Window, key, modifiers, text, true), Pressed::Pass);
            }
            let mut keys = Keys { keymap, recording: "commands.open".into(), ..Keys::default() };
            assert!(keys.capture(0x3b, qt::SHIFT, ":"));
            assert!(keys.recording.is_empty());
            keys.bind("commands.open", "Ctrl+;").unwrap();
            assert!(keys.action_for(":").is_none());
            keys.recording = "commands.open".into();
            assert!(keys.capture(0x3b, qt::SHIFT, ":"));
            assert_eq!(keys.action_for(":").unwrap().id, "commands.open");
        }
    }

    #[test]
    fn each_keymap_has_no_key_twice() {
        for keymap in [Keymap::Default, Keymap::Letters, Keymap::Vi] {
            let mut seen = std::collections::HashSet::new();
            for a in ACTIONS {
                if a.base(keymap).is_empty() { continue; }
                assert!(seen.insert(a.base(keymap)), "{} twice in {keymap:?}", a.base(keymap));
            }
        }
    }

    #[test]
    fn tab_and_queue_shortcuts_work_from_fields_and_remain_customizable() {
        let mut keys = Keys::default();
        for typing in [false, true] {
            assert_eq!(keys.pressed_in(Scope::Window, qt::TAB, qt::CONTROL, "", typing), Pressed::Run("nav.nextTab"));
            assert_eq!(keys.pressed_in(Scope::Window, qt::BACKTAB, qt::CONTROL | qt::SHIFT, "", typing), Pressed::Run("nav.previousTab"));
            assert_eq!(keys.pressed_in(Scope::Window, qt::RIGHT, qt::CONTROL | qt::ALT, "", typing), Pressed::Run("queue.next"));
            assert_eq!(keys.pressed_in(Scope::Window, qt::LEFT, qt::CONTROL | qt::ALT, "", typing), Pressed::Run("queue.previous"));
        }
        assert_eq!(keys.pressed_in(Scope::Window, qt::TAB, 0, "", true), Pressed::Pass);
        assert_eq!(keys.pressed_in(Scope::Mini, qt::TAB, qt::CONTROL, "", false), Pressed::Pass);
        keys.bind("queue.next", "Ctrl+Alt+j").unwrap();
        assert_eq!(keys.pressed_in(Scope::Window, 0x4a, qt::CONTROL | qt::ALT, "", true), Pressed::Run("queue.next"));
        assert_eq!(keys.pressed_in(Scope::Window, qt::RIGHT, qt::CONTROL | qt::ALT, "", false), Pressed::Pass);
    }

    #[test]
    fn keys_reach_their_actions_and_text_fields_keep_theirs() {
        let keys = Keys::default();
        assert_eq!(keys.pressed_in(Scope::Window, qt::SPACE, 0, " ", false), Pressed::Run("playback.toggle"));
        assert_eq!(keys.pressed_in(Scope::Window, qt::SPACE, 0, " ", true), Pressed::Pass, "a space typed is a space");
        assert_eq!(keys.pressed_in(Scope::Window, 0x4b, qt::CONTROL, "", true), Pressed::Run("nav.search"), "search is reached from a field");
        assert_eq!(keys.pressed_in(Scope::Window, S, qt::CONTROL, "", true), Pressed::Pass, "shuffle is not");
        assert_eq!(keys.pressed_in(Scope::Window, 0x41, qt::CONTROL, "", true), Pressed::Pass, "select all is the field's");
        assert_eq!(keys.pressed_in(Scope::Window, 0x5a, 0, "z", false), Pressed::Pass, "no action has it");
    }

    #[test]
    fn the_mini_players_keys_answer_in_its_window_and_the_toggle_in_both() {
        let keys = Keys::default();
        assert_eq!(keys.pressed_in(Scope::Mini, 0x31, 0, "1", false), Pressed::Run("mini.micro"));
        assert_eq!(keys.pressed_in(Scope::Window, 0x31, 0, "1", false), Pressed::Pass, "never in the window");
        assert_eq!(keys.pressed_in(Scope::Mini, 0x4d, qt::SHIFT, "M", false), Pressed::Run("ui.miniPlayer"));
        assert_eq!(keys.pressed_in(Scope::Window, 0x4d, qt::SHIFT, "M", false), Pressed::Run("ui.miniPlayer"));
        assert_eq!(keys.pressed_in(Scope::Mini, qt::SPACE, 0, " ", false), Pressed::Run("playback.toggle"), "playback reaches the mini");
        assert_eq!(keys.pressed_in(Scope::Mini, qt::ESCAPE, 0, "", false), Pressed::Run("ui.escape"));
        assert_eq!(keys.pressed_in(Scope::Mini, 0x4b, qt::CONTROL, "", false), Pressed::Pass, "search is the window's");
        let keys = Keys { keymap: Keymap::Letters, ..Keys::default() };
        assert_eq!(keys.pressed_in(Scope::Mini, 0x35, 0, "5", false), Pressed::Run("mini.lyrics"), "the digits are free in the letters keymap too");
    }

    #[test]
    fn the_immersive_view_answers_the_windows_keys_and_its_own_arrows() {
        let keys = Keys::default();
        assert_eq!(keys.pressed_in(Scope::Immersive, qt::RIGHT, 0, "", false), Pressed::Run("focus.seekForward"));
        assert_eq!(keys.pressed_in(Scope::Immersive, qt::LEFT, qt::SHIFT, "", false), Pressed::Run("focus.seekBackLong"));
        assert_eq!(keys.pressed_in(Scope::Window, qt::RIGHT, 0, "", false), Pressed::Pass, "the arrows seek only over the overlay");
        assert_eq!(keys.pressed_in(Scope::Mini, qt::RIGHT, 0, "", false), Pressed::Pass);
        assert_eq!(keys.pressed_in(Scope::Immersive, 0x4b, qt::CONTROL, "", false), Pressed::Run("nav.search"), "the window's keys reach it");
        assert_eq!(keys.pressed_in(Scope::Immersive, 0x31, 0, "1", false), Pressed::Pass, "the mini player's do not");
        assert_eq!(keys.pressed_in(Scope::Immersive, 0x49, qt::SHIFT, "I", false), Pressed::Run("ui.focusMode"));
        assert_eq!(keys.pressed_in(Scope::Mini, 0x49, qt::SHIFT, "I", false), Pressed::Run("ui.focusMode"), "the toggle answers everywhere");
        assert_eq!(Scope::named("immersive"), Scope::Immersive);
        assert_eq!(Scope::named("elsewhere"), Scope::Window);
    }

    #[test]
    fn the_letters_keymap_puts_actions_on_single_keys() {
        let keys = Keys { keymap: Keymap::Letters, ..Keys::default() };
        assert_eq!(keys.pressed_in(Scope::Window, 0x4e, 0, "n", false), Pressed::Run("playback.next"));
        assert_eq!(keys.pressed_in(Scope::Window, 0x4e, 0, "n", true), Pressed::Pass);
        assert_eq!(keys.pressed_in(Scope::Window, 0x2f, 0, "/", false), Pressed::Run("nav.search"));
        assert_eq!(keys.pressed_in(Scope::Window, qt::RIGHT, qt::CONTROL, "", false), Pressed::Pass, "its default key is no longer its key");
        assert_eq!(keys.pressed_in(Scope::Window, qt::SPACE, 0, " ", false), Pressed::Run("playback.toggle"), "what it does not change stays");
    }

    #[test]
    fn a_key_of_another_action_is_refused_and_a_free_one_is_taken() {
        let mut keys = Keys::default();
        assert_eq!(keys.bind("playback.mute", "q").unwrap_err().id, "ui.queue");
        assert!(keys.own.is_empty());
        keys.bind("playback.mute", "x").unwrap();
        assert_eq!(keys.action_for("x").unwrap().id, "playback.mute");
        assert!(keys.action_for("m").is_none(), "its old key is free");
        assert!(keys.modified(known("playback.mute").unwrap()));
        keys.bind("playback.mute", "m").unwrap();
        assert!(keys.own.is_empty(), "back on its own key, nothing of the listener's is kept");
    }

    #[test]
    fn a_key_the_listener_took_leaves_the_other_keymaps_action_without_one() {
        let mut keys = Keys::default();
        // Free in the default keymap, the key of "next" in the letters one.
        keys.bind("playback.mute", "n").unwrap();
        keys.keymap = Keymap::Letters;
        assert_eq!(keys.action_for("n").unwrap().id, "playback.mute");
        let next = keys.active().into_iter().find(|(a, _)| a.id == "playback.next").unwrap();
        assert_eq!(next.1, "", "one key, one action");
    }

    #[test]
    fn recording_takes_every_key_until_one_is_free_or_escape() {
        let mut keys = Keys { recording: "playback.mute".into(), ..Keys::default() };
        assert!(!keys.capture(0x0100_0020, qt::SHIFT, ""), "a modifier alone changes nothing");
        assert_eq!(keys.recording, "playback.mute");
        assert!(!keys.capture(0x51, 0, "q"));
        assert_eq!((keys.tried.as_str(), keys.taken_by), ("q", "Queue"), "taken: it says by what and goes on");
        assert!(keys.capture(0x58, 0, "x"));
        assert_eq!(keys.recording, "");
        assert_eq!(keys.own.get("playback.mute").map(String::as_str), Some("x"));
        let mut keys = Keys { recording: "playback.mute".into(), ..Keys::default() };
        assert!(!keys.capture(qt::ESCAPE, 0, ""));
        assert_eq!(keys.recording, "", "escape leaves it as it was");
        assert!(keys.own.is_empty());
    }

    #[test]
    fn the_sheet_lists_every_action_in_three_columns() {
        let keys = Keys::read(Some("letters".into()), Some(r#"{"playback.mute":"x","gone.action":"y","bad":3}"#.into()));
        assert_eq!(keys.keymap, Keymap::Letters);
        let sheet = keys.document();
        let rows = |column: &str| sheet[column].as_array().unwrap().iter().flat_map(|g| g["rows"].as_array().unwrap().clone()).collect::<Vec<_>>();
        assert_eq!(rows("col1").len() + rows("col2").len() + rows("col3").len(), ACTIONS.len());
        assert_eq!(sheet["col1"][0]["label"], "Playback");
        assert_eq!(sheet["modified"], 1);
        let mute = rows("col1").into_iter().find(|r| r["id"] == "playback.mute").unwrap();
        assert_eq!((mute["shortcut"].as_str(), mute["modified"].as_bool()), (Some("X"), Some(true)));
        assert_eq!(Keys::read(Some("typo".into()), Some("not json".into())), Keys::default());
    }
}
