//! The text a bug report carries: a collapsible block with a short header,
//! the reviewed diagnostics report and the last lines of the log, ready to
//! paste into an issue.
use super::logs::{self, Entry};
use super::redaction;

/// Lines of the log the bundle carries.
pub const BUNDLE_LINES: usize = 200;

fn render(version: &str, locale: &str, report: &str, tail: &[Entry]) -> String {
    let mut out = String::from("<details><summary>chilpa diagnostics</summary>\n\n");
    out.push_str(&format!("**App:** chilpa v{version}\n"));
    out.push_str(&format!("**OS:** {} {}\n", std::env::consts::OS, std::env::consts::ARCH));
    out.push_str(&format!("**Locale:** {locale}\n"));
    if !report.trim().is_empty() {
        out.push('\n');
        out.push_str(report.trim_end());
        out.push('\n');
    }
    out.push_str("\n```log\n");
    for entry in tail {
        // The ring is already redacted; the bundle never trusts that.
        out.push_str(&format!("{} {} {} {}\n", logs::format_ts(entry.ts), entry.level.to_ascii_uppercase(), entry.target, redaction::clean(&entry.message)));
    }
    out.push_str("```\n</details>\n");
    out
}

/// The bundle for `report`, the diagnostics text the window shows.
pub fn bundle(report: &str) -> String {
    render(env!("CARGO_PKG_VERSION"), chilpa_i18n::current_language(), report, &logs::tail(BUNDLE_LINES))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ts: i64, message: &str) -> Entry {
        Entry { ts, level: "info", target: "net".into(), message: message.into() }
    }

    #[test]
    fn the_bundle_has_a_collapsible_header_report_and_a_log_fence() {
        let out = render("1.2.3", "en", "# diagnostics\n- Kernel: 7.1\n", &[entry(0, "hello")]);
        assert!(out.starts_with("<details><summary>chilpa diagnostics</summary>"));
        let (header, body, block) = (out.find("**App:**").unwrap(), out.find("- Kernel").unwrap(), out.find("```log").unwrap());
        assert!(header < body && body < block);
        assert!(out.contains("1970-01-01 00:00:00.000 INFO net hello"));
        assert!(out.trim_end().ends_with("</details>"));
    }

    #[test]
    fn a_secret_that_reached_the_ring_still_does_not_leave() {
        let out = render("1", "en", "", &[entry(0, "token=fixture-secret")]);
        assert!(!out.contains("fixture-secret"));
    }

    #[test]
    fn an_empty_report_leaves_no_blank_section() {
        let with = render("1", "en", "  \n", &[]);
        assert!(!with.contains("\n\n\n"));
    }
}
