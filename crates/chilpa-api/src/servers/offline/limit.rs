//! How much offline music Chilpa keeps: a total from 512 MiB to 100 GiB,
//! or unlimited, where Chilpa stops managing the space and the disk is the
//! limit. Downloads are admitted against it; nothing is ever removed to
//! make room.

use crate::Chilpa;

/// The setting, in MiB, or `unlimited`.
pub(crate) const LIMIT: &str = "sources.offlineLimit";
const UNLIMITED: &str = "unlimited";
const DEFAULT_STEP: u64 = 2048;
const MEBIBYTE: u64 = 1024 * 1024;
/// One file under a total limit.
pub(crate) const LIMITED_FILE_BYTES: u64 = 512 * MEBIBYTE;
/// The largest file accepted in any case: the byte budget of one transfer
/// when the disk is the only limit, and the sanity bound of a saved copy.
pub(crate) const CEILING_FILE_BYTES: u64 = 64 * 1024 * MEBIBYTE;
const STEPS: [u64; 8] = [512, 1024, 2048, 5120, 10240, 20480, 51200, 102400];

/// What a download is admitted against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Limit {
    /// The total, or none when Chilpa does not manage the space.
    pub(crate) quota: Option<u64>,
}

impl Default for Limit {
    fn default() -> Self {
        Limit { quota: Some(DEFAULT_STEP * MEBIBYTE) }
    }
}

impl Limit {
    /// A stored value; anything unknown is the default.
    pub(crate) fn parse(value: Option<&str>) -> Limit {
        match value {
            Some(UNLIMITED) => Limit { quota: None },
            Some(value) => value
                .parse::<u64>()
                .ok()
                .filter(|step| STEPS.contains(step))
                .map_or_else(Limit::default, |step| Limit { quota: Some(step * MEBIBYTE) }),
            None => Limit::default(),
        }
    }

    pub(crate) fn value(self) -> String {
        self.quota.map_or_else(|| UNLIMITED.to_string(), |bytes| (bytes / MEBIBYTE).to_string())
    }

    /// The most one file may take before its folder is looked at.
    pub(crate) fn file_bytes(self) -> u64 {
        if self.quota.is_some() { LIMITED_FILE_BYTES } else { CEILING_FILE_BYTES }
    }
}

/// The choices, as (value, label).
pub(crate) fn choices() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = STEPS
        .iter()
        .map(|step| (step.to_string(), if *step < 1024 { format!("{step} MiB") } else { format!("{} GiB", step / 1024) }))
        .collect();
    out.push((UNLIMITED.into(), chilpa_i18n::t("Unlimited")));
    out
}

/// A size as the offline page says it: "830 KiB", "512 MiB", "1.6 GiB".
pub(crate) fn size(bytes: u64) -> String {
    if bytes >= 1024 * MEBIBYTE {
        format!("{:.1} GiB", bytes as f64 / (1024 * MEBIBYTE) as f64)
    } else if bytes >= MEBIBYTE {
        format!("{} MiB", bytes.div_ceil(MEBIBYTE))
    } else if bytes >= 1024 {
        format!("{} KiB", bytes.div_ceil(1024))
    } else {
        format!("{bytes} B")
    }
}

impl Chilpa {
    pub(crate) fn offline_limit(&self) -> Limit {
        Limit::parse(self.preference(LIMIT).as_deref())
    }

    /// Settings' and the offline page's way to change it. Only a listed
    /// value is kept.
    pub(crate) fn offline_limit_setting(&mut self, key: &str, value: &str) -> bool {
        if key != LIMIT || Limit::parse(Some(value)).value() != value {
            return false;
        }
        self.remember_preference(LIMIT, value);
        true
    }

    pub(crate) fn offline_limit_row(&self, row: &mut crate::settings_rows::SettingsRow) -> bool {
        if row.key != LIMIT {
            return false;
        }
        row.value = self.offline_limit().value();
        for (value, label) in choices() {
            row.values.push(value);
            row.options.push(label);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_values_parse_and_anything_else_is_the_default_of_two_gib() {
        assert_eq!(Limit::parse(None).quota, Some(2048 * MEBIBYTE));
        assert_eq!(Limit::parse(Some("512")).quota, Some(512 * MEBIBYTE));
        assert_eq!(Limit::parse(Some("102400")).quota, Some(100 * 1024 * MEBIBYTE));
        assert_eq!(Limit::parse(Some("unlimited")).quota, None);
        for odd in ["0", "256", "-1", "2 GiB", ""] {
            assert_eq!(Limit::parse(Some(odd)), Limit::default(), "{odd}");
        }
        assert_eq!(Limit::parse(Some("5120")).value(), "5120");
        assert_eq!(Limit::parse(Some("unlimited")).value(), "unlimited");
    }

    #[test]
    fn one_file_keeps_its_cap_under_a_limit_and_only_the_ceiling_without() {
        assert_eq!(Limit::parse(Some("102400")).file_bytes(), LIMITED_FILE_BYTES);
        assert_eq!(Limit::parse(Some("unlimited")).file_bytes(), CEILING_FILE_BYTES);
    }

    #[test]
    fn the_choices_go_from_the_smallest_step_to_unlimited() {
        let all = choices();
        assert_eq!(all.first().map(|c| c.1.as_str()), Some("512 MiB"));
        assert_eq!(all[2], ("2048".to_string(), "2 GiB".to_string()));
        assert_eq!(all.last().map(|c| c.0.as_str()), Some("unlimited"));
        assert!(all.iter().all(|(value, _)| Limit::parse(Some(value)).value() == *value));
    }

    #[test]
    fn sizes_read_in_binary_units() {
        assert_eq!(size(0), "0 B");
        assert_eq!(size(2048), "2 KiB");
        assert_eq!(size(554 * MEBIBYTE), "554 MiB");
        assert_eq!(size(1024 * MEBIBYTE + 640 * MEBIBYTE), "1.6 GiB");
    }

    #[test]
    fn only_a_listed_value_is_kept() {
        let mut api = Chilpa::new();
        assert!(!api.offline_limit_setting(LIMIT, "300"));
        assert!(!api.offline_limit_setting("sources.other", "512"));
        assert!(api.offline_limit_setting(LIMIT, "unlimited"));
        assert_eq!(api.offline_limit().quota, None);
        assert!(api.offline_limit_setting(LIMIT, "5120"));
        assert_eq!(api.offline_limit().quota, Some(5120 * MEBIBYTE));
    }
}
