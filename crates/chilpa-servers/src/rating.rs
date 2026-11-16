//! The listener's own ratings, on the servers that keep them.
//!
//! What each protocol says, and this follows:
//!
//! - **OpenSubsonic** writes `userRating`, whole stars from 1 to 5, on a song
//!   and, as an OpenSubsonic addition, on a record (`AlbumID3`). Absent or 0
//!   is unrated: Navidrome 0.64.2 writes `"userRating": 0` on an unrated
//!   record and leaves it out of an unrated song. `setRating` takes 1 to 5,
//!   and 0 takes the rating off; Navidrome applies it to a song, a record, an
//!   artist or a playlist, by its id.
//! - **Plex** writes `userRating` from 0.0 to 10.0, two to a star, on a
//!   track, a record and an artist, and leaves it out when there is none.
//!   `PUT /:/rate?key=<ratingKey>&identifier=com.plexapp.plugins.library&rating=<0-10>`
//!   sets it, and `rating=-1` takes it off. A track listing does not carry
//!   its record's rating: the records are listed apart (`album_ratings`).
//! - **Jellyfin** keeps a `Rating` number in a user's item data, with no
//!   scale said, and its own clients show likes, not stars. Nothing is read
//!   or written there until a scale is known.
//! - **A media server** keeps none.

use serde_json::Value;
use std::time::{Duration, Instant};

use crate::model::{Credential, Failure, Kind, Server};
use crate::net::{Net, Request};
use crate::text::{encode, id, string};
use crate::{plex, subsonic};

/// How long a server has to take one rating.
const WRITE_TIME: Duration = Duration::from_secs(15);

/// What a rating is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rated {
    Track,
    Album,
}

impl Rated {
    pub fn name(self) -> &'static str {
        match self {
            Rated::Track => "track",
            Rated::Album => "album",
        }
    }

    pub fn from_name(name: &str) -> Option<Rated> {
        match name {
            "track" => Some(Rated::Track),
            "album" => Some(Rated::Album),
            _ => None,
        }
    }
}

/// A rating as the server wrote it, on its own scale. Zero is unrated.
#[derive(Clone, Debug, PartialEq)]
pub struct RatingRead {
    pub rated: Rated,
    /// The server's own id of the track or the record.
    pub id: String,
    pub value: f64,
}

/// The values a kind of server rates with: the lowest one that is a
/// rating, the highest, and the step between two of them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

impl Kind {
    /// The scale its ratings are on, or `None` when Chilpa reads and writes
    /// none there.
    pub fn rating_scale(self) -> Option<Scale> {
        match self {
            Kind::Subsonic => Some(Scale { min: 1.0, max: 5.0, step: 1.0 }),
            Kind::Plex => Some(Scale { min: 1.0, max: 10.0, step: 1.0 }),
            Kind::Jellyfin | Kind::Dlna => None,
        }
    }

    /// Whether its track listing leaves out the records' ratings, which are
    /// then listed apart (`album_ratings`).
    pub fn lists_record_ratings_apart(self) -> bool {
        self == Kind::Plex
    }
}

/// The rating a server wrote on one item, or 0 when it wrote none. A
/// number written as text is read too; anything below zero is none.
pub(crate) fn user_rating(item: &Value) -> f64 {
    let value = match item.get("userRating") {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    };
    value.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(0.0)
}

/// Gives one track or record that rating on the server, on the server's
/// own scale; `None` takes the rating off.
pub fn rate(net: &dyn Net, server: &Server, key: &Credential, _rated: Rated, id: &str, value: Option<f64>) -> Result<(), Failure> {
    match server.kind {
        Kind::Subsonic => {
            let stars = value.map_or(0, |v| v.round().clamp(1.0, 5.0) as u8);
            let url = subsonic::url(&server.address, "setRating", Some(key), &format!("&id={}&rating={stars}", encode(id)));
            let mut request = Request::get(url, Vec::new());
            request.timeout = WRITE_TIME;
            subsonic::send(net, request, refused_rating).map(|_| ())
        }
        Kind::Plex => {
            let rating = value.map_or_else(|| "-1".to_string(), |v| number_text(v.clamp(0.0, 10.0)));
            let url = format!("{}/:/rate?key={}&identifier=com.plexapp.plugins.library&rating={rating}", server.address, encode(id));
            let mut request = Request::put(url, plex::headers(&server.client, plex::server_token(key)), None);
            request.timeout = WRITE_TIME;
            let reply = net.send(&request).map_err(Failure::Unreachable)?;
            if reply.status == 404 {
                return Err(Failure::Gone(chilpa_i18n::t("the server no longer has it")));
            }
            plex::read(&reply).map(|_| ())
        }
        Kind::Jellyfin | Kind::Dlna => Err(Failure::Unexpected(chilpa_i18n::t("this server keeps no ratings Chilpa can change"))),
    }
}

