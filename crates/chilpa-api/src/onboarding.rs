//! Welcome to Chilpa and What's new: QBZ's What's New deck
//! (`qbz-qt/src/whats_new_qt.rs`), and a Welcome deck for a new install.
//!
//! A deck is a few cards, each an image, a title, a sentence or two and an
//! optional button; What's new adds the version's release notes, read from
//! GitHub, as its last page. Each opens by itself once: the Welcome on a new
//! install, What's new after an update that brings a deck. Closing either,
//! from any page, marks the version seen; the menu reopens What's new.

use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;

/// The version a deck was last closed in, or the one Chilpa started on when
/// it had nothing to show.
pub(crate) const SEEN: &str = "onboarding.seenVersion";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASES: &str = "https://api.github.com/repos/vicrodh/chilpa/releases/tags";

/// Block kinds of the release notes, QBZ's (`whats_new_qt.rs:56-61`).
const SECTION: i32 = 0;
const BULLET: i32 = 1;
const PARAGRAPH: i32 = 2;
const LINK: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Welcome,
    News,
}

/// One card. `action` is where its button leads, for the window to follow:
/// `libraryFolders`, a Settings section, or `hifiWizard`; empty, no button.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
struct Card {
    image: &'static str,
    title: String,
    body: String,
    #[serde(rename = "actionLabel")]
    action_label: String,
    action: &'static str,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
struct Block {
    kind: i32,
    level: i32,
    text: String,
    id: String,
    url: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
struct TocEntry {
    id: String,
    label: String,
}

fn card(image: &'static str, title: &str, body: &str, action_label: &str, action: &'static str) -> Card {
    let action_label = if action_label.is_empty() { String::new() } else { chilpa_i18n::t(action_label) };
    Card { image, title: chilpa_i18n::t(title), body: chilpa_i18n::t(body), action_label, action }
}

/// The cards of a deck. What's new has none for a version without news of
/// its own; the menu then opens straight into the release notes, as QBZ.
/// A version's cards go here, matched on MAJOR.MINOR.PATCH so its -rc builds
/// show them too (QBZ's `deck_for`); 0.1.0 is the first release and the
/// Welcome is its deck.
fn deck(kind: Kind) -> Vec<Card> {
    match kind {
        Kind::Welcome => vec![
            card("welcome.png", chilpa_i18n::mark("A player for your own music"),
                chilpa_i18n::mark("Chilpa plays the files on your disks and the servers you run, and plugins add more sources. It tells you plainly whether what reaches your DAC is bit-perfect."), "", ""),
            card("your-music.png", chilpa_i18n::mark("Your music"),
                chilpa_i18n::mark("Add the folders where your music lives. Chilpa reads their tags and covers and builds your library from them."), chilpa_i18n::mark("Add a folder"), "libraryFolders"),
            card("plugins.png", chilpa_i18n::mark("Servers and plugins"),
                chilpa_i18n::mark("Play from the servers you run, and add sources and features with plugins from the official catalog or the third-party one."), chilpa_i18n::mark("Open plugins"), "plugins"),
            card("hifi-wizard.png", chilpa_i18n::mark("Your DAC"),
                chilpa_i18n::mark("The HiFi Wizard checks your audio stack, finds your DAC and helps you set it up for bit-perfect playback."), chilpa_i18n::mark("Open the HiFi Wizard"), "hifiWizard"),
            card("make-it-yours.png", chilpa_i18n::mark("Make it yours"),
                chilpa_i18n::mark("Pick a theme, or keep System to follow your desktop, choose your language, and press : to open the command line."), chilpa_i18n::mark("Open appearance"), "appearance"),
        ],
        Kind::News => Vec::new(),
    }
}

/// What this start opens by itself. A version never recorded and a library
/// without folders is a new install; a version recorded and not this one is
/// an update, which opens What's new only when this version has a deck.
fn decide(seen: Option<&str>, version: &str, empty_library: bool, has_deck: bool) -> Option<Kind> {
    match seen {
        Some(seen) if seen == version => None,
        None if empty_library => Some(Kind::Welcome),
        _ => has_deck.then_some(Kind::News),
    }
}

struct State {
    started: bool,
    open: bool,
    kind: Kind,
    loading: bool,
    version: String,
    date: String,
    has_body: bool,
    blocks: Vec<Block>,
    toc: Vec<TocEntry>,
    /// Which opening the notes being read belong to: a reply for an earlier
    /// one is dropped.
    opening: u64,
    revision: u64,
}

static STATE: Mutex<State> = Mutex::new(State {
    started: false, open: false, kind: Kind::Welcome, loading: false, version: String::new(), date: String::new(),
    has_body: false, blocks: Vec::new(), toc: Vec::new(), opening: 0, revision: 1,
});

fn change(f: impl FnOnce(&mut State)) {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut state);
    state.revision = state.revision.wrapping_add(1);
}

/// Shows a deck. What's new also reads its release notes, off the window's
/// thread, every time it opens, as QBZ; offline there are none.
fn open(kind: Kind) {
    let mut opening = 0;
    change(|s| {
        s.open = true;
        s.kind = kind;
        s.version = VERSION.to_string();
        s.date.clear();
        s.has_body = false;
        s.blocks.clear();
        s.toc.clear();
        s.loading = kind == Kind::News;
        s.opening = s.opening.wrapping_add(1);
        opening = s.opening;
    });
    if kind != Kind::News {
        return;
    }
    let read = move || {
        let notes = release_notes(VERSION);
        change(|s| {
            if s.opening != opening {
                return;
            }
            s.loading = false;
            if let Some((date, body)) = notes {
                let (blocks, toc) = render_markdown(&body);
                s.date = date;
                s.has_body = !blocks.is_empty();
                s.blocks = blocks;
                s.toc = toc;
            }
        });
    };
    if std::thread::Builder::new().name("release-notes".into()).spawn(read).is_err() {
        change(|s| s.loading = false);
    }
}

/// The published release of `version`: its civil date and its body. None
/// offline, on any failure, and for a draft or a prerelease.
fn release_notes(version: &str) -> Option<(String, String)> {
    if chilpa_core::network::current().offline {
        return None;
    }
    let url = format!("{RELEASES}/v{version}");
    let response = chilpa_core::call_server(&chilpa_core::Call {
        method: "GET", url: &url, headers: &[("Accept".into(), "application/vnd.github+json".into())], body: None,
        timeout: Duration::from_secs(8), max_bytes: 512 * 1024,
    }).ok()?;
    if response.status != 200 {
        return None;
    }
    let release: serde_json::Value = serde_json::from_slice(&response.body).ok()?;
    if release["draft"] == true || release["prerelease"] == true {
        return None;
    }
    Some((release_date(release["published_at"].as_str().unwrap_or("")), release["body"].as_str().unwrap_or("").to_string()))
}

/// GitHub's RFC 3339 date reduced to its civil date as written; the window
/// formats it for the reader's language. Anything else comes back as is.
fn release_date(iso: &str) -> String {
    let bytes = iso.as_bytes();
    if bytes.len() < 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return iso.to_string();
    }
    let number = |a: usize, b: usize| iso[a..b].parse::<u32>().ok();
    match (number(0, 4), number(5, 7), number(8, 10)) {
        (Some(_), Some(month), Some(day)) if (1..=12).contains(&month) && (1..=31).contains(&day) => iso[..10].to_string(),
        _ => iso.to_string(),
    }
}

