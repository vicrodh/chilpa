//! Listening stats: a period's report against the one before, with its
//! charts, and the history a page at a time. The numbers are the facade's
//! (`chilpa_api::Chilpa::stats_report_json`); this crate draws them.

pub mod bridge {
    pub mod stats;
}
