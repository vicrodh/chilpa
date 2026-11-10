//! What the About window says that is not a sentence to translate: the
//! name, the version, the platform, the build and where to read more.

use serde_json::json;

#[path = "onboarding.rs"]
pub(crate) mod onboarding;

/// The project's wiki, the menu's Documentation.
pub const DOCUMENTATION: &str = "https://github.com/vicrodh/chilpa/wiki";
/// A new issue on the project, the menu's Report an issue.
pub const NEW_ISSUE: &str = "https://github.com/vicrodh/chilpa/issues/new?template=bug_report.yml";

/// The platform as the About window names it: the system and the toolkit.
fn platform_label() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macOS (Qt)",
        "windows" => "Windows (Qt)",
        "android" => "Android (Qt)",
        _ => "Linux (Qt)",
    }
}

/// Release credits are a bounded snapshot, independent of network access.
fn profile(handle: &str) -> serde_json::Value {
    json!({ "name": handle, "url": format!("https://github.com/{handle}"), "avatar": format!("{handle}.png") })
}

impl crate::Chilpa {
    /// The About window's document. The build's date and commit are the
    /// shell's to know, from its build script; either may be empty.
    pub fn about_document(&self, built: &str, commit: &str) -> String {
        let build = if commit.is_empty() { built.to_string() } else { format!("{built} ({commit})") };
        json!({
            "name": "Chilpa",
            "version": env!("CARGO_PKG_VERSION"),
            "platform": platform_label(),
            "build": build.trim(),
            "author": profile("vicrodh"),
            "contributors": [profile("afonsojorgeramos"), profile("jakumule"), profile("TheMelmacian")],
            "links": [{ "label": chilpa_i18n::t("Website"), "url": "https://chilpa.app" }],
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn credits_keep_the_author_and_human_contributors_separate() {
        let api = crate::Chilpa::new();
        let doc: serde_json::Value = serde_json::from_str(&api.about_document("", "")).unwrap();
        assert_eq!(doc["author"]["name"], "vicrodh");
        let contributors = doc["contributors"].as_array().unwrap();
        assert_eq!(contributors.len(), 3);
        for (person, handle) in contributors.iter().zip(["afonsojorgeramos", "jakumule", "TheMelmacian"]) {
            assert_eq!(person["name"], handle);
            assert_eq!(person["url"], format!("https://github.com/{handle}"));
            assert_eq!(person["avatar"], format!("{handle}.png"));
        }
    }

    #[test]
    fn the_build_says_its_date_and_its_commit_and_neither_is_required() {
        let api = crate::Chilpa::new();
        let doc: serde_json::Value = serde_json::from_str(&api.about_document("2026-10-09", "abcdef123")).unwrap();
        assert_eq!(doc["build"], "2026-10-09 (abcdef123)");
        assert_eq!(doc["version"], env!("CARGO_PKG_VERSION"));
        assert!(doc["links"][0]["url"].as_str().unwrap().starts_with("https://"));
        let bare: serde_json::Value = serde_json::from_str(&api.about_document("", "")).unwrap();
        assert_eq!(bare["build"], "");
    }
}
