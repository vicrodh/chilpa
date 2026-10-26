//! The request that is handed in, and how often it may be made.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Who to write to about what Chilpa asks for. MusicBrainz requires one.
///
/// The value is mine to give and nobody else's: a URL or an address of the
/// project. Anything that is not a test fails to build while it is empty,
/// on purpose.
const CONTACT: &str = "https://chilpa.app";

#[cfg(not(test))]
const _: () = assert!(!CONTACT.is_empty(), "chilpa-meta: CONTACT has to be set before this ships");

/// `Chilpa/<version> ( <contact> )`.
pub fn user_agent(version: &str) -> String {
    format!("Chilpa/{version} ( {CONTACT} )")
}

pub struct Reply {
    pub status: u16,
    /// Names in lower case.
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// The network, as whoever has one hands it in.
pub trait Http: Send + Sync {
    /// `Err` is nothing came back. Any status is an `Ok`.
    fn get(&self, url: &str, accept: &str, timeout: Duration, max_bytes: usize) -> Result<Reply, String>;
    /// I allow bodyless OAuth exchanges only in clients with a scoped transport.
    fn post_as(&self, _: &str, _: &str, _: Duration, _: usize, _: &str) -> Result<Reply, String> {
        Err("this client sends no OAuth exchanges".into())
    }
    /// I refuse rather than silently drop credentials in an older client.
    fn get_as(&self, _: &str, _: &str, _: Duration, _: usize, _: &str) -> Result<Reply, String> {
        Err("this client sends no credentials".into())
    }
}

/// One request start at a time, no closer together than `every`.
pub struct Pace {
    every: Duration,
    last: Mutex<Option<Instant>>,
}

impl Pace {
    pub const fn new(every: Duration) -> Self {
        Self { every, last: Mutex::new(None) }
    }

    /// Sleeps the calling thread until it may go. Whoever waits holds the
    /// line: two callers go one after the other, never together.
    pub fn wait(&self) {
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(at) = *last {
            let since = at.elapsed();
            if since < self.every {
                std::thread::sleep(self.every - since);
            }
        }
        *last = Some(Instant::now());
    }
}

/// MusicBrainz allows one request a second from an address. One pace for
/// the whole process, so everything that asks shares it.
static MUSICBRAINZ: Pace = Pace::new(Duration::from_millis(1_100));

/// The proxy allows thirty requests a minute from an address, whatever
/// they are for.
static PROXY: Pace = Pace::new(Duration::from_millis(2_100));
static DISCOGS_ANONYMOUS: Pace = Pace::new(Duration::from_millis(2_500));
static DISCOGS_AUTHENTICATED: Pace = Pace::new(Duration::from_millis(1_100));

/// For what has no limit of its own to keep.
static FREE: Pace = Pace::new(Duration::ZERO);

/// What a request goes through.
pub struct Net<'a> {
    pub http: &'a dyn Http,
    pub pace: &'a Pace,
    /// How a wait between two tries is spent. A test does not wait.
    pub nap: fn(Duration),
}

impl<'a> Net<'a> {
    pub fn musicbrainz(http: &'a dyn Http) -> Self {
        Self { http, pace: &MUSICBRAINZ, nap: std::thread::sleep }
    }

    pub fn proxy(http: &'a dyn Http) -> Self {
        Self { http, pace: &PROXY, nap: std::thread::sleep }
    }
    pub fn discogs_direct(http: &'a dyn Http, authenticated: bool) -> Self {
        Self { http, pace: if authenticated { &DISCOGS_AUTHENTICATED } else { &DISCOGS_ANONYMOUS }, nap: std::thread::sleep }
    }

    /// Unpaced: the Cover Art Archive, an image's own host, and tests.
    pub fn free(http: &'a dyn Http) -> Self {
        Self { http, pace: &FREE, nap: |_| {} }
    }
}

#[cfg(test)]
mod pacing_tests {
    use super::*;

