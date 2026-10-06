//! A track's lyrics: what they are, where they come from and who wins.
//!
//! This crate talks to nobody over the network. It holds the shape every
//! lyric ends up in ([`model`]), reads the open formats ([`lrc`], [`ttml`]),
//! finds what came with the music ([`local`]), judges whether some lyrics
//! belong to a track ([`matching`]), remembers what was found
//! ([`cache`]) and decides who is asked and who wins ([`pipeline`]). The
//! plugins that do go out to the network are handed in by the facade.
//!
//! The order a track's lyrics are looked for in, and why, is in
//! [`pipeline`].

pub mod cache;
pub mod local;
pub mod lrc;
pub mod matching;
pub mod model;
pub mod pipeline;
pub mod service;
pub mod timeline;
pub mod ttml;
pub mod wire;

pub use cache::{Cache, Origin};
pub use matching::Track;
pub use model::{Document, Line, Provenance, Segment, Timing};
pub use pipeline::{Answer, Limits, Outcome, Pipeline, Provider, Request, Resolved, Source};
