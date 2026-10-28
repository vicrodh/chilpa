//! Posts the reviewed bundle to a public paste when the listener asks for it.
use chilpa_core::Fetch;
use std::time::Duration;

/// Where the bundle goes: bpa.st's JSON interface, which answers with the
/// address of the paste and one to delete it.
const PASTE: &str = "https://bpa.st/api/v1/paste";
/// The pastes this host hands out; an answer pointing elsewhere is refused.
const PASTE_HOST: &str = "https://bpa.st/";
/// How long the paste lives: a week, the middle of what it offers.
const EXPIRY: &str = "1week";
/// A bundle is a few hundred lines; a larger one is refused.
const MAX_BODY: usize = 512 * 1024;
const MAX_REPLY: usize = 4096;

/// Uploads `bundle` and returns the address of the paste. Offline mode and
/// transport failures come back as the text the window shows.
pub fn upload(bundle: &str) -> Result<String, String> {
    let failed = || chilpa_i18n::t("Upload failed.");
    if bundle.len() > MAX_BODY { return Err(failed()); }
    let body = serde_json::json!({"expiry": EXPIRY, "files": [{"name": "chilpa-logs.txt", "lexer": "text", "content": bundle}]}).to_string();
    let request = Fetch { url: PASTE, user_agent: concat!("Chilpa/", env!("CARGO_PKG_VERSION")), accept: "application/json", timeout: Duration::from_secs(20), max_bytes: MAX_REPLY, authorization: None };
    let reply = chilpa_core::post_body(&request, "application/json", &body).map_err(|_| failed())?;
    if !(200..300).contains(&reply.status) { return Err(failed()); }
    let answer: serde_json::Value = serde_json::from_slice(&reply.body).map_err(|_| failed())?;
    answer["link"].as_str().filter(|link| link.starts_with(PASTE_HOST)).map(str::to_owned).ok_or_else(failed)
}
