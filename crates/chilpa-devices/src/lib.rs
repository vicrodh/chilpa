//! I adapt mounted volumes and USB protocols to the core's storage contract.
//! The planner never links platform APIs or chooses a transport on its own.
pub mod transport;
pub mod volumes;
pub use chilpa_sync::{Cancellation, Error, RelativePath, Result, BUFFER_BYTES, MAX_ASSETS};

fn invalid(text: impl Into<String>) -> Error { Error::Invalid(text.into()) }
fn conflict(text: impl Into<String>) -> Error { Error::Conflict(text.into()) }
fn hash_text(text: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

#[cfg(test)]
mod tests;
