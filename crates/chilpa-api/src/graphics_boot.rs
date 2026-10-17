//! The graphics at startup: what draws the window, chosen before Qt exists,
//! and how a choice that cannot start is undone. Ported from QBZ
//! (`renderer_qt.rs`, `renderer_preflight_linux.rs` and the renderer half of
//! its `main.rs`); the order a launch meets the pieces is the window's
//! `main`:
//!
//! 1. the interface size, as `QT_SCALE_FACTOR`, before any Qt object;
//! 2. a saved GPU, proved in a disposable child before the window uses it
//!    ([`gpu`]);
//! 3. the renderer preference, as the environment Qt reads, with a sentinel
//!    armed for a choice that forces a backend;
//! 4. managed Auto: a child proves the default backend presents frames, or
//!    that software does, and then this launch alone falls to software
//!    ([`preflight`]);
//! 5. once the window exists: the api Qt actually gave (the GPU tier) and
//!    the frames it presented, which disarm the sentinels.
//!
//! A sentinel still armed at the next start means that launch never
//! presented a frame: the choice goes back to Auto, so a renderer cannot
//! lock anyone out of Settings.
//!
//! Nothing here holds a Qt type. The window's crate draws the probe window
//! and lists Qt's Vulkan devices, and hands what it finds to this module.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub mod gpu;
pub mod inventory;
pub mod preflight;

/// Settings > Appearance > Renderer.
pub const RENDERER: &str = "appearance.renderer";
/// Settings > Appearance > Interface size.
pub const INTERFACE_SIZE: &str = "appearance.interfaceSize";

/// The renderer row, in QBZ's order. The GPU-tier names all mean Qt's own
/// default backend; `gl` forces OpenGL and `software` Qt's rasterizer.
pub const RENDERER_CHOICES: [(&str, &str); 4] = [
    ("auto", chilpa_i18n::mark("Auto (recommended)")),
    ("wgpu", chilpa_i18n::mark("GPU")),
    ("gl", chilpa_i18n::mark("GPU (compatibility)")),
    ("software", chilpa_i18n::mark("Software")),
];

/// The interface sizes, in QBZ's order.
pub const SIZE_CHOICES: [(&str, &str); 5] = [
    ("xs", chilpa_i18n::mark("Extra small")),
    ("small", chilpa_i18n::mark("Small")),
    ("default", chilpa_i18n::mark("Default")),
    ("large", chilpa_i18n::mark("Large")),
    ("xl", chilpa_i18n::mark("Extra large")),
];

/// The renderer named in the environment wins over the preference for one
/// launch; it never arms the sentinel, which would reset a choice the user
/// did not make.
const RENDERER_ENV: &str = "CHILPA_RENDERER";
/// Forces the GPU tier on for verification runs whose probe reports "null"
/// while a real RHI draws (offscreen with a forced Vulkan backend).
const FORCE_GPU_TIER_ENV: &str = "CHILPA_FORCE_GPU_TIER";
const SCALE_ENV: &str = "QT_SCALE_FACTOR";
const RHI_ENV: &str = "QSG_RHI_BACKEND";
const QUICK_ENV: &str = "QT_QUICK_BACKEND";

/// How long the window watches its frames before calling the start alive:
/// a renderer that dies two seconds in is not healthy, and the next launch
/// would keep forcing it.
pub const LIVENESS_WINDOW_MS: i32 = 10_000;

// ---------------------------------------------------------------------------
// Preferences, read and written before the facade exists
// ---------------------------------------------------------------------------

