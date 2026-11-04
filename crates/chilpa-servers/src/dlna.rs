//! A UPnP AV media server, which DLNA names (MiniDLNA, Gerbera, the one in
//! Plex Media Server): its ContentDirectory, read for music (ADR-017).
//!
//! What real servers answered, and this follows:
//!
//! - **There is no sign-in.** The protocol has none; who may read the
//!   server is the server's own setting. Nothing is sealed for it.
//! - **The address kept is its description's.** The ContentDirectory takes
//!   SOAP requests at the `controlURL` the description names, which
//!   `Details::control` keeps.
//! - **A library is read one of two ways.** A server that offers `Search` by
//!   class gives every audio item under a container in pages. One that does
//!   not (Plex Media Server 1.43.5 answers `SearchCaps` empty) is walked,
//!   container by container, with `Browse`.
//! - **One item shows under many containers** (by album, by artist, by
//!   folder). Its `refID`, or its own id when it has none, names it, so the
//!   sync keeps it once.
//! - **A listing carries what there is.** Bit depth and size are often
//!   missing, and nothing else asks for them: a track is complete as listed.
//! - **Only the original is read.** A `res` marked as converted
//!   (`DLNA.ORG_CI=1`), raw PCM, DSD or one on another host is never chosen,
//!   and an item without another one is not a track here.
//! - **The `res` is where the bytes are.** The track's revision keeps it,
//!   after a fingerprint of everything read of the item: what the server
//!   changes is a new revision.
//! - **Nothing is skipped quietly.** A container that gives fewer entries
//!   than it counts, changes its count halfway, does not count them, or sits
//!   deeper than a walk goes makes the page partial, and the sync takes
//!   nothing out.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::time::Duration;

use crate::model::{
    extension_of, image, Client, Details, Failure, Kind, Library, Page, Server, Stream, Track,
};
use crate::net::{Net, Request};
use crate::text::artist_line;

/// The device type a search on the network asks for.
pub const DEVICE: &str = "urn:schemas-upnp-org:device:MediaServer:1";

/// The container every server has at its top.
pub const ROOT: &str = "0";

/// The service every request goes to.
const SERVICE: &str = "urn:schemas-upnp-org:service:ContentDirectory:1";

/// Where a description usually is when only a host and a port were typed:
/// MiniDLNA (ReadyMedia), Gerbera, Plex Media Server.
const DESCRIPTIONS: [&str; 3] = [
    "/rootDesc.xml",
    "/description.xml",
    "/DeviceDescription.xml",
];

/// Entries asked for at once. A page of 200 records with their notes
/// measured 475 KB on a real server; one of tracks, about 1.2 KB each.
const PAGE: u64 = 200;

/// How far below a chosen container a walk goes, how many containers may
/// wait to be read, and how many entries one container is read for. Past
/// any of them the sync takes nothing out.
const DEPTH: u8 = 12;
const WAITING: usize = 20_000;
const ENTRIES: u64 = 1_000_000;

/// The most nodes one answer is read into.
const NODES: u32 = 200_000;

/// What `Search` asks for: every audio item.
const AUDIO: &str = "upnp:class derivedfrom \"object.item.audioItem\"";

/// Prefixes some servers use without declaring them, which no XML reader
/// accepts, and what they stand for.
const PREFIXES: [(&str, &str); 6] = [
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("upnp", "urn:schemas-upnp-org:metadata-1-0/upnp/"),
    ("dlna", "urn:schemas-dlna-org:metadata-1-0/"),
    ("sec", "http://www.sec.co.kr/"),
    ("pv", "http://www.pv.com/pvns/"),
    ("av", "urn:schemas-sony-com:av"),
];

/// A media server, as its description says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// Where the description was read: the address kept.
    pub location: String,
    pub name: String,
    /// What it is and its version: "Plex Media Server 1.43.5.11079".
    pub product: String,
    /// Its `UDN`, to refuse adding it twice.
    pub identity: String,
    /// Where its ContentDirectory takes requests (`Details::control`).
    pub control: String,
    /// It answers `Search` by class.
    pub search: bool,
}

impl Found {
    /// What the library keeps of it beyond the address.
    pub fn details(&self) -> Details {
        Details {
            control: self.control.clone(),
            search: self.search,
            ..Details::default()
        }
    }
}

/// A server of this kind, as a request is built for it.
pub fn server(address: &str, client: Client, libraries: Vec<String>, details: Details) -> Server {
    Server {
        kind: Kind::Dlna,
        address: address.to_string(),
        client,
        libraries,
        details,
    }
}

