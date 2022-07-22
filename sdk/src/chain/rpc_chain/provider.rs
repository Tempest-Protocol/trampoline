use std::cell::RefCell;
use std::collections::{hash_map::Entry, HashMap, HashSet};
use std::prelude::v1::*;
use std::sync::Arc;

use ckb_chain_spec::consensus::{Consensus, ConsensusBuilder};
use ckb_hash::blake2b_256;
use ckb_jsonrpc_types as json_types;
use ckb_script::{TransactionScriptsVerifier, TxVerifyEnv};
use ckb_sdk::traits::{TransactionDependencyError, TransactionDependencyProvider};
use ckb_sdk::CkbRpcClient;
use ckb_traits::{CellDataProvider, HeaderProvider};
use ckb_types::core::cell::{
    resolve_transaction_with_options, CellMeta, CellMetaBuilder, CellProvider, CellStatus,
    HeaderChecker, ResolveOptions,
};
use ckb_types::core::error::OutPointError;
use ckb_types::core::{cell::ResolvedTransaction, hardfork::HardForkSwitch, TransactionView};
use ckb_types::core::{EpochNumberWithFraction, HeaderView, TransactionInfo};
use ckb_types::packed::{Byte32, CellOutput, OutPoint, Transaction as CkbTransaction};
use ckb_types::prelude::{Pack, Unpack};
use ckb_types::H256;
use ckb_util::Mutex;
use ckb_verification::NonContextualTransactionVerifier;
use json_types::TransactionWithStatus;
use lru::LruCache;

use ckb_types::bytes::Bytes;

use crate::types::transaction::Transaction;

use super::RpcChain;
use crate::chain::{ChainError, TransactionResolver, TransactionResolverError};
use crate::contract::generator::TransactionProvider;

const MAX_CYCLES: u64 = 500_0000;

// RpcChain Transaction provider type
pub struct RpcProvider {
    chain: RefCell<RpcChain>,
    inner: Arc<Mutex<RpcProviderInner>>,
}

impl Clone for RpcProvider {
    fn clone(&self) -> RpcProvider {
        let inner = Arc::clone(&self.inner);
        let chain = self.chain.clone();
        RpcProvider { chain, inner }
    }
}

struct RpcProviderInner {
    rpc_client: CkbRpcClient,
    // indexer: IndexerRpcClient,
    tx_cache: LruCache<Byte32, TransactionView>,
    cell_cache: LruCache<OutPoint, (CellOutput, Bytes)>,
    header_cache: LruCache<Byte32, HeaderView>,
}