fn pref(key: &str, default: &str) -> String {
    chilpa_core::settings::load()
        .window
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn save_prefs(pairs: &[(&str, &str)]) {
    let mut settings = chilpa_core::settings::load();
    for (key, value) in pairs {
        settings.window.insert(key.to_string(), value.to_string());
    }
    if let Err(error) = chilpa_core::settings::save(&settings) {
        eprintln!("graphics: the preference could not be written: {error}");
    }
}

// ---------------------------------------------------------------------------
// The environment
// ---------------------------------------------------------------------------

/// A variable set to something. `var_os(..).is_some()` answers yes for an
/// empty value, which is what a shell leaves when it clears a variable
/// instead of unsetting it, and then the whole preference silently stopped
/// applying (QBZ, `main.rs:3624-3651`).
fn value_overrides(value: Option<std::ffi::OsString>) -> bool {
    value.is_some_and(|v| !v.to_string_lossy().trim().is_empty())
}

pub(crate) fn env_overrides(name: &str) -> bool {
    value_overrides(std::env::var_os(name))
}

// ---------------------------------------------------------------------------
// Interface size
// ---------------------------------------------------------------------------

/// The factor of an interface size, QBZ's table.
fn factor_for(size: &str) -> f32 {
    match size {
        "xs" => 0.8,
        "small" => 0.9,
        "large" => 1.2,
        "xl" => 1.5,
        _ => 1.0,
    }
}

/// The interface size, as Qt's own high-DPI factor. It must be set before
/// the application object exists: Qt then lays the scene out in logical
/// pixels and rasterizes text at the effective ratio, where a QML scale
/// would enlarge an already drawn scene and blur it. A value already in the
/// environment is kept. Children inherit it, so they draw at the same size.
pub fn apply_interface_size() {
    if env_overrides(SCALE_ENV) {
        eprintln!("graphics: explicit {SCALE_ENV} preserved");
        return;
    }
    let factor = factor_for(&pref(INTERFACE_SIZE, "default"));
    std::env::set_var(SCALE_ENV, factor.to_string());
}

/// The factor Qt is actually using: the environment's when it carries one,
/// the preference's otherwise.
pub fn interface_scale() -> f32 {
    std::env::var(SCALE_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or_else(|| factor_for(&pref(INTERFACE_SIZE, "default")))
}

/// A new interface size, with the kept window size rebased so the window
/// keeps its footprint on the screen: the size is kept in logical pixels,
/// and without this the next launch would multiply the old ones by the new
/// factor (QBZ, `set_ui_scale_in_doc`).
pub(crate) fn rebase_window_size(window: &mut std::collections::BTreeMap<String, String>, size: &str) -> bool {
    let old = window.get(INTERFACE_SIZE).cloned().unwrap_or_else(|| "default".to_string());
    if old == size {
        return false;
    }
    let ratio = f64::from(factor_for(&old)) / f64::from(factor_for(size));
    for key in ["window.width", "window.height"] {
        if let Some(value) = window.get(key).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.0) {
            window.insert(key.to_string(), ((value * ratio).round() as i64).to_string());
        }
    }
    window.insert(INTERFACE_SIZE.to_string(), size.to_string());
    true
}

// ---------------------------------------------------------------------------
// The renderer preference and its sentinel
// ---------------------------------------------------------------------------

/// `<data>/renderer_attempt`: armed before a forced backend starts, cleared
/// once the window proves it presents frames.
fn renderer_sentinel() -> PathBuf {
    chilpa_dirs::data().join("renderer_attempt")
}

fn arm(path: &Path, marker: &str) -> bool {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, marker).is_ok()
}

