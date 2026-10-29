//! What a record is called elsewhere.
//!
//! Release metadata from MusicBrainz and Discogs, and covers from the Cover
//! Art Archive, turned into one shape whatever the provider.
//!
//! This crate has no network. It builds the addresses, reads the answers
//! and knows how often each provider may be asked; the request itself is
//! handed in by whoever calls. So every test here runs without going out,
//! against answers that were saved from the real thing.

pub mod artist;
pub mod cover_art;
pub mod credits;
pub mod contributions;
pub mod discogs;
pub mod http;
pub mod musicbrainz;
pub mod music_link;
pub mod proxy;
pub mod release;
pub mod scene;
pub mod text;

pub use http::{user_agent, Failure, Http, Net, Pace, Reply};
pub use release::{display, Credit, Hit, Provider, Release, ReleaseTrack};
