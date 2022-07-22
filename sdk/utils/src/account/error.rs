use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("Wrong password")]
    WrongPassword,

    #[error("Invalid key length")]
    InvalidKeyLength,

    #[error("Invalid key")]
    InvalidKey(#[from] secp256k1::Error),

    #[error("IO error")]
    Io(#[from] io::Error),

    #[error("JSON error")]
    Json(#[from] serde_json::Error),
}
