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
    data_base().join(NAME)
}

/// The directory data goes under, before Chilpa's own folder: where the
/// desktop looks for application entries and icons too (`applications/` and
/// `icons/` on Linux). Same bases as [`data`].
pub fn data_base() -> PathBuf {
    from_env("XDG_DATA_HOME")
        .or_else(platform_data)
        .unwrap_or_else(std::env::temp_dir)
}

/// Where what can be thrown away and rebuilt goes: covers, mostly.
pub fn cache() -> PathBuf {
    from_env("XDG_CACHE_HOME")
        .or_else(platform_cache)
        .unwrap_or_else(std::env::temp_dir)
        .join(NAME)
}

/// Environment overrides for launching a child process with its own profile.
/// I leave the parent's environment alone so parallel tests remain independent.
pub fn profile_environment(root: &std::path::Path) -> [(&'static str, PathBuf); 3] {
    [
        ("XDG_CONFIG_HOME", root.join("config")),
        ("XDG_DATA_HOME", root.join("data")),
        ("XDG_CACHE_HOME", root.join("cache")),
    ]
}

/// The variables a desktop hands a launched program to raise its first
/// window: Wayland's activation token and X11's startup notification id. A
/// helper process the program starts must not inherit them, or it uses up
/// the token meant for the real window.
pub fn activation_variables() -> [&'static str; 2] {
    ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"]
}

/// What a Linux session calls its desktop, in the order QBZ's auto theme
/// asks: `XDG_CURRENT_DESKTOP`, `XDG_SESSION_DESKTOP`, `DESKTOP_SESSION`.
/// Each is `None` when unset or empty; elsewhere all three usually are.
pub fn desktop_names() -> [Option<String>; 3] {
    ["XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP", "DESKTOP_SESSION"]
        .map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()))
}

/// Whether the desktop asks for dark colours, as GNOME, GTK or KDE say.
/// False when none of them says anything.
///
/// Ported from QBZ's `prefer_dark_tray` (`qbz-qt/src/tray_linux.rs`), with
/// its early returns: the portal-less GNOME answer only ever says yes, while
/// GTK and KDE answer on the first line that names their key, a "no"
/// included, so `gtk-application-prefer-dark-theme=0` never reaches KDE's
/// file. One shot; it does not follow a switch made while Chilpa runs.
pub fn desktop_prefers_dark() -> bool {
    #[cfg(target_os = "linux")]
    let gnome = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned());
    #[cfg(not(target_os = "linux"))]
    let gnome: Option<String> = None;
    prefers_dark_from(gnome.as_deref(), &config_base())
}

fn prefers_dark_from(gnome: Option<&str>, config: &std::path::Path) -> bool {
    if gnome.is_some_and(|scheme| scheme.contains("prefer-dark")) {
        return true;
    }
    for toolkit in ["gtk-4.0", "gtk-3.0"] {
        if let Ok(settings) = std::fs::read_to_string(config.join(toolkit).join("settings.ini")) {
            for line in settings.lines() {
                if let Some(rest) = line.trim().strip_prefix("gtk-application-prefer-dark-theme") {
                    let value = rest.trim_start_matches(['=', ' ']);
                    return value.starts_with('1') || value.starts_with("true");
                }
            }
        }
    }
    if let Ok(globals) = std::fs::read_to_string(config.join("kdeglobals")) {
        for line in globals.lines() {
            if let Some(rest) = line.trim().strip_prefix("ColorScheme") {
                return rest.trim_start_matches(['=', ' ']).to_lowercase().contains("dark");
            }
        }
    }
    false
}

/// The packaging sandbox Chilpa runs in. Inside one, `/run`, `/proc/1`,
/// `os-release` and the desktop's configuration are the sandbox's, not the
/// host's, and the desktop is reached through its portals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sandbox {
    None,
    /// Marked by `/.flatpak-info`, which Flatpak puts at the root of every
    /// sandbox it builds.
    Flatpak,
    /// Marked by `SNAP`, which snapd sets for every confined program.
    Snap,
}

/// The sandbox this process runs in, read once: it cannot change while the
/// process lives. `None` away from Linux.
pub fn sandbox() -> Sandbox {
    static FOUND: std::sync::OnceLock<Sandbox> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| sandbox_from(std::path::Path::new("/.flatpak-info").exists(), from_env("SNAP").is_some()))
}

