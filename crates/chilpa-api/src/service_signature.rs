//! Signatures for Chilpa's own services, api.chilpa.app and lyrics.chilpa.app.
//!
//! The services answer the builds that carry a key. A build compiled with
//! `CHILPA_SERVICE_KEY` (64 hex characters) signs every request it makes to
//! them; one compiled without it signs nothing, and the services may refuse
//! it. The key is not hidden from whoever holds the binary: it keeps a copy
//! of the source from using the services by default, and nothing more.
//!
//! If requests to the services fail with 403 in a fork or a build of your
//! own, this is why: deploy your own services and point the build to them,
//! as https://github.com/vicrodh/chilpa-api#forks asks.
//!
//! A signature is HMAC-SHA256 over the method, the host, the path without
//! its query, and the time, sent as `Authorization: Chilpa-Signature
//! t=<time>, v1=<hex>`. In that header it does not follow a redirect, which
//! drops it, and a request that already carries its own authorization goes
//! out with that one instead. The services answer their own time in
//! `X-Chilpa-Time`, and the next signature follows that clock, so a machine
//! whose clock is off loses at most its first request.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use sha2::Sha256;

/// The hosts that are signed for. Nothing else gets the headers.
const HOSTS: [&str; 2] = ["api.chilpa.app", "lyrics.chilpa.app"];

/// The services' clock, in seconds since 1970, on every reply.
const TIME: &str = "x-chilpa-time";

/// The times a service's clock is believed in: 2017 to 2100. Anything else
/// is a broken header, and the clock already followed stays.
const PLAUSIBLE: std::ops::RangeInclusive<i64> = 1_500_000_000..=4_102_444_800;

/// XORed into the key when this crate is compiled, so the key's text is not
/// in the binary for `strings` to find.
const MASK: [u8; 32] = [
    0xce, 0xec, 0x40, 0x3f, 0x66, 0x3c, 0x69, 0x51, 0xab, 0x95, 0x58, 0x23, 0x72, 0x7b, 0x32, 0x1b,
    0x43, 0x25, 0xd3, 0xdc, 0x95, 0x36, 0xb3, 0x42, 0x44, 0x57, 0xa6, 0x22, 0x87, 0x94, 0x4f, 0xf8,
];

/// The build's key, masked. Evaluated by the compiler: a key of the wrong
/// shape fails the build, and an empty or absent one leaves it unsigned.
const KEY: Option<[u8; 32]> = masked(option_env!("CHILPA_SERVICE_KEY"));

/// 64 hex characters, surrounding whitespace allowed, to 32 masked bytes.
const fn masked(key: Option<&str>) -> Option<[u8; 32]> {
    let Some(key) = key else { return None };
    let text = key.as_bytes();
    let (mut start, mut end) = (0, text.len());
    while start < end && text[start].is_ascii_whitespace() { start += 1; }
    while end > start && text[end - 1].is_ascii_whitespace() { end -= 1; }
    if start == end { return None; }
    assert!(end - start == 64, "CHILPA_SERVICE_KEY must be 64 hex characters");
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = (nibble(text[start + 2 * i]) << 4 | nibble(text[start + 2 * i + 1])) ^ MASK[i];
        i += 1;
    }
    Some(out)
}

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("CHILPA_SERVICE_KEY must be 64 hex characters"),
    }
}

fn unmask(mut key: [u8; 32]) -> [u8; 32] {
    for (byte, mask) in key.iter_mut().zip(MASK) {
        *byte ^= mask;
    }
    key
}

/// Installs the signer when this build carries a key; says once which it is.
pub(crate) fn install() {
    static SAID: std::sync::Once = std::sync::Once::new();
    if let Some(key) = KEY {
        chilpa_core::network::sign_with(Box::new(Services::new(unmask(key))));
    }
    // Said on purpose in a build without a key too: it is where a fork's
    // developer looking into a 403 from the services finds the reason.
    SAID.call_once(|| eprintln!("services: {}", if KEY.is_some() { "this build signs its requests" }
        else { "this build carries no key, so its requests go unsigned; see https://github.com/vicrodh/chilpa-api#forks" }));
}

struct Services {
    key: [u8; 32],
    /// The services' clock minus this machine's, in seconds.
    offset: AtomicI64,
}

impl Services {
    fn new(key: [u8; 32]) -> Self {
        Self { key, offset: AtomicI64::new(0) }
    }
}

impl chilpa_core::network::Signer for Services {
    fn sign(&self, method: &str, host: &str, path: &str) -> Vec<(&'static str, String)> {
        if !HOSTS.contains(&host) {
            return Vec::new();
        }
        let time = now().saturating_add(self.offset.load(Ordering::Relaxed));
        vec![("authorization", format!("Chilpa-Signature t={time}, v1={}", signature(&self.key, method, host, path, time)))]
    }

    fn answered(&self, host: &str, headers: &ureq::http::HeaderMap) {
        if !HOSTS.contains(&host) {
            return;
        }
        let theirs = headers.get(TIME).and_then(|value| value.to_str().ok()).and_then(|value| value.trim().parse::<i64>().ok());
        if let Some(theirs) = theirs.filter(|theirs| PLAUSIBLE.contains(theirs)) {
            self.offset.store(theirs.saturating_sub(now()), Ordering::Relaxed);
        }
    }
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|since| since.as_secs() as i64).unwrap_or(0)
}

