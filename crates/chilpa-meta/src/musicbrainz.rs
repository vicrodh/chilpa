//! MusicBrainz, asked directly.
//!
//! Not through the proxy: its limit is per address, and behind a proxy
//! every listener would share the proxy's. Reading needs no key.

use crate::http::{retry_after, Failure, Net};
use crate::release::{display, Credit, Hit, Provider, Release, ReleaseTrack};
use crate::text::{encode, is_uuid, query_escape};
use serde_json::Value;
use std::time::Duration;

pub const API: &str = "https://musicbrainz.org/ws/2";

/// How many rows a search asks for.
const ROWS: usize = 12;

const TIMEOUT: Duration = Duration::from_secs(6);
const LARGEST: usize = 4 * 1024 * 1024;

/// The address of a search, built from what there is. `None` when there is
/// nothing to search for.
pub fn search_url(title: &str, artist: &str, catalog_number: &str, limit: usize) -> Option<String> {
    let part = |field: &str, value: &str| {
        let value = value.trim();
        (!value.is_empty()).then(|| format!("{field}:\"{}\"", query_escape(value)))
    };
    let parts: Vec<String> = match part("catno", catalog_number) {
        // A catalogue number says which record it is. The title would only
        // narrow it to the spelling somebody typed.
        Some(number) => [Some(number), part("artist", artist)].into_iter().flatten().collect(),
        None => [part("release", title), part("artist", artist)].into_iter().flatten().collect(),
    };
    if parts.is_empty() {
        return None;
    }
    let query = parts.join(" AND ");
    Some(format!("{API}/release?query={}&fmt=json&limit={}", encode(&query), limit.clamp(1, 25)))
}

/// The address of one release, with its tracks, credits, labels, genres
/// and the group it belongs to.
pub fn release_url(id: &str) -> Result<String, Failure> {
    let id = id.trim().to_lowercase();
    if !is_uuid(&id) {
        return Err(Failure::BadId);
    }
    Ok(format!("{API}/release/{id}?inc=recordings+artist-credits+labels+genres+release-groups&fmt=json"))
}

/// Asks, waiting out what MusicBrainz says to wait out: three tries for a
/// "too many requests", two for a silence.
pub(crate) fn get(net: &Net, url: &str) -> Result<Value, Failure> {
    let mut silences = 0;
    let mut refusals = 0;
    loop {
        // Before every try, not only the first: a retry does not jump the line.
        net.pace.wait();
        match net.http.get(url, "application/json", TIMEOUT, LARGEST) {
            Err(why) => {
                silences += 1;
                if silences >= 2 {
                    return Err(Failure::Unreachable(why));
                }
                (net.nap)(Duration::from_secs(1));
            }
            Ok(reply) if reply.status == 200 => {
                return serde_json::from_slice(&reply.body).map_err(|e| Failure::Unexpected(e.to_string()));
            }
            Ok(reply) if reply.status == 429 || reply.status == 503 => {
                let wait = retry_after(&reply).unwrap_or(1).min(8);
                refusals += 1;
                if refusals >= 3 {
                    return Err(Failure::Busy(wait));
                }
                (net.nap)(Duration::from_secs(wait));
            }
            Ok(reply) if reply.status == 404 => return Err(Failure::Unexpected("not found".to_string())),
            Ok(reply) => return Err(Failure::Unexpected(reply.status.to_string())),
        }
    }
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().trim().to_string()
}

fn number(value: &Value) -> u16 {
    value.as_u64().and_then(|n| u16::try_from(n).ok()).unwrap_or(0)
}

fn credits_of(value: &Value) -> Vec<Credit> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|credit| {
            let artist = &credit["artist"];
            // The name as it was credited, which may not be the artist's own.
            let name = credit["name"].as_str().or_else(|| artist["name"].as_str())?.to_string();
            Some(Credit { name, join: credit["joinphrase"].as_str().unwrap_or_default().to_string(), id: text(&artist["id"]) })
        })
        .collect()
}

