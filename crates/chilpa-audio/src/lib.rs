//! El motor de audio: qué se le pide al dispositivo, cómo se acomodan las
//! muestras y cómo se entrega DSD sin romperlo.
//!
//! Tres reglas que vienen de la auditoría del motor de QBZ (CHD-0001):
//!
//! 1. **Un entero nunca pasa por flotante.** Escalar y redondear movía las
//!    muestras un LSB mientras la UI decía "bit-perfect".
//! 2. **No hay caída silenciosa.** O se abre lo que se pidió, o se dice qué
//!    se cambió ([`negotiate::Delivery`]), o se falla.
//! 3. **DoP no se interrumpe.** Una escritura parcial se reanuda donde se
//!    quedó, y los huecos se rellenan con silencio DSD, no con ceros.

pub mod dop;
pub mod format;
pub mod gapless;
pub mod negotiate;
pub mod pack;
pub mod render;
pub mod seek;
pub mod sink;

pub use format::{SampleFormat, StreamFormat};
pub use negotiate::{negotiate, DeviceCaps, Delivery, NegotiationError, Policy, Transform};
pub use pack::{pack, PackError, Samples};
pub use render::{render, FrameSource, Outcome};
pub use gapless::{transition, Transition, Trimmer};
pub use seek::{Cursor, SeekPlan};
pub use sink::{AudioSink, SinkError, StopSignal};
