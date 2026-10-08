//! Where Chilpa's own files live, on each platform it ships to.
//!
//! There used to be four copies of the XDG convention written out by hand,
//! one per crate that needed a directory. On Windows that is not
//! `%APPDATA%`, on macOS it is not `~/Library/Application Support`, and on
//! Android it is not anything: the four copies were four bugs waiting for
//! the day someone built for another platform (ADR-010 M3).
//!
//! This is the one place that knows. Everything else asks.
//!
//! **The environment always wins.** `XDG_CONFIG_HOME` and its siblings are
//! honoured on every platform, not only Linux, because that is what the
//! tests set and what someone moving their library between machines
//! reaches for. It is also the one knob that works the same everywhere.

use std::path::PathBuf;

/// The folder Chilpa's files sit in, under whichever base applies.
const NAME: &str = "chilpa";

/// Where settings go.
///
/// - Linux, and anything unnamed below: `$XDG_CONFIG_HOME` or `~/.config`
/// - Windows: `%APPDATA%`
/// - macOS: `~/Library/Application Support`
/// - Android: the app's private directory, which the shell hands over in
///   `CHILPA_HOME` because there is no `$HOME` worth the name
pub fn config() -> PathBuf {
    config_base().join(NAME)
}

/// The directory settings go under, before Chilpa's own folder: where the
/// desktop keeps its configuration too (KDE's `plasma-org.kde.plasma.
/// desktop-appletsrc` sits here on Linux). Same bases as [`config`].
pub fn config_base() -> PathBuf {
    from_env("XDG_CONFIG_HOME")
        .or_else(platform_config)
        .unwrap_or_else(std::env::temp_dir)
}

/// Where the library, the plugins and anything else that must survive go.
pub fn data() -> PathBuf {
    from_env("XDG_DATA_HOME")
        .or_else(platform_data)
        .unwrap_or_else(std::env::temp_dir)
        .join(NAME)
}

/// Where what can be thrown away and rebuilt goes: covers, mostly.
pub fn cache() -> PathBuf {
    from_env("XDG_CACHE_HOME")
        .or_else(platform_cache)
        .unwrap_or_else(std::env::temp_dir)
        .join(NAME)
}

/// A variable, when it is set to something.
///
/// An empty value is the same as unset: a shell that exports `XDG_DATA_HOME=`
/// means it has none, and joining onto "" would put the library at the root.
fn from_env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

/// The user's home, by whatever name this platform gives it.
///
/// `CHILPA_HOME` comes first because on Android there is nothing else: the
/// app's private directory is handed to the process, not discovered by it.
fn home() -> Option<PathBuf> {
    from_env("CHILPA_HOME").or_else(|| from_env("HOME")).or_else(|| from_env("USERPROFILE"))
}

#[cfg(target_os = "windows")]
fn platform_config() -> Option<PathBuf> {
    from_env("APPDATA").or_else(|| home().map(|h| h.join("AppData").join("Roaming")))
}

#[cfg(target_os = "windows")]
fn platform_data() -> Option<PathBuf> {
    platform_config()
}

#[cfg(target_os = "windows")]
fn platform_cache() -> Option<PathBuf> {
    from_env("LOCALAPPDATA")
        .or_else(|| home().map(|h| h.join("AppData").join("Local")))
        .map(|p| p.join("cache"))
}

#[cfg(target_os = "macos")]
fn platform_config() -> Option<PathBuf> {
    home().map(|h| h.join("Library").join("Application Support"))
}

#[cfg(target_os = "macos")]
fn platform_data() -> Option<PathBuf> {
    platform_config()
}

#[cfg(target_os = "macos")]
fn platform_cache() -> Option<PathBuf> {
    home().map(|h| h.join("Library").join("Caches"))
}

// Android has no XDG layout and no home worth the name, but it does have a
// private directory the app is given. The shell puts it in `CHILPA_HOME`;
// the split into files/ and cache/ is the platform's own.
#[cfg(target_os = "android")]
fn platform_config() -> Option<PathBuf> {
    home().map(|h| h.join("files"))
}

#[cfg(target_os = "android")]
fn platform_data() -> Option<PathBuf> {
    platform_config()
}

#[cfg(target_os = "android")]
fn platform_cache() -> Option<PathBuf> {
    home().map(|h| h.join("cache"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
fn platform_config() -> Option<PathBuf> {
    home().map(|h| h.join(".config"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
fn platform_data() -> Option<PathBuf> {
    home().map(|h| h.join(".local").join("share"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
fn platform_cache() -> Option<PathBuf> {
    home().map(|h| h.join(".cache"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three of them share a process's environment, so they take turns.
    static LOOKING: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn the_environment_wins_on_every_platform() {
        let _looking = LOOKING.lock();
        std::env::set_var("XDG_DATA_HOME", "/somewhere/else");
        assert_eq!(data(), PathBuf::from("/somewhere/else/chilpa"));
        std::env::remove_var("XDG_DATA_HOME");
    }

    #[test]
    fn an_empty_variable_is_the_same_as_none() {
        // Joining onto "" would put the library at the root of the disk.
        let _looking = LOOKING.lock();
        std::env::set_var("XDG_CONFIG_HOME", "");
        assert_ne!(config(), PathBuf::from("/chilpa"));
        assert!(config().ends_with("chilpa"));
        std::env::remove_var("XDG_CONFIG_HOME");
    }

    #[test]
    fn the_config_is_the_desktops_base_plus_our_folder() {
        let _looking = LOOKING.lock();
        std::env::set_var("XDG_CONFIG_HOME", "/somewhere/config");
        assert_eq!(config_base(), PathBuf::from("/somewhere/config"));
        assert_eq!(config(), config_base().join("chilpa"));
        std::env::remove_var("XDG_CONFIG_HOME");
    }

    #[test]
    fn everything_ends_up_under_a_folder_of_ours() {
        // Whatever the base turns out to be, including the temp directory
        // when there is no home at all.
        let _looking = LOOKING.lock();
        for path in [config(), data(), cache()] {
            assert!(path.ends_with("chilpa"), "{path:?}");
        }
    }
}
