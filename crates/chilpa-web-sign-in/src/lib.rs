//! The sign-in window's rules, kept apart from the window so they are tested
//! without a display: the arguments it takes, the address that ends a
//! sign-in, the user agent the page sees and how the process exits.

use std::ops::Range;

pub mod native;
pub mod parent;

pub const USAGE: &str = "usage: chilpa-web-sign-in --url https://... --until https://... [--title TITLE] [--user-agent AGENT] \
     [--data-dir DIR --cache-dir DIR]";

#[derive(Debug, PartialEq)]
pub struct Request {
    pub url: String,
    pub until: String,
    pub user_agent: Option<String>,
    pub title: String,
    pub data_dir: Option<String>,
    pub cache_dir: Option<String>,
}

fn https(value: &str) -> bool {
    value.len() <= 8192 && value.starts_with("https://") && value.len() > "https://".len()
}

/// A user agent the page can be given; any other leaves the engine's own.
fn agent(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.contains(['\r', '\n'])
}

impl Request {
    /// The arguments, given as name and value pairs; nothing when one is
    /// unknown, lacks its value, or an address is not `https`.
    pub fn parse(arguments: impl IntoIterator<Item = String>) -> Option<Request> {
        let (mut url, mut until, mut user_agent, mut title) = (None, None, None, String::new());
        let (mut data_dir, mut cache_dir) = (None, None);
        let mut arguments = arguments.into_iter();
        while let Some(name) = arguments.next() {
            let value = arguments.next()?;
            match name.as_str() {
                "--url" => url = Some(value),
                "--until" => until = Some(value),
                "--user-agent" => user_agent = Some(value).filter(|value| agent(value)),
                "--title" => title = value,
                "--data-dir" => data_dir = Some(value).filter(|dir| !dir.is_empty()),
                "--cache-dir" => cache_dir = Some(value).filter(|dir| !dir.is_empty()),
                _ => return None,
            }
        }
        let (url, until) = (url.filter(|url| https(url))?, until.filter(|until| https(until))?);
        Some(Request { url, until, user_agent, title, data_dir, cache_dir })
    }
}

/// Whether an address is the return address: the prefix, then nothing, a
/// query or a fragment.
pub fn reaches(until: &str, address: &str) -> bool {
    address.strip_prefix(until).is_some_and(|rest| rest.is_empty() || rest.starts_with(['?', '#']))
}

/// Where the window saw an address. The codes are the ones `cxx/window.cpp`
/// passes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seen {
    /// A navigation of the page itself, a server redirect included.
    MainFrame,
    /// A navigation inside a frame of the page.
    SubFrame,
    /// A page asking for a new window.
    NewWindow,
    /// The page's address once it changed or finished loading, which also
    /// catches a page restored from history.
    Loaded,
    /// Anything else the page requests: scripts, images, its own calls.
    Resource,
}

impl Seen {
    pub fn from_code(code: u8) -> Option<Seen> {
        Some(match code {
            0 => Seen::MainFrame,
            1 => Seen::SubFrame,
            2 => Seen::NewWindow,
            3 => Seen::Loaded,
            4 => Seen::Resource,
            _ => return None,
        })
    }
}

/// Whether what the window saw ends the sign-in. Only a page or a frame going
/// to the return address does; a script or an image there is not the
/// provider sending the person back.
pub fn ends(until: &str, seen: Seen, address: &str) -> bool {
    seen != Seen::Resource && reaches(until, address)
}

/// The byte range of the engine's own `QtWebEngine/<version> ` token in its
/// user agent, when what is left is the user agent of the Chromium it is
/// built on. Qt WebEngine 6.11 builds it from Chromium's template with the
/// product `QtWebEngine/6.11.2 Chrome/140.0.0.0`
/// (`ContentBrowserClientQt::getUserAgent`), and its client hints already name
/// only Chromium (`ProfileQt::initUserAgentMetadata`); without the token the
/// header says the same as the hints. Anything of another shape is left
/// whole.
pub fn engine_token(agent: &str) -> Option<Range<usize>> {
    const ENGINE: &str = ") AppleWebKit/537.36 (KHTML, like Gecko) ";
    const TOKEN: &str = "QtWebEngine/";
    if !agent.starts_with("Mozilla/5.0 (") || !agent.ends_with(" Safari/537.36") {
        return None;
    }
    let start = agent.find(ENGINE)? + ENGINE.len();
    let version = agent[start..].strip_prefix(TOKEN)?;
    let (number, rest) = version.split_once(' ')?;
    let (chrome, tail) = rest.strip_prefix("Chrome/")?.split_once(' ')?;
    let numeric = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit() || byte == b'.');
    if !numeric(number) || !numeric(chrome) || tail != "Safari/537.36" {
        return None;
    }
    Some(start..start + TOKEN.len() + number.len() + 1)
}

/// How a sign-in ended, and the exit status that says it.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// The return address was reached; it goes on stdout as one line.
    Reached(String),
    /// The person closed the window first.
    Closed,
    /// The arguments were not understood.
    Usage,
    /// There is no web engine: this build has none, or its process is not
    /// installed.
    Unavailable,
}

