//! The HiFi Wizard (QBZ's DAC setup): the modal that checks the audio stack,
//! finds the DACs, writes the files to paste and plays the test. The work
//! is the facade's (`chilpa_api` `outputs/hifi_wizard.rs`); this crate is
//! the surface.

pub mod bridge {
    pub mod wizard;
}
