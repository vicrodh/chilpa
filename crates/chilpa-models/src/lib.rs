//! Tipos compartidos por todo Chilpa: sin Qt, sin I/O y sin lógica de
//! negocio. Nadie más puede estar en la base del grafo (ADR-001).
//!
//! Tres reglas que vienen de la auditoría de QBZ (CHD-0001) y del ADR-004:
//!
//! 1. **La identidad de un track es `(source, id externo)`**, nunca un número
//!    suelto. En QBZ el id era un `u64` que colisionaba entre orígenes.
//! 2. **La procedencia es un tipo**, no un `Option<String>` acompañado de un
//!    `is_local: bool` que se contradicen entre sí.
//! 3. **La calidad se describe con formato, bits, frecuencia y canales**, no
//!    con un `hires: bool`. Ningún tipo nombra un servicio.

pub mod artwork;
pub mod quality;
pub mod queue;
pub mod source;
pub mod track;

pub use artwork::ArtworkRef;
pub use quality::AudioQuality;
pub use queue::{OrderError, PlaybackState, Progress, Queue, QueueItem, RepeatMode};
pub use source::{preferred, Availability, Reachability, SourceId};
pub use track::{AlbumRef, ArtistRef, Track, TrackRef};
