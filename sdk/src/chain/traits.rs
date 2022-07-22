use crate::types::{bytes::Bytes, cell::Cell};
use crate::{contract::generator::TransactionProvider, types::script::Script};
use ckb_sdk::{unlock::ScriptUnlocker, ScriptId};
use std::collections::HashMap;
use std::prelude::v1::*;

use crate::ckb_types::{
    core::TransactionView,
    packed::{Byte32, OutPoint},
};

pub type Unlockers = HashMap<ScriptId, Box<dyn ScriptUnlocker>>;

#[derive(Debug, Clone)]
pub enum CellInputs {
    ScriptQuery(Script),
    Empty,
}

impl From<Script> for CellInputs {
    fn from(script: Script) -> Self {
        CellInputs::ScriptQuery(script)
    }
}

use super::{ChainError, ChainResult};
use ckb_jsonrpc_types::TransactionView as JsonTransaction;
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
        let view_tx: TransactionView = tx.into();
        let json_tx = JsonTransaction::from(view_tx);
        match self.inner().send_tx(json_tx) {
            Some(hash) => Ok(hash.into()),
            None => Err(ChainError::TransactionSendError),
        }
    }

    fn deploy_cell(
        &mut self,
        cell: &Cell,
        unlockers: Unlockers,
        inputs: &CellInputs,
    ) -> ChainResult<OutPoint>;
    fn deploy_cells(
        &mut self,
        cells: &Vec<Cell>,
        unlockers: Unlockers,
        inputs: &CellInputs,
    ) -> ChainResult<Vec<OutPoint>>;

    fn set_default_lock(&mut self, cell: Cell) -> Result<(), ChainError>;

    fn generate_cell_with_default_lock(&self, lock_args: Bytes) -> Cell;
}