/// The first label of a release, and its number.
fn label_of(value: &Value) -> (String, String) {
    let first = &value["label-info"][0];
    (text(&first["label"]["name"]), text(&first["catalog-number"]))
}

/// A release's labels do not explicitly point at its media. Numbered runs
/// are the only ones safe to offer as a disc-by-disc catalogue choice.
fn numbered_catalogs(numbers: Vec<String>, discs: usize) -> Vec<String> {
    if numbers.len() != discs || discs < 2 { return Vec::new(); }
    let pieces: Option<Vec<_>> = numbers.iter().map(|value| {
        let start = value.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        let (prefix, digits) = value.split_at(start);
        if digits.is_empty() { None } else { digits.parse::<u64>().ok().map(|number| (prefix, number)) }
    }).collect();
    let Some(pieces) = pieces else { return Vec::new() };
    if pieces.windows(2).all(|pair| pair[0].0 == pair[1].0 && pair[0].1.checked_add(1) == Some(pair[1].1)) {
        numbers
    } else { Vec::new() }
}

/// The release's genres, or its group's when it has none of its own: most
/// voted first, five at most. A genre one person voted for is left out,
/// unless that is all there is.
fn genres_of(release: &Value) -> Vec<String> {
    let listed = |value: &Value| -> Vec<(String, u64)> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|genre| Some((genre["name"].as_str()?.trim().to_string(), genre["count"].as_u64().unwrap_or(0))))
            .filter(|(name, _)| !name.is_empty())
            .collect()
    };
    let mut genres = listed(&release["genres"]);
    if genres.is_empty() {
        genres = listed(&release["release-group"]["genres"]);
    }
    genres.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if genres.iter().any(|(_, votes)| *votes >= 2) {
        genres.retain(|(_, votes)| *votes >= 2);
    }
    genres.truncate(5);
    genres.into_iter().map(|(name, _)| name).collect()
}

fn hit_of(release: &Value) -> Option<Hit> {
    let id = text(&release["id"]);
    if id.is_empty() {
        return None;
    }
    let media = release["media"].as_array();
    let tracks = match number(&release["track-count"]) {
        0 => media.into_iter().flatten().map(|m| number(&m["track-count"])).sum(),
        n => n,
    };
    let (label, catalog_number) = label_of(release);
    Some(Hit {
        provider: Provider::MusicBrainz,
        id,
        title: text(&release["title"]),
        artist: display(&credits_of(&release["artist-credit"])),
        date: text(&release["date"]),
        tracks,
        country: text(&release["country"]),
        label,
        catalog_number,
        format: text(&release["media"][0]["format"]),
        score: release["score"].as_u64().unwrap_or(0).min(100) as u8,
        release_group_id: text(&release["release-group"]["id"]),
    })
}

