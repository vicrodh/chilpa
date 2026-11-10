//! Startup reconciles registered files on the existing single worker.
//! Complete copies need a stat, and only interrupted promotions are decoded.

use super::*;

impl Chilpa {
    pub(crate) fn start_offline_recovery(&mut self) {
        if self.servers.offline.task.is_some() {
            return;
        }
        let total = self
            .library()
            .and_then(|lib| lib.offline_copy_count().ok())
            .unwrap_or(0);
        if total == 0 {
            return;
        }
        let progress = Arc::new(Mutex::new(Progress {
            phase: "checking",
            total,
            running: true,
            ..Default::default()
        }));
        let cancelled = Arc::new(AtomicBool::new(false));
        let changed = Arc::new(AtomicBool::new(false));
        let (worker_progress, worker_cancelled, worker_changed) =
            (progress.clone(), cancelled.clone(), changed.clone());
        let task = std::thread::Builder::new()
            .name("chilpa-offline-check".into())
            .spawn(move || {
                if let Ok(lib) = Library::open_existing(&Library::default_path()) {
                    let mut after = String::new();
                    loop {
                        let copies = match lib.offline_copy_page(&after) {
                            Ok(copies) => copies,
                            Err(_) => {
                                worker_progress
                                    .lock()
                                    .unwrap_or_else(|error| error.into_inner())
                                    .error = chilpa_i18n::t("the library could not be opened");
                                break;
                            }
                        };
                        if copies.is_empty() || worker_cancelled.load(Ordering::Acquire) {
                            break;
                        }
                        for copy in copies {
                            after = copy.key.clone();
                            if worker_cancelled.load(Ordering::Acquire) {
                                break;
                            }
                            let result = reconcile(&lib, &copy, &worker_cancelled);
                            let mut progress = worker_progress
                                .lock()
                                .unwrap_or_else(|error| error.into_inner());
                            match result {
                                Ok(changed) => {
                                    progress.done += 1;
                                    if changed {
                                        worker_changed.store(true, Ordering::Release);
                                    }
                                }
                                Err(error) => {
                                    progress.failed += 1;
                                    progress.error = error;
                                }
                            }
                        }
                    }
                } else {
                    worker_progress
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .error = chilpa_i18n::t("the library could not be opened");
                }
                worker_progress
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .running = false;
            });
        if let Ok(task) = task {
            self.servers.offline = State {
                task: Some(task),
                progress,
                cancelled,
                changed,
                removal: None,
            };
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
