//! Discogs, asked through the proxy.
//!
//! The proxy answers with what Discogs answered, as it came. What it takes:
//! `/discogs/search?q=&type=release`, `/discogs/release/<id>` and
//! `/discogs/image?url=`. It does not pass `per_page` on, so it is not
//! sent, and the rows are cut here.

use crate::http::{retry_after, Failure, Net};
use crate::proxy::ORIGIN;
use crate::release::{Credit, Hit, Provider, Release, ReleaseTrack};
use crate::text::{encode, is_number};
use serde_json::Value;
use std::time::Duration;

/// How many rows a search keeps.
const ROWS: usize = 12;
/// How many covers a search for them keeps.
const COVERS: usize = 10;

const TIMEOUT: Duration = Duration::from_secs(10);
const LARGEST: usize = 4 * 1024 * 1024;

/// A cover found, not yet fetched.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Image {
    pub url: String,
    pub width: u32,
    pub height: u32,
    /// The release it is of.
    pub release: String,
    pub year: String,
}

/// What is searched for: the catalogue number when there is one, the
/// artist and the title when there is not.
fn wanted(artist: &str, title: &str, catalog_number: &str) -> String {
    match catalog_number.trim() {
        "" => format!("{} {}", artist.trim(), title.trim()).trim().to_string(),
        number => number.to_string(),
    }
}

pub fn search_url(artist: &str, title: &str, catalog_number: &str) -> Option<String> {
    let wanted = wanted(artist, title, catalog_number);
    (!wanted.is_empty()).then(|| format!("{ORIGIN}/discogs/search?q={}&type=release", encode(&wanted)))
}

pub fn release_url(id: &str) -> Result<String, Failure> {
    let id = id.trim();
    if !is_number(id) {
        return Err(Failure::BadId);
    }
    Ok(format!("{ORIGIN}/discogs/release/{id}"))
}

/// An image of Discogs's, to be fetched through the proxy. Anywhere else
/// is refused here, before the proxy refuses it.
pub fn image_url(image: &str) -> Result<String, Failure> {
    if !is_discogs(image) {
        return Err(Failure::BadId);
    }
    Ok(format!("{ORIGIN}/discogs/image?url={}", encode(image)))
}

/// An https address at discogs.com or under it.
pub fn is_discogs(image: &str) -> bool {
    let Some(rest) = image.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default().to_lowercase();
    // A host with an `@` or a port is not one this has any business with.
    !host.contains(['@', ':']) && (host == "discogs.com" || host.ends_with(".discogs.com"))
}

/// "1", "A1", "1-1", "CD1-1", "1.1" as disc and track. What is not
/// understood is 0: an empty position is not the first track, and the
/// tracklist numbers it by its place (`numbered_in_order`).
pub fn position(text: &str) -> (u16, u16) {
    let text = text.trim();
    if text.is_empty() {
        return (0, 0);
    }
    let digits = |part: &str| part.chars().filter(char::is_ascii_digit).collect::<String>().parse::<u16>().unwrap_or(1);
    if let Some((disc, track)) = text.split_once('-') {
        return (digits(disc), track.trim().parse().unwrap_or(0));
    }
    if let Some((disc, track)) = text.split_once('.') {
        return (disc.trim().parse().unwrap_or(1), track.trim().parse().unwrap_or(0));
    }
    let mut letters = text.chars();
    if let Some(side) = letters.next().filter(char::is_ascii_alphabetic) {
        // A and B are the two sides of the first record, C and D of the second.
        let disc = match side.to_ascii_uppercase() {
            'C' | 'D' => 2,
            'E' | 'F' => 3,
            _ => 1,
        };
        return (disc, letters.as_str().trim().parse().unwrap_or(0));
    }
    match text.parse() {
        Ok(number) => (1, number),
        Err(_) => (0, 0),
    }
}

