//! Posts the reviewed bundle to a public paste when the listener asks for it.
use chilpa_core::Fetch;
use std::time::Duration;

/// Where the bundle goes.
const PASTE: &str = "https://paste.rs/";
/// A bundle is a few hundred lines; a paste larger than this is refused.
const MAX_BODY: usize = 1024 * 1024;
const MAX_REPLY: usize = 4096;

/// Uploads `bundle` and returns the address of the paste. Offline mode and
/// transport failures come back as the text the window shows.
pub fn upload(bundle: &str) -> Result<String, String> {
    if bundle.len() > MAX_BODY { return Err(chilpa_i18n::t("Upload failed.")); }
    let request = Fetch { url: PASTE, user_agent: concat!("Chilpa/", env!("CARGO_PKG_VERSION")), accept: "text/plain", timeout: Duration::from_secs(20), max_bytes: MAX_REPLY, authorization: None };
    let reply = chilpa_core::post_text(&request, bundle).map_err(|_| chilpa_i18n::t("Upload failed."))?;
    let address = String::from_utf8_lossy(&reply.body).trim().to_string();
    if !(200..300).contains(&reply.status) || !address.starts_with("https://") { return Err(chilpa_i18n::t("Upload failed.")); }
    Ok(address)
}
