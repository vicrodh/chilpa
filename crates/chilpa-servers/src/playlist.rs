//! A server's own playlists, read and changed (ADR-017, as amended for
//! them): the words the three protocols read them into, and what can be
//! asked of one.
//!
//! The server owns each playlist. A change is asked of the server and
//! nothing else is changed until it answers; whoever asks reads the
//! playlist again afterwards and keeps what the server says.

use crate::model::Failure;

/// The most playlists read from one server at a time.
pub const PLAYLISTS: usize = 1_000;

/// The most entries read of one playlist.
pub const PLAYLIST_TRACKS: usize = 10_000;

/// The most ids one request names in its address.
pub(crate) const IDS_PER_REQUEST: usize = 100;

/// A playlist as a server lists it. Its entries come apart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    /// What the server says of it; empty when it says nothing.
    pub description: String,
    /// Whether the account may change it there. `None` when the listing
    /// does not say, and `playlist_editable` asks.
    pub editable: Option<bool>,
}

/// The playlists a server lists.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaylistListing {
    pub found: Vec<Playlist>,
    /// Every playlist the server has is in `found`. When not, none of the
    /// ones kept is taken for gone.
    pub whole: bool,
}

/// One entry of a playlist.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// The item's id on the server; for a track, the one `page` gives.
    pub id: String,
    /// The entry's own id, when the server gives entries one apart from
    /// their item's. Empty when it does not.
    pub entry: String,
    /// It is a track. A video, an episode or a book is not: it keeps its
    /// place on the server and is never shown.
    pub track: bool,
}

/// The entries of one playlist, in its order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaylistTracks {
    /// Every entry, what is not a track included, so a place here is a
    /// place on the server. A track that is in it twice is here twice.
    pub entries: Vec<Entry>,
    /// Every entry was read. Not so when the server did not say how many
    /// it holds, or when it holds more than `PLAYLIST_TRACKS`.
    pub whole: bool,
}

impl PlaylistTracks {
    /// The ids of its tracks, in its order and with its repeats.
    pub fn tracks(&self) -> Vec<String> {
        self.entries.iter().filter(|e| e.track).map(|e| e.id.clone()).collect()
    }

    /// How many of its entries are not tracks.
    pub fn skipped(&self) -> usize {
        self.entries.iter().filter(|e| !e.track).count()
    }
}

/// What is asked of a server's playlist. A place is one among the entries
/// as they were read just before (`PlaylistTracks::entries`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// A new playlist with these tracks, by their ids on the server.
    Create {
        name: String,
        tracks: Vec<String>,
    },
    /// What is said of it: a new name, a new description, or both. `None`
    /// leaves that one as it is.
    Edit {
        name: Option<String>,
        description: Option<String>,
    },
    Delete,
    /// These tracks, by their ids on the server, at its end.
    Add {
        tracks: Vec<String>,
    },
    /// The entries at these places, out.
    Remove {
        at: Vec<usize>,
    },
    /// Every entry in a new order: the place each one had, in the order
    /// they end up in.
    Reorder {
        order: Vec<usize>,
    },
}

/// One move of a reorder: the entry that had place `item` goes to `to`,
/// right after the entry that had place `after`, or first without one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Move {
    pub(crate) item: usize,
    pub(crate) to: usize,
    pub(crate) after: Option<usize>,
}

/// Whether `order` puts each of `len` entries somewhere, once.
pub(crate) fn is_order(order: &[usize], len: usize) -> bool {
    let mut seen = vec![false; len];
    order.len() == len && order.iter().all(|&at| at < len && !std::mem::replace(&mut seen[at], true))
}

/// The single moves that take a list to `order` (a valid one, `is_order`).
/// The entries of the longest run already in their old order stay; each
/// of the others goes right after the one before it in the new order. One
/// entry dragged anywhere is one move.
pub(crate) fn moves(order: &[usize]) -> Vec<Move> {
    let still = longest_run(order);
    let mut list: Vec<usize> = (0..order.len()).collect();
    let mut out = Vec::new();
    for (i, &item) in order.iter().enumerate() {
        if still[item] {
            continue;
        }
        let Some(from) = list.iter().position(|&x| x == item) else { continue };
        list.remove(from);
        let after = i.checked_sub(1).map(|before| order[before]);
        let to = after.and_then(|a| list.iter().position(|&x| x == a)).map_or(0, |at| at + 1);
        list.insert(to, item);
        out.push(Move { item, to, after });
    }
    out
}

/// Which places are in the longest run of `order` already in their old
/// order: a longest increasing subsequence, by its places.
fn longest_run(order: &[usize]) -> Vec<bool> {
    // `tails[k]`: where in `order` the run of length k + 1 with the
    // smallest end ends.
    let mut tails: Vec<usize> = Vec::new();
    let mut before: Vec<Option<usize>> = vec![None; order.len()];
    for (i, &value) in order.iter().enumerate() {
        let k = tails.partition_point(|&t| order[t] < value);
        before[i] = k.checked_sub(1).map(|p| tails[p]);
        if k == tails.len() {
            tails.push(i);
        } else {
            tails[k] = i;
        }
    }
    let mut still = vec![false; order.len()];
    let mut at = tails.last().copied();
    while let Some(i) = at {
        still[order[i]] = true;
        at = before[i];
    }
    still
}

/// A change that names places the playlist as read does not have.
pub(crate) fn misfit() -> Failure {
    Failure::Unexpected(chilpa_i18n::t("that change does not fit the playlist as it was read"))
}

/// A new order for a playlist that was not read whole.
pub(crate) fn too_long() -> Failure {
    Failure::Unexpected(chilpa_i18n::t("the playlist is longer than Chilpa reads; change its order on the server"))
}

#[cfg(test)]
mod tests;
