use ckb_sdk::traits::{DefaultCellCollector, DefaultCellDepResolver, DefaultHeaderDepResolver};
use ckb_sdk::tx_builder::{
    balance_tx_capacity, unlock_tx, BalanceTxCapacityError, CapacityBalancer,
};
use ckb_sdk::ScriptGroup;
use ckb_types::packed::{CellOutput, Script, WitnessArgs};
use ckb_types::{
    core::{TransactionBuilder as CkbTxBuilder, TransactionView},
    packed::CellInput,
};
use std::prelude::v1::*;

use crate::chain::{Chain, ChainError, Unlockers};
use crate::types::{cell::Cell, transaction::Transaction};

use super::RpcChain;

const DEFAULT_FEE_RATE: u64 = 1200_u64;

pub struct TransactionBuilder {
    inner: CkbTxBuilder,
}

impl TransactionBuilder {
    pub fn default() -> Self {
        Self {
            inner: CkbTxBuilder::default(),
        }
    }

    pub fn unlock(
        self,
        unlockers: &Unlockers,
        chain: &RpcChain,
    ) -> Result<(Self, Vec<ScriptGroup>), ChainError> {
        let unlocked_tx = unlock_tx(self.build().into(), &chain.inner(), unlockers);

        match unlocked_tx {
            Ok(tx) => Ok((Self::from(Transaction::from(tx.0)), tx.1)),
            Err(e) => Err(ChainError::TransactionUnlockError(e)),
        }
    }

    pub fn build(self) -> Transaction {
        Transaction::from(self.inner.build())
    }

    pub fn add_output(self, cell: Cell) -> Self {
        let required_capacity = cell.required_capacity().unwrap().as_u64();
        let mut output = cell.clone();
        output
            .set_capacity_shannons(required_capacity)
            .expect("Failed to set capacity");
        let output = CellOutput::from(output);

        let output_data = cell.data();

        let new_builder = self.inner.output(output).output_data(output_data.into());

        Self { inner: new_builder }
    }

    pub fn add_outputs(self, cells: Vec<Cell>) -> Self {
        let mut builder = self;
        for cell in cells {
            builder = builder.add_output(cell);
        }
        builder
    }

    pub fn add_input(self, cell: CellInput) -> Self {
        Self {
            inner: self.inner.input(cell),
        }
    }

    /// Script field points to the lockscript that should be used
    /// for searching and filling inputs. Returns the balanced transaction
    pub fn balance(
        &self,
        script: Script,
        fee_rate: Option<u64>,
        chain: &RpcChain,
    ) -> Result<Self, BalanceTxCapacityError> {
        let balancer = CapacityBalancer::new_simple(
            script,
            WitnessArgs::default(),
            fee_rate.unwrap_or(DEFAULT_FEE_RATE),
        );

        let mut collector =
            DefaultCellCollector::new(chain.indexer_url.as_str(), chain.ckb_url.as_str());

        let mut provider = chain.inner();

        let cell_dep_resolver =
            DefaultCellDepResolver::from_genesis(&chain.genesis_block().unwrap())
                .expect("Failed to create DefaultCellDepResolver from RpcChain");

        let header_dep_resolver = DefaultHeaderDepResolver::new(chain.ckb_url.as_str());

        let balanced_tx = balance_tx_capacity(
            &self.inner.clone().build(),
            &balancer,
            &mut collector,
            &mut provider,
            &cell_dep_resolver,
            &header_dep_resolver,
        );

        match balanced_tx {
            Ok(tx) => Ok(Self {
                inner: tx.as_advanced_builder(),
            }),
            Err(e) => Err(e),
        }
    }
}

impl From<Transaction> for TransactionBuilder {
    fn from(tx: Transaction) -> Self {
        Self {
            inner: TransactionView::from(tx).as_advanced_builder(),
        }
    }
}
