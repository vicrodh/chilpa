//! This installation's identifier, which serves as salt for the fallback.
//!
//! **It is not a secret.** Keeping it in the clear is fine. It is there for
//! two things: so two installations on the same machine derive different
//! keys, and so moving the directory to another machine breaks the
//! derivation, because the `machine-id` there is another one.
//!
//! # One id, however many ask at once
//!
//! Several boxes open at startup over the same directory, each on a thread of
//! its own, and a second process can be starting too. Every one of them has
//! to end up with the id on disk: a box holding any other seals what no later
//! start can open.
//!
//! The first id is made under `File::lock` on a file beside it, which is
//! `flock` on Linux, macOS and Android and `LockFileEx` on Windows. It holds
//! across threads because each opener has its own handle, across processes,
//! and the system drops it when the holder dies, so a crash leaves no stale
//! lock. Where the filesystem refuses locks, `fs::hard_link` elects the id
//! instead, because it never replaces an existing name.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use uuid::Uuid;

const FILE: &str = "install-id";
const LOCK: &str = "install-id.lock";

/// The id this install already has, if any. Nothing is written.
pub(crate) fn load(storage_dir: &Path) -> Option<Uuid> {
    read(storage_dir).ok().flatten()
}

/// What the file says: `None` when it is missing or its contents are not an
/// id. Any other failure says nothing about the id, which may be fine, and
/// comes back as the error it is.
fn read(storage_dir: &Path) -> io::Result<Option<Uuid>> {
    let bytes = match fs::read(storage_dir.join(FILE)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    Ok(std::str::from_utf8(&bytes).ok().and_then(|text| Uuid::parse_str(text.trim()).ok()))
}

pub(crate) fn load_or_create(storage_dir: &Path) -> io::Result<Uuid> {
    if let Some(uuid) = read(storage_dir)? {
        return Ok(uuid);
    }
    // A missing one, or one whose contents are not an id, is made anew.
    // Whatever was encrypted with a previous one is lost, and that is better
    // than failing to start. A file that only failed to read is never
    // replaced: that would lose everything derived from a good id.
    fs::create_dir_all(storage_dir)?;

    // Held until this returns, past the directory sync, so whoever waited
    // here finds a finished id on the next line.
    let lock = take_lock(storage_dir);
    if let Some(uuid) = read(storage_dir)? {
        return Ok(uuid);
    }

    let uuid = Uuid::new_v4();
    // It is written aside and published in one step: a power cut mid write
    // would leave a truncated id, and a truncated id is a different key. The
    // name is this attempt's own, so nobody else truncates it or moves it.
    let tmp = storage_dir.join(format!("{FILE}.{uuid}.tmp"));
    let published = write_synced(&tmp, &uuid).and_then(|()| publish(&tmp, storage_dir, lock.is_some()));
    // After a rename there is nothing left here to remove.
    let _ = fs::remove_file(&tmp);
    published?;
    sync_directory(storage_dir);

    // What is on disk is the id, whoever wrote it.
    read(storage_dir)?.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "the install id did not read back"))
}