impl crate::Chilpa {
    /// Opens what this start has to show, once a session. The window asks
    /// until the plugins are up: the splash covers it until then.
    pub fn onboarding_start(&mut self) {
        if self.plugins_starting() {
            return;
        }
        let mut first = false;
        change(|s| first = !std::mem::replace(&mut s.started, true));
        if !first {
            return;
        }
        let seen = self.preference(SEEN);
        let has_deck = !deck(Kind::News).is_empty();
        match decide(seen.as_deref(), VERSION, self.roots().is_empty(), has_deck) {
            Some(kind) => open(kind),
            None if seen.as_deref() != Some(VERSION) => self.remember_preference(SEEN, VERSION),
            None => {}
        }
    }

    /// `welcome` and `whatsNew` open a deck; `close` closes it, from any
    /// page, and marks this version seen.
    pub fn onboarding_command(&mut self, action: &str) {
        match action {
            "welcome" => open(Kind::Welcome),
            "whatsNew" => open(Kind::News),
            "close" => {
                change(|s| s.open = false);
                self.remember_preference(SEEN, VERSION);
            }
            _ => eprintln!("onboarding: unknown action {action}"),
        }
    }

    /// The deck's document when it moved since `seen`: `{started, open, kind,
    /// loading, version, date, hasBody, toc, blocks, deck}`.
    pub fn onboarding_doc(&self, seen: u64) -> Option<(u64, String)> {
        let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        if state.revision == seen {
            return None;
        }
        let doc = serde_json::json!({
            "started": state.started,
            "open": state.open,
            "kind": if state.kind == Kind::Welcome { "welcome" } else { "whatsNew" },
            "loading": state.loading,
            "version": state.version,
            "date": state.date,
            "hasBody": state.has_body,
            "toc": state.toc,
            "blocks": state.blocks,
            "deck": deck(state.kind),
        });
        Some((state.revision, doc.to_string()))
    }
}

