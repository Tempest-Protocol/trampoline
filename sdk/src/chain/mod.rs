use ckb_types::{
    bytes::Bytes as CoreBytes,
    core::TransactionView,
    packed::{Byte32, CellOutput, OutPoint},
    prelude::*,
};
use ckb_verification::TransactionError;
use ckb_sdk::{GenesisInfo, ParseGenesisInfoError};
use ckb_jsonrpc_types::TransactionView as JsonTransaction;
mod mock_chain;
pub use mock_chain::*;
use thiserror::Error;
use crate::contract::{generator::TransactionProvider, Contract, schema::{JsonByteConversion, MolConversion, BytesConversion}};
use crate::types::{
    transaction::{CellMetaTransaction, Transaction},
    cell::{Cell, CellOutputWithData, CellError},
    bytes::{Bytes, BytesError},
};

#[derive(Debug, Error)]
pub enum ChainError {
    #[error(transparent)]
    TransactionError(#[from] TransactionError),
    #[error(transparent)]
    GenesisInfoError(#[from] ParseGenesisInfoError),
    #[error("Cannot verify transaction")]
    TransactionVerificationError,
    #[error("Failed to send transaction to network")]
    TransactionSendError,
}


// Most of this is taken from https://github.com/nervosnetwork/ckb-tool.
// Reimplementation here due to slight changes in the API & version conflicts
pub type ChainResult<T> = Result<T, ChainError>;

// Modify trait TransactionProvider to be more flexible about input type
// Then define TransactionProviderError to use in Chain trait
// This way, anything that accepts TransactionProvider trait can accept chain trait
pub trait Chain {
    type Inner: TransactionProvider;

    fn inner(&self) -> Self::Inner;
    fn verify_tx<T: Into<TransactionView> + Clone>(&self, tx: T) -> ChainResult<T> {
        let view_tx: TransactionView = tx.clone().into();
        let json_tx = JsonTransaction::from(view_tx);
        if self.inner().verify_tx(json_tx) {
            Ok(tx)
        } else {
            Err(ChainError::TransactionVerificationError)
        }
    }

    fn send_tx<T: Into<TransactionView> + Clone>(&self, tx: T) -> ChainResult<Byte32> {
        let view_tx: TransactionView = tx.clone().into();
        let json_tx = JsonTransaction::from(view_tx);
        match self.inner().send_tx(json_tx) {
            Some(hash) => Ok(hash.into()),
            None => Err(ChainError::TransactionSendError)
        }
    }

    fn deploy_cell(&mut self, cell: &Cell) -> ChainResult<OutPoint>;
    fn genesis_info(&self) -> Option<GenesisInfo>;
    fn set_genesis_info(&mut self, genesis_info: GenesisInfo) ;
    fn set_default_lock<A,D>(&mut self,lock: Contract<A,D>)
    where 
    D: JsonByteConversion + MolConversion + BytesConversion + Clone + Default,
    A: JsonByteConversion + MolConversion + BytesConversion + Clone + Default,
    ;
    fn generate_cell_with_default_lock(&self, lock_args: Bytes) -> Cell;

    

}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: Byte32,
    pub message: String,
}

pub struct OutputsDataVerifier<'a> {
    transaction: &'a TransactionView,
}

impl<'a> OutputsDataVerifier<'a> {
    pub fn new(transaction: &'a TransactionView) -> Self {
        Self { transaction }
    }

    pub fn verify(&self) -> Result<(), TransactionError> {
        let outputs_len = self.transaction.outputs().len();
        let outputs_data_len = self.transaction.outputs_data().len();

        if outputs_len != outputs_data_len {
            return Err(TransactionError::OutputsDataLengthMismatch {
                outputs_data_len,
                outputs_len,
            });
        }
        Ok(())
    }
}