/// The request for a description. Short: a device that takes longer is not
/// one to wait for while a list is shown.
pub fn description_request(location: &str) -> Request {
    let mut request = Request::get(location.to_string(), Vec::new());
    request.timeout = Duration::from_secs(4);
    request
}

/// Reads one description.
pub fn describe(net: &dyn Net, location: &str) -> Result<Found, Failure> {
    let reply = net
        .send(&description_request(location))
        .map_err(Failure::Unreachable)?;
    match reply.status {
        200 => description(location, &reply.body),
        300..=399 => Err(Failure::moved(
            reply
                .headers
                .get("location")
                .map(String::as_str)
                .unwrap_or(""),
        )),
        status => Err(not_a_media_server_status(status)),
    }
}

/// What a description says, when it is a media server's and its
/// ContentDirectory is on the host that answered.
pub fn description(location: &str, body: &[u8]) -> Result<Found, Failure> {
    let text = std::str::from_utf8(body).map_err(|_| not_a_media_server())?;
    let doc = parse(text).ok_or_else(not_a_media_server)?;
    let device = doc
        .descendants()
        .find(|n| {
            n.has_tag_name("device") && field(*n, "deviceType").contains(":device:MediaServer:")
        })
        .ok_or_else(not_a_media_server)?;
    let base = doc
        .descendants()
        .find(|n| n.has_tag_name("URLBase"))
        .and_then(|n| n.text())
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .unwrap_or(location);
    let control = device
        .descendants()
        .filter(|n| {
            n.has_tag_name("service")
                && field(*n, "serviceType")
                    .starts_with("urn:schemas-upnp-org:service:ContentDirectory:")
        })
        .find_map(|n| resolve(base, &field(n, "controlURL")).and_then(|url| kept(location, &url)))
        .ok_or_else(not_a_media_server)?;
    let identity = field(device, "UDN");
    if identity.is_empty() {
        return Err(not_a_media_server());
    }
    let product = format!(
        "{} {}",
        field(device, "modelName"),
        field(device, "modelNumber")
    )
    .trim()
    .to_string();
    Ok(Found {
        location: location.to_string(),
        name: field(device, "friendlyName"),
        product,
        identity,
        control,
        search: false,
    })
}

/// Finds the media server at an address: its description, or a host and a
/// port, where the usual places are tried. Then asks whether it searches.
pub fn probe(net: &dyn Net, address: &str) -> Result<Found, Failure> {
    let path = address
        .split_once("://")
        .map_or("", |(_, rest)| rest.find('/').map_or("", |at| &rest[at..]));
    let tried: Vec<String> = if path.len() > 1 {
        vec![address.to_string()]
    } else {
        DESCRIPTIONS
            .iter()
            .map(|place| format!("{}{place}", address.trim_end_matches('/')))
            .collect()
    };
    let mut failure = not_a_media_server();
    for location in tried {
        match describe(net, &location) {
            Ok(mut found) => {
                found.search = searches(net, &found);
                return Ok(found);
            }
            // Nothing answers there: no other path will.
            Err(f) if f.is_unreachable() => return Err(f),
            Err(f) => failure = f,
        }
    }
    Err(failure)
}

/// Whether the server takes `Search` by class. Asked once, when it is
/// added; a server that cannot say is walked.
fn searches(net: &dyn Net, found: &Found) -> bool {
    let Ok(text) = call(
        net,
        &absolute(&found.location, &found.control),
        "GetSearchCapabilities",
        "",
    ) else {
        return false;
    };
    let Some(doc) = parse(&text) else {
        return false;
    };
    let caps = doc
        .descendants()
        .find(|n| n.has_tag_name("SearchCaps"))
        .and_then(|n| n.text())
        .unwrap_or("");
    caps.split(',')
        .any(|c| matches!(c.trim(), "upnp:class" | "*"))
}

/// Whether the server answers.
pub fn ping(net: &dyn Net, server: &Server) -> Result<(), Failure> {
    call(net, &control(server), "GetSystemUpdateID", "").map(|_| ())
}

/// The containers in a container, to choose which come in: the first page
/// of them.
pub fn folders(net: &dyn Net, server: &Server, id: &str) -> Result<Vec<Library>, Failure> {
    let answer = answer(&call(net, &control(server), "Browse", &browse(id, 0))?)?;
    Ok(listing(server, &answer.result, None, None)?
        .folders
        .into_iter()
        .map(|f| Library {
            id: f.id,
            name: f.title,
        })
        .collect())
}

