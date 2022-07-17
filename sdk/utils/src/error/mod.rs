use ckb_sdk::{tx_builder::TxBuilderError, ScriptGroup};
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
