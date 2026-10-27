//! I compare saved preferences with reported runtime facts, never a hardware guess.
use crate::{Chilpa, graphics_boot};
use super::redaction;
use serde_json::{Value, json};

fn row(label: &str, saved: &str, runtime: &str, comparable: bool) -> Value {
    let known = !runtime.is_empty();
    json!({"label":chilpa_i18n::t(label),"saved":redaction::clean(saved).chars().take(4096).collect::<String>(),
        "runtime":if known {redaction::clean(runtime).chars().take(4096).collect::<String>()} else {chilpa_i18n::t("Not reported")},
        "status":if comparable && known && !saved.is_empty() {if saved == runtime {"match"} else {"different"}} else {"info"}})
}

fn version_row(label: &str, runtime: &str) -> Value {
    let mut value = row(label, "", "", false);
    value["format"] = json!("version");
    if !runtime.is_empty() {
        value["runtime"] = json!(redaction::version(runtime).chars().take(4096).collect::<String>());
    }
    value
}
fn playback_state(playing: bool, paused: bool) -> &'static str {
    if !playing { chilpa_i18n::mark("Stopped") }
    else if paused { chilpa_i18n::mark("Paused") }
    else { chilpa_i18n::mark("Playing") }
}
impl Chilpa {
    /// Native facts are supplied by Qt on a manual refresh. I keep no snapshot.
    pub fn developer_document(&self, native: &str, built: &str, commit: &str) -> String {
        let native: Value = serde_json::from_str(native).unwrap_or_default();
        let saved = self.settings();
        let pref = |key: &str, fallback: &str| saved.window.get(key).cloned().unwrap_or_else(|| fallback.into());
        let native_text = |key: &str| native[key].as_str().unwrap_or("");
        let system = vec![
            row(chilpa_i18n::mark("Application"), "", &format!("Chilpa {} ({built}, {commit})", env!("CARGO_PKG_VERSION")), false),
            row(chilpa_i18n::mark("Operating system"), "", native_text("system"), false),
            row(chilpa_i18n::mark("Architecture"), "", native_text("architecture"), false),
            row(chilpa_i18n::mark("Kernel"), "", native_text("kernel"), false),
            version_row(chilpa_i18n::mark("Qt version"), native_text("qt")),
            row(chilpa_i18n::mark("Distribution"), "", native_text("distribution"), false),
            row(chilpa_i18n::mark("Distribution version"), "", native_text("distributionVersion"), false),
            row(chilpa_i18n::mark("Install method"), "", if chilpa_dirs::appimage().is_some() { "AppImage" } else { "" }, false),
            version_row("libc", native_text("libc")),
            version_row("ALSA", native_text("alsa")),
            version_row("PipeWire", native_text("pipewire")),
            version_row("PulseAudio", native_text("pulse")),
        ];
        let now = &self.now;
        let verdict = if now.path.is_empty() { String::new() } else if now.bit_perfect { chilpa_i18n::t("Yes") } else { chilpa_i18n::t("No") };
        let state = chilpa_i18n::t(playback_state(now.playing, now.paused));
        let playback = vec![
            row(chilpa_i18n::mark("Playback state"), "", &state, false),
            row(chilpa_i18n::mark("Queue length"), "", &now.queue_len.to_string(), false),
            row(chilpa_i18n::mark("Position"), "", &format!("{:.1} s", now.seconds), false),
            row(chilpa_i18n::mark("Output format"), "", &now.format, false),
            row(chilpa_i18n::mark("Bit-perfect"), "", &verdict, false),
            row(chilpa_i18n::mark("Playback error"), "", &now.error, false),
            row(chilpa_i18n::mark("Duration"), "", &format!("{:.1} s", now.total_seconds), false),
            row(chilpa_i18n::mark("Track title"), "", &now.title, false),
            row(chilpa_i18n::mark("Track artist"), "", &now.artist, false),
            row(chilpa_i18n::mark("Track album"), "", &now.album, false),
            row(chilpa_i18n::mark("Track format"), "", &now.quality.format, false),
            row(chilpa_i18n::mark("Track bit depth"), "", &now.quality.bits.map(|value|value.to_string()).unwrap_or_default(), false),
            row(chilpa_i18n::mark("Track sample rate"), "", &now.quality.rate_hz.map(|value|format!("{value} Hz")).unwrap_or_default(), false),
        ];
        let (output, _) = self.chosen_output();
        let (_, route) = self.output_route();
        let audio = vec![
            row(chilpa_i18n::mark("Selected output"), &saved.output, &output, false),
            row(chilpa_i18n::mark("Requested output route"), "", &route, false),
            row(chilpa_i18n::mark("Exclusive mode"), &pref("audio.exclusive", "false"), "", false),
            row(chilpa_i18n::mark("Gapless playback"), &pref("playback.gapless", ""), "", false),
            row(chilpa_i18n::mark("Normalization"), &pref("playback.normalize", ""), "", false),
            row(chilpa_i18n::mark("DSD output mode"), &pref("audio.dsdMode", ""), "", false),
            row(chilpa_i18n::mark("ALSA plugin"), &pref("audio.alsaPlugin", ""), "", false),
            row(chilpa_i18n::mark("Hardware volume"), &pref("audio.hwVolume", ""), "", false),
            row(chilpa_i18n::mark("Software volume"), &pref("playback.volume", ""), "", false),
            row(chilpa_i18n::mark("Muted"), &pref("playback.muted", ""), "", false),
            row(chilpa_i18n::mark("Output format"), "", &now.format, false),
        ];
        let graphics = vec![
            row(chilpa_i18n::mark("Renderer"), &pref(graphics_boot::RENDERER,"auto"), &graphics_boot::active_api(), false),
            row(chilpa_i18n::mark("Preferred GPU"), &pref(graphics_boot::gpu::GPU,"auto"), "", false),
            row(chilpa_i18n::mark("Available GPUs"), "", &graphics_boot::gpu::gpus().iter().take(16).map(|gpu| gpu.name.clone()).collect::<Vec<_>>().join(", "), false),
            row(chilpa_i18n::mark("Window system"), "", native_text("platform"), false),
            row(chilpa_i18n::mark("Desktop environment"), "", &chilpa_dirs::desktop_names().into_iter().flatten().collect::<Vec<_>>().join(", "), false),
            row(chilpa_i18n::mark("Shader rendering available"), "", &graphics_boot::gpu_tier().to_string(), false),
            row(chilpa_i18n::mark("Interface scale"), &pref(graphics_boot::INTERFACE_SIZE,"default"), &graphics_boot::interface_scale().to_string(), false),
        ];
        let environment: Vec<_> = ["QSG_RHI_BACKEND","QT_QUICK_BACKEND","QT_SCALE_FACTOR","QT_VK_PHYSICAL_DEVICE_INDEX","QT_QPA_PLATFORM","LIBGL_ALWAYS_SOFTWARE"]
            .into_iter().filter_map(|key| std::env::var(key).ok().map(|value| row(key,"",&value,false))).collect();
        let cast: Value = serde_json::from_str(&self.cast_json()).unwrap_or_default();
        let cast_rows = vec![
            row(chilpa_i18n::mark("Cast state"), "", cast["state"].as_str().unwrap_or(""), false),
            row(chilpa_i18n::mark("Cast protocol"), "", cast["protocol"].as_str().unwrap_or(""), false),
            row(chilpa_i18n::mark("Cast destination"), "", cast["name"].as_str().unwrap_or(""), false),
        ];
        json!({"sections":[
            {"title":chilpa_i18n::t("System"),"rows":system},
            {"title":chilpa_i18n::t("Playback"),"rows":playback},
            {"title":chilpa_i18n::t("Audio"),"saved":true,"rows":audio},
            {"title":chilpa_i18n::t("Graphics"),"saved":true,"rows":graphics},
            {"title":chilpa_i18n::t("Environment"),"rows":environment},
            {"title":chilpa_i18n::t("Cast"),"rows":cast_rows},
        ]}).to_string()
    }
}

