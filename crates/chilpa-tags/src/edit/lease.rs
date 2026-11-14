//! In-process ownership of staged metadata destinations. A refused acquisition
//! is retryable. Waiting while holding another destination cannot deadlock.
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

fn held() -> &'static Mutex<BTreeSet<PathBuf>> {
    static HELD: OnceLock<Mutex<BTreeSet<PathBuf>>> = OnceLock::new();
    HELD.get_or_init(|| Mutex::new(BTreeSet::new()))
}

pub struct WriteLease {
    path: PathBuf,
}
impl WriteLease {
    pub fn acquire(path: &Path) -> std::io::Result<Self> {
        let parent = std::fs::canonicalize(path.parent().unwrap_or(Path::new(".")))?;
        let path = parent.join(path.file_name().unwrap_or_default());
        if !held()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(path.clone())
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "another metadata write is using this destination",
            ));
        }
        Ok(Self { path })
    }
}
impl Drop for WriteLease {
    fn drop(&mut self) {
        held()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.path);
    }
}