impl Outcome {
    pub fn code(&self) -> u8 {
        match self {
            Outcome::Reached(_) => 0,
            Outcome::Closed => 1,
            Outcome::Usage => 2,
            Outcome::Unavailable => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn a_request_needs_two_secure_addresses() {
        let request = Request::parse(arguments(&["--url", "https://login.example/a?x=1", "--until", "https://example/return", "--title", "Source",
            "--data-dir", "/d", "--cache-dir", "/c"])).unwrap();
        assert_eq!(request.url, "https://login.example/a?x=1");
        assert_eq!(request.title, "Source");
        assert_eq!((request.data_dir.as_deref(), request.cache_dir.as_deref()), (Some("/d"), Some("/c")));
        assert!(Request::parse(arguments(&["--url", "http://login.example/a", "--until", "https://example/return"])).is_none());
        assert!(Request::parse(arguments(&["--url", "https://login.example/a"])).is_none());
        assert!(Request::parse(arguments(&["--url", "https://login.example/a", "--until", "https://example/r", "--other", "x"])).is_none());
        assert!(Request::parse(arguments(&["--url", "https://login.example/a", "--until"])).is_none(), "a name without its value");
        assert!(Request::parse(arguments(&["--url", "https://", "--until", "https://example/r"])).is_none(), "a scheme with nothing after it");
        let long = format!("https://login.example/{}", "a".repeat(8192));
        assert!(Request::parse(arguments(&["--url", &long, "--until", "https://example/r"])).is_none(), "an address over 8192 bytes");
    }

    #[test]
    fn only_a_short_single_line_user_agent_is_kept() {
        let with = |agent: &str| Request::parse(arguments(&["--url", "https://login.example/a", "--until", "https://example/r",
            "--user-agent", agent])).unwrap().user_agent;
        assert_eq!(with("Agent/1").as_deref(), Some("Agent/1"));
        assert_eq!(with(""), None);
        assert_eq!(with("Agent/1\r\nCookie: x"), None);
        assert_eq!(with(&"a".repeat(513)), None);
        assert_eq!(with(&"a".repeat(512)).map(|agent| agent.len()), Some(512));
    }

    #[test]
    fn only_the_return_address_itself_is_reached() {
        let until = "https://example/return";
        assert!(reaches(until, "https://example/return?code=1&state=2"));
        assert!(reaches(until, "https://example/return"));
        assert!(reaches(until, "https://example/return#code=1"));
        assert!(!reaches(until, "https://example/returned?code=1"));
        assert!(!reaches(until, "https://example/return/next"));
        assert!(!reaches(until, "https://login.example/a?redirect=https://example/return"));
    }

    #[test]
    fn a_page_or_a_frame_ends_the_sign_in_and_a_resource_does_not() {
        let (until, address) = ("https://example/return", "https://example/return?code=1");
        for seen in [Seen::MainFrame, Seen::SubFrame, Seen::NewWindow, Seen::Loaded] {
            assert!(ends(until, seen, address), "{seen:?}");
            assert!(!ends(until, seen, "https://login.example/next"), "{seen:?}");
        }
        assert!(!ends(until, Seen::Resource, address));
        assert_eq!((0..=4).map(Seen::from_code).collect::<Vec<_>>(),
            [Seen::MainFrame, Seen::SubFrame, Seen::NewWindow, Seen::Loaded, Seen::Resource].map(Some).to_vec());
        assert_eq!(Seen::from_code(5), None);
    }

    #[test]
    fn the_engine_token_leaves_the_chromium_user_agent() {
        let engine = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) QtWebEngine/6.11.2 Chrome/140.0.0.0 Safari/537.36";
        let range = engine_token(engine).unwrap();
        let mut left = engine.to_string();
        left.replace_range(range, "");
        assert_eq!(left, "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36");
        let chromium = left.as_str();
        assert_eq!(engine_token(chromium), None, "nothing to cut");
        // On macOS Chromium names an Intel Mac whatever the processor; the
        // result is what the window printed on an arm64 Mac with Qt 6.11.1.
        let mac = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) QtWebEngine/6.11.1 Chrome/140.0.0.0 Safari/537.36";
        let mut left_on_mac = mac.to_string();
        left_on_mac.replace_range(engine_token(mac).unwrap(), "");
        assert_eq!(left_on_mac, "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36");
        assert_eq!(engine_token("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) QtWebEngine/6.11.2 Safari/537.36"), None,
            "no Chromium version");
        assert_eq!(engine_token("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) QtWebEngine/6.11.2 Chrome/140.0.0.0 \
            Safari/537.36 Other/1"), None, "a product after Safari");
        assert_eq!(engine_token("Agent/1"), None);
    }

    #[test]
    fn each_outcome_has_its_exit_status() {
        let codes = [Outcome::Reached("https://example/return".into()), Outcome::Closed, Outcome::Usage, Outcome::Unavailable].map(|outcome| outcome.code());
        assert_eq!(codes, [0, 1, 2, 3]);
    }
}
