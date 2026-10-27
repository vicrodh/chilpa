//! Developer snapshots and session logs; the shell only carries native facts.
pub mod logs;
mod redaction;
mod diagnostics;
pub use diagnostics::report;

/// I export only the exact redacted report reviewed in the window.
pub fn save_report(destination: &str, report: &str) -> Result<(), String> {
    use std::io::Write;
    let path = url::Url::parse(destination).ok().and_then(|url| url.to_file_path().ok())
        .ok_or_else(|| chilpa_i18n::t("Choose a local file for the report."))?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| chilpa_i18n::t("The report could not be saved. Choose a new file name."))?;
    file.write_all(report.as_bytes()).and_then(|()| file.sync_all())
        .map_err(|_| chilpa_i18n::t("The report could not be written."))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_export_requires_a_new_local_file_and_keeps_it_private() {
        let directory=tempfile::tempdir().unwrap();
        let path=directory.path().join("report.txt");
        let url=url::Url::from_file_path(&path).unwrap().to_string();
        save_report(&url,"reviewed fixture").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(),"reviewed fixture");
        assert!(save_report(&url,"replacement").is_err());
        assert!(save_report("https://example.invalid/report","fixture").is_err());
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
}
