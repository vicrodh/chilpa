//! The audio engine: what is asked of the device, how samples are laid out,
//! and how DSD is delivered without breaking it.
//!
//! Three rules that come from the audit of QBZ's engine (CHD-0001):
//!
//! 1. **An integer never goes through a float.** Scaling and rounding moved
//!    samples by an LSB while the UI said "bit-perfect".
//! 2. **There is no silent fallback.** Either what was asked for opens, or
//!    what changed is stated ([`negotiate::Delivery`]), or it fails.
//! 3. **DoP is never interrupted.** A partial write resumes where it left
//!    off, and gaps are filled with DSD silence, not with zeros.

pub mod capture;
pub mod dop;
pub mod format;
pub mod gapless;
pub mod negotiate;
pub mod pack;
pub mod render;
pub mod seek;
pub mod sink;
pub mod spectrum;
pub mod tap;

pub use capture::{CaptureSink, CAPTURE_ID};
pub use format::{SampleFormat, StreamFormat};
pub use negotiate::{negotiate, DeviceCaps, Delivery, NegotiationError, Policy, Transform};
pub use pack::{pack, PackError, Samples};
pub use render::{render, render_with, FrameSource, Outcome};
pub use gapless::{transition, Transition, Trimmer};
pub use seek::{Cursor, SeekPlan, SeekSignal};
pub use sink::{AudioSink, SinkError, StopSignal};
