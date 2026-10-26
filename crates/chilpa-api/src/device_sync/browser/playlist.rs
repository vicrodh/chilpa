use super::{
    Browser, UiError,
    navigation::{index, unavailable},
};
use crate::device_sync::failure;
use chilpa_sync::RelativePath;
use std::collections::BTreeMap;

impl Browser {
    pub fn playlist_path(&self, id: &str) -> Option<RelativePath> {
        let i = index(id, "playlist:")?;
        let track = self.tracks.get(i)?;
        if !self.recorded
            || !track.playlist
            || self.playlist.is_some()
            || !(self.view == "playlists" || self.album.is_some_and(|a| self.albums[a].playlist))
        {
            return None;
        }
        RelativePath::new(track.path.clone()).ok()
    }
    pub fn open_playlist(&mut self, id: &str, text: &str) -> Result<(), UiError> {
        self.playlist_path(id).ok_or_else(unavailable)?;
        let known: BTreeMap<_, _> = self
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.playlist)
            .map(|(i, t)| (t.path.as_str(), i))
            .collect();
        let mut members = Vec::new();
        for line in text
            .trim_start_matches('\u{feff}')
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.starts_with('#'))
        {
            let path = line
                .strip_prefix("../")
                .and_then(|s| RelativePath::new(s.to_owned()).ok());
            let member = path.as_ref().and_then(|p| known.get(p.as_str())).copied().ok_or_else(|| failure(
                chilpa_i18n::mark("This playlist references music outside the recorded selection. Sync it again to refresh its contents."), ""))?;
            if members.len() >= chilpa_sync::MAX_ASSETS {
                return Err(super::limit());
            }
            members.push(member);
        }
        self.playlist = index(id, "playlist:");
        self.playlist_members = members;
        self.playlist_parent = Some((std::mem::take(&mut self.query), self.page));
        self.search.clear();
        self.page = 0;
        Ok(())
    }
}
