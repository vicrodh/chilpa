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
