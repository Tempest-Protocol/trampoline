use ckb_sdk::{ScriptGroup, tx_builder::TxBuilderError};
use thiserror::Error;
#[derive(Debug, Error)]
pub enum HelperError {
    #[error("Failed to connect to node via RPC")]
    RpcError,
    #[error("Failed to build transaction")]
    BuildError(TxBuilderError),
    #[error("Failed to unlock some inputs")]
    LockedGroupNotEmpty(Vec<ScriptGroup>),
}