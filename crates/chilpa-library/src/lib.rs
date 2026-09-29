//! The local library: what music there is and where.
//!
//! The scan is incremental and conservative. The first part comes from the
//! QBZ audit: a rescan with no changes did three passes and rewrote every
//! row, around a million writes on a mid-sized library. So does the second:
//! a root whose mountpoint was not mounted looked empty and the prune
//! deleted all its tracks.

pub mod album_folder;
pub mod playlist;
pub mod scan;
mod scan_sidecar;
pub mod scan_job;
pub mod schema;
pub mod sidecar;
pub mod smart;
pub mod store;

pub use album_folder::VARIOUS_ARTISTS;
pub use playlist::Playlist;
pub use scan::{scan_root, scan_root_with_cache, ScanReport};
pub use scan_job::{ScanJob, ScanProgress};
pub use smart::{builtins as smart_builtins, Order as SmartOrder, Rule, SmartList};
pub use store::{NewServer, Removal, ServerTrack, StoredServer};
pub use store::{is_various, split_genres, letter_of, AlbumEdit, ArtistRole, EditTrack, Edited, Put, TrackEdit, AlbumSort, BrowseTrack, FacetEntry, Filter, FolderEntry, Library, PlaylistEdit, PlaylistEntry, PlaylistFolder, PlaylistMembership, PlaylistTrackMetadata, Snapshot, Stats, StoredAlbum, StoredArtist, StoredPlaylist, StoredRoot, StoredTrack, Totals, TrackOrder, TrackStat};
pub use sidecar::{Home, Override, Sidecar, SidecarError};
