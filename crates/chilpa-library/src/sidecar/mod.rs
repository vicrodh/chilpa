//! What is laid over a file's tags without touching the file.
//!
//! A sidecar sits beside the music: one in each folder that holds files,
//! one beside each file that holds tracks. It carries only what somebody
//! changed. A field it does not name is the file's own word; a field it
//! names as null is empty whatever the file says.
//!
//! The tracks in it go by their file's name, not by their path, so the
//! folder can be moved and the sidecar still knows its files.

pub mod document;
pub mod files;
pub mod qbz;

pub use document::{join, split, Album, Override, Sidecar, SidecarError, Track, ALBUM_FIELDS, FORMAT, TRACK_FIELDS, VERSION};
pub use files::{in_force, load, stage, store, Home, StagedHome};