fn armed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn disarm(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// What the renderer row asks of Qt: the variable it sets, or none for a
/// choice that means Qt's default. `true` when the choice forces a backend,
/// which is the only kind that can keep a launch from starting and the
/// only kind that arms the sentinel.
fn force_renderer(choice: &str) -> bool {
    match choice {
        "software" | "cpu" | "soft" => {
            std::env::set_var(QUICK_ENV, "software");
            eprintln!("renderer: '{choice}' -> {QUICK_ENV}=software");
            true
        }
        "gl" | "gles" | "femtovg" => {
            if cfg!(target_os = "macos") {
                eprintln!("renderer: '{choice}' on macOS -> Metal, the default");
                false
            } else if cfg!(target_os = "windows") {
                // Direct3D 11 is Qt's healthy default there; desktop GL is a
                // step down, not the GPU tier the row means.
                eprintln!("renderer: '{choice}' on Windows -> Direct3D 11, the default");
                false
            } else {
                std::env::set_var(RHI_ENV, "opengl");
                eprintln!("renderer: '{choice}' -> {RHI_ENV}=opengl");
                true
            }
        }
        "auto" | "wgpu" | "gpu" | "hardware" | "hw" => {
            eprintln!("renderer: '{choice}' -> Qt's default backend");
            false
        }
        other => {
            eprintln!("renderer: unknown choice '{other}' -> Qt's default backend");
            false
        }
    }
}

/// A previous launch that forced a backend and never presented a frame left
/// its sentinel: the choice goes back to Auto, and `true` says so.
pub fn revert_if_previous_launch_died() -> bool {
    let path = renderer_sentinel();
    let Some(choice) = armed(&path) else { return false };
    disarm(&path);
    eprintln!(
        "renderer: the previous launch forced '{choice}' and never presented a frame; \
         back to Auto, so the choice cannot lock you out of Settings"
    );
    save_prefs(&[(RENDERER, "auto")]);
    true
}

/// The renderer preference, applied as the environment Qt reads when the
/// application object builds its backend. Precedence: Qt's own variables,
/// then `CHILPA_RENDERER`, then the preference, then Qt's default.
pub fn apply_renderer_preference() {
    // First, before any early return: a launch that died on a forced
    // backend is undone even when this one is overridden by the environment.
    let reverted = revert_if_previous_launch_died();

    // Whitespace is not a backend; Qt does not trim every variable.
    #[cfg(target_os = "linux")]
    for name in [RHI_ENV, QUICK_ENV] {
        if std::env::var_os(name).is_some() && !env_overrides(name) {
            std::env::remove_var(name);
        }
    }
    if env_overrides(RHI_ENV) || env_overrides(QUICK_ENV) {
        eprintln!("renderer: an explicit Qt backend is set; the choice is left to it");
        return;
    }
    let from_env = std::env::var(RENDERER_ENV)
        .ok()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty() && s != "auto");
    let choice = match from_env.clone() {
        Some(choice) => choice,
        None if reverted => "auto".to_string(),
        None => pref(RENDERER, "auto"),
    };
    let forces = force_renderer(&choice);
    if forces && from_env.is_none() {
        if arm(&renderer_sentinel(), &choice) {
            eprintln!("renderer: startup sentinel armed for '{choice}'");
        } else {
            eprintln!("renderer: the startup sentinel could not be armed");
        }
    } else {
        disarm(&renderer_sentinel());
        if from_env.is_some() && forces {
            eprintln!("renderer: {RENDERER_ENV} override; the startup sentinel stays out of it");
        }
    }
}

/// Whether either startup sentinel is armed: the liveness report stays quiet
/// when there is nothing to protect.
pub fn sentinel_armed() -> bool {
    renderer_sentinel().exists() || gpu::sentinel().exists()
}

/// The window presented frames for the whole liveness window: whatever was
/// forced works, and both sentinels are cleared. Once per process.
pub fn disarm_on_liveness(frames: i32) {
    static DISARMED: AtomicBool = AtomicBool::new(false);
    if frames <= 0 {
        // A window that swapped no frames is not alive: the sentinel stays
        // armed and the next launch goes back to Auto.
        if sentinel_armed() {
            eprintln!(
                "graphics: the liveness window closed with no frame presented; the startup \
                 sentinel stays armed so the next launch goes back to Auto"
            );
        }
        return;
    }
    if DISARMED.swap(true, Ordering::SeqCst) {
        return;
    }
    let (renderer, gpu) = (renderer_sentinel().exists(), gpu::sentinel().exists());
    if renderer || gpu {
        let what = match (renderer, gpu) {
            (true, true) => "renderer + GPU",
            (true, false) => "renderer",
            _ => "GPU",
        };
        eprintln!("graphics: {what} startup sentinel disarmed ({frames} frames presented)");
        disarm(&renderer_sentinel());
        disarm(&gpu::sentinel());
    }
}

