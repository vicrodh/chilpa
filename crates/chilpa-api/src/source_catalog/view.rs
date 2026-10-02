//! What the window reads of the catalog, in the shapes it already reads
//! for the library: records as `Album`, tracks as `TrackRow`.

use super::store::{Card, CardKind, Track};
use super::split;
use crate::album_versions::{AlbumSource, AlbumVersion};
use crate::{Album, Chilpa, TrackRow};

/// How many folders of a discover route become shelves of their own.
const EXPANDED: usize = 6;

pub(crate) fn album_of(card: &Card) -> Album {
    Album {
        id: card.id,
        title: card.title.clone(),
        artist: card.artist.clone(),
        year: card.year,
        track_count: card.tracks.as_ref().map_or(0, Vec::len),
        artwork_path: card.art.clone(),
        play_count: 0,
    }
}

pub(crate) fn row_of(track: &Track, art: String) -> TrackRow {
    TrackRow {
        id: track.id,
        path: track.path.clone(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        album_id: track.album_id,
        disc: track.disc,
        number: track.number,
        duration_ms: track.duration_ms,
        plays: 0,
        quality: track.format.as_ref().map(|f| f.quality()).unwrap_or_default(),
        artwork_path: art,
        available: track.playable,
    }
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase().split_whitespace().map(String::from).collect()
}

impl Chilpa {
    /// The source and its icon, as a record's version selector shows it.
    fn source_of(&mut self, plugin: &str) -> AlbumSource {
        self.source_panels();
        let name = self.source_name(plugin);
        AlbumSource {
            key: plugin.to_string(),
            label: if name.is_empty() { plugin.to_string() } else { name },
            icon: crate::plugin_icons::image_url(&Self::plugins_dir(), plugin),
            local: false,
        }
    }

    /// The records of a listing, in the order and groups asked for, and the
    /// ones whose title or artist hold every word of `filter`. `key` may end
    /// in a membership ("favorites") after a second space.
    pub fn source_shelf(&mut self, key: &str, order: &str, group: &str, filter: &str) -> (Vec<Album>, Vec<(usize, String)>) {
        let (plugin, rest) = split(key);
        let (address, membership) = rest.split_once(' ').unwrap_or((rest, ""));
        let shelf_key = format!("{plugin} {address}");
        let Some(shelf) = self.catalog.shelf(&shelf_key) else { return (Vec::new(), Vec::new()) };
        let needle = words(filter);
        let mut cards: Vec<Card> = self.catalog.cards(&shelf.cards).into_iter()
            .filter(|c| membership.is_empty() || shelf.membership.get(&c.id).is_some_and(|m| m.split(',').any(|m| m == membership)))
            .filter(|c| {
                let haystack = format!("{} {}", c.title, c.artist).to_lowercase();
                needle.iter().all(|w| haystack.contains(w.as_str()))
            })
            .collect();
        match order {
            "artist" => cards.sort_by_cached_key(|c| (c.artist.to_lowercase(), c.year, c.title.to_lowercase())),
            "title" => cards.sort_by_cached_key(|c| c.title.to_lowercase()),
            "year" => cards.sort_by_key(|c| std::cmp::Reverse(c.year)),
            _ => {}
        }
        if !matches!(group, "letter" | "artist") {
            return (cards.iter().map(album_of).collect(), Vec::new());
        }
        let named = cards.iter().map(|c| {
            let label = match (group, order) {
                ("artist", _) => c.artist.trim().to_string(),
                (_, "artist") => chilpa_library::letter_of(&c.artist),
                _ => chilpa_library::letter_of(&c.title),
            };
            (label, c.id)
        }).collect();
        let found = crate::albums_view::arrange(named);
        let by_id: std::collections::HashMap<i64, Album> = cards.iter().map(|c| (c.id, album_of(c))).collect();
        (found.ids.iter().filter_map(|id| by_id.get(id).cloned()).collect(), found.groups)
    }

    /// The tracks a listing holds, in its order.
    pub fn source_shelf_tracks(&mut self, key: &str) -> Vec<TrackRow> {
        let (plugin, rest) = split(key);
        let address = rest.split(' ').next().unwrap_or_default();
        self.catalog.shelf(&format!("{plugin} {address}"));
        self.catalog.shelf_tracks(&format!("{plugin} {address}")).into_iter().map(|(t, art)| row_of(&t, art)).collect()
    }

    /// What a source page needs besides its records and tracks, as JSON:
    /// its title, whether it waits or failed, its folders, the memberships
    /// its records have, and whether it is a source the catalog draws.
    pub fn source_page(&mut self, key: &str) -> String {
        let (plugin, rest) = split(key);
        let address = rest.split(' ').next().unwrap_or_default();
        let music = self.source_draws_music(plugin);
        // The facade's copy: the host is often busy answering the catalog.
        self.source_panels();
        let layout = self.sources.layouts.get(plugin).cloned();
        let name = self.source_name(plugin);
        if address.is_empty() {
            // The source's own page: a shelf for each route of its discover
            // section, or for each folder of its root when it has no layout.
            let shelves: Vec<serde_json::Value> = match layout.as_ref().and_then(|l| l.sections.get("discover")) {
                Some(section) => section.paths.iter().enumerate().flat_map(|(i, route)| {
                    let key = format!("{plugin} route:discover:{i}");
                    // A route that leads to folders, not records, is a row of
                    // shelves: one for each of its first folders.
                    match self.catalog.shelf(&key) {
                        Some(shelf) if shelf.cards.is_empty() && shelf.tracks.is_empty() && !shelf.folders.is_empty() => shelf.folders.iter().take(EXPANDED)
                            .map(|f| serde_json::json!({"key": format!("{plugin} at:{}", f.id), "title": f.title})).collect::<Vec<_>>(),
                        _ => vec![serde_json::json!({"key": key, "title": route.path.last().cloned().unwrap_or_default()})],
                    }
                }).collect(),
                None => {
                    let root = self.catalog.shelf(&format!("{plugin} root")).unwrap_or_default();
                    root.folders.iter().take(12).map(|f| serde_json::json!({"key": format!("{plugin} at:{}", f.id), "title": f.title})).collect()
                }
            };
            let loading = layout.is_none() && self.catalog.loading(&format!("{plugin} root"));
            return serde_json::json!({"music": music, "name": name, "title": name, "shelves": shelves, "loading": loading}).to_string();
        }
        let shelf_key = format!("{plugin} {address}");
        let shelf = self.catalog.shelf(&shelf_key).unwrap_or_default();
        let section = address.strip_prefix("section:").and_then(|s| layout.as_ref()?.sections.get(s));
        let mut memberships: Vec<String> = shelf.membership.values().flat_map(|m| m.split(',')).filter(|m| !m.is_empty()).map(String::from).collect();
        memberships.sort();
        memberships.dedup();
        let cards = self.catalog.cards(&shelf.cards);
        let artists = !cards.is_empty() && cards.iter().all(|c| c.kind == CardKind::Artist);
        let title = section.map(|s| s.title.clone()).filter(|t| !t.is_empty()).unwrap_or(shelf.title.clone());
        serde_json::json!({
            "music": music,
            "name": name,
            "title": title,
            "loading": self.catalog.loading(&shelf_key),
            "error": self.catalog.shelf_error(&shelf_key).unwrap_or_default(),
            "folders": shelf.folders.iter().map(|f| serde_json::json!({"key": format!("{plugin} at:{}", f.id), "title": f.title})).collect::<Vec<_>>(),
            "memberships": memberships,
            "cards": shelf.cards.len(),
            "tracks": shelf.tracks.len(),
            "round": artists,
            "entity": section.map(|s| s.entity.clone()).unwrap_or_default(),
            "albumsOnly": shelf.tracks.is_empty() && shelf.folders.is_empty(),
        }).to_string()
    }

    /// What a listing is called, for its tab: its section's title, or the
    /// title it was read with.
    pub(crate) fn source_listing_title(&mut self, key: &str) -> Option<String> {
        let (plugin, rest) = split(key);
        let address = rest.split(' ').next().unwrap_or_default();
        if let Some(name) = address.strip_prefix("section:") {
            self.source_panels();
            let layout = self.sources.layouts.get(plugin).cloned();
            if let Some(title) = layout.and_then(|l| l.sections.get(name).map(|s| s.title.clone())).filter(|t| !t.is_empty()) {
                return Some(title);
            }
        }
        self.catalog.shelf_title(&format!("{plugin} {address}")).filter(|t| !t.is_empty())
    }

    /// Where a screen and an id lead. A source's artist is drawn as a card
    /// among records but opens its own listing, not a record page.
    pub fn route_to(&mut self, screen: &str, id: i64) -> Option<crate::navigation::Route> {
        use crate::navigation::{Route, Screen};
        let screen = Screen::from_name(screen)?;
        match self.catalog.card(id).filter(|c| screen == Screen::Album && c.kind == CardKind::Artist) {
            Some(card) => Some(Route { screen: Screen::Source, id: 0, key: format!("{} card:{}", card.plugin, card.id) }),
            None => Some(Route { screen, id, key: String::new() }),
        }
    }

    pub(crate) fn source_retry(&mut self, key: &str) {
        let (plugin, rest) = split(key);
        let address = rest.split(' ').next().unwrap_or_default();
        self.catalog.retry(&format!("{plugin} {address}"));
    }

    /// A source whose tree the catalog draws as records: one that runs as a
    /// compatibility worker or that has a layout. Others keep their tree.
    pub fn source_draws_music(&mut self, plugin: &str) -> bool {
        let Some(plugins) = self.plugins.as_ref().and_then(|p| p.try_lock().ok()) else {
            return self.catalog.drawn(plugin);
        };
        let drawn = plugins.layouts.get(plugin).is_some_and(|l| !l.sections.is_empty()) || plugins.worker_domains(plugin).is_some();
        drop(plugins);
        self.catalog.remember_drawn(plugin, drawn);
        drawn
    }

    pub fn source_revisions(&self) -> (i32, i32) {
        self.catalog.revisions()
    }

    // Records of a source, through the library's own functions.

    pub(crate) fn source_album(&mut self, id: i64) -> Option<Album> {
        self.catalog.card(id).map(|c| album_of(&c))
    }

    pub(crate) fn source_album_rows(&mut self, id: i64) -> Vec<TrackRow> {
        let art = self.catalog.art_of_card(id);
        self.catalog.tracks(id).unwrap_or_default().iter().map(|t| {
            let own = crate::pictures::kept(&t.image).filter(|_| t.album_id != id);
            row_of(t, own.unwrap_or_else(|| art.clone()))
        }).collect()
    }

    pub(crate) fn source_album_detail(&mut self, id: i64) -> Option<crate::AlbumDetail> {
        let card = self.catalog.card(id)?;
        let rows = self.source_album_rows(id);
        let mut discs: Vec<u16> = rows.iter().map(|r| r.disc.max(1)).collect();
        discs.dedup();
        let best = rows.iter().map(|r| &r.quality).max_by_key(|q| (q.tier.rank(), q.bits, q.rate_hz)).cloned()
            .or_else(|| card.format.as_ref().map(|f| f.quality())).unwrap_or_default();
        Some(crate::AlbumDetail {
            duration_ms: rows.iter().map(|r| r.duration_ms).sum(),
            discs: discs.len().max(1),
            quality: best,
            genre: card.genre.clone(),
            album: album_of(&card),
        })
    }

    /// The line under a source record's header: where it comes from, or why
    /// its tracks are not there.
    pub(crate) fn source_album_info(&mut self, id: i64) -> String {
        let Some(card) = self.catalog.card(id) else { return String::new() };
        if let Some(error) = self.catalog.card_error(id).filter(|_| card.tracks.is_none()) {
            return error;
        }
        let source = self.source_of(&card.plugin);
        if card.tracks.is_none() {
            return chilpa_i18n::t_args("Reading it from {}…", &[&source.label]);
        }
        chilpa_i18n::t_args("Info: {}", &[&source.label])
    }

    pub(crate) fn source_album_kind(&mut self, id: i64) -> String {
        match self.catalog.card(id).map(|c| c.kind) {
            Some(CardKind::Playlist) => self.t("Playlist").to_uppercase(),
            Some(CardKind::Artist) => self.t("Artist").to_uppercase(),
            _ => self.t("Album").to_uppercase(),
        }
    }

    /// A source record has one version: the source's, with its quality.
    pub(crate) fn source_album_versions(&mut self, id: i64) -> Vec<AlbumVersion> {
        let Some(detail) = self.source_album_detail(id) else { return Vec::new() };
        let Some(card) = self.catalog.card(id) else { return Vec::new() };
        let source = self.source_of(&card.plugin);
        let (long, brief) = (detail.quality.long(), detail.quality.brief());
        let join = |a: &str, b: &str| if b.is_empty() { a.to_string() } else { format!("{a} · {b}") };
        vec![AlbumVersion { id, label: join(&source.label, &long), brief: join(&source.label, &brief), detail: card.title.clone(), source }]
    }

    pub(crate) fn source_album_sources(&mut self, id: i64) -> Vec<AlbumSource> {
        self.catalog.card(id).map(|c| vec![self.source_of(&c.plugin)]).unwrap_or_default()
    }

    /// A queue entry of a source the catalog knows, as a row.
    pub(crate) fn source_row(&self, path: &str) -> Option<TrackRow> {
        let (track, art) = self.catalog.track_by_path(path)?;
        Some(row_of(&track, art))
    }

    pub(crate) fn source_album_of_path(&self, path: &str) -> i64 {
        self.catalog.track_by_path(path).map(|(t, _)| t.album_id).unwrap_or_default()
    }

    /// Plays, queues or puts next records, local or of a source. A source's
    /// record whose tracks were never read waits for them first.
    pub fn album_action(&mut self, ids: &[i64], act: &str, shuffled: bool) {
        let waiting = ids.iter().any(|id| *id < 0 && self.catalog.tracks(*id).is_none());
        if waiting {
            self.catalog.defer(ids.to_vec(), act, shuffled);
            return;
        }
        let paths = self.album_paths(ids);
        match act {
            "play" => {
                self.set_shuffle(shuffled);
                self.play_from(&paths, 0);
            }
            "next" => self.play_next_here(&paths),
            _ => self.enqueue_here(&paths),
        }
    }

    /// Runs what waited for a record's tracks, and puts a cover that came
    /// late on what plays. Called from `poll`.
    pub(crate) fn advance_catalog(&mut self) {
        for deferred in self.catalog.ready() {
            self.album_action(&deferred.albums, &deferred.act, deferred.shuffled);
        }
        // A queue restored before the catalog was read from disk is named
        // once it is.
        if self.now.title.is_empty() && crate::source_opener::of_plugin(&self.now.path).is_some() {
            self.name_from_heard();
        }
        if self.now.artwork_path.is_empty() && crate::source_opener::of_plugin(&self.now.path).is_some() {
            self.catalog.want_track_cover(&self.now.path);
            if let Some((_, art)) = self.catalog.track_by_path(&self.now.path).filter(|(_, art)| !art.is_empty()) {
                self.now.artwork_path = art;
            }
        }
    }
}
