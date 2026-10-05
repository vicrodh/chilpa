//! The rows of the search flyout, and what each one offers.
//!
//! What was found is laid out in sections here, and the menu of a row and
//! what each of its entries does are decided here too, so a window only
//! draws the rows and goes where it is told.

use crate::{track_menu, Album, Artist, Chilpa, Found, TrackMenuContext, TrackOrder, TrackRow};
use chilpa_i18n as i18n;
use serde_json::{json, Value};

/// A row of the flyout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FoundRow {
    Header(String),
    /// The artist found first, drawn larger.
    Top(Artist),
    Artist(Artist),
    Album(Album),
    Playlist(crate::Playlist),
    Track(TrackRow),
    Remote(crate::source_search::RemoteFound),
    /// The way to the rest of a source's results, after its first few:
    /// which source, and how many more there are.
    More(String, usize),
}

/// Where an action takes the window, when it takes it anywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Leads {
    Nowhere,
    Source(String),
    Screen(&'static str, i64),
    /// The same, in a tab of its own.
    Tab(&'static str, i64),
    Artist(String),
    /// One of the artists a credit names, by its place in it.
    Credit(String, i32),
    Folder(String),
}

impl FoundRow {
    /// What a window tells the rows apart by: 0 a header, 1 the top
    /// result, 2 an artist, 3 a record, 4 a song.
    pub fn kind(&self) -> i32 {
        match self {
            FoundRow::Remote(row) => match row.entity.as_str() { "artist" => 2, "playlist" => 6, _ => if row.collection { 3 } else { 4 } },
            FoundRow::More(..) => 5,
            FoundRow::Header(_) => 0,
            FoundRow::Top(_) => 1,
            FoundRow::Artist(_) => 2,
            FoundRow::Album(_) => 3,
            FoundRow::Playlist(_) => 6,
            FoundRow::Track(_) => 4,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            FoundRow::Remote(row) => &row.title,
            FoundRow::Header(label) => label,
            FoundRow::More(..) => "",
            FoundRow::Top(a) | FoundRow::Artist(a) => &a.name,
            FoundRow::Album(a) => &a.title,
            FoundRow::Playlist(p) => &p.name,
            FoundRow::Track(t) => &t.title,
        }
    }

    pub fn detail(&self) -> String {
        match self {
            FoundRow::Remote(row) => row.detail.clone(),
            FoundRow::Playlist(p) => i18n::tf("{} track", "{} tracks", p.track_count as i64, &[&p.track_count.to_string()]),
            FoundRow::More(_, more) => i18n::tf("See {} more", "See {} more", *more as i64, &[&more.to_string()]),
            FoundRow::Header(_) => String::new(),
            FoundRow::Top(a) | FoundRow::Artist(a) => {
                let records = i18n::tf("{} record", "{} records", a.album_count as i64, &[&a.album_count.to_string()]);
                format!("{} · {records}", i18n::t(i18n::mark("Artist")))
            }
            FoundRow::Album(a) => {
                let mut parts = vec![i18n::t(i18n::mark("Album")), a.artist.clone()];
                parts.extend(a.year.map(|y| y.to_string()));
                parts.join(" · ")
            }
            FoundRow::Track(t) => format!("{} — {}", t.artist, t.album),
        }
    }

    pub fn source_icon(&self) -> String {
        match self { Self::Remote(row) => crate::plugin_icons::image_url(&Chilpa::plugins_dir(), &row.source), _ => String::new() }
    }
    pub fn source_name(&self) -> String {
        match self { Self::Remote(row) => row.name.clone(), _ => String::new() }
    }
    /// Its cover, empty when it has none.
    pub fn artwork(&self) -> &str {
        match self {
            FoundRow::Remote(row) => &row.art,
            FoundRow::Header(_) | FoundRow::More(..) => "",
            FoundRow::Top(a) | FoundRow::Artist(a) => &a.artwork_path,
            FoundRow::Album(a) => &a.artwork_path,
            FoundRow::Playlist(p) => &p.artwork_path,
            FoundRow::Track(t) => &t.artwork_path,
        }
    }

    /// A song's length, and nothing for the rest.
    pub fn duration(&self) -> String {
        match self {
            FoundRow::Remote(row) if row.duration_ms > 0 => crate::format::clock(row.duration_ms),
            FoundRow::Track(t) => crate::format::clock(t.duration_ms),
            _ => String::new(),
        }
    }
}

/// The rows a scope shows of what was found: "artists", "albums" or
/// "songs" show one kind, and anything else a few of each.
pub fn found_rows(found: &Found, scope: &str) -> Vec<FoundRow> {
    let mut out = Vec::new();
    let mut section = |label: &str, rows: Vec<FoundRow>| {
        if !rows.is_empty() {
            out.push(FoundRow::Header(i18n::t(label)));
            out.extend(rows);
        }
    };
    match scope {
        "artists" => section(i18n::mark("Artists"), found.artists.iter().cloned().map(FoundRow::Artist).collect()),
        "albums" => section(i18n::mark("Albums"), found.albums.iter().cloned().map(FoundRow::Album).collect()),
        "songs" => section(i18n::mark("Songs"), found.tracks.iter().cloned().map(FoundRow::Track).collect()),
        _ => {
            section(i18n::mark("Top result"), found.artists.iter().take(1).cloned().map(FoundRow::Top).collect());
            section(i18n::mark("Albums"), found.albums.iter().take(3).cloned().map(FoundRow::Album).collect());
            section(i18n::mark("Songs"), found.tracks.iter().take(4).cloned().map(FoundRow::Track).collect());
        }
    }
    out
}

/// What a row offers. `names` are the artists a song's credit names and
/// `favorite` whether an artist or a record is one already.
pub fn found_menu(row: &FoundRow, names: Vec<String>, favorite: bool) -> Vec<Value> {
    let item = |act: &str, label: &str| json!({"act": act, "text": i18n::t(label)});
    let mark = |kind: &str, key: String| {
        let text = if favorite { i18n::mark("Remove from favorites") } else { i18n::mark("Add to favorites") };
        json!({"act": "mark", "kind": kind, "key": key, "on": !favorite, "text": i18n::t(text)})
    };
    let mut entries = match row {
        FoundRow::Playlist(_) => return vec![item("open", i18n::mark("Open")), item("play", i18n::mark("Play now")), item("queue", i18n::mark("Add to queue"))],
        FoundRow::More(..) => return Vec::new(),
        FoundRow::Remote(row) => return if row.collection {
            vec![item("open", i18n::mark("Open"))]
        } else {
            let metadata = row.item.as_ref().unwrap_or(&Value::Null);
            track_menu(TrackMenuContext {
                count: 1,
                album: metadata["album_ref"].as_str().is_some_and(|v| !v.is_empty()),
                artist: metadata["artist_ref"].as_str().is_some_and(|v| !v.is_empty()),
                unavailable: metadata["available"] == false,
                remote: true,
                ..Default::default()
            })
        },
        FoundRow::Header(_) => return Vec::new(),
        FoundRow::Track(t) => {
            return track_menu(TrackMenuContext {
                count: 1,
                album: t.album_id > 0,
                artist: !t.artist.is_empty(),
                artists: names,
                folder: !crate::on_a_server(&t.path),
                unavailable: !t.available,
                ..TrackMenuContext::default()
            })
        }
        FoundRow::Album(_) | FoundRow::Top(_) | FoundRow::Artist(_) => vec![
            item("play", i18n::mark("Play now")),
            item("next", i18n::mark("Play next")),
            item("queue", i18n::mark("Add to queue")),
            json!({"separator": true}),
            item("pick", i18n::mark("Add to playlist…")),
            json!({"separator": true}),
        ],
    };
    match row {
        FoundRow::Album(a) => {
            entries.push(item("open", i18n::mark("Open album")));
            entries.push(item("tab", i18n::mark("Open in new tab")));
            if !a.artist.is_empty() {
                entries.push(item("artist", i18n::mark("Go to artist")));
            }
            entries.push(json!({"separator": true}));
            entries.push(mark("album", a.id.to_string()));
        }
        FoundRow::Top(a) | FoundRow::Artist(a) => {
            entries.push(item("open", i18n::mark("Go to artist")));
            entries.push(json!({"separator": true}));
            entries.push(mark("artist", a.name.clone()));
        }
        _ => {}
    }
    entries
}

impl Chilpa {
    /// What the row offers, as the library stands.
    pub fn found_menu(&mut self, row: &FoundRow) -> Vec<Value> {
        let mut favorite = |kind: &str, key: &str| self.marks().is_some_and(|marks| marks.favorite(kind, key));
        let (names, favorite) = match row {
            FoundRow::Track(t) => (self.credit_names(&t.artist), false),
            FoundRow::Album(a) => (Vec::new(), favorite("album", &a.id.to_string())),
            FoundRow::Top(a) | FoundRow::Artist(a) => (Vec::new(), favorite("artist", &a.name)),
            FoundRow::Header(_) | FoundRow::Remote(_) | FoundRow::More(..) | FoundRow::Playlist(_) => (Vec::new(), false),
        };
        found_menu(row, names, favorite)
    }

    /// The tracks a row stands for: a song, a record's, or an artist's.
    fn found_tracks(&mut self, row: &FoundRow) -> Vec<TrackRow> {
        match row {
            FoundRow::Header(_) | FoundRow::Remote(_) | FoundRow::More(..) => Vec::new(),
            FoundRow::Track(t) => vec![t.clone()],
            FoundRow::Album(a) => self.album_track_rows(a.id),
            FoundRow::Playlist(p) => self.playlist_rows(p.id),
            FoundRow::Top(a) | FoundRow::Artist(a) => self.artist_tracks(&a.name, TrackOrder::Artist, 100_000),
        }
    }

    /// The ids of those tracks, for a playlist to take.
    pub fn found_track_ids(&mut self, row: &FoundRow) -> Vec<i64> {
        if let FoundRow::Remote(found) = row {
            if !found.collection {
                let id = self.catalog.adopt(&found.source, None, found);
                return if id != 0 { vec![id] } else { Vec::new() };
            }
        }
        self.found_tracks(row).into_iter().map(|t| t.id).collect()
    }

    /// Does what an entry of the menu of `rows[at]` says, and tells where
    /// it leads. "open" is the row's own click: an artist's page, a
    /// record's page, and a song plays with the songs listed after it.
    pub fn act_on_found(&mut self, rows: &[FoundRow], at: usize, act: &str) -> Leads {
        let Some(row) = rows.get(at) else { return Leads::Nowhere };
        if let FoundRow::Remote(row) = row { return self.act_on_remote_found(row, act); }
        if let FoundRow::More(source, _) = row {
            // The rest of that source's results, in place of its first few.
            self.search.expanded.insert(source.clone());
            return Leads::Nowhere;
        }
        match (act.split(':').next().unwrap_or_default(), row) {
            ("open" | "play", FoundRow::Track(t)) => {
                let songs: Vec<&TrackRow> = rows.iter().filter_map(|r| if let FoundRow::Track(t) = r { Some(t) } else { None }).collect();
                let start = songs.iter().position(|s| s.id == t.id).unwrap_or(0);
                let paths: Vec<String> = songs.iter().map(|s| s.path.clone()).collect();
                self.play_from(&paths, start);
            }
            ("open", FoundRow::Top(a) | FoundRow::Artist(a)) => return Leads::Artist(a.name.clone()),
            ("open", FoundRow::Playlist(p)) => return Leads::Screen("playlist", p.id),
            ("open", FoundRow::Album(a)) => return Leads::Screen("album", a.id),
            ("tab", FoundRow::Album(a)) => return Leads::Tab("album", a.id),
            ("album", FoundRow::Track(t)) if t.album_id > 0 => return Leads::Screen("album", t.album_id),
            ("artist", FoundRow::Album(a)) => return Leads::Artist(a.artist.clone()),
            ("artist", FoundRow::Track(t)) => {
                let place = act.split_once(':').and_then(|(_, place)| place.parse().ok()).unwrap_or(0);
                return Leads::Credit(t.artist.clone(), place);
            }
            ("folder", FoundRow::Track(t)) => {
                let dir = std::path::Path::new(&t.path).parent().map(|d| d.display().to_string());
                return dir.map(Leads::Folder).unwrap_or(Leads::Nowhere);
            }
            ("play" | "queue" | "next" | "later", row) => {
                let paths: Vec<String> = self.found_tracks(row).into_iter().map(|t| t.path).collect();
                match act {
                    "play" => self.play_from(&paths, 0),
                    "queue" => self.enqueue_here(&paths),
                    "next" => self.play_next_here(&paths),
                    _ => {
                        let label = match row {
                            FoundRow::Track(t) if !t.album.is_empty() => t.album.clone(),
                            FoundRow::Track(_) => i18n::t(i18n::mark("Play later")),
                            other => other.title().to_string(),
                        };
                        self.play_later_here(&paths, &label);
                    }
                }
            }
            _ => {}
        }
        Leads::Nowhere
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artist(name: &str) -> Artist {
        Artist { name: name.into(), album_count: 2, track_count: 9, artwork_path: String::new() }
    }

    fn album(id: i64, by: &str) -> Album {
        Album { id, title: format!("Record {id}"), artist: by.into(), year: Some(1999), track_count: 3, artwork_path: String::new(), play_count: 0 }
    }

    fn acts(menu: &[Value]) -> Vec<&str> {
        menu.iter().filter_map(|entry| entry["act"].as_str()).collect()
    }

    #[test]
    fn everything_shows_a_few_of_each_and_a_scope_shows_its_own() {
        let found = Found { artists: vec![artist("A"), artist("B")], albums: (1..=5).map(|id| album(id, "A")).collect(), tracks: Vec::new() };
        let kinds = |scope| found_rows(&found, scope).iter().map(FoundRow::kind).collect::<Vec<_>>();
        assert_eq!(kinds("all"), [0, 1, 0, 3, 3, 3], "one artist on top, three records, and no section for no songs");
        assert_eq!(kinds("artists"), [0, 2, 2]);
        assert_eq!(kinds("albums"), [0, 3, 3, 3, 3, 3]);
        assert!(kinds("songs").is_empty());
    }

    #[test]
    fn a_record_and_an_artist_offer_to_play_to_go_and_to_be_a_favorite() {
        let menu = found_menu(&FoundRow::Album(album(7, "A")), Vec::new(), false);
        assert_eq!(acts(&menu), ["play", "next", "queue", "pick", "open", "tab", "artist", "mark"]);
        let mark = menu.last().unwrap();
        assert_eq!((mark["kind"].as_str(), mark["key"].as_str(), mark["on"].as_bool()), (Some("album"), Some("7"), Some(true)));

        let menu = found_menu(&FoundRow::Top(artist("A")), Vec::new(), true);
        assert_eq!(acts(&menu), ["play", "next", "queue", "pick", "open", "mark"]);
        assert_eq!(menu.last().unwrap()["on"].as_bool(), Some(false), "a favorite offers to stop being one");
        assert!(found_menu(&FoundRow::Header("Songs".into()), Vec::new(), false).is_empty());
    }

    #[test]
    fn a_record_by_nobody_does_not_offer_its_artist() {
        let menu = found_menu(&FoundRow::Album(album(7, "")), Vec::new(), false);
        assert!(!acts(&menu).contains(&"artist"));
    }
}
