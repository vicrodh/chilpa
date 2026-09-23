use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("no hay dónde guardar: el keyring falló y el respaldo tampoco arrancó: {0}")]
    NoBackend(String),

    #[error("el keyring del sistema: {0}")]
    Keyring(String),

    #[error("el cifrado: {0}")]
    Cipher(String),

    #[error("el sobre está mal formado: {0}")]
    Malformed(String),

    #[error("esto lo cifró un backend que este proceso no puede usar")]
    BackendMismatch,

    #[error("disco: {0}")]
    Io(#[from] std::io::Error),

    #[error("inesperado: {0}")]
    Other(String),
}