/// Where a sync stands in a server's containers.
#[derive(Serialize, Deserialize, Default)]
struct Cursor {
    /// The containers chosen, and which one is being read.
    scopes: Vec<String>,
    scope: usize,
    /// The container being read, how deep it is below its scope, the
    /// container its records are grouped under (`grouped_under`), and its
    /// art, for the tracks in it that name none.
    current: Option<(String, u8, String, String)>,
    /// Where its next page starts, and what its pages counted.
    offset: u64,
    total: u64,
    /// Containers met and not read yet, as `current`.
    waiting: Vec<(String, u8, String, String)>,
    /// `Search` is not used: each scope is walked.
    walk: bool,
    /// A fingerprint of the last page of the container being read. A server
    /// that answers the next page with the same one is not moving.
    #[serde(default)]
    last: u64,
}

impl Cursor {
    /// The next container to read: the last one met, then the next scope.
    fn advance(&mut self) {
        self.offset = 0;
        self.total = 0;
        self.last = 0;
        self.current = self.waiting.pop();
        if self.current.is_none() && self.scope + 1 < self.scopes.len() {
            self.scope += 1;
            self.current = self.scopes.get(self.scope).map(|scope| top(scope));
        }
    }

    fn onward(&self) -> Option<String> {
        self.current.as_ref().and_then(|_| serde_json::to_string(self).ok())
    }
}

/// A chosen container, as a walk starts reading it.
fn top(scope: &str) -> (String, u8, String, String) {
    (scope.to_string(), 0, scope.to_string(), String::new())
}

/// How long a walk waits before asking again for a container that did not
/// come: once, then once more.
const RETRIES: [Duration; 2] = [Duration::from_millis(500), Duration::from_secs(2)];

/// One page of the containers chosen. `None` starts at the first; nothing
/// chosen is the whole server.
///
/// A container that cannot be read is asked for again, twice. If it still
/// does not come and the server answers otherwise, it is left out, the
/// page says so (`Page::skipped`), and the walk goes on with the next one:
/// one folder the server cannot give does not end the sync.
pub fn page(net: &dyn Net, server: &Server, cursor: Option<&str>) -> Result<Page, Failure> {
    let mut cursor: Cursor = match cursor {
        Some(text) => serde_json::from_str(text).map_err(|_| {
            Failure::Unexpected(chilpa_i18n::t("the sync lost its place; start it again"))
        })?,
        None => {
            let scopes = if server.libraries.is_empty() {
                vec![ROOT.to_string()]
            } else {
                server.libraries.clone()
            };
            let current = scopes.first().map(|scope| top(scope));
            Cursor {
                scopes,
                current,
                walk: !server.details.search,
                ..Cursor::default()
            }
        }
    };
    let (Some(scope), Some((container, depth, group, art))) = (
        cursor.scopes.get(cursor.scope).cloned(),
        cursor.current.clone(),
    ) else {
        return Ok(Page::default());
    };
    // Each page counts its own container, and the walk repeats a track
    // across views: the sync cannot add the counts up, so no total is said.
    let mut page = Page {
        scope,
        ..Page::default()
    };
    let (action, args) = if cursor.walk {
        ("Browse", browse(&container, cursor.offset))
    } else {
        ("Search", search(&container, cursor.offset))
    };
    // A walk knows which container it reads; a search gives items from
    // anywhere below the scope.
    let regroup = cursor.walk.then_some((container.as_str(), group.as_str()));
    let ask = || -> Result<(Answer, Listing), Failure> {
        let answer = answer(&call(net, &control(server), action, &args)?)?;
        let listing = listing(server, &answer.result, regroup, Some(art.as_str()))?;
        Ok((answer, listing))
    };
    let at = cursor.offset;
    let read = if cursor.walk {
        let mut tries = 0;
        loop {
            match ask() {
                Ok(read) => break Some(read),
                Err(failure) if tries < RETRIES.len() => {
                    eprintln!(
                        "chilpa-servers: media server container {container}: Browse at StartingIndex {at} failed ({failure}); asked again in {:?}",
                        RETRIES[tries]
                    );
                    std::thread::sleep(RETRIES[tries]);
                    tries += 1;
                }
                Err(failure) => {
                    // A server that no longer answers ends the sync; one that
                    // answers, and cannot give this container, leaves it out.
                    if failure.is_unreachable() && ping(net, server).is_err() {
                        return Err(failure);
                    }
                    eprintln!(
                        "chilpa-servers: media server container {container} left out after {} tries at StartingIndex {at}: {failure}",
                        tries + 1
                    );
                    break None;
                }
            }
        }
    } else {
        match ask() {
            Ok(read) => Some(read),
            // A server that announced `Search` and then refuses it is
            // walked, from the top of the scope.
            Err(Failure::Unexpected(_)) => {
                cursor.walk = true;
                cursor.offset = 0;
                cursor.total = 0;
                cursor.waiting.clear();
                cursor.current = Some(top(&page.scope));
                page.next = cursor.onward();
                return Ok(page);
            }
            Err(failure) => return Err(failure),
        }
    };
    let Some((answer, listing)) = read else {
        page.partial = true;
        page.skipped = 1;
        cursor.advance();
        page.next = cursor.onward();
        return Ok(page);
    };
    // What each odd answer was, for the log: a sync that ends short says
    // where.
    let said = |why: &str| {
        eprintln!(
            "chilpa-servers: media server container {container}: {why} (StartingIndex {at}, NumberReturned {}, TotalMatches {})",
            answer.returned, answer.total
        );
    };
    // The same page again where the next one was asked for: reading on would
    // ask for it until a cap, and nothing new would come.
    let print = fingerprint(&answer.result);
    let repeated = at > 0 && answer.returned > 0 && print == cursor.last;
    if repeated {
        said("the page before came again; the container is left there");
        page.partial = true;
    }
    // A count that changes between pages: the container changed while it
    // was read.
    if at > 0 && answer.total != cursor.total {
        said("its count changed while it was read");
        page.partial = true;
    }
    cursor.total = answer.total;
    if cursor.walk && !repeated {
        for folder in listing.folders {
            if depth >= DEPTH || cursor.waiting.len() >= WAITING {
                said("containers below it were left out: too deep, or too many waiting");
                page.partial = true;
                break;
            }
            let under = grouped_under(&folder, &group);
            // A container without art of its own shows the one above it.
            let shown = if folder.art.is_empty() { art.clone() } else { folder.art };
            cursor.waiting.push((folder.id, depth + 1, under, shown));
        }
    }
    if !repeated {
        page.tracks = listing.tracks;
    }
    let read = at + answer.returned;
    // One that does not count is read until a page brings nothing: a page
    // shorter than asked is not its end, some servers give less at a time.
    let mut more = !repeated && answer.returned > 0 && (answer.total == 0 || read < answer.total);
    if more && read >= ENTRIES {
        said("it holds more entries than a container is read for");
        more = false;
        page.partial = true;
    }
    // A container that does not count its entries, or gives fewer than it
    // counts, cannot be shown to have been read whole.
    if answer.total == 0 && answer.returned > 0 {
        if at == 0 {
            said("it does not count its entries");
        }
        page.partial = true;
    }
    if !more && !repeated && read < answer.total {
        said("it gave fewer entries than it counts");
        page.partial = true;
    }
    if more {
        said("read in pages");
        cursor.offset = read;
        cursor.last = print;
    } else {
        cursor.advance();
    }
    page.next = cursor.onward();
    Ok(page)
}