/// When positions do not say one track each -- a record's sides start
/// over at B1, a bonus track has none -- every track is numbered by its
/// place on its disc instead, which is how the files of such a record are
/// numbered.
fn numbered_in_order(tracks: &mut [ReleaseTrack]) {
    let mut seen = std::collections::HashSet::new();
    let clear = tracks.iter().all(|t| t.number > 0 && t.disc > 0 && seen.insert((t.disc, t.number)));
    if clear {
        return;
    }
    let mut disc = 1;
    let mut number = 0;
    for track in tracks.iter_mut() {
        let this = if track.disc == 0 { disc } else { track.disc };
        if this != disc {
            disc = this;
            number = 0;
        }
        number += 1;
        track.disc = disc;
        track.number = number;
    }
}

/// The entries of a tracklist that are tracks, with what an index holds in
/// its place: an index is a title over tracks of its own.
fn entries(list: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    for entry in list.as_array().into_iter().flatten() {
        match entry["type_"].as_str() {
            Some("track") => out.push(entry),
            Some("index") => out.extend(entries(&entry["sub_tracks"])),
            // A heading names what follows.
            _ => {}
        }
    }
    out
}

/// "3:45" and "1:02:03" in milliseconds. Zero for anything else.
pub fn duration_ms(text: &str) -> u32 {
    let parts: Vec<Option<u32>> = text.trim().split(':').map(|p| p.parse().ok()).collect();
    match parts.as_slice() {
        [Some(minutes), Some(seconds)] => (minutes * 60 + seconds) * 1000,
        [Some(hours), Some(minutes), Some(seconds)] => (hours * 3600 + minutes * 60 + seconds) * 1000,
        _ => 0,
    }
}

/// A name as Discogs writes it: "Nirvana (2)" is the second Nirvana it
/// knows, and "Nirvana*" is Nirvana credited under another spelling.
pub fn artist_name(text: &str) -> String {
    let name = text.trim().trim_end_matches('*').trim_end();
    if let Some((before, inside)) = name.rsplit_once(" (") {
        if inside.len() > 1 && inside.ends_with(')') && inside[..inside.len() - 1].bytes().all(|b| b.is_ascii_digit()) {
            return before.trim_end().to_string();
        }
    }
    name.to_string()
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().trim().to_string()
}

/// A number, which Discogs gives as one in some places and as text in others.
fn numbered(value: &Value) -> String {
    match value {
        Value::Number(n) if n.as_u64() != Some(0) => n.to_string(),
        Value::String(s) if s.trim() != "0" => s.trim().to_string(),
        _ => String::new(),
    }
}

fn texts(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().map(text).filter(|t| !t.is_empty()).collect()
}

fn credits_of(value: &Value) -> Vec<Credit> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|artist| {
            let name = artist_name(artist["name"].as_str()?);
            if name.is_empty() {
                return None;
            }
            // Discogs writes "," and "&" bare, with the spaces left to whoever prints them.
            let join = match text(&artist["join"]).as_str() {
                "" => String::new(),
                "," => ", ".to_string(),
                join => format!(" {join} "),
            };
            Some(Credit { name, join, id: numbered(&artist["id"]) })
        })
        .collect()
}

/// Asks the proxy once.
fn get(net: &Net, url: &str) -> Result<Value, Failure> {
    net.pace.wait();
    let reply = net.http.get(url, "application/json", TIMEOUT, LARGEST).map_err(Failure::Unreachable)?;
    let said = || serde_json::from_slice::<Value>(&reply.body).ok().map(|v| text(&v["error"])).unwrap_or_default();
    match reply.status {
        200 => serde_json::from_slice(&reply.body).map_err(|e| Failure::Unexpected(e.to_string())),
        429 => Err(Failure::Busy(retry_after(&reply).unwrap_or(60))),
        // 403 is the proxy turning away a client that is not Chilpa; 503 is
        // the proxy without its Discogs key. Neither is Discogs speaking.
        403 | 503 => Err(Failure::Refused(said())),
        status => Err(Failure::Unexpected(format!("{status} {}", said()).trim().to_string())),
    }
}

