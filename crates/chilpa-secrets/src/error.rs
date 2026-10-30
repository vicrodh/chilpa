use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("there is nowhere to keep it: the keyring failed and the fallback did not start either: {0}")]
    NoBackend(String),

    #[error("the system keyring: {0}")]
    Keyring(String),

    #[error("the encryption: {0}")]
    Cipher(String),

    #[error("the envelope is malformed: {0}")]
    Malformed(String),

    #[error("this was encrypted by a backend this process cannot use")]
    BackendMismatch,

    #[error("disk: {0}")]
    Io(#[from] std::io::Error),

    #[error("unexpected: {0}")]
    Other(String),
}

impl SecretError {
    /// What failed, in one word a log line can carry: no detail, nothing
    /// sealed, no key and no path.
    pub fn kind(&self) -> &'static str {
        match self {
            SecretError::NoBackend(_) => "no-backend",
            SecretError::Keyring(_) => "keyring",
            SecretError::Cipher(_) => "cipher",
            SecretError::Malformed(_) => "malformed",
            SecretError::BackendMismatch => "backend-mismatch",
            SecretError::Io(_) => "disk",
            SecretError::Other(_) => "other",
        }
    }
}