fn signature(key: &[u8; 32], method: &str, host: &str, path: &str, time: i64) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes a key of any length");
    mac.update(format!("chilpa-service-v1\n{method}\n{host}\n{path}\n{time}").as_bytes());
    mac.finalize().into_bytes().iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_core::network::Signer;

    const COUNTING: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    fn counting() -> [u8; 32] {
        std::array::from_fn(|i| i as u8)
    }

    #[test]
    fn the_key_is_masked_when_compiled_and_recovered_whole() {
        let masked = masked(Some(COUNTING)).expect("a key of the right shape");
        assert_ne!(masked, counting(), "the bytes the binary carries are not the key");
        assert_eq!(unmask(masked), counting());
        assert_eq!(super::masked(Some(&format!("  {}\n", COUNTING.to_uppercase()))).map(unmask), Some(counting()), "case and surrounding whitespace do not matter");
    }

    #[test]
    fn an_absent_or_empty_key_leaves_the_build_unsigned() {
        assert_eq!(masked(None), None);
        assert_eq!(masked(Some("")), None);
        assert_eq!(masked(Some(" \n")), None);
    }

    #[test]
    #[should_panic(expected = "64 hex characters")]
    fn a_short_key_is_refused() {
        masked(Some("00ff"));
    }

    #[test]
    #[should_panic(expected = "64 hex characters")]
    fn a_key_that_is_not_hex_is_refused() {
        masked(Some(&format!("{}zz", &COUNTING[..62])));
    }

    /// The same vectors are in the services' tests.
    #[test]
    fn signatures_match_the_services_vectors() {
        assert_eq!(signature(&counting(), "GET", "api.chilpa.app", "/lastfm", 1_700_000_000),
            "dee612c81e2d81b791f21b2b0377ad09ac0b6b8137fae824c8d8de3daf379556");
        assert_eq!(signature(&counting(), "POST", "lyrics.chilpa.app", "/v1/lyrics/resolve", 1_700_000_000),
            "1463868fb51b036297d83a04bcceb054bb5ed579294c3ec8cd4ccd3b120d57a1");
    }

    /// The time and the signature out of `Chilpa-Signature t=<time>, v1=<hex>`.
    fn parts(headers: &[(&'static str, String)]) -> (i64, String) {
        assert_eq!(headers.len(), 1, "{headers:?}");
        assert_eq!(headers[0].0, "authorization");
        let rest = headers[0].1.strip_prefix("Chilpa-Signature t=").expect("the scheme comes first");
        let (time, signature) = rest.split_once(", v1=").expect("then the signature");
        (time.parse().unwrap(), signature.to_string())
    }

    #[test]
    fn only_the_two_services_are_signed() {
        let services = Services::new(counting());
        for host in ["api.chilpa.app", "lyrics.chilpa.app"] {
            let (time, signature) = parts(&services.sign("GET", host, "/x"));
            assert_eq!(signature, super::signature(&counting(), "GET", host, "/x", time), "{host}");
            assert_eq!(signature.len(), 64);
        }
        for host in ["api.chilpa.app.example", "chilpa.app", "ws.audioscrobbler.com", "127.0.0.1"] {
            assert!(services.sign("GET", host, "/x").is_empty(), "{host}");
        }
    }

    #[test]
    fn the_next_signature_follows_the_services_clock() {
        let services = Services::new(counting());
        let ahead = now() + 3_600;
        let mut reply = ureq::http::HeaderMap::new();
        reply.insert(TIME, ahead.to_string().parse().unwrap());
        services.answered("ws.audioscrobbler.com", &reply);
        assert_eq!(services.offset.load(Ordering::Relaxed), 0, "another host's clock is not followed");
        services.answered("lyrics.chilpa.app", &reply);
        let (time, signature) = parts(&services.sign("POST", "lyrics.chilpa.app", "/v1/lyrics/resolve"));
        assert!((time - ahead).abs() <= 2, "signed at {time}, the service said {ahead}");
        assert_eq!(signature, super::signature(&counting(), "POST", "lyrics.chilpa.app", "/v1/lyrics/resolve", time));
    }

    #[test]
    fn a_broken_clock_header_leaves_the_clock_as_it_was() {
        let services = Services::new(counting());
        let mut reply = ureq::http::HeaderMap::new();
        reply.insert(TIME, (now() + 600).to_string().parse().unwrap());
        services.answered("api.chilpa.app", &reply);
        let followed = services.offset.load(Ordering::Relaxed);
        for broken in [i64::MIN.to_string(), i64::MAX.to_string(), "-1".into(), "0".into(), "99999999999".into(), "soon".into(), "".into()] {
            reply.insert(TIME, broken.parse().unwrap());
            services.answered("api.chilpa.app", &reply);
            assert_eq!(services.offset.load(Ordering::Relaxed), followed, "{broken:?}");
        }
        parts(&services.sign("GET", "api.chilpa.app", "/lastfm"));
    }
}