fn sandbox_from(flatpak_info: bool, snap: bool) -> Sandbox {
    if !cfg!(target_os = "linux") {
        Sandbox::None
    } else if flatpak_info {
        Sandbox::Flatpak
    } else if snap {
        Sandbox::Snap
    } else {
        Sandbox::None
    }
}

/// Where the document portal shows the files and folders a sandboxed
/// program was handed through the file chooser: `$XDG_RUNTIME_DIR/doc`, and
/// `/run/flatpak/doc`, the same mount, which Flatpak adds to every sandbox.
/// Each entry under them is `<id>/<name>`, with an id that stays the same
/// while the program keeps its permission. Empty away from Linux.
pub fn document_portal_mounts() -> Vec<PathBuf> {
    if !cfg!(target_os = "linux") {
        return Vec::new();
    }
    let mut mounts: Vec<PathBuf> = from_env("XDG_RUNTIME_DIR").map(|runtime| runtime.join("doc")).into_iter().collect();
    mounts.push(PathBuf::from("/run/flatpak/doc"));
    mounts
}

/// Where a Linux desktop mounts memory cards and USB drives: udisks2's
/// `/run/media`, and `/media`, which older systems and some distributions
/// use. A sandbox shows them only when it is given them. Empty elsewhere.
pub fn removable_media_roots() -> Vec<PathBuf> {
    if !cfg!(target_os = "linux") {
        return Vec::new();
    }
    vec![PathBuf::from("/run/media"), PathBuf::from("/media")]
}

/// The original image outside its read-only runtime mount, on Linux only.
pub fn appimage() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") { return None; }
    from_env("APPIMAGE").filter(|path| path.is_absolute() && path.is_file()).and_then(|path| path.canonicalize().ok())
}

/// An application-owned executable beside the running binary.
pub fn executable_helper(name: &str) -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let helper = executable.parent()?.join(name);
    helper.is_file().then_some(helper)
}

/// I keep the native mount table and its event endpoint beside the other OS conventions.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn mount_info_file() -> &'static std::path::Path { std::path::Path::new("/proc/self/mountinfo") }
#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn mount_events_file() -> &'static std::ffi::CStr { c"/proc/self/mounts" }
#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn block_device_node(device: &str) -> Option<PathBuf> {
    let (major, minor) = device.split_once(':')?;
    if major.is_empty() || minor.is_empty() || !major.bytes().chain(minor.bytes()).all(|b| b.is_ascii_digit()) { return None; }
    Some(std::path::Path::new("/sys/dev/block").join(device))
}
#[cfg(target_os = "macos")]
pub fn mounted_volume_directory() -> &'static std::path::Path { std::path::Path::new("/Volumes") }

/// PipeWire's own socket, when it is there: the one its client library
/// connects to, which is how its ALSA plugin, its JACK library and its tools
/// reach the graph. Its PulseAudio socket is another, and a sandbox can have
/// that one without this. `None` when there is no socket where the library
/// would look, and away from Linux.
pub fn pipewire_socket() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let path = pipewire_socket_from(
        from_env("PIPEWIRE_REMOTE"),
        from_env("PIPEWIRE_RUNTIME_DIR").or_else(|| from_env("XDG_RUNTIME_DIR")).or_else(|| from_env("USERPROFILE")),
    )?;
    is_socket(&path).then_some(path)
}

/// Where the client library looks, as `libpipewire-module-protocol-native`
/// does: `PIPEWIRE_REMOTE` when it is a path, otherwise that name, or
/// `pipewire-0`, under the first runtime directory that is set. A list of
/// names in `PIPEWIRE_REMOTE` is not read.
fn pipewire_socket_from(remote: Option<PathBuf>, runtime: Option<PathBuf>) -> Option<PathBuf> {
    let name = remote.unwrap_or_else(|| PathBuf::from("pipewire-0"));
    if name.is_absolute() {
        return Some(name);
    }
    runtime.map(|directory| directory.join(name))
}

#[cfg(unix)]
fn is_socket(path: &std::path::Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::metadata(path).is_ok_and(|found| found.file_type().is_socket())
}

