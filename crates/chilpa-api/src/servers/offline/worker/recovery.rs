//! Startup reconciles registered files on the existing single worker.
//! Complete copies need a stat, and only interrupted promotions are decoded.

use super::*;

impl Chilpa {
    /// Queued before anything else at startup, so no download meets a file
    /// that is still being reconciled.
    pub(crate) fn start_offline_recovery(&mut self) {
        let total = self
            .library()
            .and_then(|lib| lib.offline_copy_count().ok())
            .unwrap_or(0);
        if total == 0 || queue::lock(&self.servers.offline.tracker).busy() {
            return;
        }
        let job = |seq| queue::Job { seq, scopes: HashMap::new(), collection: String::new(), root: PathBuf::new(), limit: Limit::default() };
        self.offline_enqueue("recovery", Vec::new(), job, vec![(0, queue::Work::Recover)]);
        let mut tracker = queue::lock(&self.servers.offline.tracker);
        tracker.progress.total = total;
        tracker.progress.phase = "checking";
    }
}

/// Every registered file, a page at a time; `each` hears how each went.
pub(super) fn check_all(lib: &Library, cancelled: &AtomicBool, each: impl Fn(Result<bool, String>)) -> Result<(), String> {
    let mut after = String::new();
    loop {
        let copies = lib.offline_copy_page(&after).map_err(|_| chilpa_i18n::t("the library could not be opened"))?;
        if copies.is_empty() || cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        for copy in copies {
            after = copy.key.clone();
            if cancelled.load(Ordering::Acquire) {
                return Ok(());
            }
            each(reconcile(lib, &copy, cancelled));
        }
    }
}

pub(super) fn reconcile(
    lib: &Library,
    copy: &Copy,
    cancelled: &AtomicBool,
) -> Result<bool, String> {
    let failed = || chilpa_i18n::t("the library could not be written");
    let path = Path::new(&copy.path);
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        _ => {
            return Err(chilpa_i18n::t(
                "The downloaded file could not be verified as complete audio.",
            ))
        }
    };
    if copy.state == "complete"
        && metadata
            .as_ref()
            .is_some_and(|metadata| metadata.len() == copy.bytes && copy.bytes > 0)
    {
        return Ok(false);
    }
    let retained = metadata
        .as_ref()
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if copy.state == "downloading" && retained > 0 {
        let result = inspect(path, None, cancelled);
        if let Ok((bytes, checksum, codec, extension)) = result {
            let stored = lib.server(copy.source).ok().flatten().filter(|stored| {
                stored.account == copy.account && stored.identity == copy.identity
            });
            if let Some(stored) = stored {
                if copy.checksum.is_empty() || copy.checksum == checksum {
                    let mut item = copy.clone();
                    item.bytes = bytes;
                    item.checksum = checksum;
                    item.codec = codec;
                    item.extension = extension;
                    if lib
                        .complete_offline_copy(&item, stored.session_revision)
                        .map_err(|_| failed())?
                    {
                        return Ok(true);
                    }
                }
            }
        }
    }
    let partial = match std::fs::metadata(path.with_extension("part")) {
        Ok(metadata) if metadata.is_file() => Some(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        _ => {
            return Err(chilpa_i18n::t(
                "The downloaded file could not be verified as complete audio.",
            ))
        }
    };
    if copy.state == "removing" && metadata.is_none() && partial.is_none() {
        lib.finish_offline_removal(copy).map_err(|_| failed())?;
        return Ok(true);
    }
    let bytes = retained.saturating_add(partial.unwrap_or(0));
    if copy.state == "failed" && copy.bytes == bytes {
        return Ok(false);
    }
    lib.invalidate_offline_copy(copy, "", bytes)
        .map_err(|_| failed())?;
    Ok(true)
}
