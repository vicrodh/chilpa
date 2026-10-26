use super::{Browser, PAGE};
use serde_json::{Value, json};

#[derive(Clone, Copy)]
pub(super) enum Item {
    Artist(usize),
    Album(usize),
    Track(usize),
}

impl Browser {
    pub(super) fn items(&self) -> Vec<Item> {
        let tracks = |indices: &[usize]| {
            indices
                .iter()
                .copied()
                .filter(|i| self.matches(*i))
                .map(Item::Track)
                .collect()
        };
        if self.playlist.is_some() {
            return tracks(&self.playlist_members);
        }
        if let Some(album) = self.album {
            return tracks(&self.albums[album].tracks);
        }
        if let Some(artist) = self.artist {
            return self.artists[artist]
                .albums
                .iter()
                .copied()
                .filter(|i| self.albums[*i].tracks.iter().any(|t| self.matches(*t)))
                .map(Item::Album)
                .collect();
        }
        match self.view {
            "artists" => self
                .artists
                .iter()
                .enumerate()
                .filter(|(_, a)| {
                    a.albums
                        .iter()
                        .any(|i| self.albums[*i].tracks.iter().any(|t| self.matches(*t)))
                })
                .map(|(i, _)| Item::Artist(i))
                .collect(),
            "tracks" | "playlists" => self
                .tracks
                .iter()
                .enumerate()
                .filter(|(i, t)| t.playlist == (self.view == "playlists") && self.matches(*i))
                .map(|(i, _)| Item::Track(i))
                .collect(),
            _ => self
                .albums
                .iter()
                .enumerate()
                .filter(|(_, a)| {
                    (self.view == "music" || !a.playlist)
                        && a.tracks.iter().any(|i| self.matches(*i))
                })
                .map(|(i, _)| Item::Album(i))
                .collect(),
        }
    }
    pub fn document(&self) -> Value {
        let items = self.items();
        let pages = items.len().div_ceil(PAGE).max(1);
        let page = self.page.min(pages - 1);
        let rows: Vec<_> = items.iter().enumerate().skip(page * PAGE).take(PAGE).map(|(position, item)| match *item {
            Item::Artist(i) => {
                let artist = &self.artists[i];
                let count = chilpa_i18n::tf("{} album", "{} albums", artist.albums.len() as i64, &[&artist.albums.len().to_string()]);
                json!({"id":format!("artist:{i}"),"title":if artist.name.is_empty(){chilpa_i18n::t("Unknown artist")}else{artist.name.clone()},"detail":count,"kind":"artist","open":true})
            }
            Item::Album(i) => {
                let album = &self.albums[i];
                let count = chilpa_i18n::tf("{} track", "{} tracks", album.tracks.len() as i64, &[&album.tracks.len().to_string()]);
                let detail = [album.artist.clone(),count].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join(" · ");
                json!({"id":format!("album:{i}"),"title":album.title,"detail":detail,"kind":"album","open":true})
            }
            Item::Track(i) => {
                let track = &self.tracks[i];
                let disc = if track.disc > 0 { chilpa_i18n::t_args("Disc {}", &[&track.disc.to_string()]) } else { String::new() };
                let album = if self.album.is_none() || self.playlist.is_some() { track.album.clone() } else { String::new() };
                let detail = [track.artist.clone(),album,disc,track.size.clone()].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join(" · ");
                let id = if self.playlist.is_some() {format!("member:{position}")} else {format!("{}:{i}",if track.playlist{"playlist"}else{"track"})};
                json!({"id":id,"title":track.title,"detail":detail,
                    "path":track.path,"number":if self.playlist.is_some(){position as u32 + 1}else{track.number},"kind":if track.playlist{"playlist"}else{"track"},"open":track.playlist && self.recorded && self.playlist.is_none()})
            }
        }).collect();
        let location = [
            self.artist.map(|i| self.artists[i].name.as_str()),
            self.album.map(|i| self.albums[i].title.as_str()),
            self.playlist.map(|i| self.tracks[i].title.as_str()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" › ");
        json!({"source":self.source,"recorded":self.recorded,"title":self.title,"view":self.view,"artist":self.artist.map(|i|&self.artists[i].name),"album":if location.is_empty(){None}else{Some(location)},
            "can_back":self.artist.is_some() || self.album.is_some() || self.playlist.is_some() || !self.recorded,"rows":rows,"page":page,"pages":pages,"total":items.len(),
            "level":if self.album.is_some() || self.playlist.is_some() || ["tracks","playlists"].contains(&self.view){"tracks"}else{"albums"},"query":self.query,
            "notice":if self.recorded {chilpa_i18n::t("Last synced with Chilpa. Files changed elsewhere may differ.")}else{String::new()}})
    }
}