/// Where a track's bytes are: the `res` its revision keeps. Without one the
/// description is named, and the reader refuses that as not audio.
pub fn stream(server: &Server, codec: Option<&str>, revision: Option<&str>) -> Stream {
    let url = revision.and_then(|r| r.split_once(' ')).map_or_else(
        || server.address.clone(),
        |(_, kept)| absolute(&server.address, kept),
    );
    Stream {
        url,
        headers: Vec::new(),
        extension: codec.and_then(extension_of).map(str::to_string),
    }
}

/// The request for a record's cover. The server has one size of it.
pub fn cover(server: &Server, cover: &str) -> Request {
    image(absolute(&server.address, cover), Vec::new())
}

/// Sends one action to the ContentDirectory and returns the answer's text.
fn call(net: &dyn Net, control: &str, action: &str, args: &str) -> Result<String, Failure> {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} xmlns:u=\"{SERVICE}\">{args}\
         </u:{action}></s:Body></s:Envelope>"
    );
    let headers = vec![("SOAPAction".to_string(), format!("\"{SERVICE}#{action}\""))];
    let kind = "text/xml; charset=\"utf-8\"".to_string();
    let reply = net
        .send(&Request::post(
            control.to_string(),
            headers,
            Some((kind, body.into_bytes())),
        ))
        .map_err(Failure::Unreachable)?;
    match reply.status {
        200..=299 => String::from_utf8(reply.body).map_err(|_| not_a_listing()),
        300..=399 => Err(Failure::moved(
            reply
                .headers
                .get("location")
                .map(String::as_str)
                .unwrap_or(""),
        )),
        401 | 403 => Err(Failure::Unexpected(chilpa_i18n::t(
            "the media server does not let this computer in; allow it in its own settings",
        ))),
        500 => Err(fault(&reply.body)),
        status => Err(not_a_media_server_status(status)),
    }
}