#[cfg(not(unix))]
fn is_socket(_: &std::path::Path) -> bool {
    false
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
    fn the_data_is_the_desktops_base_plus_our_folder() {
        let _looking = LOOKING.lock();
        std::env::set_var("XDG_DATA_HOME", "/somewhere/share");
        assert_eq!(data_base(), PathBuf::from("/somewhere/share"));
        assert_eq!(data(), data_base().join("chilpa"));
        std::env::remove_var("XDG_DATA_HOME");
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

    #[test]
    fn pipewire_s_socket_is_looked_for_where_its_library_looks() {
        let runtime = Some(PathBuf::from("/run/user/1000"));
        assert_eq!(pipewire_socket_from(None, runtime.clone()), Some(PathBuf::from("/run/user/1000/pipewire-0")));
        assert_eq!(
            pipewire_socket_from(Some("other".into()), runtime.clone()),
            Some(PathBuf::from("/run/user/1000/other"))
        );
        assert_eq!(
            pipewire_socket_from(Some("/elsewhere/pw".into()), None),
            Some(PathBuf::from("/elsewhere/pw")),
            "a path needs no runtime directory"
        );
        assert_eq!(pipewire_socket_from(None, None), None, "a name with nowhere to look");
    }

    #[cfg(unix)]
    #[test]
    fn only_a_socket_counts_as_pipewire_s() {
        let root = desktop_files(&[("pipewire-0", "")]);
        assert!(!is_socket(&root.join("pipewire-0")), "a plain file is not a socket");
        assert!(!is_socket(&root.join("absent")));
        let listener = std::os::unix::net::UnixListener::bind(root.join("pipewire-1")).unwrap();
        assert!(is_socket(&root.join("pipewire-1")));
        drop(listener);
    }

    fn desktop_files(files: &[(&str, &str)]) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir()
            .join(format!("chilpa-dirs-dark-{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
        for (name, text) in files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn gnome_saying_dark_wins_over_files_that_say_light() {
        let root = desktop_files(&[("gtk-3.0/settings.ini", "gtk-application-prefer-dark-theme=0\n")]);
        assert!(prefers_dark_from(Some("'prefer-dark'\n"), &root));
    }

    #[test]
    fn gnome_saying_default_falls_through_to_gtk() {
        let root = desktop_files(&[("gtk-3.0/settings.ini", "[Settings]\ngtk-application-prefer-dark-theme=1\n")]);
        assert!(prefers_dark_from(Some("'default'\n"), &root));
    }

    #[test]
    fn a_gtk_no_is_final_and_never_asks_kde() {
        let root = desktop_files(&[
            ("gtk-4.0/settings.ini", "gtk-application-prefer-dark-theme=false\n"),
            ("kdeglobals", "ColorScheme=BreezeDark\n"),
        ]);
        assert!(!prefers_dark_from(None, &root));
    }

    #[test]
    fn without_a_gtk_key_kde_decides() {
        let root = desktop_files(&[
            ("gtk-3.0/settings.ini", "gtk-theme-name=adw-gtk3-dark\n"),
            ("kdeglobals", "[General]\nColorScheme=BreezeDark\n"),
        ]);
        assert!(prefers_dark_from(None, &root));
    }

    #[test]
    fn flatpak_marks_win_over_snap_and_only_linux_has_a_sandbox() {
        let expected = |sandbox: Sandbox| if cfg!(target_os = "linux") { sandbox } else { Sandbox::None };
        assert_eq!(sandbox_from(true, true), expected(Sandbox::Flatpak));
        assert_eq!(sandbox_from(false, true), expected(Sandbox::Snap));
        assert_eq!(sandbox_from(false, false), Sandbox::None);
    }

    #[test]
    fn the_document_portal_sits_under_the_runtime_directory() {
        let _looking = LOOKING.lock();
        let before = std::env::var_os("XDG_RUNTIME_DIR");
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        let mounts = document_portal_mounts();
        match before {
            Some(runtime) => std::env::set_var("XDG_RUNTIME_DIR", runtime),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }
        if cfg!(target_os = "linux") {
            assert_eq!(mounts, [PathBuf::from("/run/user/1000/doc"), PathBuf::from("/run/flatpak/doc")]);
        } else {
            assert!(mounts.is_empty());
        }
    }

    #[test]
    fn when_nothing_says_anything_it_is_light() {
        let root = desktop_files(&[]);
        assert!(!prefers_dark_from(None, &root));
    }
}
