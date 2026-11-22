//! Where the proxy is.
//!
//! I keep its address here. Discogs uses it for the OAuth handshake and
//! optional anonymous backup; account requests go directly to Discogs.
//!
//! It answers official builds, which sign their requests. A fork or a build
//! of your own runs its own worker: https://github.com/vicrodh/chilpa-api#forks

pub const ORIGIN: &str = "https://api.chilpa.app";