/// A SOAP fault, by the code UPnP gave it.
fn fault(body: &[u8]) -> Failure {
    let text = String::from_utf8_lossy(body);
    let mut code = String::new();
    if let Some(doc) = parse(&text) {
        if let Some(found) = doc
            .descendants()
            .find(|n| n.has_tag_name("errorCode"))
            .and_then(|n| n.text())
        {
            code = found.trim().to_string();
        }
    }
    if code.is_empty() || code.len() > 8 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return not_a_media_server_status(500);
    }
    Failure::Unexpected(chilpa_i18n::t_args(
        "the media server refused the request (UPnP error {})",
        &[&code],
    ))
}

/// What a `Browse` or a `Search` answered.
struct Answer {
    /// The DIDL-Lite document, as text.
    result: String,
    returned: u64,
    /// What the container counts. Zero when it does not say.
    total: u64,
}

fn answer(text: &str) -> Result<Answer, Failure> {
    let doc = parse(text).ok_or_else(not_a_listing)?;
    let value = |name: &str| {
        doc.descendants()
            .find(|n| n.has_tag_name(name))
            .and_then(|n| n.text())
            .map(|t| t.trim().to_string())
    };
    let result = doc
        .descendants()
        .find(|n| n.has_tag_name("Result"))
        .ok_or_else(not_a_listing)?;
    let result = result.text().unwrap_or("").to_string();
    let returned = value("NumberReturned")
        .and_then(|n| n.parse().ok())
        .ok_or_else(not_a_listing)?;
    let total = value("TotalMatches")
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    Ok(Answer {
        result,
        returned,
        total,
    })
}

fn browse(id: &str, start: u64) -> String {
    format!(
        "<ObjectID>{}</ObjectID><BrowseFlag>BrowseDirectChildren</BrowseFlag><Filter>*</Filter>\
         <StartingIndex>{start}</StartingIndex><RequestedCount>{PAGE}</RequestedCount><SortCriteria></SortCriteria>",
        escape(id)
    )
}