fn is_release(row: &Value) -> bool {
    matches!(row["type"].as_str(), Some("release" | "master"))
}

fn hit_of(row: &Value) -> Option<Hit> {
    let id = numbered(&row["id"]);
    if id.is_empty() || !is_release(row) {
        return None;
    }
    // "Artist - Title", as one text.
    let whole = text(&row["title"]);
    let (artist, title) = match whole.split_once(" - ") {
        Some((artist, title)) => (artist_name(artist), title.trim().to_string()),
        None => (String::new(), whole),
    };
    Some(Hit {
        provider: Provider::Discogs,
        id,
        title,
        artist,
        date: numbered(&row["year"]),
        tracks: 0,
        country: text(&row["country"]),
        label: texts(&row["label"]).into_iter().next().unwrap_or_default(),
        catalog_number: text(&row["catno"]),
        format: texts(&row["format"]).join(", "),
        score: 0,
        release_group_id: String::new(),
    })
}

fn rows_of(net: &Net, artist: &str, title: &str, catalog_number: &str) -> Result<Vec<Value>, Failure> {
    let Some(url) = search_url(artist, title, catalog_number) else {
        return Ok(Vec::new());
    };
    let rows = |found: Value| -> Vec<Value> { found["results"].as_array().cloned().unwrap_or_default().into_iter().filter(is_release).collect() };
    let found = rows(get(net, &url)?);
    if !found.is_empty() || catalog_number.trim().is_empty() {
        return Ok(found);
    }
    match search_url(artist, title, "") {
        Some(url) => Ok(rows(get(net, &url)?)),
        None => Ok(found),
    }
}

pub fn search(net: &Net, artist: &str, title: &str, catalog_number: &str) -> Result<Vec<Hit>, Failure> {
    Ok(rows_of(net, artist, title, catalog_number)?.iter().filter_map(hit_of).take(ROWS).collect())
}

fn release_of(value: &Value) -> Result<Release, Failure> {
    let id = numbered(&value["id"]);
    if id.is_empty() {
        return Err(Failure::Unexpected("no release in the answer".to_string()));
    }
    let mut genres = texts(&value["genres"]);
    for style in texts(&value["styles"]) {
        if !genres.iter().any(|g| g.eq_ignore_ascii_case(&style)) {
            genres.push(style);
        }
    }
    let mut tracks: Vec<ReleaseTrack> = entries(&value["tracklist"])
        .into_iter()
        .map(|entry| {
            let (disc, number) = position(&text(&entry["position"]));
            ReleaseTrack {
                disc,
                number,
                title: text(&entry["title"]),
                duration_ms: duration_ms(&text(&entry["duration"])),
                // Only what Discogs credits on this track. The record's
                // artist is not every track's: on a compilation it is nobody's.
                credits: credits_of(&entry["artists"]),
                recording_id: String::new(),
                track_id: String::new(),
            }
        })
        .collect();
    numbered_in_order(&mut tracks);
    let label = &value["labels"][0];
    Ok(Release {
        provider: Provider::Discogs,
        id,
        title: text(&value["title"]),
        credits: credits_of(&value["artists"]),
        date: numbered(&value["year"]),
        genres,
        label: text(&label["name"]),
        catalog_number: text(&label["catno"]),
        catalog_numbers: Vec::new(),
        country: text(&value["country"]),
        barcode: String::new(),
        discs: tracks.iter().map(|t| t.disc).max().unwrap_or(1),
        media_titles: Vec::new(),
        release_group_id: String::new(),
        url: text(&value["uri"]),
        tracks,
    })
}

pub fn release(net: &Net, id: &str) -> Result<Release, Failure> {
    release_of(&get(net, &release_url(id)?)?)
}

