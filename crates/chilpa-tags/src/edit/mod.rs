//! What the metadata editor reads from a file and writes into it.
//!
//! The scan reads through `read`, which keeps what the library stores. This
//! reads one field per thing the editor shows, and writes only what it is
//! told changed: a field nobody touched is not rewritten, because rewriting
//! it is how a date lost its day and three genres became one.

pub mod inspect;
pub mod read;
pub mod write;

pub use inspect::{inspect, Layer, Layers, Refusal};
pub use read::{snapshot, Snapshot};
pub use write::{placed, stage, Change, Cover, FileChanges, Id3Version, Options, Staged, WriteError};