fn release_of(value: &Value) -> Result<Release, Failure> {
    let id = text(&value["id"]);
    if id.is_empty() {
        return Err(Failure::Unexpected("no release in the answer".to_string()));
    }
    let credits = credits_of(&value["artist-credit"]);
    let media = value["media"].as_array().cloned().unwrap_or_default();
    let mut tracks = Vec::new();
    for medium in &media {
        let disc = match number(&medium["position"]) {
            0 => 1,
            n => n,
        };
        for track in medium["tracks"].as_array().into_iter().flatten() {
            let recording = &track["recording"];
            // The track's own credit, then the recording's, then the record's.
            let mut credited = credits_of(&track["artist-credit"]);
            if credited.is_empty() {
                credited = credits_of(&recording["artist-credit"]);
            }
            if credited.is_empty() {
                credited = credits.clone();
            }
            let title = match text(&track["title"]) {
                empty if empty.is_empty() => text(&recording["title"]),
                title => title,
            };
            let length = track["length"].as_u64().or_else(|| recording["length"].as_u64()).unwrap_or(0);
            tracks.push(ReleaseTrack {
                disc,
                // Its place on the side, not what is printed on it: a
                // vinyl's first track is "A1" and is track 1.
                number: number(&track["position"]),
                title,
                duration_ms: u32::try_from(length).unwrap_or(0),
                credits: credited,
                recording_id: text(&recording["id"]),
                track_id: text(&track["id"]),
            });
        }
    }
    let (label, catalog_number) = label_of(value);
    let catalog_numbers: Vec<String> = value["label-info"].as_array().into_iter().flatten()
        .map(|entry| text(&entry["catalog-number"])).collect();
    let catalog_numbers = numbered_catalogs(catalog_numbers, media.len());
    Ok(Release {
        provider: Provider::MusicBrainz,
        url: format!("https://musicbrainz.org/release/{id}"),
        id,
        title: text(&value["title"]),
        credits,
        date: text(&value["date"]),
        genres: genres_of(value),
        label,
        catalog_number,
        catalog_numbers,
        country: text(&value["country"]),
        barcode: text(&value["barcode"]),
        discs: u16::try_from(media.len()).unwrap_or(1).max(1),
        media_titles: media.iter().map(|medium| text(&medium["title"])).collect(),
        release_group_id: text(&value["release-group"]["id"]),
        tracks,
    })
}

fn hits_of(value: &Value) -> Vec<Hit> {
    value["releases"].as_array().into_iter().flatten().filter_map(hit_of).collect()
}

/// Searches for a release. A catalogue number that finds nothing is tried
/// again without it: what is printed on a spine is not always what was
/// typed into the catalogue.
pub fn search(net: &Net, title: &str, artist: &str, catalog_number: &str) -> Result<Vec<Hit>, Failure> {
    let Some(url) = search_url(title, artist, catalog_number, ROWS) else {
        return Ok(Vec::new());
    };
    let hits = hits_of(&get(net, &url)?);
    if !hits.is_empty() || catalog_number.trim().is_empty() {
        return Ok(hits);
    }
    match search_url(title, artist, "", ROWS) {
        Some(url) => Ok(hits_of(&get(net, &url)?)),
        None => Ok(hits),
    }
}

