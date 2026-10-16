//! How messages pass between Chilpa and a plugin.
//!
//! Wasm only knows integers, so anything that is not an integer travels
//! through the guest's memory. The deal is short on purpose (ADR-007):
//!
//! 1. Chilpa asks the plugin for a piece of memory with `chilpa_alloc(len)`.
//! 2. It writes the message there, as JSON.
//! 3. It calls the operation's function with `(ptr, len)`.
//! 4. The plugin answers with an `i64` packing the address and length of its
//!    reply, and Chilpa reads it.
//!
//! **Errors are data, not a separate channel.** The reply is always JSON; if
//! something went wrong, it carries `error`. That way there is one way to
//! answer and not two that can contradict each other.

/// The name of the memory a plugin has to export.
pub const MEMORY: &str = "memory";

/// The functions every plugin exports, no matter what.
pub const F_VERSION: &str = "chilpa_api_version";
pub const F_ALLOC: &str = "chilpa_alloc";

/// The module the host functions are imported under.
pub const HOST_MODULE: &str = "chilpa";

/// **Everything** a plugin can ask of the world.
///
/// This list is the sandbox's whole surface: what is not here does not exist
/// for the guest, because there are no syscalls in WASM. That is why it is
/// written in one place and there is a test that fails if the linker offers
/// anything not listed.
///
/// **An import that returns a secret is never added** (ADR-008). No
/// `credential()`. The plugin says where it wants to go with `http` and
/// Chilpa adds the permission; if it could ask for the token it would hold
/// it, and it would ship it out on its next request to its own domain.
pub const HOST_IMPORTS: &[&str] = &["log", "now", "store_get", "store_set", "http"];

/// Packs an address and a length into the `i64` the guest answers with.
pub fn pack(ptr: u32, len: u32) -> i64 {
    ((u64::from(ptr) << 32) | u64::from(len)) as i64
}

/// The opposite. A zero means "I answered nothing".
pub fn unpack(v: i64) -> Option<(u32, u32)> {
    if v == 0 {
        return None;
    }
    let v = v as u64;
    Some(((v >> 32) as u32, (v & 0xffff_ffff) as u32))
}

/// What a plugin answered, already read.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Reply {
    /// It answered, and this is its JSON.
    Ok(Vec<u8>),
    /// It answered with an error of its own, with its text.
    Error(String),
}

/// Reads a plugin's reply, which is always JSON.
///
/// JSON with `error` is an error from the plugin, not a failure of the
/// plugin: they are different things and they are counted differently.
pub fn read_reply(bytes: Vec<u8>) -> Reply {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Reply::Error(chilpa_i18n::t("it answered something that is not JSON"));
    };
    if let Some(e) = v.get("error") {
        let text = e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string());
        return Reply::Error(text);
    }
    Reply::Ok(bytes)
}

/// What a host function answers when it returns data.
///
/// The guest gives a buffer and its size; if it does not fit, nothing is
/// written and the needed size is answered. That way there is never a half
/// write.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Room {
    /// It fit, and this many bytes were written.
    Fits(u32),
    /// It did not fit; this many were needed.
    TooBig(u32),
    /// There was nothing to give.
    Nothing,
}

impl Room {
    /// What the guest is answered: positive fits, negative does not fit,
    /// minus one means there is nothing.
    pub fn code(self) -> i32 {
        match self {
            Room::Fits(n) => n as i32,
            Room::TooBig(n) => -(n as i32),
            Room::Nothing => -1,
        }
    }

    /// Decides whether some data fits in the buffer the guest gave.
    pub fn holding(data: Option<&[u8]>, room: u32) -> Self {
        match data {
            None => Room::Nothing,
            Some(d) if d.len() as u32 <= room => Room::Fits(d.len() as u32),
            Some(d) => Room::TooBig(d.len() as u32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_and_unpacking_are_inverses() {
        for (ptr, len) in [(0u32, 5u32), (1024, 0), (0xdead_beef, 0x1234)] {
            let v = pack(ptr, len);
            assert_eq!(unpack(v), Some((ptr, len)), "with ({ptr}, {len})");
        }
    }

    #[test]
    fn a_zero_means_it_did_not_answer() {
        assert_eq!(unpack(0), None);
        // And a zero address with zero length is the same as nothing.
        assert_eq!(pack(0, 0), 0);
    }

    #[test]
    fn a_reply_carrying_an_error_reads_as_an_error() {
        let r = read_reply(r#"{"error":"I did not find that track"}"#.as_bytes().to_vec());
        assert_eq!(r, Reply::Error("I did not find that track".into()));
    }

    #[test]
    fn a_good_reply_is_handed_over_unchanged() {
        let json = br#"{"tracks":[]}"#.to_vec();
        assert_eq!(read_reply(json.clone()), Reply::Ok(json));
    }

    #[test]
    fn anything_that_is_not_json_is_the_plugin_fault() {
        let r = read_reply(b"junk".to_vec());
        assert!(matches!(r, Reply::Error(e) if e.contains("not JSON")));
    }

    #[test]
    fn the_room_check_says_how_much_was_needed() {
        assert_eq!(Room::holding(Some(b"four"), 10), Room::Fits(4));
        assert_eq!(Room::holding(Some(b"four"), 4), Room::Fits(4), "it fits exactly");
        assert_eq!(Room::holding(Some(b"four"), 2), Room::TooBig(4));
        assert_eq!(Room::holding(None, 10), Room::Nothing);
    }

    #[test]
    fn the_room_codes_are_not_mistaken_for_one_another() {
        assert_eq!(Room::Fits(4).code(), 4);
        assert_eq!(Room::TooBig(4).code(), -4);
        assert_eq!(Room::Nothing.code(), -1);
        // A zero size that did fit is zero, and is not confused with "nothing".
        assert_eq!(Room::Fits(0).code(), 0);
        assert_ne!(Room::Fits(0).code(), Room::Nothing.code());
    }
}
