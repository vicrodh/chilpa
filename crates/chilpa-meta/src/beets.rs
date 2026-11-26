//! An optional external metadata engine. A request receives proposals only.
//! One caller-owned job holds a child, two pipe threads, at most 1 MiB input
//! and 8 MiB output. The child and pipes are reaped on completion/cancellation;
//! disabled integrations allocate no worker, process or persistent cache.
mod protocol;
mod process;
pub use protocol::{Candidate, Credit, Mapping, Operation, Release, ReleaseTrack, Request, Response, Source, Track,
    BEETS_VERSION, MAX_CANDIDATES, MAX_IDS, MAX_INPUT, MAX_OUTPUT, MAX_TRACKS, PROTOCOL};
pub use process::{lookup, TIMEOUT};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Unavailable, IncompatibleVersion, InvalidRequest, InvalidResponse, InputLimit, OutputLimit,
    TrackLimit, Timeout, Cancelled, LookupFailed,
}
impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "beets runtime is unavailable",
            Self::IncompatibleVersion => "beets runtime version is unsupported",
            Self::InvalidRequest => "beets lookup request is invalid",
            Self::InvalidResponse => "beets lookup response is invalid",
            Self::InputLimit => "beets lookup input limit exceeded",
            Self::OutputLimit => "beets lookup output limit exceeded",
            Self::TrackLimit => "beets lookup track limit exceeded",
            Self::Timeout => "beets lookup timed out",
            Self::Cancelled => "beets lookup was cancelled",
            Self::LookupFailed => "beets lookup failed",
        })
    }
}
impl std::error::Error for Error {}
