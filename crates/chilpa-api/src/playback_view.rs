//! The transport view omits artwork alternatives and unchanged lyric bodies.
use crate::{NowPlaying, Quality};

pub struct PlaybackView {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_id: i64,
    pub artwork_path: String,
    pub playing: bool,
    pub paused: bool,
    pub seconds: f64,
    pub total_seconds: f64,
    pub format: String,
    pub quality: Quality,
    pub bit_perfect: bool,
    pub index: usize,
    pub queue_len: usize,
    pub lyrics: Vec<String>,
    pub lyrics_line: i32,
    pub lyrics_synced: bool,
    pub lyrics_revision: u64,
    pub error: String,
    pub live: bool,
}
impl PlaybackView {
    /// I copy lyric bodies only when the receiver needs to replace them.
    /// Position, transport state and the current lyric line remain fresh.
    pub fn read(now: &NowPlaying, lyrics_shown: u64) -> Self {
        Self {
            path: now.path.clone(), title: now.title.clone(), artist: now.artist.clone(),
            album: now.album.clone(), album_id: now.album_id,
            artwork_path: now.artwork_path.clone(), playing: now.playing, paused: now.paused,
            seconds: now.seconds, total_seconds: now.total_seconds, format: now.format.clone(),
            quality: now.quality.clone(), bit_perfect: now.bit_perfect,
            index: now.index, queue_len: now.queue_len,
            lyrics: if now.lyrics_revision != lyrics_shown { now.lyrics.clone() } else { Vec::new() },
            lyrics_line: now.lyrics_line, lyrics_synced: now.lyrics_synced,
            lyrics_revision: now.lyrics_revision, error: now.error.clone(), live: now.live,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_updates_do_not_copy_unchanged_lyrics() {
        let mut now = NowPlaying { lyrics: vec!["A line".into(); 4000], lyrics_revision: 7,
            covers: vec!["alternative".into(); 1000], playing: true, ..Default::default() };
        assert_eq!(PlaybackView::read(&now, 6).lyrics, now.lyrics);
        now.seconds = 23.75; now.lyrics_line = 20; now.paused = true; now.error = "failure".into();
        let frame = PlaybackView::read(&now, 7);
        assert!(frame.lyrics.is_empty());
        assert_eq!((frame.seconds, frame.lyrics_line, frame.playing, frame.paused), (23.75, 20, true, true));
        assert_eq!(frame.error, "failure");
        now.lyrics.clear(); now.lyrics_revision += 1;
        let frame = PlaybackView::read(&now, 7);
        assert_eq!(frame.lyrics_revision, 8);
        assert!(frame.lyrics.is_empty(), "an empty replacement still advances the revision");
    }
}