// The release notes are rendered as QBZ renders them (`whats_new_qt.rs:379-560`),
// which is not Markdown on purpose: `#` and `##` are one level and both enter
// the contents; an unindented `- item` is a section, not a bullet; bold,
// code and inline links are stripped; a line that is only a link is a link
// block. The window lays the blocks out.

fn strip_inline(text: &str) -> String {
    strip_links(text).replace("**", "").replace('`', "")
}

/// `[label](url)` at `start`: its label, its address and where it ends.
fn link_at(s: &str, start: usize) -> Option<(&str, &str, usize)> {
    let rest = &s[start..];
    if !rest.starts_with('[') {
        return None;
    }
    let close = rest.find(']')?;
    if rest.as_bytes().get(close + 1) != Some(&b'(') {
        return None;
    }
    let open = close + 1;
    let end = open + rest[open..].find(')')?;
    let url = &rest[open + 1..end];
    if url.is_empty() {
        return None;
    }
    Some((&rest[1..close], url, start + end + 1))
}

fn strip_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text.as_bytes()[i] == b'[' {
            if let Some((label, _, end)) = link_at(text, i) {
                out.push_str(label);
                i = end;
                continue;
            }
        }
        let ch = text[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn whole_link(s: &str) -> Option<(&str, &str)> {
    let t = s.trim();
    let (label, url, end) = link_at(t, 0)?;
    (end == t.len()).then_some((label, url))
}

fn slugify(input: &str) -> String {
    let lowered = input.trim().to_lowercase();
    let cleaned: String = lowered.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == ' ' || *ch == '-').collect();
    let mut out = String::with_capacity(cleaned.len());
    let mut hyphen = false;
    for ch in cleaned.chars() {
        if ch == ' ' || ch == '-' {
            if !hyphen {
                out.push('-');
                hyphen = true;
            }
        } else {
            out.push(ch);
            hyphen = false;
        }
    }
    out.trim_matches('-').to_string()
}

fn indentation(line: &str) -> usize {
    line.chars().take_while(|ch| *ch == ' ' || *ch == '\t').map(|ch| if ch == '\t' { 2 } else { 1 }).sum()
}

fn heading(label: &str, level: i32, blocks: &mut Vec<Block>, toc: &mut Vec<TocEntry>) {
    let clean = label.trim();
    if clean.is_empty() {
        return;
    }
    let id = slugify(clean);
    if level == 0 {
        toc.push(TocEntry { id: id.clone(), label: clean.to_string() });
    }
    blocks.push(Block { kind: SECTION, level, text: strip_inline(clean), id, url: String::new() });
}

fn link_block(label: &str, url: &str) -> Block {
    Block { kind: LINK, level: 0, text: strip_inline(label), id: String::new(), url: url.to_string() }
}