/// The lock on making the id; `None` where this filesystem has none to give.
fn take_lock(storage_dir: &Path) -> Option<fs::File> {
    // The file stays: removing it would let the next opener lock a new file
    // while a waiter still gets the lock on the old one.
    let file = fs::OpenOptions::new().create(true).truncate(false).write(true).open(storage_dir.join(LOCK)).ok()?;
    loop {
        match file.lock() {
            Ok(()) => return Some(file),
            // `flock` returns EINTR if a signal lands while it waits.
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}

fn write_synced(tmp: &Path, uuid: &Uuid) -> io::Result<()> {
    let mut file = fs::OpenOptions::new().create_new(true).write(true).open(tmp)?;
    file.write_all(uuid.to_string().as_bytes())?;
    file.sync_all()
}

/// Puts the temporary at the id's name, whole or not at all. Holding the lock,
/// nobody else is publishing, and a rename also replaces contents that are not
/// an id.
fn publish(tmp: &Path, storage_dir: &Path, locked: bool) -> io::Result<()> {
    let path = storage_dir.join(FILE);
    if !locked {
        match fs::hard_link(tmp, &path) {
            Ok(()) => return Ok(()),
            // Another thread or process won, and its id is the one. Failing
            // to read it is an error, never a reason to replace it.
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if read(storage_dir)?.is_some() {
                    return Ok(());
                }
            }
            // No hard links here either. With neither primitive nothing can
            // elect an id: the last rename wins, and the read back narrows
            // the window without closing it.
            Err(_) => {}
        }
    }
    fs::rename(tmp, path)
}

/// A new name lasts through a power cut once its directory is on disk. Windows
/// does not open a directory this way, so there this does nothing.
fn sync_directory(dir: &Path) {
    if let Ok(directory) = fs::File::open(dir) {
        let _ = directory.sync_all();
    }
}

/// The machine's identifier, if the system gives one.
///
/// Its absence is acceptable: the derivation falls back to the install
/// UUID, which is still tied to the machine because the directory is.
pub(crate) fn machine_id() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        for candidate in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
            if let Ok(contents) = fs::read_to_string(candidate) {
                let trimmed = contents.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output =
            Command::new("ioreg").args(["-rd1", "-c", "IOPlatformExpertDevice"]).output().ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let Some(idx) = line.find("IOPlatformUUID") else { continue };
            let tail = &line[idx..];
            let Some(start) = tail.find('"').map(|i| i + 1) else { continue };
            let rest = &tail[start..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
        None
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        // Without this, Windows opens a console for `reg` and flashes a black
        // box on every startup.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let output = Command::new("reg")
            .creation_flags(CREATE_NO_WINDOW)
            .args(["query", "HKLM\\SOFTWARE\\Microsoft\\Cryptography", "/v", "MachineGuid"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("MachineGuid") {
                return line.split_whitespace().last().map(str::to_string).filter(|s| !s.is_empty());
            }
        }
        None
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Barrier;
    use std::time::{Duration, Instant};

    const ROUNDS: usize = 200;
    const THREADS: usize = 16;

    /// Every thread's answer, with all of them let go at the same moment.
    fn race(dir: &Path, threads: usize) -> Vec<std::io::Result<Uuid>> {
        let gate = Barrier::new(threads);
        std::thread::scope(|scope| {
            let racers: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| {
                        gate.wait();
                        load_or_create(dir)
                    })
                })
                .collect();
            racers.into_iter().map(|racer| racer.join().unwrap()).collect()
        })
    }

    fn on_disk(dir: &Path) -> String {
        fs::read_to_string(dir.join("install-id")).unwrap_or_default()
    }

    /// Everyone got the same id, and it is the one in the file. Returns it.
    fn one_id_on_disk(dir: &Path, answers: Vec<std::io::Result<Uuid>>) -> Uuid {
        let ids: Vec<Uuid> = answers.into_iter().map(|answer| answer.expect("nobody fails")).collect();
        assert!(ids.iter().all(|id| *id == ids[0]), "one id for all: {ids:?}");
        assert_eq!(on_disk(dir), ids[0].to_string(), "the id in use is the one on disk");
        ids[0]
    }

    /// What is left in the directory, sorted.
    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn threads_racing_on_a_fresh_directory_agree_on_the_id_on_disk() {
        let (mut failed_calls, mut several_ids, mut off_the_disk) = (0, 0, 0);
        let mut first_error = None;
        for _ in 0..ROUNDS {
            let root = tempfile::tempdir().unwrap();
            // Not created beforehand: making the directory is part of the race.
            let dir = root.path().join("fresh");
            let mut ids = Vec::new();
            for answer in race(&dir, THREADS) {
                match answer {
                    Ok(id) => ids.push(id),
                    Err(e) => {
                        failed_calls += 1;
                        first_error.get_or_insert(e.to_string());
                    }
                }
            }
            several_ids += usize::from(ids.iter().any(|id| *id != ids[0]));
            off_the_disk += usize::from(ids.iter().any(|id| id.to_string() != on_disk(&dir)));
        }
        assert!(
            failed_calls == 0 && several_ids == 0 && off_the_disk == 0,
            "{ROUNDS} rounds of {THREADS} threads: {failed_calls} calls failed (first: {first_error:?}), \
             {several_ids} rounds handed out several ids, {off_the_disk} rounds handed out an id not on disk"
        );
    }

    #[test]
    fn the_id_on_disk_is_whole_every_time_it_is_read_during_a_race() {
        for _ in 0..ROUNDS / 4 {
            let root = tempfile::tempdir().unwrap();
            let done = AtomicBool::new(false);
            let torn = std::thread::scope(|scope| {
                let watcher = scope.spawn(|| {
                    let mut torn = Vec::new();
                    while !done.load(Ordering::Relaxed) {
                        if let Ok(contents) = fs::read_to_string(root.path().join("install-id")) {
                            if Uuid::parse_str(&contents).is_err() {
                                torn.push(contents);
                            }
                        }
                    }
                    torn
                });
                let _ = race(root.path(), THREADS);
                done.store(true, Ordering::Relaxed);
                watcher.join().unwrap()
            });
            assert_eq!(torn, Vec::<String>::new(), "a partial id is a different key");
        }
    }

    /// Set by the test below for the processes it starts.
    const RACE_ROOT: &str = "CHILPA_SECRETS_TEST_RACE_ROOT";

    /// One process of the race below. On its own it does nothing.
    #[test]
    #[ignore = "started by processes_racing_on_a_fresh_directory_agree_on_the_id_on_disk"]
    fn one_racing_process() {
        let Some(root) = std::env::var_os(RACE_ROOT).map(PathBuf::from) else { return };
        let pid = std::process::id();
        fs::write(root.join(format!("ready-{pid}")), "").unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        while !root.join("go").exists() {
            assert!(Instant::now() < deadline, "the race never started");
            std::thread::yield_now();
        }
        let id = load_or_create(&root.join("fresh")).expect("nobody fails");
        fs::write(root.join(format!("answer-{pid}")), id.to_string()).unwrap();
    }

    #[test]
    fn processes_racing_on_a_fresh_directory_agree_on_the_id_on_disk() {
        const PROCESSES: usize = 8;
        let this_test_binary = std::env::current_exe().unwrap();
        for _ in 0..10 {
            let root = tempfile::tempdir().unwrap();
            let children: Vec<_> = (0..PROCESSES)
                .map(|_| {
                    Command::new(&this_test_binary)
                        .args(["--exact", "install_id::tests::one_racing_process", "--ignored"])
                        .env(RACE_ROOT, root.path())
                        // The test harness reports a child's panic on stdout.
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null())
                        .spawn()
                        .unwrap()
                })
                .collect();
            let deadline = Instant::now() + Duration::from_secs(60);
            while names_in(root.path()).iter().filter(|name| name.starts_with("ready-")).count() < PROCESSES {
                assert!(Instant::now() < deadline, "the processes never got ready");
                std::thread::sleep(Duration::from_millis(1));
            }
            fs::write(root.path().join("go"), "").unwrap();
            for child in children {
                let output = child.wait_with_output().unwrap();
                assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
            }
            let id = on_disk(&root.path().join("fresh"));
            let answers: Vec<String> = names_in(root.path())
                .iter()
                .filter(|name| name.starts_with("answer-"))
                .map(|name| fs::read_to_string(root.path().join(name)).unwrap())
                .collect();
            assert_eq!(answers.len(), PROCESSES);
            assert!(answers.iter().all(|answer| *answer == id), "the file holds {id}: {answers:?}");
        }
    }

    #[test]
    fn what_a_crash_left_behind_is_neither_used_nor_in_the_way() {
        let foreign = Uuid::new_v4();
        for _ in 0..ROUNDS / 10 {
            let root = tempfile::tempdir().unwrap();
            let dir = root.path();
            // A whole id at the name earlier versions used as their temporary,
            // a torn temporary, and a lock whose holder is gone.
            fs::write(dir.join("install-id.tmp"), foreign.to_string()).unwrap();
            fs::write(dir.join(format!("install-id.{foreign}.tmp")), "1234").unwrap();
            fs::write(dir.join("install-id.lock"), "left over").unwrap();
            let id = one_id_on_disk(dir, race(dir, THREADS));
            assert_ne!(id, foreign, "a temporary is never adopted");
        }
    }

    #[test]
    fn a_directory_at_the_old_temporary_name_is_not_in_the_way() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("install-id.tmp")).unwrap();
        one_id_on_disk(root.path(), race(root.path(), THREADS));
    }

    #[test]
    fn without_a_usable_lock_the_threads_still_agree_on_one_id() {
        // A directory where the lock file goes makes the lock unavailable, as
        // a filesystem without locks does.
        for _ in 0..ROUNDS / 2 {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir(root.path().join("install-id.lock")).unwrap();
            one_id_on_disk(root.path(), race(root.path(), THREADS));
        }
    }

    #[test]
    fn an_unreadable_id_is_remade_once_however_many_threads_find_it() {
        let whole = Uuid::new_v4().to_string();
        let unreadable: [&[u8]; 3] = [b"", &whole.as_bytes()[..20], &[0xff, 0xfe, 0x00]];
        for _ in 0..ROUNDS / 10 {
            for contents in unreadable {
                let root = tempfile::tempdir().unwrap();
                fs::write(root.path().join("install-id"), contents).unwrap();
                one_id_on_disk(root.path(), race(root.path(), THREADS));
            }
        }
    }

    #[test]
    fn without_a_usable_lock_an_unreadable_id_is_still_remade() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("install-id.lock")).unwrap();
        fs::write(root.path().join("install-id"), "torn").unwrap();
        one_id_on_disk(root.path(), vec![load_or_create(root.path())]);
    }

    #[test]
    fn an_id_already_on_disk_is_kept_and_nothing_is_written() {
        let root = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4();
        // A line ending is how an id that went through an editor looks.
        let contents = format!("{id}\n");
        fs::write(root.path().join("install-id"), &contents).unwrap();
        for answer in race(root.path(), THREADS) {
            assert_eq!(answer.unwrap(), id);
        }
        assert_eq!(on_disk(root.path()), contents);
        assert_eq!(names_in(root.path()), ["install-id"]);
    }

    /// Takes every permission off the file. `false` when that does not stop
    /// a read, as for root, and a test built on it has nothing to show.
    #[cfg(unix)]
    fn make_unreadable(file: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(file, fs::Permissions::from_mode(0o000)).unwrap();
        fs::read(file).is_err()
    }

    #[cfg(unix)]
    fn make_readable(file: &Path) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(file, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn a_valid_id_that_cannot_be_read_right_now_is_not_replaced() {
        let root = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4();
        let file = root.path().join("install-id");
        fs::write(&file, id.to_string()).unwrap();
        if !make_unreadable(&file) {
            return;
        }
        let answer = load_or_create(root.path());
        assert_eq!(answer.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(names_in(root.path()), ["install-id"], "nothing is written");
        make_readable(&file);
        assert_eq!(load_or_create(root.path()).unwrap(), id, "everything derived from it still opens");
    }

    #[test]
    #[cfg(unix)]
    fn without_the_lock_a_valid_id_that_cannot_be_read_is_not_replaced() {
        let root = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4();
        let file = root.path().join("install-id");
        fs::write(&file, id.to_string()).unwrap();
        let tmp = root.path().join("candidate.tmp");
        fs::write(&tmp, Uuid::new_v4().to_string()).unwrap();
        if !make_unreadable(&file) {
            return;
        }
        assert!(publish(&tmp, root.path(), false).is_err());
        make_readable(&file);
        assert_eq!(on_disk(root.path()), id.to_string());
    }

    #[test]
    fn something_at_the_id_name_that_is_not_a_file_is_an_error_and_stays() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("install-id");
        fs::create_dir(&path).unwrap();
        assert!(load_or_create(root.path()).is_err());
        assert!(path.is_dir());
        assert_eq!(names_in(root.path()), ["install-id"], "nothing is written");
    }

    #[test]
    fn a_race_leaves_only_the_id_and_the_lock() {
        let root = tempfile::tempdir().unwrap();
        one_id_on_disk(root.path(), race(root.path(), THREADS));
        assert_eq!(names_in(root.path()), ["install-id", "install-id.lock"]);
    }
}
