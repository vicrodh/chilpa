//! glibc's retained worker heaps, bounded once at the start of the window's
//! `main`, before Qt or any thread of the app exists.

/// Two arenas, unless the environment already says how many.
pub fn cap() {
    let explicit_tunable = std::env::var_os("GLIBC_TUNABLES").is_some_and(|value| {
        value.to_string_lossy().split(':').any(|entry| {
            entry.split_once('=').is_some_and(|(key, _)| key == "glibc.malloc.arena_max")
        })
    });
    if std::env::var_os("MALLOC_ARENA_MAX").is_some() || explicit_tunable { return; }
    // SAFETY: the caller is the beginning of main, before Qt and the workers.
    if unsafe { libc::mallopt(libc::M_ARENA_MAX, 2) } != 1 {
        eprintln!("memory: could not limit glibc malloc arenas");
    }
}