/// I use the same reviewed document for copying and exporting.
pub fn report(document: &str) -> String {
    let doc: Value=serde_json::from_str(document).unwrap_or_default();
    let mut text=String::from("Chilpa diagnostics\n\n");
    for section in doc["sections"].as_array().into_iter().flatten() {
        text.push_str(&redaction::clean(section["title"].as_str().unwrap_or(""))); text.push('\n');
        for row in section["rows"].as_array().into_iter().flatten() {
            let runtime = row["runtime"].as_str().unwrap_or("");
            let runtime = if row["format"] == "version" { redaction::version(runtime) } else { redaction::clean(runtime) };
            text.push_str(&format!("{}: {}",redaction::clean(row["label"].as_str().unwrap_or("")),runtime));
            if let Some(saved)=row["saved"].as_str().filter(|value| !value.is_empty()) { text.push_str(&format!(" (saved: {})", redaction::clean(saved))); }
            text.push('\n');
        }
        text.push('\n');
    }
    text.push_str("Session logs (redacted)\n"); text.push_str(&super::logs::export()); text
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_restored_paused_track_is_not_reported_as_playing() {
        assert_eq!(playback_state(true, true), "Paused");
        assert_eq!(playback_state(true, false), "Playing");
        assert_eq!(playback_state(false, false), "Stopped");
        assert_eq!(playback_state(false, true), "Stopped");
    }
    #[test]
    fn native_versions_survive_display_and_reviewed_export() {
        let version = version_row("ALSA", "1.2.15.3");
        assert_eq!(version["runtime"], "1.2.15.3");
        assert_eq!(version["format"], "version");
        assert_eq!(version_row("ALSA", "")["runtime"], chilpa_i18n::t("Not reported"));
        let doc = json!({"sections":[{"title":"System","rows":[version,
            {"label":"Endpoint","runtime":"192.168.1.2"}]}]}).to_string();
        let text = report(&doc);
        assert!(text.contains("ALSA: 1.2.15.3\n"));
        assert!(text.contains("Endpoint: [address]\n"));
    }
    #[test]
    fn typed_versions_still_redact_private_values() {
        for runtime in ["password=fixture", "/home/fixture/library", "https://fixture.invalid/library"] {
            assert!(!version_row("ALSA", runtime)["runtime"].as_str().unwrap().contains("fixture"));
            let doc = json!({"sections":[{"title":"System","rows":[
                {"label":"ALSA","format":"version","runtime":runtime}]}]}).to_string();
            assert!(!report(&doc).contains("fixture"));
        }
    }
    #[test]
    fn a_saved_request_never_claims_an_unreported_runtime_match() {
        let unknown=row("Exclusive mode","true","",true);
        assert_eq!(unknown["status"],"info");
        assert_ne!(unknown["runtime"],"true");
        let reported=row("Fixture","one","two",true);
        assert_eq!(reported["status"],"different");
    }
    #[test]
    fn diagnostic_rows_and_exports_redact_values_before_display() {
        let doc=json!({"sections":[{"title":"Fixture","rows":[{"label":"Address","runtime":"https://person:fixture@example.invalid/private?key=value","saved":"password=fixture"}]}]}).to_string();
        let text=report(&doc);
        assert!(!text.contains("fixture")); assert!(!text.contains("example.invalid"));
        assert_eq!(row("Fixture","","Authorization: Bearer fixture",false)["runtime"],"[redacted sensitive log entry]");
    }
}