fn search(id: &str, start: u64) -> String {
    format!(
        "<ContainerID>{}</ContainerID><SearchCriteria>{}</SearchCriteria><Filter>*</Filter>\
         <StartingIndex>{start}</StartingIndex><RequestedCount>{PAGE}</RequestedCount><SortCriteria></SortCriteria>",
        escape(id),
        escape(AUDIO)
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// A container a page listed.
struct Folder {
    id: String,
    title: String,
    /// Its `upnp:albumArtURI`, as kept (`kept`); empty when it has none.
    art: String,
}

/// What one page listed: containers and the tracks among its items.
#[derive(Default)]
struct Listing {
    folders: Vec<Folder>,
    tracks: Vec<Track>,
}

/// The container a walk groups the records of `folder` under: the folder
/// itself, or, when it is one disc of a set ("CD1", "Disc 2 - Live"), the
/// one its parent is grouped under. The scan of local files reads a disc's
/// folder the same way (`chilpa_library::album_folder`), so the two discs
/// of a record are one record here too.
fn grouped_under(folder: &Folder, parent: &str) -> String {
    if is_disc(&folder.title) {
        parent.to_string()
    } else {
        folder.id.clone()
    }
}

/// A container named for one disc: a disc word, its number, and at most a
/// subtitle after a separator. "Now 75 - CD1" is a record, not a disc.
fn is_disc(title: &str) -> bool {
    let lower = title.trim().to_lowercase();
    // Longest first, so "disco" is not read as "disc" and an "o".
    let Some(rest) = ["disco", "disc", "disk", "cd"]
        .into_iter()
        .find_map(|word| lower.strip_prefix(word))
    else {
        return false;
    };
    let rest = rest.trim_start_matches([' ', '_', '-', '.']);
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || rest[..digits].parse::<u16>().map_or(true, |n| n == 0) {
        return false;
    }
    let tail = rest[digits..].trim_start();
    tail.is_empty() || tail.starts_with(['-', '\u{2013}', ':', '(', '['])
}

/// What a page lists. `regroup` is the container a walk reads and the one
/// its records are grouped under, when they differ (`grouped_under`);
/// `art` is the container's art, for a track that names none.
fn listing(
    server: &Server,
    didl: &str,
    regroup: Option<(&str, &str)>,
    art: Option<&str>,
) -> Result<Listing, Failure> {
    let mut out = Listing::default();
    if didl.trim().is_empty() {
        return Ok(out);
    }
    let text = declare_missing(didl);
    let doc = parse(&text).ok_or_else(not_a_listing)?;
    for node in doc.root_element().children().filter(|n| n.is_element()) {
        if node.has_tag_name("container") {
            if let Some(id) = node
                .attribute("id")
                .map(str::trim)
                .filter(|id| !id.is_empty())
            {
                out.folders.push(Folder {
                    id: id.to_string(),
                    title: field(node, "title"),
                    art: art_of(server, node).unwrap_or_default(),
                });
            }
        } else if node.has_tag_name("item") {
            out.tracks.extend(track(server, node, regroup, art));
        }
    }
    Ok(out)
}

/// An item, when it is a track with an original Chilpa can read.
fn track(
    server: &Server,
    node: roxmltree::Node<'_, '_>,
    regroup: Option<(&str, &str)>,
    art: Option<&str>,
) -> Option<Track> {
    let class = field(node, "class");
    if !class.starts_with("object.item.audioItem")
        || class.starts_with("object.item.audioItem.audioBroadcast")
    {
        return None;
    }
    let reference = node
        .attribute("refID")
        .map(str::trim)
        .filter(|r| !r.is_empty());
    let id = reference
        .or_else(|| node.attribute("id").map(str::trim))
        .filter(|id| !id.is_empty())?
        .to_string();
    let original = node
        .children()
        .filter(|n| n.has_tag_name("res"))
        .find_map(|res| original(server, res))?;
    let names = performers(node);
    let artist = if names.is_empty() {
        field(node, "creator")
    } else {
        artist_line(&names)
    };
    let album = field(node, "album");
    let album_artist = node
        .children()
        .find(|n| {
            n.has_tag_name("albumArtist")
                || (n.has_tag_name("artist")
                    && n.attribute("role")
                        .is_some_and(|r| r.trim().eq_ignore_ascii_case("AlbumArtist")))
        })
        .and_then(|n| n.text())
        .map(|t| t.trim().to_string())
        .unwrap_or_default();
    // A record is grouped the way the library groups local files: its
    // title in the folder that holds it, the container here, and a disc's
    // folder stands for the one above it (`grouped_under`). A reference sits
    // in a view the server made up, not a folder, so there the record artist
    // decides, when the server says it: an empty id is the library's own
    // title and album artist.
    let parent = node.attribute("parentID").unwrap_or("").trim();
    let parent = match regroup {
        Some((container, under)) if parent == container => under,
        _ => parent,
    };
    let album_id = if album.is_empty() || (reference.is_some() && !album_artist.is_empty()) {
        String::new()
    } else {
        format!("{parent}\u{1f}{album}")
    };
    let mut genres: Vec<String> = Vec::new();
    for genre in node
        .children()
        .filter(|n| n.has_tag_name("genre"))
        .filter_map(|n| n.text())
        .map(str::trim)
    {
        // Plex Media Server writes "Unknown" for a track without one.
        if !genre.is_empty()
            && !genre.eq_ignore_ascii_case("Unknown")
            && !genres.iter().any(|g| g == genre)
        {
            genres.push(genre.to_string());
        }
    }
    // Its own art, or the container's when it names none.
    let cover = art_of(server, node)
        .or_else(|| art.filter(|a| !a.is_empty()).map(str::to_string));
    let mut track = Track {
        id,
        title: field(node, "title"),
        artist,
        artists: if names.len() > 1 { names } else { Vec::new() },
        album,
        album_artist,
        album_id,
        cover,
        disc: number(&field(node, "originalDiscNumber")),
        discs: None,
        number: number(&field(node, "originalTrackNumber")),
        year: year(&field(node, "date")),
        genres,
        duration_ms: original.duration_ms,
        codec: original.codec.clone(),
        bits: original.bits,
        rate: original.rate,
        channels: original.channels,
        isrc: None,
        recording_mbid: None,
        revision: None,
        added_at: None,
        complete: true,
    };
    let stamp = fingerprint(&format!(
        "{track:?}\u{1f}{:?}\u{1f}{}",
        original.size, original.url
    ));
    track.revision = Some(format!("{stamp:016x} {}", original.url));
    Some(track)
}

/// The art an item or a container names, as kept (`kept`): its
/// `upnp:albumArtURI`, or else a `res` that is an image, the way some
/// servers give a cover. Only on the server's own host.
fn art_of(server: &Server, node: roxmltree::Node<'_, '_>) -> Option<String> {
    let on_host = |uri: &str| resolve(&server.address, uri).and_then(|url| kept(&server.address, &url));
    let named = node
        .children()
        .filter(|n| n.has_tag_name("albumArtURI"))
        .filter_map(|n| n.text())
        .find_map(&on_host);
    named.or_else(|| {
        node.children()
            .filter(|n| n.has_tag_name("res"))
            .filter(|res| {
                res.attribute("protocolInfo")
                    .and_then(|info| info.split(':').nth(2))
                    .is_some_and(|mime| mime.trim().to_ascii_lowercase().starts_with("image/"))
            })
            .filter_map(|res| res.text())
            .find_map(&on_host)
    })
}

/// The performers an item names: its artists without a role, or with the
/// role of one who plays. A composer or a record artist is not one.
fn performers(node: roxmltree::Node<'_, '_>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for artist in node.children().filter(|n| n.has_tag_name("artist")) {
        let role = artist
            .attribute("role")
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if !matches!(role.as_str(), "" | "performer" | "artist") {
            continue;
        }
        if let Some(name) = artist.text().map(str::trim).filter(|n| !n.is_empty()) {
            if !names.iter().any(|known| known == name) {
                names.push(name.to_string());
            }
        }
    }
    names
}

/// An original among an item's `res`.
struct Original {
    /// As kept: a path on the server, or a whole address on its host.
    url: String,
    codec: Option<String>,
    duration_ms: u64,
    rate: Option<u32>,
    bits: Option<u8>,
    channels: Option<u16>,
    size: Option<u64>,
}

fn original(server: &Server, res: roxmltree::Node<'_, '_>) -> Option<Original> {
    let info = res.attribute("protocolInfo")?;
    let mut parts = info.splitn(4, ':');
    let (protocol, _network, mime, flags) = (
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next().unwrap_or(""),
    );
    // Converted on the fly is never the original, whatever else it says.
    if !protocol.trim().eq_ignore_ascii_case("http-get")
        || flags
            .split(';')
            .any(|flag| flag.trim().eq_ignore_ascii_case("DLNA.ORG_CI=1"))
    {
        return None;
    }
    let codec = codec_of(mime)?;
    let url = resolve(&server.address, res.text()?).and_then(|url| kept(&server.address, &url))?;
    let attribute = |name: &str| {
        res.attribute(name)
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|n| *n > 0)
    };
    Some(Original {
        url,
        codec: codec.map(str::to_string),
        duration_ms: res.attribute("duration").and_then(duration_ms).unwrap_or(0),
        rate: attribute("sampleFrequency").and_then(|n| u32::try_from(n).ok()),
        bits: attribute("bitsPerSample").and_then(|n| u8::try_from(n).ok()),
        channels: attribute("nrAudioChannels").and_then(|n| u16::try_from(n).ok()),
        size: attribute("size"),
    })
}

/// What a `res` type says of its file. `None` when Chilpa does not read it
/// as it is: not audio, raw PCM a renderer is fed, or DSD, which a server's
/// track cannot play yet (ADR-017 §10). Otherwise its codec, when the type
/// names one.
fn codec_of(mime: &str) -> Option<Option<&'static str>> {
    let mime = mime
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    Some(match mime.as_str() {
        "audio/flac" | "audio/x-flac" | "application/flac" | "application/x-flac" => Some("flac"),
        "audio/mpeg" | "audio/mp3" | "audio/x-mp3" | "audio/mpeg3" => Some("mp3"),
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => Some("m4a"),
        "audio/ogg" | "audio/x-ogg" | "application/ogg" | "audio/vorbis" => Some("ogg"),
        "audio/opus" => Some("opus"),
        "audio/wav" | "audio/x-wav" | "audio/wave" | "audio/vnd.wave" => Some("wav"),
        "audio/aiff" | "audio/x-aiff" => Some("aiff"),
        "audio/x-wavpack" | "audio/wavpack" => Some("wavpack"),
        "audio/x-ape" | "audio/ape" | "audio/x-monkeys-audio" => Some("ape"),
        "audio/l8" | "audio/l16" | "audio/l24" | "audio/lpcm" => return None,
        "audio/x-dsf" | "audio/dsf" | "audio/x-dff" | "audio/dff" | "audio/x-dsd" | "audio/dsd" => {
            return None
        }
        other if other.starts_with("audio/") => None,
        _ => return None,
    })
}

