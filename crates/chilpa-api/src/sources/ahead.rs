use super::*;

impl Chilpa {
    /// Near the end of a track, resolves the next one when it is a plugin's,
    /// in queue order. With shuffle on the next one is the player's to pick.
    pub(crate) fn resolve_next_ahead(&mut self) {
        let Some(next) = next_ahead(&self.now, self.shuffle, || self.gapless(), self.queue_paths()) else { return };
        if self.sources.ahead_asked == *next { return; }
        let next = next.clone();
        self.sources.ahead_asked = next.clone();
        if let Some(openers) = &self.sources.openers { openers.resolve_ahead(next); }
    }
}

fn next_ahead<'a>(now: &crate::NowPlaying, shuffle: bool, gapless: impl FnOnce() -> bool, paths: &'a [String]) -> Option<&'a String> {
    // I reject transport states before scanning the queue or reading settings.
    if !now.playing || now.paused || shuffle || now.total_seconds <= 0.0
        || now.total_seconds - now.seconds > 30.0 || paths.is_empty() { return None; }
    // With gapless on, the player owns prefetch; resolving here duplicates it.
    if gapless() { return None; }
    let at = paths.iter().position(|p| *p == now.path)?;
    paths.get(at + 1).filter(|p| of_plugin(p).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_non_gapless_near_end_window_checks_the_queue() {
        let paths = vec![entry_of("fixture", "a"), entry_of("fixture", "b")];
        let mut now = crate::NowPlaying { path: paths[0].clone(), playing: true,
            total_seconds: 200.0, seconds: 20.0, ..Default::default() };
        let untouched = || -> bool { panic!("settings are not read outside the prefetch window") };
        assert!(next_ahead(&now, false, untouched, &paths).is_none());
        now.seconds = 175.0; now.paused = true;
        assert!(next_ahead(&now, false, untouched, &paths).is_none());
        now.paused = false;
        assert!(next_ahead(&now, true, untouched, &paths).is_none());
        assert!(next_ahead(&now, false, || true, &paths).is_none());
        assert_eq!(next_ahead(&now, false, || false, &paths), Some(&paths[1]));
        now.seconds = 170.0;
        assert_eq!(next_ahead(&now, false, || false, &paths), Some(&paths[1]));
        now.path = paths[1].clone();
        assert!(next_ahead(&now, false, || false, &paths).is_none());
        now.path = paths[0].clone();
        assert!(next_ahead(&now, false, || false, &[paths[0].clone(), "/local.flac".into()]).is_none());
    }
}
