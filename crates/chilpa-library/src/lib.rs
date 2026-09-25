//! The local library: what music there is and where.
//!
//! The scan is incremental and conservative. The first part comes from the
//! QBZ audit: a rescan with no changes did three passes and rewrote every
//! row, around a million writes on a mid-sized library. So does the second:
//! a root whose mountpoint was not mounted looked empty and the prune
//! deleted all its tracks.

pub mod playlist;
pub mod schema;
pub mod scan;
pub mod scan_job;
pub mod smart;
pub mod store;

pub use scan::{scan_root, scan_root_with_cache, ScanReport};
pub use scan_job::{ScanJob, ScanProgress};
pub use playlist::Playlist;
pub use smart::{builtins as smart_builtins, Order as SmartOrder, Rule, SmartList};
pub use store::{
    AlbumSort, BrowseTrack, FacetEntry, Filter, FolderEntry, Library, PlaylistEntry, Stats, StoredAlbum, StoredArtist, StoredPlaylist,
    Snapshot, StoredRoot, StoredTrack, Totals, TrackOrder, TrackStat,
};