/// `H+:MM:SS[.F+]`, as `res@duration` writes it, in milliseconds.
fn duration_ms(text: &str) -> Option<u64> {
    let mut parts = text.trim().split(':');
    let (hours, minutes, seconds) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let (hours, minutes): (u64, u64) = (hours.parse().ok()?, minutes.parse().ok()?);
    let (whole, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    let whole: u64 = whole.parse().ok()?;
    if hours > 1_000 || minutes > 59 || whole > 59 {
        return None;
    }
    // Decimal, or F0/F1: only its first three digits count.
    let digits: String = fraction
        .chars()
        .take_while(char::is_ascii_digit)
        .take(3)
        .collect();
    let millis = format!("{digits:0<3}").parse::<u64>().unwrap_or(0);
    Some(((hours * 60 + minutes) * 60 + whole) * 1_000 + millis)
}

fn number(text: &str) -> Option<u16> {
    text.parse::<u16>().ok().filter(|n| *n > 0)
}

/// The year of a `dc:date` ("2016-01-08", "2016").
fn year(text: &str) -> Option<u32> {
    text.get(..4)
        .and_then(|y| y.parse::<u32>().ok())
        .filter(|y| (1000..=9999).contains(y))
}

/// A child's text, trimmed; empty when it has none.
fn field(node: roxmltree::Node<'_, '_>, name: &str) -> String {
    node.children()
        .find(|n| n.has_tag_name(name))
        .and_then(|n| n.text())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn parse(text: &str) -> Option<roxmltree::Document<'_>> {
    let options = roxmltree::ParsingOptions {
        nodes_limit: NODES,
        ..roxmltree::ParsingOptions::default()
    };
    roxmltree::Document::parse_with_options(text, options).ok()
}

/// The DIDL-Lite with the usual prefixes declared on its root, when it uses
/// them and they are missing there.
fn declare_missing(didl: &str) -> Cow<'_, str> {
    let Some(start) = didl.find("<DIDL-Lite") else {
        return Cow::Borrowed(didl);
    };
    let open = start + "<DIDL-Lite".len();
    let root = didl[open..].split('>').next().unwrap_or("");
    let missing: String = PREFIXES
        .iter()
        .filter(|(prefix, _)| {
            !root.contains(&format!("xmlns:{prefix}=")) && didl.contains(&format!("{prefix}:"))
        })
        .map(|(prefix, uri)| format!(" xmlns:{prefix}=\"{uri}\""))
        .collect();
    if missing.is_empty() {
        return Cow::Borrowed(didl);
    }
    Cow::Owned(format!("{}{missing}{}", &didl[..open], &didl[open..]))
}

