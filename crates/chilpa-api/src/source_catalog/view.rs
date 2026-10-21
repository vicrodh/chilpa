//! What the window reads of the catalog, in the shapes it already reads
//! for the library: records as `Album`, tracks as `TrackRow`.

use super::store::{Card, CardKind, Track};
use super::split;
use crate::album_versions::{AlbumSource, AlbumVersion};
use crate::{Album, Chilpa, TrackRow};

/// I keep the displayed albums and their load state in one snapshot.
pub struct SourceShelfSnapshot {
    pub albums: Vec<Album>,
    pub groups: Vec<(usize, String)>,
    pub tracks: usize,
    pub loading: bool,
    pub error: String,
}

impl SourceShelfSnapshot {
    pub fn ready(&self) -> bool {
        !self.albums.is_empty() || !self.loading || !self.error.is_empty()
    }
}

/// How many folders of a discover route become shelves of their own.
const EXPANDED: usize = 6;

pub(crate) fn album_of(card: &Card) -> Album {
    Album {
        id: card.id,
        title: card.title.clone(),
        artist: card.artist.clone(),
        year: card.year,
        track_count: card.tracks.as_ref().map_or_else(|| card.metadata["tracks"].as_u64().unwrap_or_default() as usize, Vec::len),
        artwork_path: card.art.clone(),
        play_count: 0,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    #[test]
    fn a_delayed_shelf_keeps_its_pending_snapshot_until_rows_arrive() {
        let mut app = crate::Chilpa::new();
        let key = "fixture section:albums";
        app.catalog.lock().loaded = true;
        let waiting = app.source_shelf_snapshot(key, "", "", "");
        assert!(waiting.loading);
        assert!(!waiting.ready());
        {
            let mut inner = app.catalog.lock();
            inner.store.cards.insert(-1, super::Card { id: -1, title: "Record".into(), ..Default::default() });
            inner.store.shelves.insert(key.into(), super::super::store::Shelf {
                cards: vec![-1], epoch: 1, read_at: super::super::now(), ..Default::default()
            });
            inner.queue.queued.clear();
        }
        assert!(!waiting.ready(), "I keep an earlier empty snapshot pending after the worker finishes");
        let arrived = app.source_shelf_snapshot(key, "", "", "");
        assert!(arrived.ready());
        assert!(!arrived.loading);
        assert_eq!(arrived.albums.len(), 1);
        app.catalog.lock().running.insert(super::super::Job::Shelf(key.into()));
        let refreshing = app.source_shelf_snapshot(key, "", "", "");
        assert!(refreshing.loading);
        assert!(refreshing.ready(), "I keep populated rows visible during refresh");
    }

    #[test]
    fn an_unloaded_or_missing_shelf_waits_and_a_failed_shelf_completes() {
        let mut app = crate::Chilpa::new();
        let key = "fixture section:missing";
        assert!(!app.source_shelf_snapshot(key, "", "", "").ready());
        {
            let mut inner = app.catalog.lock();
            inner.loaded = true;
            inner.shelf_errors.insert(key.into(), "Unavailable".into());
        }
        let failed = app.source_shelf_snapshot(key, "", "", "");
        assert!(failed.ready());
        assert!(!failed.loading);
        assert!(failed.albums.is_empty());
        assert_eq!(failed.error, "Unavailable");
    }

    #[test]
    fn completed_empty_and_filtered_shelves_are_ready() {
        let mut app = crate::Chilpa::new();
        let key = "fixture section:albums";
        {
            let mut inner = app.catalog.lock();
            inner.loaded = true;
            inner.store.shelves.insert(key.into(), super::super::store::Shelf {
                epoch: 1, read_at: super::super::now(), ..Default::default()
            });
        }
        let empty = app.source_shelf_snapshot(key, "", "", "");
        assert!(empty.ready());
        assert!(empty.albums.is_empty());
        {
            let mut inner = app.catalog.lock();
            inner.store.cards.insert(-1, super::Card { id: -1, title: "Record".into(), ..Default::default() });
            inner.store.shelves.get_mut(key).unwrap().cards.push(-1);
        }
        let filtered = app.source_shelf_snapshot(key, "", "", "absent");
        assert!(filtered.ready());
        assert!(filtered.albums.is_empty());
        let membership = app.source_shelf_snapshot(&format!("{key} favorites"), "", "", "");
        assert!(membership.ready());
        assert!(membership.albums.is_empty());
        app.catalog.lock().running.insert(super::super::Job::Shelf(key.into()));
        assert!(!app.source_shelf_snapshot(key, "", "", "absent").ready());
    }

    #[test]
    fn declared_tabs_keep_their_presentation_before_items_arrive() {
        let mut app = crate::Chilpa::new();
        app.catalog.lock().loaded = true;
        app.catalog.remember_drawn("fixture", true);
        app.sources.layouts.insert("fixture".into(), serde_json::from_value(json!({"sections":{
            "library":{"title":"Library"},
            "tracks":{"title":"Tracks","entity":"track","under":"library"},
            "artists":{"title":"Artists","entity":"artist","under":"library"}
        }})).unwrap());
        let tracks: serde_json::Value = serde_json::from_str(&app.source_page("fixture section:tracks")).unwrap();
        assert_eq!(tracks["music"], true);
        assert_eq!(tracks["albumsOnly"], false, "a waiting track tab must retain its track layout");
        assert_eq!(tracks["entity"], "track");
        let artists: serde_json::Value = serde_json::from_str(&app.source_page("fixture section:artists")).unwrap();
        assert_eq!(artists["round"], true, "a waiting artist tab must reserve round portraits");
    }
}

#[cfg(test)]
mod work_tests {
    use super::*;

    #[test]
    fn source_track_keeps_work_and_the_composer_artist_route() {
        let track = Track {
            plugin: "fixture".into(),
            metadata: serde_json::json!({"work":"Das Rheingold", "composer":"Richard Wagner", "composer_ref":"artist:42", "plays":9}),
            ..Track::default()
        };
        let row = row_of(&track, String::new());
        assert_eq!(row.work, "Das Rheingold");
        assert_eq!(row.composer, "Richard Wagner");
        assert_eq!(row.composer_target, "fixture native:artist:42");
        assert_eq!(row.plays, 9);
        assert_eq!(crate::tracks_view::group_of("plays", &row), chilpa_i18n::t("Played"));
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
        plays: track.metadata["plays"].as_i64().filter(|n| *n >= 0).unwrap_or(0) as usize,
        quality: track.format.as_ref().map(|f| f.quality()).unwrap_or_default(),
        artwork_path: art,
        available: track.playable,
        composer: super::composers::own_composers(&track.metadata).join(", "),
        disc_subtitle: track.metadata["disc_subtitle"].as_str().unwrap_or_default().to_string(),
        work: track.metadata["work"].as_str().unwrap_or_default().to_string(),
        composer_target: track.metadata["composer_ref"].as_str()
            .filter(|reference| reference.starts_with("artist:") && !track.plugin.is_empty())
            .map(|reference| format!("{} native:{reference}", track.plugin)).unwrap_or_default(),
        groups: super::library_index::track_groups(track),
    }
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase().split_whitespace().map(String::from).collect()
}

impl Chilpa {
    /// The source and its icon, as a record's version selector shows it.
    pub(crate) fn source_of(&mut self, plugin: &str) -> AlbumSource {
        self.source_panels();
        let name = self.source_name(plugin);
        AlbumSource {
            key: plugin.to_string(),
            label: if name.is_empty() { plugin.to_string() } else { name },
            icon: crate::plugin_icons::image_url(&Self::plugins_dir(), plugin),
            local: false,
            contain_art: false,
        }
    }

    /// The records of a listing, in the order and groups asked for, and the
    /// ones whose title or artist hold every word of `filter`. `key` may end
    /// in a membership ("favorites") after a second space.
    pub fn source_shelf(&mut self, key: &str, order: &str, group: &str, filter: &str) -> (Vec<Album>, Vec<(usize, String)>) {
        let snapshot = self.source_shelf_snapshot(key, order, group, filter);
        (snapshot.albums, snapshot.groups)
    }

    /// I pair filtered rows with the completion state that produced them.
    pub fn source_shelf_snapshot(&mut self, key: &str, order: &str, group: &str, filter: &str) -> SourceShelfSnapshot {
        let (plugin, rest) = split(key);
        let (address, membership) = if rest.starts_with("matching-artist-") { (rest, "") } else { rest.split_once(' ').unwrap_or((rest, "")) };
        let shelf_key = format!("{plugin} {address}");
        let (rows, tracks, loading, error) = self.catalog.shelf_snapshot(&shelf_key, membership);
        let needle = words(filter);
        let mut cards: Vec<Card> = rows.into_iter()
            .filter(|c| super::sorting::matches(c, &needle))
            .collect();
        super::sorting::sort_cards(&mut cards, order);
        if !matches!(group, "letter" | "artist" | "year" | "decade") {
            return SourceShelfSnapshot { albums: cards.iter().map(album_of).collect(), groups: Vec::new(), tracks, loading, error };
        }
        let named = cards.iter().map(|c| {
            let label = match (group, order.split(':').next().unwrap_or_default()) {
                ("year", _) => c.year.map(|y| y.to_string()).unwrap_or_default(),
                ("decade", _) => c.year.map(|y| format!("{}s", y.div_euclid(10) * 10)).unwrap_or_default(),
                ("artist", _) => c.artist.trim().to_string(),
                (_, "artist") => chilpa_library::letter_of(&c.artist),
                _ => chilpa_library::letter_of(&c.title),
            };
            (label, c.id)
        }).collect();
        let found = crate::albums_view::arrange(named);
        let by_id: std::collections::HashMap<i64, Album> = cards.iter().map(|c| (c.id, album_of(c))).collect();
        SourceShelfSnapshot { albums: found.ids.iter().filter_map(|id| by_id.get(id).cloned()).collect(), groups: found.groups, tracks, loading, error }
    }

    /// The tracks a listing holds, in its order.
    pub fn source_shelf_tracks(&mut self, key: &str) -> Vec<TrackRow> {
        let (plugin, rest) = split(key);
        let address = if rest.starts_with("matching-artist-") { rest } else { rest.split(' ').next().unwrap_or_default() };
        self.catalog.shelf(&format!("{plugin} {address}"));
        self.catalog.shelf_tracks(&format!("{plugin} {address}")).into_iter().map(|(t, art)| row_of(&t, art)).collect()
    }

    /// What a source page needs besides its records and tracks, as JSON:
    /// its title, whether it waits or failed, its folders, the memberships
    /// its records have, and whether it is a source the catalog draws.
    pub fn source_page(&mut self, key: &str) -> String {
        if key.trim().is_empty() { return "{}".into(); }
        let (plugin, rest) = split(key);
        // The library's artist is drawn as a source's page would be.
        if plugin == "local" {
            if let Some(name) = rest.strip_prefix("artist:") {
                return self.artist_document(name);
            }
        }
        if let Some(reference) = rest.strip_prefix("artist-library:") {
            return self.artist_library_view(plugin, reference);
        }
        if rest.starts_with("matching-artist-v3:") || rest.starts_with("matching-artist-v2:") {
            let key = format!("{plugin} {rest}");
            if let Some(reference) = self.catalog.shelf(&key).and_then(|s| s.document["resolved_ref"].as_str().map(str::to_owned)) {
                return self.native_page_view(plugin, &reference);
            }
        }
        let address = if rest.starts_with("matching-artist-") { rest } else { rest.split(' ').next().unwrap_or_default() };
        let music = self.source_draws_music(plugin);
        // The facade's copy: the host is often busy answering the catalog.
        self.source_panels();
        let layout = self.sources.layouts.get(plugin).cloned();
        let name = self.source_name(plugin);
        if self.natives.has_pages(plugin) && (address.is_empty() || matches!(address, "home" | "root" | "section:discover") || address.starts_with("native:")) {
            let reference = address.strip_prefix("native:").unwrap_or("discover:home");
            return self.native_page_view(plugin, reference);
        }
        if address.is_empty() && self.natives.is_native(plugin) {
            // A process plugin's home is read whole, in one call; each of its
            // shelves is a route, kept between runs with its title.
            let home = format!("{plugin} home");
            let _ = self.catalog.shelf(&home);
            let shelves: Vec<serde_json::Value> = (0..).map_while(|at| {
                let key = format!("{plugin} route:discover:{at}");
                self.catalog.stored_shelf(&key).map(|s| serde_json::json!({"key": key, "title": s.title}))
            }).collect();
            let loading = shelves.is_empty() && self.catalog.loading(&home);
            let error = if shelves.is_empty() { self.catalog.shelf_error(&home).unwrap_or_default() } else { String::new() };
            return serde_json::json!({"music": true, "name": name, "title": name, "shelves": shelves, "loading": loading, "error": error}).to_string();
        }
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
        let section_name = address.strip_prefix("section:").unwrap_or_default();
        let section = layout.as_ref().and_then(|l| l.sections.get(section_name));
        // A section with tabs: its own and each tab's, the first being "All".
        let tabs: Vec<serde_json::Value> = layout.as_ref().map(|l| l.tabs(section_name)).unwrap_or_default().into_iter().enumerate()
            .map(|(at, (tab, s))| serde_json::json!({"key": format!("{plugin} section:{tab}"), "active": tab == section_name,
                "title": if at == 0 { chilpa_i18n::t("All") } else { s.title.clone() }})).collect();
        if let Some(parent) = section.filter(|s| s.under.is_empty() && !tabs.is_empty()) {
            // "All": a shelf for each tab. Tracks are not cards: theirs is a link.
            let shelves: Vec<serde_json::Value> = layout.as_ref().map(|l| l.tabs(section_name)).unwrap_or_default().into_iter().skip(1)
                .map(|(tab, s)| serde_json::json!({"key": format!("{plugin} section:{tab}"), "title": s.title, "tracks": s.entity == "track", "round": s.entity == "artist"}))
                .collect();
            return serde_json::json!({"music": music, "name": name, "title": parent.title, "tabs": tabs, "shelves": shelves}).to_string();
        }
        let shelf_key = format!("{plugin} {address}");
        let shelf = self.catalog.shelf(&shelf_key).unwrap_or_default();
        let mut memberships: Vec<String> = shelf.membership.values().flat_map(|m| m.split(',')).filter(|m| !m.is_empty()).map(String::from).collect();
        memberships.sort();
        memberships.dedup();
        let cards = self.catalog.cards(&shelf.cards);
        let artists = !cards.is_empty() && cards.iter().all(|c| c.kind == CardKind::Artist);
        // A tab's page is titled after the page it is a tab of.
        let named = section.and_then(|s| if s.under.is_empty() { Some(s) } else { layout.as_ref()?.sections.get(&s.under) });
        let title = named.map(|s| s.title.clone()).filter(|t| !t.is_empty()).unwrap_or(shelf.title.clone());
        serde_json::json!({
            "music": music,
            "plugin": plugin,
            "playlist_edit": section_name == "playlists" && self.natives.edits_playlists(plugin),
            "name": name,
            "title": title,
            "loading": self.catalog.loading(&shelf_key),
            "error": self.catalog.shelf_error(&shelf_key).unwrap_or_default(),
            "folders": shelf.folders.iter().map(|f| serde_json::json!({"key": format!("{plugin} at:{}", f.id), "title": f.title})).collect::<Vec<_>>(),
            "memberships": memberships,
            "cards": shelf.cards.len(),
            "tracks": shelf.tracks.len(),
            "round": artists || (shelf.cards.is_empty() && section.is_some_and(|s| s.entity == "artist")),
            "entity": section.map(|s| s.entity.clone()).unwrap_or_default(),
            "tabs": tabs,
            "albumsOnly": shelf.tracks.is_empty() && shelf.folders.is_empty() && section.is_none_or(|s| s.entity != "track"),
        }).to_string()
    }

    /// What a listing is called, for its tab: its section's title, or the
    /// title it was read with.
    pub(crate) fn source_listing_title(&mut self, key: &str) -> Option<String> {
        let (plugin, rest) = split(key);
        let address = if rest.starts_with("matching-artist-") { rest } else { rest.split(' ').next().unwrap_or_default() };
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
        if screen == "album" && id < 0 { self.source_panels(); }
        let mut plan = self.plan_route(screen, id)?;
        if plan.needs_preparation() { plan.media_parent = self.source_card_parent(id); }
        let route = plan.route.clone();
        self.prepare_planned_route(plan);
        Some(route)
    }

    /// I resolve typed routes using metadata already in memory, leaving source work for presentation.
    pub fn plan_route(&self, screen: &str, id: i64) -> Option<crate::navigation::PlannedRoute> {
        use crate::navigation::{PlannedRoute, Route, Screen};
        let screen = Screen::from_name(screen)?;
        let mut route = Route { screen, id, key: String::new() };
        if screen != Screen::Album || id >= 0 { return Some(PlannedRoute { route, media_parent: None }); }
        let card = self.catalog.navigation_card(id);
        let media_parent = card.as_ref().and_then(|card| self.planned_media_parent(card.clone()));
        if let Some((source, _)) = &media_parent {
            route = Route { screen: Screen::Source, id: 0, key: source.clone() };
        } else if let Some(card) = card.filter(|card| matches!(card.kind, CardKind::Artist | CardKind::Label)) {
            route = Route { screen: Screen::Source, id: 0, key: if self.natives.has_pages(&card.plugin) {
                format!("{} native:{}", card.plugin, card.identity)
            } else { format!("{} card:{}", card.plugin, card.id) } };
        }
        Some(PlannedRoute { route, media_parent })
    }

    pub(crate) fn source_retry(&mut self, key: &str) {
        let (plugin, rest) = split(key);
        let address = if rest.starts_with("matching-artist-") { rest } else { rest.split(' ').next().unwrap_or_default() };
        self.catalog.retry(&format!("{plugin} {address}"));
    }

    /// A source whose tree the catalog draws as records: one that runs as a
    /// compatibility worker or that has a layout. Others keep their tree.
    pub fn source_draws_music(&mut self, plugin: &str) -> bool {
        if self.natives.is_native(plugin) {
            self.catalog.remember_drawn(plugin, true);
            return true;
        }
        let Some(plugins) = self.plugins.as_ref().and_then(|p| p.try_lock().ok()) else {
            return self.catalog.drawn(plugin);
        };
        let drawn = plugins.layouts.get(plugin).is_some_and(|l| !l.sections.is_empty()) || plugins.worker_domains(plugin).is_some();
        drop(plugins);
        self.catalog.remember_drawn(plugin, drawn);
        drawn
    }

    pub fn source_entity_revision(&self, id: i64) -> (i32, i32) {
        self.catalog.entity_revision(id)
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
        // A list is numbered by its order, not by each track's place on its record.
        let list = self.catalog.card(id).is_some_and(|c| c.kind == CardKind::Playlist);
        self.catalog.tracks(id).unwrap_or_default().iter().enumerate().map(|(at, t)| {
            let own = crate::pictures::kept(&t.image).filter(|_| t.album_id != id);
            let mut row = row_of(t, own.unwrap_or_else(|| art.clone()));
            if list {
                (row.number, row.disc) = (u16::try_from(at + 1).unwrap_or(u16::MAX), 0);
            }
            row
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
            genre: super::genres::own_genres(&card.metadata, &card.genre).join(", "),
            label: String::new(),
            album: album_of(&card),
        })
    }

    /// The line under a source record's header: where it comes from, or why
    /// its tracks are not there.
    pub(crate) fn source_album_info(&mut self, id: i64) -> String {
        let Some(card) = self.catalog.card(id) else { return String::new() };
        if card.metadata["available"] == false { return chilpa_i18n::t("This record cannot be played now."); }
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
        self.source_album_versions_for(&detail)
    }

    pub(crate) fn source_album_versions_for(&mut self, detail: &crate::AlbumDetail) -> Vec<AlbumVersion> {
        let id = detail.album.id;
        let Some(card) = self.catalog.card(id) else { return Vec::new() };
        let source = self.source_of(&card.plugin);
        let (long, brief) = (detail.quality.long(), detail.quality.brief());
        let join = |a: &str, b: &str| if b.is_empty() { a.to_string() } else { format!("{a} · {b}") };
        vec![AlbumVersion { id, label: join(&source.label, &long), brief: join(&source.label, &brief), detail: card.title.clone(), source }]
    }

    pub(crate) fn source_album_sources(&mut self, id: i64) -> Vec<AlbumSource> {
        self.catalog.card(id).map(|c| { let mut source = self.source_of(&c.plugin); source.contain_art = c.contain_art(); vec![source] }).unwrap_or_default()
    }

    /// A queue entry of a source the catalog knows, as a row.
    pub(crate) fn source_row(&self, path: &str) -> Option<TrackRow> {
        let (track, art) = self.catalog.track_by_path(path)?;
        if art.is_empty() { self.catalog.want_art_of(path); }
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
            self.catalog.defer(ids.to_vec(), act, shuffled, self.browse_origin());
            return;
        }
        let paths = self.album_paths(ids);
        match act {
            "play" => {
                if !paths.is_empty() {
                    for id in ids.iter().copied().filter(|id| *id < 0) {
                        if self.catalog.card(id).is_some_and(|card| card.kind == super::store::CardKind::Playlist) {
                            self.catalog.record_playlist_play(id);
                            self.count_playlist_play(id);
                        }
                    }
                }
                self.play_list(&paths, shuffled);
            }
            "next" => self.play_next_here(&paths),
            _ => self.enqueue_here(&paths),
        }
    }

    /// Puts records after what is queued next, as one group with that
    /// label; a source's record whose tracks were never read waits for them.
    pub fn album_later(&mut self, ids: &[i64], label: &str) {
        if ids.iter().any(|id| *id < 0 && self.catalog.tracks(*id).is_none()) {
            self.catalog.defer(ids.to_vec(), "later", false, self.browse_origin());
            return;
        }
        let paths = self.album_paths(ids);
        let title = if label.is_empty() && ids.len() == 1 {
            self.catalog.card(ids[0]).map(|card| card.title).unwrap_or_default()
        } else { label.to_string() };
        self.play_later_here(&paths, &title);
    }

    /// Sends records to an extra queue, by the `buffer:…` actions the menu
    /// offers; a source's record whose tracks were never read waits for them.
    pub fn album_listen_action(&mut self, ids: &[i64], action: &str) {
        if ids.iter().any(|id| *id < 0 && self.catalog.tracks(*id).is_none()) {
            self.catalog.defer(ids.to_vec(), action, false, self.browse_origin());
            return;
        }
        let paths = self.album_paths(ids);
        self.listen_paths_action(action, &paths);
    }

    /// Runs what waited for a record's tracks, and puts a cover that came
    /// late on what plays. Called from `poll`.
    pub(crate) fn advance_catalog(&mut self) {
        self.catalog.advance_genres();
        self.catalog.advance_composers();
        let (ready, _hold) = self.catalog.ready();
        for deferred in ready {
            self.with_queue_origin(deferred.origin, |app| match deferred.act.as_str() {
                "later" => app.album_later(&deferred.albums, ""),
                act if act.starts_with("buffer:") => app.album_listen_action(&deferred.albums, act),
                act => app.album_action(&deferred.albums, act, deferred.shuffled),
            });
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