    #[test]
    fn musicbrainz_callers_share_the_same_1100_millisecond_pace() {
        let http = super::testing::Canned::new();
        let first = Net::musicbrainz(&http);
        let second = Net::musicbrainz(&http);
        assert!(std::ptr::eq(first.pace, second.pace));
        assert_eq!(first.pace.every, Duration::from_millis(1_100));
        let pace = Pace::new(Duration::from_millis(1_100));
        pace.wait();
        let start = pace.last.lock().unwrap().unwrap();
        pace.wait();
        assert!(start.elapsed() >= Duration::from_millis(1_100));
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Failure {
    /// The network. Try again.
    Unreachable(String),
    /// Told to wait, in seconds.
    Busy(u64),
    /// Anonymous direct access and its optional backup have no quota left.
    Quota(u64),
    /// An authenticated request was refused. I never retry it anonymously.
    Credential,
    /// The proxy does not serve this to Chilpa.
    Refused(String),
    /// Answered, and not with what was expected.
    Unexpected(String),
    /// What was asked for is not a valid id.
    BadId,
}

/// How long the other side asks to be left alone, in seconds.
pub(crate) fn retry_after(reply: &Reply) -> Option<u64> {
    reply.headers.get("retry-after").and_then(|v| v.trim().parse::<u64>().ok()).filter(|s| *s > 0)
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::collections::VecDeque;

    /// Answers what it was loaded with, in order, and writes down what it
    /// was asked.
    #[derive(Default)]
    pub(crate) struct Canned {
        replies: Mutex<VecDeque<Result<Reply, String>>>,
        asked: Mutex<Vec<String>>,
        authorization: Mutex<Vec<Option<String>>>,
    }

    impl Canned {
        pub(crate) fn new() -> Self {
            Self::default()
        }

        pub(crate) fn json(self, body: &str) -> Self {
            self.status(200, &[], body.as_bytes())
        }

        pub(crate) fn status(self, status: u16, headers: &[(&str, &str)], body: &[u8]) -> Self {
            let headers = headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            self.replies.lock().unwrap().push_back(Ok(Reply { status, headers, body: body.to_vec() }));
            self
        }

        pub(crate) fn silence(self) -> Self {
            self.replies.lock().unwrap().push_back(Err("nothing came back".to_string()));
            self
        }

        pub(crate) fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }
        pub(crate) fn authorization(&self) -> Vec<Option<String>> { self.authorization.lock().unwrap().clone() }
    }

    impl Http for Canned {
        fn get(&self, url: &str, _: &str, _: Duration, _: usize) -> Result<Reply, String> {
            self.asked.lock().unwrap().push(url.to_string());
            self.authorization.lock().unwrap().push(None);
            self.replies.lock().unwrap().pop_front().unwrap_or_else(|| Err("nothing was loaded".to_string()))
        }
        fn get_as(&self, url: &str, _: &str, _: Duration, _: usize, authorization: &str) -> Result<Reply, String> {
            self.asked.lock().unwrap().push(url.into());
            self.authorization.lock().unwrap().push(Some(authorization.into()));
            self.replies.lock().unwrap().pop_front().unwrap_or_else(|| Err("nothing was loaded".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn get_as_default_refuses_instead_of_sending_anonymous_requests() {
        struct Old;
        impl Http for Old {
            fn get(&self,_:&str,_:&str,_:Duration,_:usize)->Result<Reply,String> {panic!("credentials must not be dropped")}
        }
        assert!(Old.get_as("https://api.discogs.com/oauth/identity","application/json",Duration::ZERO,1,"Discogs token=marker").is_err());
    }
    #[test]
    fn discogs_modes_have_distinct_process_wide_paces() {
        let http=testing::Canned::new();
        assert_eq!(Net::discogs_direct(&http,false).pace.every,Duration::from_millis(2500));
        assert_eq!(Net::discogs_direct(&http,true).pace.every,Duration::from_millis(1100));
        assert!(std::ptr::eq(Net::discogs_direct(&http,true).pace,Net::discogs_direct(&http,true).pace));
    }

    #[test]
    fn two_callers_share_one_pace() {
        let pace = Pace::new(Duration::from_millis(120));
        let start = Instant::now();
        pace.wait();
        assert!(start.elapsed() < Duration::from_millis(60), "the first one does not wait");
        pace.wait();
        pace.wait();
        assert!(start.elapsed() >= Duration::from_millis(240), "the second and the third do");
    }

    #[test]
    fn the_user_agent_names_chilpa_first() {
        let agent = user_agent("1.2.3");
        assert!(agent.starts_with("Chilpa/1.2.3 ( "), "{agent}");
        assert!(agent.ends_with(" )"));
        assert!(agent.to_lowercase().starts_with("chilpa"), "which is how the proxy tells who is asking");
    }
}
