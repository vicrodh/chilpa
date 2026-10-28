//! Developer snapshots and session logs; the shell only carries native facts.
pub mod logs;
mod redaction;
mod repeat;
mod bundle;
mod upload;
mod diagnostics;
pub use diagnostics::report;
pub use bundle::bundle;
pub use upload::upload;

/// I export only the exact redacted report reviewed in the window. The system's
/// save dialog already asked about replacing a file, so an existing one is
/// overwritten; the file stays private to the user.
pub fn save_report(path: &std::path::Path, report: &str) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
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
    fn report_export_overwrites_the_chosen_file_and_keeps_it_private() {
        let directory=tempfile::tempdir().unwrap();
        let path=directory.path().join("report.txt");
        save_report(&path,"reviewed fixture").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(),"reviewed fixture");
        save_report(&path,"replacement").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(),"replacement");
        assert!(save_report(&directory.path().join("missing").join("report.txt"),"fixture").is_err());
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
}
