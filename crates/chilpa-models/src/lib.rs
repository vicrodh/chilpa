//! The types the whole of Chilpa shares: no Qt, no I/O and no business
//! logic. Nobody else may sit at the base of the graph (ADR-001).
//!
//! Three rules that come from the QBZ audit (CHD-0001) and from ADR-004:
//!
//! 1. **A track's identity is `(source, external id)`**, never a bare
//!    number. In QBZ the id was a `u64` that collided across origins.
//! 2. **Provenance is a type**, not an `Option<String>` next to an
//!    `is_local: bool` that contradict each other.
//! 3. **Quality is described with format, bits, rate and channels**, not
//!    with a `hires: bool`. No type names a service.

pub mod artwork;
pub mod quality;
pub mod queue;
pub mod remote;
pub mod source;
pub mod track;

pub use artwork::ArtworkRef;
pub use quality::AudioQuality;
pub use remote::RemoteEntry;
pub use queue::{OrderError, PlaybackState, Progress, Queue, QueueItem, RepeatMode};
pub use source::{preferred, Availability, Reachability, SourceId};
pub use track::{AlbumRef, ArtistRef, Track, TrackRef};