// ---------------------------------------------------------------------------
// The GPU tier: what actually draws
// ---------------------------------------------------------------------------

/// The api Qt resolved, lowercased: "opengl", "vulkan", "metal", "d3d11",
/// "d3d12", "software" or "null".
static ACTIVE_API: Mutex<String> = Mutex::new(String::new());
/// Until the window reports, the tier reads GPU-capable: every platform
/// Chilpa ships to was measured on the GPU, and a `false` start would hide
/// GPU features for the first frames of every launch.
static GPU_TIER: AtomicBool = AtomicBool::new(true);
static RESOLVED: AtomicBool = AtomicBool::new(false);

/// The two api names that mean "no shaders".
fn api_is_gpu(api: &str) -> bool {
    !matches!(api, "software" | "null" | "unknown" | "")
}

/// Latches the api the window reports. "unknown" is what it says before the
/// scene graph resolves and is not a verdict. `true` when the tier changed.
pub fn set_active_api(api: &str) -> bool {
    let api = api.trim().to_ascii_lowercase();
    if api.is_empty() || api == "unknown" {
        return false;
    }
    let forced = std::env::var_os(FORCE_GPU_TIER_ENV).is_some_and(|v| v == "1");
    let tier = api_is_gpu(&api) || forced;
    let before = GPU_TIER.swap(tier, Ordering::SeqCst);
    let first = !RESOLVED.swap(true, Ordering::SeqCst);
    {
        let mut slot = ACTIVE_API.lock().unwrap_or_else(|e| e.into_inner());
        if !first && *slot == api {
            return false;
        }
        *slot = api.clone();
    }
    eprintln!("renderer: requested={} resolved={api} gpu_tier={tier}", pref(RENDERER, "auto"));
    if !tier {
        eprintln!("renderer: no GPU tier; what needs shaders stays hidden");
    }
    first || before != tier
}

/// Whether what draws can run shaders.
pub fn gpu_tier() -> bool {
    GPU_TIER.load(Ordering::SeqCst)
}

/// The api Qt resolved, or "unknown" before the window reports.
pub fn active_api() -> String {
    let slot = ACTIVE_API.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_empty() { "unknown".to_string() } else { slot.clone() }
}

/// What the startup found and the user should hear about, once: the saved
/// GPU that could not be used and the renderer that fell to software.
pub fn take_notices() -> Vec<String> {
    let mut said = Vec::new();
    if gpu::take_fallback_notice() {
        said.push(chilpa_i18n::t("Selected GPU is unavailable in this display session — using Auto"));
    }
    if preflight::take_software_notice() {
        said.push(chilpa_i18n::t("Renderer switched automatically — the previous renderer failed to start"));
    }
    said
}

// ---------------------------------------------------------------------------
// The rows in Settings
// ---------------------------------------------------------------------------

impl crate::Chilpa {
    /// A new value for one of the graphics rows. Each applies at the next
    /// start, which its description says.
    pub(crate) fn graphics_setting(&mut self, key: &str, value: &str) -> bool {
        match key {
            RENDERER if RENDERER_CHOICES.iter().any(|(v, _)| *v == value) => {
                self.remember_preference(RENDERER, value);
                eprintln!("renderer: -> {value} (applies at the next start)");
                true
            }
            INTERFACE_SIZE if SIZE_CHOICES.iter().any(|(v, _)| *v == value) => {
                let mut settings = chilpa_core::settings::load();
                if rebase_window_size(&mut settings.window, value) {
                    let _ = chilpa_core::settings::save(&settings);
                }
                true
            }
            gpu::GPU => {
                let chosen = gpu::gpus().iter().find(|g| g.identity == value);
                gpu::save_preference(chosen);
                true
            }
            _ => false,
        }
    }