/// What a server holds for one item now: its rating on the server's scale
/// (0 for none) and, when the server says it (Plex), the name of the
/// library the item is in.
#[derive(Clone, Debug, PartialEq)]
pub struct Held {
    pub value: f64,
    pub library: Option<String>,
}

/// Reads back what the server holds for one track or record.
pub fn held(net: &dyn Net, server: &Server, key: &Credential, rated: Rated, id: &str) -> Result<Held, Failure> {
    match server.kind {
        Kind::Subsonic => {
            let (endpoint, field) = match rated {
                Rated::Track => ("getSong", "song"),
                Rated::Album => ("getAlbum", "album"),
            };
            let url = subsonic::url(&server.address, endpoint, Some(key), &format!("&id={}", encode(id)));
            let found = subsonic::send(net, Request::get(url, Vec::new()), refused_rating)?;
            Ok(Held { value: user_rating(found.get(field).unwrap_or(&Value::Null)), library: None })
        }
        Kind::Plex => {
            let url = format!("{}/library/metadata/{}", server.address, encode(id));
            let answer = plex::ask(net, Request::get(url, plex::headers(&server.client, plex::server_token(key))))?;
            let container = answer.get("MediaContainer").cloned().unwrap_or(Value::Null);
            let item = container.get("Metadata").and_then(Value::as_array).and_then(|m| m.first()).cloned()
                .ok_or_else(|| Failure::Gone(chilpa_i18n::t("the server no longer has it")))?;
            let library = string(&container, "librarySectionTitle").or_else(|| string(&item, "librarySectionTitle"));
            Ok(Held { value: user_rating(&item), library })
        }
        Kind::Jellyfin | Kind::Dlna => Err(Failure::Unexpected(chilpa_i18n::t("this server keeps no ratings Chilpa can change"))),
    }
}

/// A Subsonic error to a rating: an id the server no longer has is gone.
fn refused_rating(error: &Value) -> Failure {
    match error.get("code").and_then(Value::as_i64) {
        Some(70) => Failure::Gone(chilpa_i18n::t("the server no longer has it")),
        _ => subsonic::error_of(error),
    }
}

/// A value as a query writes it: whole without a point.
fn number_text(value: f64) -> String {
    if value.fract() == 0.0 { format!("{}", value as i64) } else { format!("{value}") }
}

/// The ratings of records a server lists apart from its tracks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listed {
    pub found: Vec<RatingRead>,
    /// Every record of the chosen libraries was read. When not, what was
    /// read still counts; what was not is not taken for unrated.
    pub whole: bool,
}

/// The ratings of the server's records when its track listing does not
/// carry them, at most `pages` requests and until `until`. A server whose
/// pages already carried them (`Page::ratings`) is asked nothing.
pub fn album_ratings(net: &dyn Net, server: &Server, key: &Credential, pages: usize, until: Instant) -> Result<Listed, Failure> {
    if server.kind != Kind::Plex {
        return Ok(Listed { found: Vec::new(), whole: true });
    }
    let sections = if server.libraries.is_empty() {
        plex::libraries(net, server, key)?.into_iter().map(|l| l.id).collect()
    } else {
        server.libraries.clone()
    };
    let mut found = Vec::new();
    let mut asked = 0;
    for section in sections {
        let mut offset: u64 = 0;
        loop {
            if asked >= pages || Instant::now() >= until {
                return Ok(Listed { found, whole: false });
            }
            asked += 1;
            let mut headers = plex::headers(&server.client, plex::server_token(key));
            headers.push(("X-Plex-Container-Start".into(), offset.to_string()));
            headers.push(("X-Plex-Container-Size".into(), plex::PAGE.to_string()));
            let url = format!("{}/library/sections/{}/all?type=9", server.address, encode(&section));
            let answer = plex::ask(net, Request::get(url, headers))?;
            let container = answer.get("MediaContainer").cloned().unwrap_or(Value::Null);
            let items = container.get("Metadata").and_then(Value::as_array).cloned().unwrap_or_default();
            found.extend(items.iter().filter_map(|item| Some(RatingRead { rated: Rated::Album, id: id(item, "ratingKey")?, value: user_rating(item) })));
            offset += items.len() as u64;
            let more = match container.get("totalSize").and_then(Value::as_u64) {
                Some(total) => offset < total,
                None => items.len() as u64 >= plex::PAGE,
            };
            if items.is_empty() || !more {
                break;
            }
        }
    }
    Ok(Listed { found, whole: true })
}

#[cfg(test)]
mod tests;