fn usable(url: &str) -> bool {
    !url.is_empty() && !url.contains("spacer.gif") && is_discogs(url)
}

/// The covers of the releases a search finds: every image of the first two,
/// four of each at most, then the one cover of each of the others. Ten at
/// most, and up to three requests.
pub fn images(net: &Net, artist: &str, title: &str, catalog_number: &str) -> Result<Vec<Image>, Failure> {
    let rows: Vec<Value> = rows_of(net, artist, title, catalog_number)?.into_iter().take(20).collect();
    let mut found: Vec<Image> = Vec::new();
    let add = |found: &mut Vec<Image>, image: Image| {
        if usable(&image.url) && !found.iter().any(|seen| seen.url == image.url) {
            found.push(image);
        }
    };
    for row in rows.iter().take(2) {
        // A release that cannot be fetched gives no covers, and the rest go on.
        let Ok(url) = release_url(&numbered(&row["id"])) else { continue };
        let Ok(detail) = get(net, &url) else { continue };
        let release = text(&detail["title"]);
        let year = numbered(&detail["year"]);
        let mut taken = 0;
        for image in detail["images"].as_array().into_iter().flatten() {
            if taken == 4 {
                break;
            }
            let before = found.len();
            add(
                &mut found,
                Image {
                    url: text(&image["uri"]),
                    width: image["width"].as_u64().unwrap_or(0) as u32,
                    height: image["height"].as_u64().unwrap_or(0) as u32,
                    release: release.clone(),
                    year: year.clone(),
                },
            );
            taken += found.len() - before;
        }
    }
    for row in rows.iter().skip(2).take(10) {
        // The size is what Discogs serves each as, not what was measured.
        let (url, side) = match (text(&row["cover_image"]), text(&row["thumb"])) {
            (cover, _) if usable(&cover) => (cover, 600),
            (_, thumb) => (thumb, 150),
        };
        add(&mut found, Image { url, width: side, height: side, release: text(&row["title"]), year: numbered(&row["year"]) });
    }
    found.truncate(COVERS);
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release::display;
    use crate::http::testing::Canned;

    /// The shape Discogs answers a search in, as QBZ's reader took it.
    const FOUND: &str = r#"{"pagination":{"items":3},"results":[
      {"id":1001,"type":"release","title":"Nirvana (2) - Nevermind","year":"1991","country":"US",
       "label":["DGC","Sub Pop"],"catno":"DGCD-24425","format":["CD","Album"],
       "cover_image":"https://i.discogs.com/a/one.jpeg","thumb":"https://i.discogs.com/a/one-150.jpeg"},
      {"id":1002,"type":"master","title":"Nevermind","year":"1991","country":"",
       "cover_image":"https://i.discogs.com/a/two.jpeg","thumb":""},
      {"id":77,"type":"artist","title":"Nirvana (2)"},
      {"id":1003,"type":"release","title":"Nirvana* - Nevermind","year":"1992",
       "cover_image":"https://st.discogs.com/images/spacer.gif","thumb":"https://i.discogs.com/a/three-150.jpeg"}]}"#;

    const WHOLE: &str = r#"{"id":1001,"title":"Various Songs","year":1991,"country":"US",
      "uri":"https://www.discogs.com/release/1001-Various-Songs",
      "artists":[{"name":"Various","id":194,"join":""}],
      "genres":["Rock"],"styles":["Grunge","rock"],
      "labels":[{"name":"DGC","catno":"DGCD-24425","id":3}],
      "images":[{"type":"primary","uri":"https://i.discogs.com/b/front.jpeg","width":600,"height":595},
                {"type":"secondary","uri":"https://i.discogs.com/b/back.jpeg","width":600,"height":600},
                {"type":"secondary","uri":"https://i.discogs.com/b/front.jpeg","width":600,"height":595},
                {"type":"secondary","uri":"","width":0,"height":0}],
      "tracklist":[
        {"position":"","type_":"heading","title":"Side one","duration":""},
        {"position":"A1","type_":"track","title":"First","duration":"5:01",
         "artists":[{"name":"Alpha (3)","id":11,"join":"&"},{"name":"Beta*","id":12,"join":""}]},
        {"position":"A2","type_":"track","title":"Second","duration":""},
        {"position":"B","type_":"index","title":"A suite","duration":"","sub_tracks":[]},
        {"position":"C1","type_":"track","title":"Third","duration":"1:02:03"}]}"#;

    #[test]
    fn what_is_searched_for_is_the_number_or_the_names() {
        assert_eq!(search_url("Nirvana", "Nevermind", "").unwrap(), format!("{ORIGIN}/discogs/search?q=Nirvana%20Nevermind&type=release"));
        assert_eq!(search_url("Nirvana", "Nevermind", "DGCD-24425").unwrap(), format!("{ORIGIN}/discogs/search?q=DGCD-24425&type=release"));
        assert_eq!(search_url("", "Nevermind", "").unwrap(), format!("{ORIGIN}/discogs/search?q=Nevermind&type=release"));
        assert_eq!(search_url(" ", "", ""), None);
    }

    #[test]
    fn per_page_is_never_sent() {
        let http = Canned::new().json(FOUND);
        search(&Net::free(&http), "Nirvana", "Nevermind", "").unwrap();
        assert!(!http.asked()[0].contains("per_page"));
    }

    #[test]
    fn a_discogs_title_is_an_artist_and_a_record() {
        let http = Canned::new().json(FOUND);
        let hits = search(&Net::free(&http), "Nirvana", "Nevermind", "").unwrap();
        assert_eq!(hits.len(), 3, "the artist among the rows is not a release");
        assert_eq!((hits[0].artist.as_str(), hits[0].title.as_str()), ("Nirvana", "Nevermind"));
        assert_eq!((hits[0].id.as_str(), hits[0].date.as_str()), ("1001", "1991"));
        assert_eq!((hits[0].label.as_str(), hits[0].catalog_number.as_str()), ("DGC", "DGCD-24425"));
        assert_eq!(hits[0].format, "CD, Album");
        assert_eq!((hits[1].artist.as_str(), hits[1].title.as_str()), ("", "Nevermind"), "a title with no artist in it");
        assert_eq!(hits[2].artist, "Nirvana");
    }

    #[test]
    fn a_discogs_name_loses_its_number_and_its_star() {
        assert_eq!(artist_name("Nirvana (2)"), "Nirvana");
        assert_eq!(artist_name("Nirvana*"), "Nirvana");
        assert_eq!(artist_name("Nirvana (2)*"), "Nirvana");
        assert_eq!(artist_name("Sunn O)))"), "Sunn O)))");
        assert_eq!(artist_name("Tyler (The Creator)"), "Tyler (The Creator)", "what is in the brackets is not a number");
        assert_eq!(artist_name("!!! (2)"), "!!!");
    }

    #[test]
    fn discogs_positions() {
        for (said, means) in [("1", (1, 1)), ("12", (1, 12)), ("A1", (1, 1)), ("B3", (1, 3)), ("C2", (2, 2)), ("F1", (3, 1)), ("1-5", (1, 5)), ("CD2-3", (2, 3)), ("2.4", (2, 4)), ("", (0, 0)), ("A", (1, 0)), ("??", (0, 0))] {
            assert_eq!(position(said), means, "{said:?}");
        }
        assert_eq!(duration_ms("3:45"), 225_000);
        assert_eq!(duration_ms("1:02:03"), 3_723_000);
        assert_eq!(duration_ms(""), 0);
        assert_eq!(duration_ms("about four minutes"), 0);
    }

    #[test]
    fn a_release_comes_whole() {
        let http = Canned::new().json(WHOLE);
        let found = release(&Net::free(&http), "1001").unwrap();
        assert_eq!(http.asked(), [format!("{ORIGIN}/discogs/release/1001")]);
        assert_eq!(found.provider, Provider::Discogs);
        assert_eq!((found.id.as_str(), found.date.as_str()), ("1001", "1991"));
        assert_eq!(found.genres, ["Rock", "Grunge"], "styles after genres, none twice");
        assert_eq!((found.label.as_str(), found.catalog_number.as_str()), ("DGC", "DGCD-24425"));
        assert_eq!(found.url, "https://www.discogs.com/release/1001-Various-Songs");
        assert_eq!(found.discs, 2);
        assert_eq!(display(&found.credits), "Various");
    }

    #[test]
    fn a_heading_is_not_a_track() {
        let http = Canned::new().json(WHOLE);
        let found = release(&Net::free(&http), "1001").unwrap();
        let titles: Vec<&str> = found.tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["First", "Second", "Third"]);
        assert_eq!((found.tracks[2].disc, found.tracks[2].number), (2, 1));
        assert_eq!(found.tracks[2].duration_ms, 3_723_000);
        assert_eq!(found.tracks[1].duration_ms, 0);
    }

    #[test]
    fn sides_that_start_over_an_index_and_a_bonus_are_numbered_by_place() {
        let vinyl = r#"{"id":7,"title":"Sides","tracklist":[
            {"position":"A1","type_":"track","title":"One"},
            {"position":"A2","type_":"track","title":"Two"},
            {"position":"B1","type_":"track","title":"Three"},
            {"position":"B2","type_":"index","title":"A suite","sub_tracks":[
                {"position":"B2a","type_":"track","title":"Four"},
                {"position":"B2b","type_":"track","title":"Five"}]},
            {"position":"","type_":"track","title":"Bonus"}]}"#;
        let http = Canned::new().json(vinyl);
        let found = release(&Net::free(&http), "7").unwrap();
        let tracks: Vec<(u16, u16, &str)> = found.tracks.iter().map(|t| (t.disc, t.number, t.title.as_str())).collect();
        assert_eq!(tracks, [(1, 1, "One"), (1, 2, "Two"), (1, 3, "Three"), (1, 4, "Four"), (1, 5, "Five"), (1, 6, "Bonus")]);
    }

    #[test]
    fn a_discogs_track_with_no_artists_credits_nobody() {
        let http = Canned::new().json(WHOLE);
        let found = release(&Net::free(&http), "1001").unwrap();
        assert_eq!(display(&found.tracks[0].credits), "Alpha & Beta");
        assert_eq!(found.tracks[0].credits[0].id, "11");
        assert!(found.tracks[1].credits.is_empty(), "the record's \"Various\" is not this track's artist");
    }

    #[test]
    fn a_release_number_that_is_not_one_is_refused_without_asking() {
        let http = Canned::new();
        for id in ["", "abc", "12/../3", "3d3207f9-c4c2-425c-a364-f359b4320805"] {
            assert_eq!(release(&Net::free(&http), id), Err(Failure::BadId), "{id}");
        }
        assert!(http.asked().is_empty());
    }

    #[test]
    fn an_image_anywhere_else_is_refused_without_asking() {
        for image in [
            "https://example.org/a.jpeg",
            "http://i.discogs.com/a.jpeg",
            "https://i.discogs.com.evil.net/a.jpeg",
            "https://evildiscogs.com/a.jpeg",
            "https://i.discogs.com@evil.net/a.jpeg",
            "https://i.discogs.com:8443/a.jpeg",
            "file:///etc/passwd",
        ] {
            assert_eq!(image_url(image), Err(Failure::BadId), "{image}");
        }
        assert_eq!(
            image_url("https://i.discogs.com/a/one.jpeg?x=1").unwrap(),
            format!("{ORIGIN}/discogs/image?url=https%3A%2F%2Fi.discogs.com%2Fa%2Fone.jpeg%3Fx%3D1")
        );
        assert!(is_discogs("https://www.discogs.com/x") && is_discogs("https://discogs.com/x"));
    }

    #[test]
    fn the_proxy_saying_no_is_refused_with_its_reason() {
        let http = Canned::new().status(403, &[], br#"{"error":"/discogs is not available to this client"}"#);
        assert_eq!(
            search(&Net::free(&http), "Nirvana", "Nevermind", ""),
            Err(Failure::Refused("/discogs is not available to this client".to_string()))
        );
    }

    #[test]
    fn a_proxy_without_its_key_or_that_turns_chilpa_away_is_refused() {
        let http = Canned::new().status(503, &[], br#"{"error":"Discogs is not configured"}"#);
        assert_eq!(search(&Net::free(&http), "Nirvana", "Nevermind", ""), Err(Failure::Refused("Discogs is not configured".into())));
        let http = Canned::new().status(403, &[], br#"{"error":"This proxy only serves Chilpa"}"#);
        assert_eq!(search(&Net::free(&http), "Nirvana", "Nevermind", ""), Err(Failure::Refused("This proxy only serves Chilpa".into())));
    }

    #[test]
    fn the_proxy_asking_for_time_is_busy_and_is_not_asked_again() {
        let http = Canned::new().status(429, &[("retry-after", "41")], br#"{"error":"Rate limit exceeded"}"#);
        assert_eq!(search(&Net::free(&http), "Nirvana", "Nevermind", ""), Err(Failure::Busy(41)));
        assert_eq!(http.asked().len(), 1);
        let http = Canned::new().silence();
        assert!(matches!(search(&Net::free(&http), "Nirvana", "Nevermind", ""), Err(Failure::Unreachable(_))));
        assert_eq!(http.asked().len(), 1);
    }

    #[test]
    fn a_catalog_number_that_finds_nothing_is_tried_without_it() {
        let http = Canned::new().json(r#"{"results":[]}"#).json(FOUND);
        assert_eq!(search(&Net::free(&http), "Nirvana", "Nevermind", "XYZ-000").unwrap().len(), 3);
        assert_eq!(http.asked().len(), 2);
    }

    #[test]
    fn the_covers_of_a_search_are_those_of_its_first_releases_and_then_the_rest() {
        let second = r#"{"id":1002,"title":"Nevermind","year":1991,"images":[{"uri":"https://i.discogs.com/c/only.jpeg","width":500,"height":500}]}"#;
        let http = Canned::new().json(FOUND).json(WHOLE).json(second);
        let covers = images(&Net::free(&http), "Nirvana", "Nevermind", "").unwrap();
        assert_eq!(http.asked().len(), 3, "the search and its first two releases");
        let urls: Vec<&str> = covers.iter().map(|c| c.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://i.discogs.com/b/front.jpeg",
                "https://i.discogs.com/b/back.jpeg",
                "https://i.discogs.com/c/only.jpeg",
                "https://i.discogs.com/a/three-150.jpeg",
            ],
            "none twice, none empty, and the placeholder is left out for the thumb"
        );
        assert_eq!((covers[0].width, covers[0].height), (600, 595));
        assert_eq!((covers[0].release.as_str(), covers[0].year.as_str()), ("Various Songs", "1991"));
        assert_eq!(covers[3].width, 150);
    }

    #[test]
    fn a_release_that_cannot_be_fetched_gives_no_covers_and_the_rest_go_on() {
        let http = Canned::new().json(FOUND).status(500, &[], b"").json(r#"{"id":1002,"title":"Nevermind","images":[]}"#);
        let covers = images(&Net::free(&http), "Nirvana", "Nevermind", "").unwrap();
        assert_eq!(covers.len(), 1);
        assert_eq!(covers[0].url, "https://i.discogs.com/a/three-150.jpeg");
    }
}