impl RpcProvider {
    pub fn new(chain: RpcChain) -> Self {
        let ref_chain = RefCell::new(chain);
        let inner = RpcProviderInner {
            rpc_client: CkbRpcClient::new(&ref_chain.borrow().ckb_url),
            // indexer: IndexerRpcClient::new(&chain.indexer_url.as_str()),
            tx_cache: LruCache::new(20),
            cell_cache: LruCache::new(20),
            header_cache: LruCache::new(20),
        };
        Self {
            chain: ref_chain,
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub fn get_tx(&self, hash: H256) -> Result<Option<TransactionWithStatus>, ChainError> {
        let mut inner = self.inner.lock();
        let tx = inner.rpc_client.get_transaction(hash);
        match tx {
            Ok(tx) => Ok(tx),
            Err(e) => Err(ChainError::RpcError(e)),
        }
    }

    pub fn get_cell_with_data(
        &self,
        out_point: &OutPoint,
    ) -> Result<(CellOutput, Bytes), TransactionDependencyError> {
        let mut inner = self.inner.lock();
        if let Some(pair) = inner.cell_cache.get(out_point) {
            return Ok(pair.clone());
        }
        // TODO: handle proposed/pending transactions
        let cell_with_status = inner
            .rpc_client
            .get_live_cell(out_point.clone().into(), true)
            .map_err(|err| TransactionDependencyError::Other(err.into()))?;
        if cell_with_status.status != "live" {
            return Err(TransactionDependencyError::Other(
                format!("invalid cell status: {:?}", cell_with_status.status).into(),
            ));
        }
        let cell = cell_with_status.cell.unwrap();
        let output = CellOutput::from(cell.output);
        let output_data = cell.data.unwrap().content.into_bytes();
        inner
            .cell_cache
            .put(out_point.clone(), (output.clone(), output_data.clone()));
        Ok((output, output_data))
    }

    pub fn get_tip(&self) -> Option<json_types::HeaderView> {
        let mut inner = self.inner.lock();
        let tip = inner.rpc_client.get_tip_header();
        Some(tip.unwrap())
    }

    pub fn get_tx_info(&self, tx_hash: &H256) -> Result<TransactionInfo, ChainError> {
        let (_tx, block_hash) = {
            let mut inner = self.inner.lock();
            let result = inner.rpc_client.get_transaction(tx_hash.clone());
            match result {
                Ok(Some(tx)) => match tx.tx_status.block_hash {
                    Some(block_hash) => (tx.transaction.unwrap(), block_hash),
                    None => return Err(ChainError::TransactionNotIncluded(tx_hash.clone())),
                },
                Ok(None) => return Err(ChainError::TransactionNotIncluded(tx_hash.clone())),
                Err(_e) => return Err(ChainError::TransactionNotIncluded(tx_hash.clone())),
            }
        };

        let block = {
            let mut inner = self.inner.lock();
            let block = inner.rpc_client.get_block(block_hash.clone());
            match block {
                Ok(Some(block)) => block,
                Ok(None) => return Err(ChainError::BlockNotFound(block_hash)),
                Err(e) => {
                    return Err(ChainError::RpcError(e));
                }
            }
        };

        let index = {
            let finding = block
                .transactions
                .into_iter()
                .enumerate()
                .find(|tx| tx.1.hash == tx_hash.clone());
            // Unwrap is safe because TX must exist in Block
            finding.unwrap().0
        };

        Ok(TransactionInfo {
            block_hash: block.header.hash.pack(),
            block_number: block.header.inner.number.into(),
            block_epoch: EpochNumberWithFraction::from_full_value(block.header.inner.epoch.into()),
            index,
        })
    }

    pub fn genesis_block(&self) -> Result<ckb_types::core::BlockView, ChainError> {
        let mut inner = self.inner.lock();
        match inner.rpc_client.get_block_by_number(0.into()) {
            Ok(Some(block)) => Ok(block.into()),
            Ok(None) => Err(ChainError::GenesisBlockNotFound),
            Err(e) => Err(ChainError::RpcError(e)),
        }
    }
}

impl TransactionDependencyProvider for RpcProvider {
    fn get_transaction(
        &self,
        tx_hash: &Byte32,
    ) -> Result<TransactionView, ckb_sdk::traits::TransactionDependencyError> {
        let mut inner = self.inner.lock();
        if let Some(tx) = inner.tx_cache.get(tx_hash) {
            return Ok(tx.clone());
        }
        // TODO: handle proposed/pending transactions
        let tx_with_status = inner
            .rpc_client
            .get_transaction(tx_hash.unpack())
            .map_err(|err| TransactionDependencyError::Other(err.into()))?
            .ok_or_else(|| TransactionDependencyError::NotFound("transaction".to_string()))?;
        if tx_with_status.tx_status.status != json_types::Status::Committed {
            return Err(TransactionDependencyError::Other(
                format!("invalid transaction status: {:?}", tx_with_status.tx_status).into(),
            ));
        }
        let tx = CkbTransaction::from(tx_with_status.transaction.unwrap().inner).into_view();
        inner.tx_cache.put(tx_hash.clone(), tx.clone());
        Ok(tx)
    }

    fn get_cell(&self, out_point: &OutPoint) -> Result<CellOutput, TransactionDependencyError> {
        self.get_cell_with_data(out_point).map(|(output, _)| output)
    }
    fn get_cell_data(&self, out_point: &OutPoint) -> Result<Bytes, TransactionDependencyError> {
        self.get_cell_with_data(out_point)
            .map(|(_, output_data)| output_data)
    }
    fn get_header(&self, block_hash: &Byte32) -> Result<HeaderView, TransactionDependencyError> {
        let mut inner = self.inner.lock();
        if let Some(header) = inner.header_cache.get(block_hash) {
            return Ok(header.clone());
        }
        let header = inner
            .rpc_client
            .get_header(block_hash.unpack())
            .map_err(|err| TransactionDependencyError::Other(err.into()))?
            .map(HeaderView::from)
            .ok_or_else(|| TransactionDependencyError::NotFound("header".to_string()))?;
        inner.header_cache.put(block_hash.clone(), header.clone());
        Ok(header)
    }
}

impl HeaderProvider for RpcProvider {
    fn get_header(&self, hash: &Byte32) -> Option<HeaderView> {
        TransactionDependencyProvider::get_header(self, hash).ok()
    }
}

impl CellProvider for RpcProvider {
    fn cell(&self, out_point: &OutPoint, _eager_load: bool) -> ckb_types::core::cell::CellStatus {
        match self.get_transaction(&out_point.tx_hash()) {
            Ok(tx) => tx
                .outputs()
                .get(out_point.index().unpack())
                .map(|cell| {
                    let data = tx
                        .outputs_data()
                        .get(out_point.index().unpack())
                        .expect("output data");

                    let cell_meta = CellMetaBuilder::from_cell_output(cell, data.unpack())
                        .out_point(out_point.to_owned())
                        .build();

                    CellStatus::live_cell(cell_meta)
                })
                .unwrap_or(CellStatus::Unknown),
            Err(_err) => CellStatus::Unknown,
        }
    }
}

impl HeaderChecker for RpcProvider {
    fn check_valid(&self, block_hash: &Byte32) -> Result<(), OutPointError> {
        TransactionDependencyProvider::get_header(self, block_hash)
            .map(|_| ())
            .map_err(|_| OutPointError::InvalidHeader(block_hash.clone()))
    }
}

impl TransactionProvider for RpcProvider {
    fn send_tx(&self, tx: ckb_jsonrpc_types::TransactionView) -> Option<ckb_jsonrpc_types::Byte32> {
        let mut inner = self.inner.lock();
        let hash = inner.rpc_client.send_transaction(tx.inner, None);
        match hash {
            Ok(hash) => Some(hash.pack().into()),
            Err(e) => {
                println!("{:?}", e);
                None
            }
        }
    }

    fn verify_tx(&self, tx: ckb_jsonrpc_types::TransactionView) -> bool {
        let consensus = { self.get_consensus() };
        println!(
            "Verification Consensus: {:?}",
            consensus.secp256k1_blake160_sighash_all_type_hash
        );
        let transaction = Transaction::from(tx.inner.clone());

        println!("Starting to resolve TX");
        let resolved_tx = self.resolve_tx_alt(transaction);
        if resolved_tx.is_err() {
            return false;
        }
        println!("Resolved TX: Ok");

        let tx_env = {
            let epoch = EpochNumberWithFraction::new(300, 0, 1);
            let header = HeaderView::new_advanced_builder()
                .epoch(epoch.pack())
                .build();
            TxVerifyEnv::new_commit(&header)
        };

        let packed_tx = CkbTransaction::from(tx.inner);
        let rtx = resolved_tx.unwrap();
        let _converted_tx_view = packed_tx.as_advanced_builder().build();
        let non_contextual = NonContextualTransactionVerifier::new(&rtx.transaction, &consensus);
        let transaction_verifier = TransactionScriptsVerifier::new(&rtx, &consensus, self, &tx_env);
        let _verifier = {
            let script_verif = transaction_verifier.verify(MAX_CYCLES);
            let non_context_verif = non_contextual.verify();
            match script_verif {
                Ok(_) => println!("OK: Script verification passed"),
                Err(e) => {
                    println!("ERR: Script verification error: {:?}", e);
                    println!("Cell deps len \n{:?}", &rtx.resolved_cell_deps.len());
                    println!(
                        "Cell deps data \n{:?}",
                        &rtx.resolved_cell_deps[0].mem_cell_data_hash
                    );
                    println!(
                        "Conflicting script {:?}",
                        &rtx.resolved_inputs[0].cell_output.lock().code_hash()
                    );
                }
            }

            match non_context_verif {
                Ok(_) => println!("OK: Non-contextual verification passed"),
                Err(_e) => println!("ERR: Non-contextual erification error"),
            }
        };

        transaction_verifier.verify(MAX_CYCLES).is_ok() && non_contextual.verify().is_ok()
    }
}

impl CellDataProvider for RpcProvider {
    fn get_cell_data(&self, out_point: &OutPoint) -> Option<Bytes> {
        <RpcProvider as TransactionDependencyProvider>::get_cell_data(self, out_point).ok()
    }

    fn get_cell_data_hash(&self, out_point: &OutPoint) -> Option<Byte32> {
        <RpcProvider as TransactionDependencyProvider>::get_cell_data(self, out_point)
            .ok()
            .map(|data| blake2b_256(data.as_ref()).pack())
    }
}

// impl CellProvider for RpcProvider {
//     fn cell(&self, out_point: &ckb_types::packed::OutPoint, eager_load: bool) -> ckb_types::core::cell::CellStatus {
//         let chain = self.chain.borrow_mut();
//         let client = chain.client.lock();
//         let cell = client.ckb.get_live_cell(out_point.unpack().into())
//     }
// }

pub fn dummy_consensus() -> Consensus {
    let hardfork_switch = HardForkSwitch::new_without_any_enabled()
        .as_builder()
        .rfc_0032(200)
        .build()
        .unwrap();
    ConsensusBuilder::default()
        .hardfork_switch(hardfork_switch)
        .build()
}

impl TransactionResolver for RpcProvider {
    fn resolve_tx(&self, tx: Transaction) -> Result<ResolvedTransaction, TransactionResolverError> {
        // let inner_tx = CkbTransaction::from(tx.inner);
        let transaction = TransactionView::from(tx);

        let mut resolved_cells: HashMap<(OutPoint, bool), CellMeta> = HashMap::new();

        let mut seen_inputs = HashSet::new();

        // Taken from ckb::util::types::cell
        let mut resolve_cell = |out_point: &OutPoint,
                                eager_load: bool|
         -> Result<CellMeta, TransactionResolverError> {
            if seen_inputs.contains(out_point) {
                return Err(TransactionResolverError::DeadOutPoint(out_point.clone()));
            }

            match resolved_cells.entry((out_point.clone(), eager_load)) {
                Entry::Occupied(entry) => Ok(entry.get().clone()),
                Entry::Vacant(entry) => {
                    let cell_status = self.cell(out_point, eager_load);
                    match cell_status {
                        CellStatus::Dead => {
                            Err(TransactionResolverError::DeadOutPoint(out_point.clone()))
                        }
                        CellStatus::Unknown => {
                            Err(TransactionResolverError::UnknownOutPoint(out_point.clone()))
                        }
                        CellStatus::Live(cell_meta) => {
                            entry.insert(cell_meta.clone());
                            seen_inputs.insert(out_point.clone());
                            Ok(cell_meta)
                        }
                    }
                }
            }
        };

        let mut current_inputs = HashSet::new();

        let resolved_inputs = {
            let mut inputs = Vec::with_capacity(transaction.inputs().len());
            if !transaction.is_cellbase() {
                for out_point in transaction.input_pts_iter() {
                    if !current_inputs.insert(out_point.to_owned()) {
                        return Err(TransactionResolverError::DeadOutPoint(out_point));
                    }
                    inputs.push(resolve_cell(&out_point, false)?);
                }
            }
            inputs
        };

        let resolved_cell_deps = transaction
            .cell_deps()
            .into_iter()
            .map(|cell_dep| {
                let (dep_output, dep_data) = self
                    .get_cell_with_data(&cell_dep.out_point())
                    .expect("Failed to get cell with data for dep");
                let tx_info = self.get_tx_info(&cell_dep.out_point().tx_hash().unpack());
                let mut builder =
                    CellMetaBuilder::from_cell_output(dep_output, dep_data.to_vec().into())
                        .out_point(cell_dep.out_point());
                if tx_info.is_ok() {
                    builder = builder.transaction_info(tx_info.unwrap());
                }
                builder.build()
            })
            .collect();

        let result: Result<ResolvedTransaction, TransactionResolverError> =
            Ok(ResolvedTransaction {
                transaction,
                resolved_inputs,
                resolved_cell_deps,
                resolved_dep_groups: vec![],
            });

        result
    }
}

impl RpcProvider {
    pub fn resolve_tx_alt(&self, tx: Transaction) -> Result<ResolvedTransaction, ChainError> {
        let transaction = tx.into();
        let mut seen_inputs: HashSet<ckb_types::packed::OutPoint, _> = HashSet::new();

        let rtx = resolve_transaction_with_options(
            transaction,
            &mut seen_inputs,
            self,
            self,
            ResolveOptions::default(),
        );

        match rtx {
            Ok(rtx) => Ok(rtx),
            Err(e) => Err(ChainError::TxResolveError(e)),
        }
    }
}

impl RpcProvider {
    pub fn get_consensus(&self) -> Consensus {
        let mut inner = self.inner.lock();
        let genesis_block = {
            let response = inner
                .rpc_client
                .get_block_by_number(0.into())
                .expect("Failed to connect to RPC");
            match response {
                Some(block) => block,
                None => panic!("Genesis Block from chain is None"),
            }
        };

        let hardfork_switch = HardForkSwitch::new_without_any_enabled()
            .as_builder()
            .rfc_0032(200)
            .build()
            .unwrap();

        let builder = ConsensusBuilder::default()
            .hardfork_switch(hardfork_switch)
            .genesis_block(genesis_block.into());

        builder.build()
    }
}
