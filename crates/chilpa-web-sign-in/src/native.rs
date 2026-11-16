//! What the C++ window (`cxx/window.cpp`) is handed: the request as UTF-8
//! slices and the rules as functions it calls back, so every decision stays
//! in Rust and is tested here.

use crate::{Outcome, Request, Seen};
use std::ffi::c_void;
use std::marker::PhantomData;
use std::sync::OnceLock;

/// A borrowed UTF-8 string; empty means absent.
#[repr(C)]
pub struct Text {
    data: *const u8,
    size: usize,
}

impl Text {
    fn of(value: &str) -> Text {
        Text { data: value.as_ptr(), size: value.len() }
    }
}

/// The return address, and the first address that reached it.
pub struct Session {
    until: String,
    reached: OnceLock<String>,
}

impl Session {
    pub fn new(until: &str) -> Session {
        Session { until: until.to_string(), reached: OnceLock::new() }
    }
}

/// The layout of `ChilpaSignInWindow` in `cxx/window.cpp`. It borrows the
/// request and the session for as long as the window is open.
#[repr(C)]
pub struct Window<'a> {
    url: Text,
    title: Text,
    user_agent: Text,
    data_dir: Text,
    cache_dir: Text,
    /// The `Session`, opaque to the window.
    session: *const c_void,
    ends: extern "C" fn(*const c_void, u8, *const u8, usize) -> bool,
    engine_token: extern "C" fn(*const u8, usize, *mut usize, *mut usize) -> bool,
    reached: extern "C" fn(*const c_void, *const u8, usize),
    borrows: PhantomData<&'a ()>,
}

impl<'a> Window<'a> {
    pub fn new(request: &'a Request, session: &'a Session) -> Window<'a> {
        let optional = |value: &'a Option<String>| Text::of(value.as_deref().unwrap_or(""));
        // A store needs both folders; with one alone the window keeps nothing.
        let stores = request.data_dir.is_some() && request.cache_dir.is_some();
        let (data_dir, cache_dir) = if stores { (optional(&request.data_dir), optional(&request.cache_dir)) } else { (Text::of(""), Text::of("")) };
        Window {
            url: Text::of(&request.url),
            title: Text::of(&request.title),
            user_agent: optional(&request.user_agent),
            data_dir,
            cache_dir,
            session: handle(session),
            ends,
            engine_token,
            reached,
            borrows: PhantomData,
        }
    }
}

fn handle(session: &Session) -> *const c_void {
    (session as *const Session).cast()
}

/// # Safety
/// `handle` is null or comes from `handle` for a live session.
unsafe fn session_of<'a>(handle: *const c_void) -> Option<&'a Session> {
    // SAFETY: the window passes back the handle it was given, while the
    // session lives (`open`).
    unsafe { handle.cast::<Session>().as_ref() }
}

/// # Safety
/// `data` points to `size` readable bytes, or is null.
unsafe fn text<'a>(data: *const u8, size: usize) -> Option<&'a str> {
    if data.is_null() {
        return (size == 0).then_some("");
    }
    // SAFETY: the caller hands a live buffer of `size` bytes.
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(data, size) }).ok()
}

/// Whether the window should stop at this address (`crate::ends`).
extern "C" fn ends(handle: *const c_void, seen: u8, data: *const u8, size: usize) -> bool {
    // SAFETY: the window passes back its session handle and an address it
    // owns for the length of the call.
    let (Some(session), Some(address)) = (unsafe { session_of(handle) }, unsafe { text(data, size) }) else { return false };
    Seen::from_code(seen).is_some_and(|seen| crate::ends(&session.until, seen, address))
}

/// Where the engine's token is in its user agent (`crate::engine_token`).
extern "C" fn engine_token(data: *const u8, size: usize, start: *mut usize, length: *mut usize) -> bool {
    // SAFETY: as in `ends`; `start` and `length` point to writable values.
    let Some(agent) = (unsafe { text(data, size) }) else { return false };
    let Some(range) = crate::engine_token(agent) else { return false };
    if start.is_null() || length.is_null() {
        return false;
    }
    unsafe {
        *start = range.start;
        *length = range.len();
    }
    true
}

/// Keeps the first address that reached the return address.
extern "C" fn reached(handle: *const c_void, data: *const u8, size: usize) {
    // SAFETY: as in `ends`.
    if let (Some(session), Some(address)) = (unsafe { session_of(handle) }, unsafe { text(data, size) }) {
        let _ = session.reached.set(address.to_string());
    }
}