/// FNV-1a: a revision only has to change when what it covers does.
fn fingerprint(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Where the ContentDirectory of a stored server takes requests.
fn control(server: &Server) -> String {
    absolute(&server.address, &server.details.control)
}

/// Scheme, host and port of an address: `http://host:port`.
fn origin(address: &str) -> Option<&str> {
    let (scheme, rest) = address.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    (end > 0 && !scheme.is_empty()).then(|| &address[..scheme.len() + 3 + end])
}

/// The host of an address, in lower case, without its port.
fn host(address: &str) -> Option<String> {
    let authority = origin(address)?.split_once("://")?.1;
    if authority.contains('@') {
        return None;
    }
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next()?,
        None => authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host),
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// An address a description or a listing names, whole. A path, with or
/// without its leading slash (older devices leave it out), is on the base's
/// scheme and host.
fn resolve(base: &str, named: &str) -> Option<String> {
    let named = named.trim();
    if named.is_empty() || named.len() > 4096 || named.contains(char::is_whitespace) {
        return None;
    }
    if named.contains("://") {
        return Some(named.to_string());
    }
    let origin = origin(base)?;
    Some(if named.starts_with('/') {
        format!("{origin}{named}")
    } else {
        format!("{origin}/{named}")
    })
}

/// What is kept of an address a server named: its path when it is on the
/// description's scheme, host and port, the whole address when only the
/// port differs, and nothing when it points at another host. A server on
/// the LAN does not get to send this machine anywhere else.
fn kept(location: &str, url: &str) -> Option<String> {
    if !(url.starts_with("http://") || url.starts_with("https://"))
        || url.contains(char::is_whitespace)
    {
        return None;
    }
    let (here, there) = (origin(location)?, origin(url)?);
    if host(here)? != host(there)? {
        return None;
    }
    if !here.eq_ignore_ascii_case(there) {
        return Some(url.to_string());
    }
    let rest = &url[there.len()..];
    Some(if rest.starts_with('/') {
        rest.to_string()
    } else {
        format!("/{rest}")
    })
}

/// An address `kept` keeps, whole again.
fn absolute(address: &str, kept: &str) -> String {
    match origin(address) {
        Some(origin) if kept.starts_with('/') => format!("{origin}{kept}"),
        _ => kept.to_string(),
    }
}

fn not_a_media_server() -> Failure {
    Failure::Unexpected(chilpa_i18n::t(
        "that address does not describe a DLNA media server",
    ))
}

fn not_a_media_server_status(status: u16) -> Failure {
    Failure::Unexpected(chilpa_i18n::t_args(
        "that address does not answer like a DLNA media server (HTTP {})",
        &[&status.to_string()],
    ))
}

fn not_a_listing() -> Failure {
    Failure::Unexpected(chilpa_i18n::t(
        "the media server answered with something that is not a listing",
    ))
}

#[cfg(test)]
mod tests;
