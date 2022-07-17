use ckb_sdk::{unlock::UnlockError, RpcError};
use ckb_types::{core::error::OutPointError, packed::OutPoint, H256};
use ckb_verification::TransactionError;
use std::prelude::v1::*;
use thiserror::Error;

use crate::types::cell::Cell;

use super::CellInputs;
#[derive(Debug, Error)]
pub enum ChainError {
    #[error(transparent)]
    TransactionError(#[from] TransactionError),
    #[error("Cannot verify transaction")]
    TransactionVerificationError,
    #[error("Failed to send transaction to network")]
    TransactionSendError,
    #[error("Failed to connect to node via RPC")]
    RpcError(#[from] RpcError),
    #[error("Failed to get Transaction from RPC")]
    GetTransactionError(H256),
    #[error("Transaction with hash {0}not included in any block yet")]
    TransactionNotIncluded(H256),
    #[error("Transaction with hash {0} not included in any block yet")]
    BlockNotFound(H256),
    #[error("Block with number {0} was not mined yet")]
    BlockNumberNotFound(u64),
    #[error("Failed resolving transaction due to outpoint error")]
    TxResolveError(#[from] OutPointError),
    #[error("Genesis block not found in chain, check your chain setup")]
    GenesisBlockNotFound,
    #[error("Failed to connect to node via RPC")]
    RpcConnectionError,
    #[error("Selected lockscript not found in chain, make sure it is deployed")]
    LockScriptCellNotFound(Cell),
    #[error("Cell with outpoint {0} make sure it is deployed")]
    CellNotFound(OutPoint),
    #[error("Failed to unlock transaction")]
    TransactionUnlockError(#[from] UnlockError),
    #[error("Failed to unlock deploy cell transaction, it has extra lock groups")]
    DeployCellTxHasLockedGroups,
    #[error("Failed to deploy transaction due to invalid cell inputs")]
    InvalidInputs(CellInputs),
}

// Most of this is taken from https://github.com/nervosnetwork/ckb-tool.
// Reimplementation here due to slight changes in the API & version conflicts
pub type ChainResult<T> = Result<T, ChainError>;