    /// Fills one of the graphics rows; `false` for a key that is not one.
    pub(crate) fn graphics_row(&self, row: &mut crate::SettingsRow) -> bool {
        match row.key.as_str() {
            RENDERER => {
                for (value, label) in RENDERER_CHOICES {
                    row.values.push(value.to_string());
                    row.options.push(chilpa_i18n::t(label));
                }
                row.value = pref(RENDERER, "auto");
            }
            INTERFACE_SIZE => {
                for (value, label) in SIZE_CHOICES {
                    row.values.push(value.to_string());
                    row.options.push(chilpa_i18n::t(label));
                }
                row.value = pref(INTERFACE_SIZE, "default");
            }
            gpu::GPU => {
                // Only the GPUs actually present, and only when there are two
                // to choose between: a row with one real answer is not shown.
                if !gpu::selectable() {
                    row.control = String::new();
                    return true;
                }
                row.values.push("auto".to_string());
                row.options.push(chilpa_i18n::t("Auto (recommended)"));
                for gpu in gpu::gpus() {
                    row.values.push(gpu.identity.clone());
                    row.options.push(gpu.label());
                }
                row.value = gpu::resolve_saved().map_or_else(|| "auto".to_string(), |g| g.identity.clone());
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_or_empty_variable_does_not_override() {
        assert!(!value_overrides(None));
        assert!(!value_overrides(Some("".into())), "a cleared variable is not a backend");
        assert!(!value_overrides(Some("   ".into())));
        assert!(value_overrides(Some("opengl".into())));
        assert!(value_overrides(Some(" d3d11 ".into())));
    }

    #[test]
    fn only_software_and_null_lose_the_gpu_tier() {
        for api in ["opengl", "metal", "vulkan", "d3d11", "d3d12"] {
            assert!(api_is_gpu(api), "{api} should carry the GPU tier");
        }
        for api in ["software", "null"] {
            assert!(!api_is_gpu(api), "{api} must not carry the GPU tier");
        }
    }

    #[test]
    fn unknown_is_not_a_verdict() {
        assert!(!api_is_gpu("unknown"));
        assert!(!api_is_gpu(""));
        assert!(!set_active_api("unknown"), "it must not be latched");
        assert!(!set_active_api("   "));
    }

    #[test]
    fn the_interface_sizes_are_qbz_factors() {
        assert_eq!(factor_for("xs"), 0.8);
        assert_eq!(factor_for("small"), 0.9);
        assert_eq!(factor_for("default"), 1.0);
        assert_eq!(factor_for("large"), 1.2);
        assert_eq!(factor_for("xl"), 1.5);
        assert_eq!(factor_for("nonsense"), 1.0);
    }

    #[test]
    fn a_new_size_keeps_the_window_footprint() {
        let mut kept = std::collections::BTreeMap::new();
        kept.insert("window.width".to_string(), "1200".to_string());
        kept.insert("window.height".to_string(), "800".to_string());
        assert!(rebase_window_size(&mut kept, "large"));
        assert_eq!(kept["window.width"], "1000", "1200 at 1.0 is 1000 at 1.2");
        assert_eq!(kept["window.height"], "667");
        assert_eq!(kept[INTERFACE_SIZE], "large");
        assert!(!rebase_window_size(&mut kept, "large"), "the same size changes nothing");
        assert!(rebase_window_size(&mut kept, "default"));
        assert_eq!(kept["window.width"], "1200");
    }

    #[test]
    fn a_sentinel_says_what_it_protected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep").join("renderer_attempt");
        assert!(armed(&path).is_none());
        assert!(arm(&path, "software"));
        assert_eq!(armed(&path).as_deref(), Some("software"));
        disarm(&path);
        assert!(armed(&path).is_none());
        std::fs::write(&path, "  \n").unwrap();
        assert!(armed(&path).is_none(), "an empty sentinel protects nothing");
    }
}
