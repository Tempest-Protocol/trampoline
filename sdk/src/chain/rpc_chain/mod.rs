use crate::types::{cell::Cell, script::Script};
use std::prelude::v1::*;

use super::{Chain, ChainError};

use ckb_jsonrpc_types::{ScriptHashType, TransactionWithStatus};
#[allow(unused_imports)]
use ckb_sdk::IndexerRpcClient;
pub use provider::*;
pub use trait_impls::*;
pub mod provider;
pub mod resolve;
pub mod trait_impls;
pub use resolve::*;
pub mod tx_builder;
pub use tx_builder::*;

use ckb_types::{core::BlockView, packed::OutPoint, H256};

#[allow(dead_code)]
pub struct DefaultLock {
    out_point: OutPoint,
    code_hash: H256,
    hash_type: ScriptHashType,
}

#[derive(Clone)]
pub struct RpcChain {
    ckb_url: String,
    indexer_url: String,
    pub default_lock: Option<OutPoint>,
}

impl RpcChain {
    pub fn new(ckb_url: &str, indexer_url: &str) -> Self {
        let mut temp = Self {
            ckb_url: ckb_url.into(),
            indexer_url: indexer_url.into(),
            default_lock: None,
        };

        temp.set_sighash_all_as_default_lock();

        temp
    }

    pub fn set_sighash_all_as_default_lock(&mut self) {
        // Try setting SigHashAll lock script as default
        let consensus = self.inner().get_consensus();
        let tx_hash = consensus.genesis_block().tx_hashes()[0].clone();
        let lock_outpoint = OutPoint::new(tx_hash, 1); // Default location for sighashall lock cell
        self.default_lock = Some(lock_outpoint);
    }
    pub fn get_tip(&self) -> Option<ckb_jsonrpc_types::HeaderView> {
        self.inner().get_tip()
    }

    pub fn get_tx(&self, hash: H256) -> Result<Option<TransactionWithStatus>, ChainError> {
        self.inner().get_tx(hash)
    }

    pub fn genesis_block(&self) -> Result<BlockView, ChainError> {
        self.inner().genesis_block()
    }

    pub fn default_lock(&self) -> Option<OutPoint> {
        self.default_lock.clone()
    }

    pub fn reset(&self) -> Result<(), ChainError> {
        self.inner().rollback(0)
    }

    pub fn mine_once(&self) -> Result<H256, ChainError> {
        self.inner().mine_once()
    }
}
