use super::*;
use sha2::{Digest, Sha256};

/// The service identifies its original text before any spelling repair.
/// Older documents use the same digest of trimmed lead lines, without timing.
pub fn result_id(document: &Document) -> String {
    if let Some(id) = document.source.as_ref().and_then(|source| source.result_id.as_ref())
        .filter(|id| id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit())) {
        return id.to_ascii_lowercase();
    }
    let text = document.lines.iter().map(|line| line.text.trim()).collect::<Vec<_>>().join("\n");
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

impl Cache {
    /// I keep rejections when cached answers are cleared. They belong to this
    /// recording and text, and never disable the provider for another song.
    pub fn reject(&self, track: &Track, provider: &str, document: &Document) -> rusqlite::Result<()> {
        self.reject_result(track, provider, &result_id(document))
    }

    pub fn reject_result(&self, track: &Track, provider: &str, fingerprint: &str) -> rusqlite::Result<()> {
        let tx = self.db.unchecked_transaction()?;
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM rejected_lyrics", [], |row| row.get(0))?;
        let known: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM rejected_lyrics WHERE key = ?1
            AND abs(seconds - ?2) <= 2 AND provider = ?3 AND fingerprint = ?4)",
            params![identity(track), seconds_of(track), provider, fingerprint], |row| row.get(0))?;
        if count >= 10_000 && !known { return Err(rusqlite::Error::InvalidQuery); }
        tx.execute("INSERT OR REPLACE INTO rejected_lyrics (key, seconds, provider, fingerprint, rejected_at)
            VALUES (?1, ?2, ?3, ?4, ?5)", params![identity(track), seconds_of(track), provider, fingerprint, now()])?;
        tx.execute("DELETE FROM lyrics WHERE key = ?1 AND abs(seconds - ?2) <= 2 AND provider = ?3",
            params![identity(track), seconds_of(track), provider])?;
        tx.commit()
    }

    pub fn is_rejected(&self, track: &Track, provider: &str, document: &Document) -> bool {
        self.db.query_row("SELECT EXISTS(SELECT 1 FROM rejected_lyrics
            WHERE key = ?1 AND abs(seconds - ?2) <= 2 AND provider = ?3 AND fingerprint = ?4)",
            params![identity(track), seconds_of(track), provider, result_id(document)], |row| row.get(0)).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_rejection_survives_cache_clear_and_only_blocks_this_recording_and_text() {
        let cache = Cache::in_memory().unwrap();
        let track = Track { title: "Song".into(), artist: "Artist".into(), duration_ms: Some(1000), ..Default::default() };
        let wrong = crate::lrc::parse("abc");
        assert_eq!(result_id(&wrong), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        cache.reject(&track, "provider", &wrong).unwrap();
        cache.clear();
        assert!(cache.is_rejected(&track, "provider", &wrong));
        let claim = Claim { title: Some(track.title.clone()), artist: Some(track.artist.clone()), ..Default::default() };
        cache.put(&track, &claim, &wrong, &Origin { provider: "provider".into(), name: "Provider".into() }, None);
        assert!(cache.get(&track).is_none(), "a late cache write cannot restore rejected text");
        assert!(!cache.is_rejected(&track, "another", &wrong));
        assert!(!cache.is_rejected(&track, "provider", &crate::lrc::parse("def")));
        assert!(!cache.is_rejected(&Track { title: "Another Song".into(), ..track }, "provider", &wrong));
    }
}
