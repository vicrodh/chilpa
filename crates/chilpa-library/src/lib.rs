//! La biblioteca local: qué música hay y dónde.
//!
//! El escaneo es incremental y conservador. Lo primero viene de la auditoría
//! de QBZ: un rescan sin cambios hacía tres recorridos y reescribía cada
//! fila, alrededor de un millón de escrituras en una biblioteca mediana. Lo
//! segundo también: un root cuyo mountpoint no estaba montado se veía vacío
//! y el prune borraba todos sus tracks.

pub mod schema;
pub mod scan;
pub mod store;

pub use scan::{scan_root, ScanReport};
pub use store::Library;
