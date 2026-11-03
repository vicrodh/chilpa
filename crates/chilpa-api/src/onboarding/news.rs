//! What's new by version: the cards a release brings, ahead of its notes.

use super::{card, Card};

/// What's new for a version, matched on MAJOR.MINOR.PATCH so its -rc builds
/// show it too (QBZ's `deck_for`). A version with features can bring several
/// cards; one with only fixes, a single card or none.
pub(super) fn news(version: &str) -> Vec<Card> {
    match version.split('-').next().unwrap_or(version) {
        // Released on the Day of the Dead, 2026-11-02. The footer keeps the
        // holiday's own name, at the owner's request.
        "0.1.2" => vec![Card {
            ink: true,
            footer: chilpa_i18n::t(chilpa_i18n::mark("Don't worry about the AI slop in the image, it won't happen again. But día de muertos is a special occasion in Mexico, and here we pay tribute to our late QBZ.")),
            ..card("altar.png", chilpa_i18n::mark("Radio and DSD fixes"),
                chilpa_i18n::mark("Radio asks for each song's cover once, and the cover stays on screen. Native DSD through PipeWire now plays on a real DAC, with PipeWire 1.6 or later."), "", "")
        }],
        _ => Vec::new(),
    }
}
