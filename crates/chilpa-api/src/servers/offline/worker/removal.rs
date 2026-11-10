//! Confirmation captures a bounded file set. Removal shares the one worker.

use super::*;

pub(super) struct Pending {
    ticket: String,
    copies: Vec<Copy>,
    scopes: HashMap<i64, Option<super::super::super::session::Scope>>,
}

impl Chilpa {
    pub fn server_offline_removal_info(&mut self, kind: &str, selected: &str) -> String {
        self.servers.offline.removal = None;
        if self.servers.offline.task.is_some() {
            return json!({"error":chilpa_i18n::t("A download is still in progress.")}).to_string();
        }
        let refused = || {
            json!({"error":chilpa_i18n::t("Choose fewer offline files for this action.")})
                .to_string()
        };
        let Some(lib) = self.library() else {
            return refused();
        };
        let copies = if kind == "downloads" {
            match lib.offline_all_copies() {
                Ok(copies) => copies,
                Err(_) => return refused(),
            }
        } else {
            let mut copies = Vec::new();
            let mut bytes = 0usize;
            for track in lib
                .offline_selection(kind, &ids(selected))
                .unwrap_or_default()
            {
                let Some(stored) = lib.server(track.source).ok().flatten() else {
                    return refused();
                };
                let batch = match lib.offline_copies_for(
                    track.source,
                    &stored.account,
                    &stored.identity,
                    &track.entry,
                ) {
                    Ok(copies) => copies,
                    Err(_) => return refused(),
                };
                for copy in batch {
                    bytes = bytes.saturating_add(copy.retained_bytes());
                    if bytes > MAX_SELECTION_BYTES || copies.len() >= MAX_TRACKS {
                        return refused();
                    }
                    copies.push(copy);
                }
            }
            copies
        };
        let mut size: usize = copies.iter().map(Copy::retained_bytes).sum();
        if copies.is_empty() {
            return json!({"error":chilpa_i18n::t("No downloaded files were found in this selection.")}).to_string();
        }
        if copies.len() > MAX_TRACKS || size > MAX_SELECTION_BYTES {
            return refused();
        }
        let mut scopes = HashMap::new();
        for copy in &copies {
            if scopes.contains_key(&copy.source) {
                continue;
            }
            let scope = lib
                .server(copy.source)
                .ok()
                .flatten()
                .map(|server| super::super::super::session::Scope::of(&server));
            size = size.saturating_add(
                scope
                    .as_ref()
                    .map_or(0, super::super::super::session::Scope::retained_bytes),
            );
            if size > MAX_SELECTION_BYTES {
                return refused();
            }
            scopes.insert(copy.source, scope);
        }
        let ticket = super::super::super::keys::mint();
        let answer=json!({"ticket":ticket,"count":copies.len(),"bytes":copies.iter().map(|copy| copy.bytes).sum::<u64>()}).to_string();
        self.servers.offline.removal = Some(Pending {
            ticket,
            copies,
            scopes,
        });
        answer
    }

    pub fn server_offline_dismiss_removal(&mut self) {
        self.servers.offline.removal = None;
    }

    pub fn server_offline_remove(&mut self, ticket: &str) -> String {
        let refused = || {
            json!({"error":chilpa_i18n::t("The selected music changed. Open its menu and confirm again.")}).to_string()
        };
        if self.servers.offline.task.is_some() {
            return refused();
        }
        if !self
            .servers
            .offline
            .removal
            .as_ref()
            .is_some_and(|pending| pending.ticket == ticket)
        {
            return refused();
        }
        let Some(pending) = self.servers.offline.removal.take() else {
            return refused();
        };
        let progress = Arc::new(Mutex::new(Progress {
            removing: true,
            total: pending.copies.len(),
            running: true,
            ..Default::default()
        }));
        let cancelled = Arc::new(AtomicBool::new(false));
        let changed = Arc::new(AtomicBool::new(false));
        let (worker_progress, worker_cancelled, worker_changed) =
            (progress.clone(), cancelled.clone(), changed.clone());
        let task = std::thread::Builder::new()
            .name("chilpa-offline-remove".into())
            .spawn(move || {
                let result = Library::open_existing(&Library::default_path());
                if let Ok(lib) = result {
                    for copy in pending.copies {
                        if worker_cancelled.load(Ordering::Acquire) {
                            break;
                        }
                        let result = remove(
                            &lib,
                            &copy,
                            pending.scopes.get(&copy.source).and_then(Option::as_ref),
                        );
                        let mut progress = worker_progress
                            .lock()
                            .unwrap_or_else(|error| error.into_inner());
                        match result {
                            Ok(()) => {
                                progress.done += 1;
                                worker_changed.store(true, Ordering::Release);
                            }
                            Err(error) => {
                                progress.failed += 1;
                                progress.error = error;
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
        let Ok(task) = task else {
            return refused();
        };
        self.servers.offline = State {
            task: Some(task),
            cancelled,
            progress,
            changed,
            removal: None,
        };
        self.server_offline_status()
    }
}

pub(super) fn remove(
    lib: &Library,
    copy: &Copy,
    scope: Option<&super::super::super::session::Scope>,
) -> Result<(), String> {
    let changed = super::super::super::session::changed;
    match (scope, lib.server(copy.source).ok().flatten()) {
        (Some(scope), Some(server)) if scope.matches(&server) => {}
        (None, None) => {}
        _ => return Err(changed()),
    }
    let path = Path::new(&copy.path);
    if copy.key.len() != 64
        || !copy.key.bytes().all(|byte| byte.is_ascii_hexdigit())
        || path.file_stem().and_then(|name| name.to_str()) != Some(copy.key.as_str())
    {
        return Err(changed());
    }
    let failed = || {
        chilpa_i18n::t(
            "The offline file could not be removed. Check playback and folder permissions.",
        )
    };
    let gate = super::super::leases::gate(&copy.key).ok_or_else(failed)?;
    let users = gate.lock().unwrap_or_else(|error| error.into_inner());
    if *users > 0 {
        return Err(chilpa_i18n::t(
            "This offline file is in use. Stop playback before removing it.",
        ));
    }
    if !lib.begin_offline_removal(copy).map_err(|_| failed())? {
        return Err(changed());
    }
    for file in [path.to_path_buf(), path.with_extension("part")] {
        if let Err(error) = std::fs::remove_file(file) {
            if error.kind() != std::io::ErrorKind::NotFound {
                lib.invalidate_offline_copy(copy, &failed(), super::super::disk_bytes(copy))
                    .map_err(|_| failed())?;
                return Err(failed());
            }
        }
    }
    lib.finish_offline_removal(copy).map_err(|_| failed())
}
