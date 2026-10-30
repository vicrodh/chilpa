//! What is laid over a file's tags without touching the file.
//!
//! A sidecar sits beside the music: one in each folder that holds files,
//! one beside each file that holds tracks. It carries only what somebody
//! changed. A field it does not name is the file's own word; a field it
//! names as null is empty whatever the file says.
//!
//! The tracks in it go by their file's name, not by their path, so the
//! folder can be moved and the sidecar still knows its files.
//!
//! A record on a server of mine has no folder here: the same document is
//! kept as an overlay in the library, under the record's id on the server
//! (`store::overlay`), and the tracks in it go by their ids there.

pub mod document;
pub mod files;
pub mod qbz;

pub use document::{join, split, Album, Override, Sidecar, SidecarError, Track, ALBUM_FIELDS, FORMAT, TRACK_FIELDS, VERSION};
pub use files::{in_force, load, stage, store, Home, Scope, StagedHome};
