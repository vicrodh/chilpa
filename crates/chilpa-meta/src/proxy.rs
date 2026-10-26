//! Where the proxy is.
//!
//! I keep its address here. Discogs uses it for the OAuth handshake and
//! optional anonymous backup; account requests go directly to Discogs.

pub const ORIGIN: &str = "https://api.chilpa.app";
