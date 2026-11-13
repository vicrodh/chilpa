//! The plays the facade has asked of the player and not yet seen end.

// Only its verdict is asked for until the plays are kept here.
#[allow(dead_code)]
pub(crate) mod ledger;

/// What became of a play. It starts as the one in front of the window and
/// leaves that once, for one of the others.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Fate {
    /// The play in front of the window.
    Current,
    /// Another play took its place.
    Replaced(Replaced),
    /// The listener stopped it, or nothing was left to play.
    Stopped,
    /// Its player was handed away with it still playing: the music goes on
    /// somewhere the facade does not hear from.
    Abandoned,
}

/// Why a play was replaced.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Replaced {
    /// Somebody chose another track: what was left was left by choice.
    Track,
    /// The same track goes on at another output, as the play `by`.
    Output { by: u64 },
}