/// One release, whole.
pub fn release(net: &Net, id: &str) -> Result<Release, Failure> {
    release_of(&get(net, &release_url(id)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::testing::Canned;

    /// What MusicBrainz answered on 2026-09-26 to a search for "Powerslave"
    /// by "Iron Maiden", cut down to its first row and the keys read here.
    const FOUND: &str = r#"{"created":"2026-09-27T03:26:26.027Z","count":39,"offset":0,"releases":[
      {"id":"3d3207f9-c4c2-425c-a364-f359b4320805","score":100,"title":"Powerslave","date":"2014-10-27",
       "country":"US","barcode":"881034121523",
       "artist-credit":[{"name":"Iron Maiden","joinphrase":"","artist":{"id":"ca891d65-d9b0-4258-89f7-e6ba29d83767","name":"Iron Maiden"}}],
       "release-group":{"id":"60a20bc8-acac-3cbb-99f9-ce458317233a","primary-type":"Album"},
       "label-info":[{"catalog-number":"BMG14014V","label":{"id":"3027c393-daa1-4f4a-af6a-2b464722d584","name":"INgrooves"}}],
       "track-count":8,"media":[{"format":"12\" Vinyl","track-count":8}]}]}"#;

    /// And to the lookup of that release, cut down to its first two tracks.
    /// Two things were changed by hand, to have the falls to try: the first
    /// track's recording credits nobody, and the second is credited only on
    /// its recording, to "Maiden with A guest".
    const WHOLE: &str = r#"{"id":"3d3207f9-c4c2-425c-a364-f359b4320805","title":"Powerslave","date":"2014-10-27",
      "country":"US","barcode":"881034121523","genres":[],"tags":[],
      "artist-credit":[{"name":"Iron Maiden","joinphrase":"","artist":{"id":"ca891d65-d9b0-4258-89f7-e6ba29d83767","name":"Iron Maiden"}}],
      "label-info":[{"catalog-number":"BMG14014V","label":{"id":"29d7c88f-5200-4418-a683-5c94ea032e38","name":"BMG"}}],
      "release-group":{"id":"60a20bc8-acac-3cbb-99f9-ce458317233a","primary-type":"Album","first-release-date":"1984-09-03",
        "genres":[{"name":"heavy metal","count":14},{"name":"metal","count":6},{"name":"pop","count":1},{"name":"progressive metal","count":1}]},
      "cover-art-archive":{"darkened":false,"back":false,"count":0,"front":false,"artwork":false},
      "media":[{"position":1,"format":"12\" Vinyl","track-count":8,"tracks":[
        {"id":"07133f3b-eeef-42c4-85c4-ee5a62638e3a","number":"A1","position":1,"title":"Aces High","length":271000,
         "artist-credit":[{"name":"Iron Maiden","joinphrase":"","artist":{"id":"ca891d65-d9b0-4258-89f7-e6ba29d83767","name":"Iron Maiden"}}],
         "recording":{"id":"d81a4632-2845-49e1-b3d2-6132dccd5a46","title":"Aces High","length":271000,"artist-credit":[]}},
        {"id":"634d0c6c-845f-400e-8e63-0012dd5d0f24","number":"A2","position":2,"title":"2 Minutes to Midnight","length":364000,
         "artist-credit":[],
         "recording":{"id":"d94b6470-d4bd-4bd4-9a2f-10ffdbe86782","title":"2 Minutes to Midnight","length":362000,
           "artist-credit":[{"name":"Maiden","joinphrase":" with ","artist":{"id":"ca891d65-d9b0-4258-89f7-e6ba29d83767","name":"Iron Maiden"}},
                            {"name":"A guest","joinphrase":"","artist":{"id":"00000000-0000-4000-8000-000000000001","name":"A guest"}}]}}]}]}"#;

    const ID: &str = "3d3207f9-c4c2-425c-a364-f359b4320805";

    #[test]
    fn the_query_is_built_from_what_there_is() {
        let query = |title, artist, number| search_url(title, artist, number, 12).map(|url| url.replace(API, ""));
        assert_eq!(query("A", "B", "").as_deref(), Some("/release?query=release%3A%22A%22%20AND%20artist%3A%22B%22&fmt=json&limit=12"));
        assert_eq!(query("A", "", "").as_deref(), Some("/release?query=release%3A%22A%22&fmt=json&limit=12"));
        assert_eq!(query("", "B", "").as_deref(), Some("/release?query=artist%3A%22B%22&fmt=json&limit=12"));
        assert_eq!(query("A", "B", "C-1").as_deref(), Some("/release?query=catno%3A%22C%5C-1%22%20AND%20artist%3A%22B%22&fmt=json&limit=12"));
        assert_eq!(query("A", "", "C1").as_deref(), Some("/release?query=catno%3A%22C1%22&fmt=json&limit=12"));
        assert_eq!(query(" ", "", " "), None);
        assert!(search_url("A", "", "", 900).unwrap().ends_with("limit=25"));
    }

    #[test]
    fn nothing_to_search_for_asks_nothing() {
        let http = Canned::new();
        assert_eq!(search(&Net::free(&http), "", " ", ""), Ok(Vec::new()));
        assert!(http.asked().is_empty());
    }

    #[test]
    fn a_release_id_that_is_not_one_is_refused_without_asking() {
        let http = Canned::new();
        for id in ["", "12345", "3d3207f9-c4c2-425c-a364-f359b4320805/../../artist", "not-an-id-at-all-but-thirty-six-long"] {
            assert_eq!(release(&Net::free(&http), id), Err(Failure::BadId), "{id}");
        }
        assert!(http.asked().is_empty());
        assert!(release_url("3D3207F9-C4C2-425C-A364-F359B4320805").unwrap().contains(ID), "an id in capitals is the same id");
    }

    #[test]
    fn the_release_group_is_asked_for() {
        let url = release_url(ID).unwrap();
        assert_eq!(url, format!("{API}/release/{ID}?inc=recordings+artist-credits+labels+genres+release-groups&fmt=json"));
    }

    #[test]
    fn a_box_keeps_each_medium_and_its_catalog_number() {
        let value: Value = serde_json::from_str(r#"{
            "id":"37478189-f215-47ac-8138-288d9af97224", "title":"Complete Box",
            "label-info":[{"catalog-number":"SVWC-1011"},{"catalog-number":"SVWC-1012"}],
            "media":[{"position":1,"title":"Scene 1","tracks":[{"position":1,"title":"First"}]},
                     {"position":2,"title":"Scene 2","tracks":[{"position":1,"title":"Second"}]}]
        }"#).unwrap();
        let release = release_of(&value).unwrap();
        assert_eq!(release.discs, 2);
        assert_eq!(release.catalog_numbers, ["SVWC-1011", "SVWC-1012"]);
        assert_eq!(release.media_titles, ["Scene 1", "Scene 2"]);
        assert_eq!(release.tracks.iter().map(|track| (track.disc, track.number)).collect::<Vec<_>>(), [(1, 1), (2, 1)]);
        assert!(numbered_catalogs(vec!["SVWC-1011".into(), "OTHER-500".into()], 2).is_empty());
    }

    #[test]
    fn a_row_says_what_the_catalogue_says() {
        let http = Canned::new().json(FOUND);
        let hits = search(&Net::free(&http), "Powerslave", "Iron Maiden", "").unwrap();
        assert_eq!(hits.len(), 1);
        let hit = &hits[0];
        assert_eq!((hit.id.as_str(), hit.title.as_str(), hit.artist.as_str()), (ID, "Powerslave", "Iron Maiden"));
        assert_eq!(hit.date, "2014-10-27", "the date comes whole");
        assert_eq!((hit.tracks, hit.score), (8, 100));
        assert_eq!((hit.label.as_str(), hit.catalog_number.as_str()), ("INgrooves", "BMG14014V"));
        assert_eq!(hit.format, "12\" Vinyl");
        assert_eq!(hit.release_group_id, "60a20bc8-acac-3cbb-99f9-ce458317233a");
    }

    #[test]
    fn a_catalog_number_that_finds_nothing_is_tried_without_it() {
        let http = Canned::new().json(r#"{"count":0,"releases":[]}"#).json(FOUND);
        let hits = search(&Net::free(&http), "Powerslave", "Iron Maiden", "XYZ-000").unwrap();
        assert_eq!(hits.len(), 1);
        let asked = http.asked();
        assert_eq!(asked.len(), 2);
        assert!(asked[0].contains("catno") && !asked[1].contains("catno"));
    }

    #[test]
    fn a_503_is_waited_out_and_tried_again() {
        let http = Canned::new().status(503, &[("retry-after", "1")], b"").json(FOUND);
        assert_eq!(search(&Net::free(&http), "Powerslave", "", "").unwrap().len(), 1);
        assert_eq!(http.asked().len(), 2);
    }

    #[test]
    fn three_503_in_a_row_is_busy() {
        let http = Canned::new().status(503, &[("retry-after", "30")], b"").status(429, &[], b"").status(503, &[("retry-after", "3")], b"");
        assert_eq!(search(&Net::free(&http), "Powerslave", "", ""), Err(Failure::Busy(3)));
        assert_eq!(http.asked().len(), 3);
    }

    #[test]
    fn a_silence_is_tried_twice_and_no_more() {
        let http = Canned::new().silence().silence().json(FOUND);
        assert!(matches!(search(&Net::free(&http), "Powerslave", "", ""), Err(Failure::Unreachable(_))));
        assert_eq!(http.asked().len(), 2);
    }

    #[test]
    fn a_release_that_is_not_there_says_so() {
        let http = Canned::new().status(404, &[], br#"{"error":"Not Found"}"#);
        assert_eq!(release(&Net::free(&http), ID), Err(Failure::Unexpected("not found".to_string())));
    }

    #[test]
    fn a_release_comes_whole() {
        let http = Canned::new().json(WHOLE);
        let found = release(&Net::free(&http), ID).unwrap();
        assert_eq!(found.provider, Provider::MusicBrainz);
        assert_eq!(found.date, "2014-10-27");
        assert_eq!(found.release_group_id, "60a20bc8-acac-3cbb-99f9-ce458317233a");
        assert_eq!(found.url, format!("https://musicbrainz.org/release/{ID}"));
        assert_eq!((found.label.as_str(), found.catalog_number.as_str()), ("BMG", "BMG14014V"));
        assert_eq!(found.discs, 1);
        assert_eq!(found.credits.len(), 1);
        assert_eq!(found.credits[0].id, "ca891d65-d9b0-4258-89f7-e6ba29d83767");
        assert_eq!(found.tracks.len(), 2);
    }

    #[test]
    fn the_track_number_is_its_position_and_not_its_label() {
        let http = Canned::new().json(WHOLE);
        let found = release(&Net::free(&http), ID).unwrap();
        assert_eq!((found.tracks[0].disc, found.tracks[0].number), (1, 1), "A1 is track 1");
        assert_eq!(found.tracks[1].number, 2);
        assert_eq!(found.tracks[0].duration_ms, 271_000);
        assert_eq!(found.tracks[0].track_id, "07133f3b-eeef-42c4-85c4-ee5a62638e3a");
        assert_eq!(found.tracks[0].recording_id, "d81a4632-2845-49e1-b3d2-6132dccd5a46");
    }

    #[test]
    fn a_track_credits_its_own_artists() {
        let http = Canned::new().json(WHOLE);
        let found = release(&Net::free(&http), ID).unwrap();
        assert_eq!(display(&found.tracks[0].credits), "Iron Maiden", "the track's own credit");
        assert_eq!(display(&found.tracks[1].credits), "Maiden with A guest", "the recording's when the track has none");
        assert_eq!(found.tracks[1].credits[1].id, "00000000-0000-4000-8000-000000000001");
    }

    #[test]
    fn genres_come_from_the_group_when_the_release_has_none() {
        let http = Canned::new().json(WHOLE);
        assert_eq!(release(&Net::free(&http), ID).unwrap().genres, ["heavy metal", "metal"]);
    }

    #[test]
    fn a_lone_vote_is_dropped_unless_it_is_all_there_is() {
        let only_lone: Value = serde_json::from_str(r#"{"genres":[{"name":"zydeco","count":1},{"name":"cajun","count":1}]}"#).unwrap();
        assert_eq!(genres_of(&only_lone), ["cajun", "zydeco"]);
        let its_own: Value =
            serde_json::from_str(r#"{"genres":[{"name":"jazz","count":3}],"release-group":{"genres":[{"name":"rock","count":9}]}}"#).unwrap();
        assert_eq!(genres_of(&its_own), ["jazz"], "what the release says of itself comes first");
        let many: Value = serde_json::from_str(
            r#"{"genres":[{"name":"a","count":2},{"name":"b","count":9},{"name":"c","count":2},{"name":"d","count":5},{"name":"e","count":3},{"name":"f","count":4}]}"#,
        )
        .unwrap();
        assert_eq!(genres_of(&many), ["b", "d", "f", "e", "a"]);
    }

    #[test]
    fn an_answer_that_is_not_json_is_unexpected() {
        let http = Canned::new().status(200, &[], b"<html>busy</html>");
        assert!(matches!(search(&Net::free(&http), "A", "", ""), Err(Failure::Unexpected(_))));
    }
}
