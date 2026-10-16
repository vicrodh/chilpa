//! The request that is handed in.

use std::collections::BTreeMap;
use std::time::Duration;

/// One request, whole: nothing is added on the way out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// `GET` or `POST`.
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// The body and its type, for a `POST`.
    pub body: Option<(String, Vec<u8>)>,
    pub timeout: Duration,
}

impl Request {
    pub fn get(url: String, headers: Vec<(String, String)>) -> Self {
        Self { method: "GET", url, headers, body: None, timeout: Duration::from_secs(20) }
    }

    pub fn post(url: String, headers: Vec<(String, String)>, body: Option<(String, Vec<u8>)>) -> Self {
        Self { method: "POST", url, headers, body, timeout: Duration::from_secs(20) }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Reply {
    pub status: u16,
    /// Names in lower case.
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// The network, as whoever has one hands it in. `Err` is that nothing came
/// back, in words for the window; any status is an `Ok`.
pub trait Net: Send + Sync {
    fn send(&self, request: &Request) -> Result<Reply, String>;
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// Answers what it was loaded with, in order, and writes down what it
    /// was asked.
    #[derive(Default)]
    pub(crate) struct Canned {
        replies: Mutex<VecDeque<Result<Reply, String>>>,
        asked: Mutex<Vec<Request>>,
    }

    impl Canned {
        pub(crate) fn new() -> Self {
            Self::default()
        }

        pub(crate) fn answer(self, status: u16, body: &[u8]) -> Self {
            self.replies.lock().unwrap().push_back(Ok(Reply { status, headers: Default::default(), body: body.to_vec() }));
            self
        }

        pub(crate) fn file(self, path: &str) -> Self {
            let full = format!("{}/tests/answers/{path}", env!("CARGO_MANIFEST_DIR"));
            let body = std::fs::read(&full).unwrap_or_else(|e| panic!("{full}: {e}"));
            self.answer(200, &body)
        }

        pub(crate) fn silence(self) -> Self {
            self.replies.lock().unwrap().push_back(Err("the server could not be reached".into()));
            self
        }

        pub(crate) fn asked(&self) -> Vec<Request> {
            self.asked.lock().unwrap().clone()
        }
    }

    impl Net for Canned {
        fn send(&self, request: &Request) -> Result<Reply, String> {
            self.asked.lock().unwrap().push(request.clone());
            self.replies.lock().unwrap().pop_front().unwrap_or_else(|| Err("nothing was loaded".into()))
        }
    }
}
