//! The Cover Art Archive.
//!
//! Another service than MusicBrainz, with no limit of one request a second
//! to keep. It answers a cover's address with a redirect to the image, and
//! 404 when it has none.

use crate::http::Failure;
use crate::text::is_uuid;

pub const HOST: &str = "https://coverartarchive.org";

/// 500 pixels, for looking at.
pub const PREVIEW: u32 = 500;
/// The image as it was uploaded, for keeping.
pub const ORIGINAL: u32 = 0;

fn front(kind: &str, id: &str, size: u32) -> Result<String, Failure> {
    let id = id.trim().to_lowercase();
    if !is_uuid(&id) {
        return Err(Failure::BadId);
    }
    Ok(match size {
        ORIGINAL => format!("{HOST}/{kind}/{id}/front"),
        size => format!("{HOST}/{kind}/{id}/front-{size}"),
    })
}

/// The front cover of a release.
pub fn release_front(id: &str, size: u32) -> Result<String, Failure> {
    front("release", id, size)
}

/// The front cover the archive picks for a release group.
pub fn group_front(id: &str, size: u32) -> Result<String, Failure> {
    front("release-group", id, size)
}

/// The same cover, as it was uploaded.
pub fn original_of(preview: &str) -> String {
    match preview.rsplit_once("/front-") {
        Some((before, _)) => format!("{before}/front"),
        None => preview.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "3d3207f9-c4c2-425c-a364-f359b4320805";

    #[test]
    fn a_cover_has_an_address_for_looking_and_one_for_keeping() {
        assert_eq!(release_front(ID, PREVIEW).unwrap(), format!("{HOST}/release/{ID}/front-500"));
        assert_eq!(release_front(ID, ORIGINAL).unwrap(), format!("{HOST}/release/{ID}/front"));
        assert_eq!(group_front(ID, PREVIEW).unwrap(), format!("{HOST}/release-group/{ID}/front-500"));
        assert_eq!(original_of(&release_front(ID, PREVIEW).unwrap()), release_front(ID, ORIGINAL).unwrap());
        assert_eq!(original_of(&group_front(ID, PREVIEW).unwrap()), group_front(ID, ORIGINAL).unwrap());
    }

    #[test]
    fn what_is_not_an_id_has_no_cover() {
        assert_eq!(release_front("../../x", PREVIEW), Err(Failure::BadId));
    }
}
