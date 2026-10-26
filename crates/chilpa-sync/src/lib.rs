//! I synchronize a selected local collection with one storage, independently of the shell.
//! I own no idle worker or timer. A caller owns one job, its transport and its cancellation flag.

mod engine;
mod model;
mod paths;
mod plan;
mod state;
mod recorded;
pub mod transport;

pub use engine::{pair, recover, synchronize};
pub use model::{AssetKind, Cancellation, Fingerprint, Input, Playlist, Progress, Target};
pub use paths::{component, RelativePath};
pub use plan::{prepare, prepare_review, Action, ActionInfo, Plan};
pub use engine::{inspect_target, parse_target_identity};
pub use state::State;
pub use recorded::RecordedFile;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportProblem { Disconnected, Busy, Permission, Unsupported, ReadOnly, StaleReference, Timeout, InvalidData, Other }

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Database(rusqlite::Error),
    Invalid(String),
    Conflict(String),
    Transport { problem: TransportProblem, detail: String },
    Cancelled,
    Space { required: u64, available: u64 },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f), Self::Database(e) => e.fmt(f),
            Self::Invalid(e) | Self::Conflict(e) => f.write_str(e),
            Self::Transport { detail, .. } => f.write_str(detail),
            Self::Cancelled => f.write_str("sync cancelled"),
            Self::Space { required, available } => write!(f, "sync needs {required} bytes; {available} are available"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error { fn from(e: std::io::Error) -> Self { Self::Io(e) } }
impl From<rusqlite::Error> for Error { fn from(e: rusqlite::Error) -> Self { Self::Database(e) } }

pub(crate) fn invalid(text: impl Into<String>) -> Error { Error::Invalid(text.into()) }
pub(crate) fn conflict(text: impl Into<String>) -> Error { Error::Conflict(text.into()) }
pub(crate) fn token() -> String { format!("{:032x}", rand::random::<u128>()) }

/// I cap selection metadata independently of the bytes of music being streamed.
pub const MAX_ASSETS: usize = 50_000;
pub const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;
pub const BUFFER_BYTES: usize = 256 * 1024;
pub const SPACE_RESERVE: u64 = 16 * 1024 * 1024;