fn render_markdown(markdown: &str) -> (Vec<Block>, Vec<TocEntry>) {
    let (mut blocks, mut toc) = (Vec::new(), Vec::new());
    for line in markdown.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ").or_else(|| trimmed.strip_prefix("## ")) {
            heading(rest, 0, &mut blocks, &mut toc);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            heading(rest, 1, &mut blocks, &mut toc);
            continue;
        }
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let level = (indentation(line) / 2) as i32;
            let content = trimmed[2..].trim();
            if level == 0 {
                heading(content, 0, &mut blocks, &mut toc);
            } else if let Some((label, url)) = whole_link(content) {
                blocks.push(link_block(label, url));
            } else {
                blocks.push(Block { kind: BULLET, level, text: strip_inline(content), id: String::new(), url: String::new() });
            }
            continue;
        }
        if let Some((label, url)) = whole_link(trimmed) {
            blocks.push(link_block(label, url));
            continue;
        }
        blocks.push(Block { kind: PARAGRAPH, level: 0, text: strip_inline(trimmed), id: String::new(), url: String::new() });
    }
    (blocks, toc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_install_is_welcomed_and_an_update_opens_only_a_deck_it_has() {
        assert_eq!(decide(None, "0.1.0", true, false), Some(Kind::Welcome), "no version recorded, no folders");
        assert_eq!(decide(None, "0.1.0", false, false), None, "an earlier install with a library and no news");
        assert_eq!(decide(None, "0.2.0", false, true), Some(Kind::News), "an earlier install that updated into a deck");
        assert_eq!(decide(Some("0.1.0"), "0.2.0", true, true), Some(Kind::News), "seen before, even with an empty library");
        assert_eq!(decide(Some("0.1.0"), "0.2.0", false, false), None, "an update without news");
        assert_eq!(decide(Some("0.2.0"), "0.2.0", true, true), None, "already seen");
    }

    #[test]
    fn the_welcome_has_five_cards_and_0_1_0_has_no_news_of_its_own() {
        let welcome = deck(Kind::Welcome);
        assert_eq!(welcome.iter().map(|c| c.action).collect::<Vec<_>>(), ["", "libraryFolders", "plugins", "hifiWizard", "appearance"]);
        assert!(welcome.iter().all(|c| c.image.ends_with(".png") && !c.title.is_empty() && !c.body.is_empty()));
        assert!(welcome.iter().all(|c| c.action.is_empty() == c.action_label.is_empty()), "a button has a label and a place");
        assert!(deck(Kind::News).is_empty());
    }

    #[test]
    fn headings_and_unindented_items_are_sections_and_enter_the_contents() {
        let (blocks, toc) = render_markdown("# Alpha\n## Beta\n### Gamma\n- Delta\n");
        assert_eq!(blocks.iter().map(|b| (b.kind, b.level)).collect::<Vec<_>>(), [(SECTION, 0), (SECTION, 0), (SECTION, 1), (SECTION, 0)]);
        assert_eq!(toc.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), ["Alpha", "Beta", "Delta"]);
        assert_eq!(toc[0].id, "alpha");
    }

    #[test]
    fn nested_items_are_bullets_with_their_markers_stripped() {
        let (blocks, _) = render_markdown("- Fixes\n  - **Bold** and `code` and [a link](https://x.org)\n    - deeper\n\tTabbed paragraph\n");
        assert_eq!(blocks[1], Block { kind: BULLET, level: 1, text: "Bold and code and a link".into(), id: String::new(), url: String::new() });
        assert_eq!((blocks[2].kind, blocks[2].level), (BULLET, 2));
        assert_eq!((blocks[3].kind, blocks[3].text.as_str()), (PARAGRAPH, "Tabbed paragraph"));
    }

    #[test]
    fn a_line_that_is_only_a_link_is_a_link_block() {
        let (blocks, _) = render_markdown("[Full changelog](https://github.com/vicrodh/chilpa/compare/v0.1.0...v0.2.0)\n  - [Docs](https://chilpa.app)\nSee [here](https://chilpa.app) too\n");
        assert_eq!((blocks[0].kind, blocks[0].url.as_str()), (LINK, "https://github.com/vicrodh/chilpa/compare/v0.1.0...v0.2.0"));
        assert_eq!((blocks[1].kind, blocks[1].text.as_str()), (LINK, "Docs"));
        assert_eq!((blocks[2].kind, blocks[2].text.as_str()), (PARAGRAPH, "See here too"));
    }

    #[test]
    fn a_release_date_keeps_its_civil_day_and_odd_input_comes_back() {
        assert_eq!(release_date("2026-10-23T18:04:05Z"), "2026-10-23");
        assert_eq!(release_date("2026-13-01T00:00:00Z"), "2026-13-01T00:00:00Z");
        assert_eq!(release_date("yesterday"), "yesterday");
    }
}