/// The outcome a status of the window stands for.
pub fn outcome(status: u8, session: &Session) -> Outcome {
    match status {
        0 => session.reached.get().cloned().map_or(Outcome::Closed, Outcome::Reached),
        2 => Outcome::Usage,
        3 => Outcome::Unavailable,
        _ => Outcome::Closed,
    }
}

#[cfg(feature = "webengine")]
extern "C" {
    fn chilpa_sign_in_window(window: *const Window<'_>) -> u8;
}

/// Opens the window and waits until the return address is reached or the
/// window is closed.
#[cfg(feature = "webengine")]
pub fn open(request: &Request) -> Outcome {
    let session = Session::new(&request.until);
    let window = Window::new(request, &session);
    // SAFETY: the window reads `window` and calls back only while this call
    // lasts, and both it and the session outlive the call.
    let status = unsafe { chilpa_sign_in_window(&window) };
    outcome(status, &session)
}

/// This build carries no web engine.
#[cfg(not(feature = "webengine"))]
pub fn open(_request: &Request) -> Outcome {
    Outcome::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(values: &[&str]) -> Request {
        Request::parse(values.iter().map(|value| value.to_string())).unwrap()
    }

    #[test]
    fn the_window_calls_the_same_rules() {
        let owned = Session::new("https://example/return");
        let session = handle(&owned);
        let address = "https://example/return?code=7";
        assert!(ends(session, 0, address.as_ptr(), address.len()));
        assert!(ends(session, 3, address.as_ptr(), address.len()));
        assert!(!ends(session, 4, address.as_ptr(), address.len()), "a resource");
        assert!(!ends(session, 9, address.as_ptr(), address.len()), "an unknown code");
        assert!(!ends(std::ptr::null(), 0, address.as_ptr(), address.len()));
        let invalid = [0xff_u8, 0xfe];
        assert!(!ends(session, 0, invalid.as_ptr(), invalid.len()), "not UTF-8");
        assert!(!ends(session, 0, std::ptr::null(), 0));
    }

    #[test]
    fn the_first_reached_address_is_the_outcome() {
        let session = Session::new("https://example/return");
        assert_eq!(outcome(0, &session), Outcome::Closed, "a status of reached without an address");
        let (first, second) = ("https://example/return?code=1", "https://example/return?code=2");
        reached(handle(&session), first.as_ptr(), first.len());
        reached(handle(&session), second.as_ptr(), second.len());
        assert_eq!(outcome(0, &session), Outcome::Reached(first.into()));
        assert_eq!(outcome(1, &session), Outcome::Closed);
        assert_eq!(outcome(2, &session), Outcome::Usage);
        assert_eq!(outcome(3, &session), Outcome::Unavailable);
    }

    #[test]
    fn the_engine_token_comes_back_as_a_range() {
        let agent = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) QtWebEngine/6.11.2 Chrome/140.0.0.0 Safari/537.36";
        let (mut start, mut length) = (0, 0);
        assert!(engine_token(agent.as_ptr(), agent.len(), &mut start, &mut length));
        assert_eq!(&agent[start..start + length], "QtWebEngine/6.11.2 ");
        assert!(!engine_token(b"Agent/1".as_ptr(), 7, &mut start, &mut length));
        assert!(!engine_token(agent.as_ptr(), agent.len(), std::ptr::null_mut(), &mut length));
    }

    #[test]
    fn a_store_needs_both_folders() {
        let read = |text: &Text| unsafe { super::text(text.data, text.size) }.unwrap().to_string();
        let both = request(&["--url", "https://login.example/a", "--until", "https://example/r", "--data-dir", "/d", "--cache-dir", "/c"]);
        let session = Session::new(&both.until);
        let window = Window::new(&both, &session);
        assert_eq!((read(&window.url), read(&window.data_dir), read(&window.cache_dir)), ("https://login.example/a".into(), "/d".into(), "/c".into()));
        let one = request(&["--url", "https://login.example/a", "--until", "https://example/r", "--data-dir", "/d"]);
        let window = Window::new(&one, &session);
        assert_eq!((read(&window.data_dir), read(&window.cache_dir), read(&window.user_agent)), (String::new(), String::new(), String::new()));
    }
}
