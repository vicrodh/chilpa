//! I decode a folder dialog's URL before using it as a library path.
//!
//! QML hands back `file:///home/me/Music/Album%20%5BFLAC%5D`, and what is on
//! disk is `/home/me/Music/Album [FLAC]`. Stripping `file://` and stopping
//! there leaves the escapes in: every folder whose name has a space, a
//! bracket, an accent or an apostrophe would not exist. The escapes are
//! undone here. A plain path typed by hand is left alone: a `%` followed by
//! two hex digits in it is part of somebody's file name. On Windows the
//! drive follows the third slash: `file:///C:/Music` is `C:/Music`, and
//! `/C:/Music` names nothing.

pub fn path_of(url: &str) -> String {
    local_path(url, cfg!(target_os = "windows"))
}

fn local_path(url: &str, windows: bool) -> String {
    let Some(text) = url.strip_prefix("file://") else {
        return url.to_string();
    };
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    let path = String::from_utf8_lossy(&out).into_owned();
    match path.as_bytes() {
        [b'/', drive, b':', ..] if windows && drive.is_ascii_alphabetic() => path[1..].to_string(),
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use super::{local_path, path_of};

    #[test]
    fn a_dialog_url_becomes_the_path_that_is_on_disk() {
        assert_eq!(path_of("file:///home/me/Music"), "/home/me/Music");
        assert_eq!(
            path_of("file:///mnt/nas/A/(1991)%20Attack%20of%20the%20Killer%20B's%20%5BFLAC%2016-44%5D"),
            "/mnt/nas/A/(1991) Attack of the Killer B's [FLAC 16-44]",
        );
    }

    #[test]
    fn accents_survive_the_trip() {
        assert_eq!(path_of("file:///m/Metr%C3%B3nomo"), "/m/Metrónomo");
        assert_eq!(path_of("file:///m/%E3%81%B7%E3%82%8A"), "/m/ぷり");
    }

    #[test]
    fn a_windows_url_starts_at_its_drive() {
        assert_eq!(local_path("file:///C:/Users/me/M%C3%BAsica", true), "C:/Users/me/Música");
        assert_eq!(local_path("file:///g:/x", true), "g:/x");
        assert_eq!(local_path("file:///C:/odd", false), "/C:/odd", "a folder named like a drive is only a drive on Windows");
        assert_eq!(local_path("file:///home/me", true), "/home/me");
    }

    #[test]
    fn a_plain_path_is_left_alone() {
        assert_eq!(path_of("/home/me/100%20.flac"), "/home/me/100%20.flac");
        assert_eq!(path_of("/home/me/Music"), "/home/me/Music");
    }
}
