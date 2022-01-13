use super::generator::{CellQuery, CellQueryAttribute, QueryProvider, QueryStatement};
use crate::contract::generator::TransactionProvider;
use ckb_chain_spec::consensus::{Consensus, ConsensusBuilder, TYPE_ID_CODE_HASH};
use ckb_error::Error as CKBError;
use ckb_jsonrpc_types::TransactionView as JsonTransaction;
use ckb_script::{TransactionScriptsVerifier, TxVerifyEnv};
use ckb_traits::{CellDataProvider, HeaderProvider};
use ckb_types::{
    bytes::Bytes,
    core::{
        cell::{CellMeta, CellMetaBuilder, ResolvedTransaction},
        hardfork::HardForkSwitch,
        Capacity, Cycle, DepType, EpochExt, EpochNumberWithFraction, HeaderView, ScriptHashType,
        TransactionInfo, TransactionView,
    },
    packed::{Byte32, CellDep, CellOutput, OutPoint, Script},
    prelude::*,
};
use ckb_util::LinkedHashSet;
use ckb_verification::TransactionError;
use rand::{thread_rng, Rng};
use std::sync::{Arc, Mutex};
use std::{cell::RefCell, collections::HashMap};
pub type CellOutputWithData = (CellOutput, Bytes);

// Most of this is taken from https://github.com/nervosnetwork/ckb-tool.
// Reimplementation here due to slight changes in the API & version conflicts

const MAX_CYCLES: u64 = 500_0000;
pub fn random_hash() -> Byte32 {
    let mut rng = thread_rng();
    let mut buf = [0u8; 32];
    rng.fill(&mut buf);
    buf.pack()
}

pub fn random_out_point() -> OutPoint {
    OutPoint::new_builder().tx_hash(random_hash()).build()
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

#[derive(Default)]
pub struct MockChain {
    pub cells: HashMap<OutPoint, CellOutputWithData>,
    pub outpoint_txs: HashMap<OutPoint, TransactionInfo>,
    pub headers: HashMap<Byte32, HeaderView>,
    pub epoches: HashMap<Byte32, EpochExt>,
    pub cells_by_data_hash: HashMap<Byte32, OutPoint>,
    pub cells_by_lock_hash: HashMap<Byte32, Vec<OutPoint>>,
    pub cells_by_type_hash: HashMap<Byte32, Vec<OutPoint>>,
    pub debug: bool,
    messages: Arc<Mutex<Vec<Message>>>,
}

impl MockChain {
    pub fn deploy_cell_with_data(&mut self, data: Bytes) -> OutPoint {
        let data_hash = CellOutput::calc_data_hash(&data);
        if let Some(out_point) = self.cells_by_data_hash.get(&data_hash) {
            return out_point.to_owned();
        }
        let tx_hash = random_hash();
        let out_point = OutPoint::new(tx_hash, 0);
        let cell = CellOutput::new_builder()
            .capacity(Capacity::bytes(data.len()).expect("Data Capacity").pack())
            .build();

        self.cells.insert(out_point.clone(), (cell, data));
        self.cells_by_data_hash.insert(data_hash, out_point.clone());

        out_point
    }

    pub fn insert_header(&mut self, header: HeaderView) {
        self.headers.insert(header.hash(), header);
    }

    pub fn link_cell_with_block(&mut self, outp: OutPoint, hash: Byte32, tx_idx: usize) {
        let header = self.headers.get(&hash).expect("can't find the header");
        self.outpoint_txs.insert(
            outp,
            TransactionInfo::new(header.number(), header.epoch(), hash, tx_idx),
        );
    }

    pub fn get_cell_by_data_hash(&self, data_hash: &Byte32) -> Option<OutPoint> {
        self.cells_by_data_hash.get(data_hash).cloned()
    }

    pub fn create_cell(&mut self, cell: CellOutput, data: Bytes) -> OutPoint {
        let outpoint = random_out_point();
        self.create_cell_with_outpoint(outpoint.clone(), cell, data);
        outpoint
    }

    pub fn create_cell_with_outpoint(&mut self, outp: OutPoint, cell: CellOutput, data: Bytes) {
        let data_hash = CellOutput::calc_data_hash(&data);
        self.cells_by_data_hash.insert(data_hash, outp.clone());
        self.cells.insert(outp.clone(), (cell.clone(), data));
        let cells = self.get_cells_by_lock_hash(cell.calc_lock_hash());
        if let Some(mut cells) = cells {
            cells.push(outp.clone());
            self.cells_by_lock_hash.insert(cell.calc_lock_hash(), cells);
        } else {
            self.cells_by_lock_hash
                .insert(cell.calc_lock_hash(), vec![outp.clone()]);
        }

        if let Some(script) = cell.type_().to_opt() {
            let hash = script.calc_script_hash();
            let cells = self.get_cells_by_type_hash(hash.clone());
            if let Some(mut cells) = cells {
                cells.push(outp);
                self.cells_by_type_hash.insert(hash, cells);
            } else {
                self.cells_by_type_hash.insert(hash, vec![outp]);
            }
        }
    }

    pub fn get_cell(&self, out_point: &OutPoint) -> Option<CellOutputWithData> {
        self.cells.get(out_point).cloned()
    }

    pub fn build_script_with_hash_type(
        &mut self,
        outp: &OutPoint,
        typ: ScriptHashType,
        args: Bytes,
    ) -> Option<Script> {
        let (_, contract_data) = self.cells.get(outp)?;
        let data_hash = CellOutput::calc_data_hash(contract_data);
        Some(
            Script::new_builder()
                .code_hash(data_hash)
                .hash_type(typ.into())
                .args(args.pack())
                .build(),
        )
    }

    pub fn get_cells_by_lock_hash(&self, hash: Byte32) -> Option<Vec<OutPoint>> {
        self.cells_by_lock_hash.get(&hash).cloned()
    }

    pub fn get_cells_by_type_hash(&self, hash: Byte32) -> Option<Vec<OutPoint>> {
        self.cells_by_type_hash.get(&hash).cloned()
    }

    pub fn build_script(&mut self, outp: &OutPoint, args: Bytes) -> Option<Script> {
        self.build_script_with_hash_type(outp, ScriptHashType::Data1, args)
    }

    pub fn find_cell_dep_for_script(&self, script: &Script) -> CellDep {
        if script.hash_type() != ScriptHashType::Data.into()
            && script.hash_type() != ScriptHashType::Data1.into()
        {
            panic!("do not support hash_type {} yet", script.hash_type());
        }

        let out_point = self
            .get_cell_by_data_hash(&script.code_hash())
            .unwrap_or_else(|| {
                panic!(
                    "Cannot find contract out point with data_hash: {}",
                    &script.code_hash()
                )
            });
        CellDep::new_builder()
            .out_point(out_point)
            .dep_type(DepType::Code.into())
            .build()
    }

    pub fn complete_tx(&mut self, tx: TransactionView) -> TransactionView {
        let mut cell_deps: LinkedHashSet<CellDep> = LinkedHashSet::new();

        for cell_dep in tx.cell_deps_iter() {
            cell_deps.insert(cell_dep);
        }

        for i in tx.input_pts_iter() {
            if let Some((cell, _data)) = self.cells.get(&i) {
                let dep = self.find_cell_dep_for_script(&cell.lock());
                cell_deps.insert(dep);

                if let Some(script) = cell.type_().to_opt() {
                    if script.code_hash() != TYPE_ID_CODE_HASH.pack()
                        || script.hash_type() != ScriptHashType::Type.into()
                    {
                        let dep = self.find_cell_dep_for_script(&script);
                        cell_deps.insert(dep);
                    }
                }
            }
        }

        for (cell, _data) in tx.outputs_with_data_iter() {
            if let Some(script) = cell.type_().to_opt() {
                if script.code_hash() != TYPE_ID_CODE_HASH.pack()
                    || script.hash_type() != ScriptHashType::Type.into()
                {
                    let dep = self.find_cell_dep_for_script(&script);
                    cell_deps.insert(dep);
                }
            }
        }

        tx.as_advanced_builder()
            .set_cell_deps(Vec::new())
            .cell_deps(cell_deps.into_iter().collect::<Vec<_>>().pack())
            .build()
    }

    pub fn build_resolved_tx(&self, tx: &TransactionView) -> ResolvedTransaction {
        let input_cells = tx
            .inputs()
            .into_iter()
            .map(|input| {
                let previous_out_point = input.previous_output();
                let (input_output, input_data) = self.cells.get(&previous_out_point).unwrap();
                let tx_info_opt = self.outpoint_txs.get(&previous_out_point);
                let mut b = CellMetaBuilder::from_cell_output(
                    input_output.to_owned(),
                    input_data.to_vec().into(),
                )
                .out_point(previous_out_point);
                if let Some(tx_info) = tx_info_opt {
                    b = b.transaction_info(tx_info.to_owned());
                }
                b.build()
            })
            .collect();
        let resolved_cell_deps = tx
            .cell_deps()
            .into_iter()
            .map(|deps_out_point| {
                let (dep_output, dep_data) = self.cells.get(&deps_out_point.out_point()).unwrap();
                let tx_info_opt = self.outpoint_txs.get(&deps_out_point.out_point());
                let mut b = CellMetaBuilder::from_cell_output(
                    dep_output.to_owned(),
                    dep_data.to_vec().into(),
                )
                .out_point(deps_out_point.out_point());
                if let Some(tx_info) = tx_info_opt {
                    b = b.transaction_info(tx_info.to_owned());
                }
                b.build()
            })
            .collect();
        ResolvedTransaction {
            transaction: tx.clone(),
            resolved_cell_deps,
            resolved_inputs: input_cells,
            resolved_dep_groups: vec![],
        }
    }

    fn verify_tx_consensus(&self, tx: &TransactionView) -> Result<(), CKBError> {
        OutputsDataVerifier::new(tx).verify()?;
        Ok(())
    }

    pub fn capture_debug(&self) -> bool {
        self.debug
    }

    /// Capture debug output, default value is false
    pub fn set_capture_debug(&mut self, capture_debug: bool) {
        self.debug = capture_debug;
    }

    /// return captured messages
    pub fn captured_messages(&self) -> Vec<Message> {
        self.messages.lock().unwrap().clone()
    }

    /// Verify the transaction by given context (Consensus, TxVerifyEnv) in CKB-VM
    ///
    /// Please see below links for more details:
    ///   - https://docs.rs/ckb-chain-spec/0.101.2/ckb_chain_spec/consensus/struct.Consensus.html
    ///   - https://docs.rs/ckb-types/0.101.2/ckb_types/core/hardfork/struct.HardForkSwitch.html
    ///   - https://docs.rs/ckb-script/0.101.2/ckb_script/struct.TxVerifyEnv.html
    pub fn verify_tx_by_context(
        &self,
        tx: &TransactionView,
        max_cycles: u64,
        consensus: &Consensus,
        tx_env: &TxVerifyEnv,
    ) -> Result<Cycle, CKBError> {
        self.verify_tx_consensus(tx)?;
        let resolved_tx = self.build_resolved_tx(tx);
        let mut verifier = TransactionScriptsVerifier::new(&resolved_tx, consensus, self, tx_env);
        if self.debug {
            let captured_messages = self.messages.clone();
            verifier.set_debug_printer(move |id, message| {
                let msg = Message {
                    id: id.clone(),
                    message: message.to_string(),
                };
                captured_messages.lock().unwrap().push(msg);
            });
        } else {
            verifier.set_debug_printer(|_id, msg| {
                println!("[contract debug] {}", msg);
            });
        }
        verifier.verify(max_cycles)
    }

    /// Verify the transaction in CKB-VM
    ///
    /// This method use a default verify context with:
    ///   - use HardForkSwitch to set `rfc_0032` field to 200 (means enable VM selection feature after epoch 200)
    ///   - use TxVerifyEnv to set currently transaction `epoch` number to 300
    pub fn verify_tx(&self, tx: &TransactionView, max_cycles: u64) -> Result<Cycle, CKBError> {
        let consensus = {
            let hardfork_switch = HardForkSwitch::new_without_any_enabled()
                .as_builder()
                .rfc_0032(200)
                .build()
                .unwrap();
            ConsensusBuilder::default()
                .hardfork_switch(hardfork_switch)
                .build()
        };
        let tx_env = {
            let epoch = EpochNumberWithFraction::new(300, 0, 1);
            let header = HeaderView::new_advanced_builder()
                .epoch(epoch.pack())
                .build();
            TxVerifyEnv::new_commit(&header)
        };
        self.verify_tx_by_context(tx, max_cycles, &consensus, &tx_env)
    }

    pub fn receive_tx(&mut self, tx: &TransactionView) -> Result<Byte32, CKBError> {
        match self.verify_tx(tx, MAX_CYCLES) {
            Ok(_) => {
                let tx_hash = tx.hash();
                let mut idx: u32 = 0;
                tx.outputs_with_data_iter().for_each(|out| {
                    let outpoint = OutPoint::new_builder()
                        .tx_hash(tx_hash.clone())
                        .index(idx.pack())
                        .build();
                    self.create_cell_with_outpoint(outpoint, out.0, out.1);
                    idx += 1;
                });
                Ok(tx_hash)
            }
            Err(_) => todo!(),
        }
    }
}

impl CellDataProvider for MockChain {
    // load Cell Data
    fn load_cell_data(&self, cell: &CellMeta) -> Option<Bytes> {
        cell.mem_cell_data
            .as_ref()
            .map(|data| Bytes::from(data.to_vec()))
            .or_else(|| self.get_cell_data(&cell.out_point))
    }

    fn get_cell_data(&self, out_point: &OutPoint) -> Option<Bytes> {
        self.cells
            .get(out_point)
            .map(|(_, data)| Bytes::from(data.to_vec()))
    }

    fn get_cell_data_hash(&self, out_point: &OutPoint) -> Option<Byte32> {
        self.cells
            .get(out_point)
            .map(|(_, data)| CellOutput::calc_data_hash(data))
    }
}

impl HeaderProvider for MockChain {
    // load header
    fn get_header(&self, block_hash: &Byte32) -> Option<HeaderView> {
        self.headers.get(block_hash).cloned()
    }
}

pub struct MockChainTxProvider {
    pub chain: RefCell<MockChain>,
}

impl MockChainTxProvider {
    pub fn new(chain: MockChain) -> Self {
        Self {
            chain: RefCell::new(chain),
        }
    }
}

impl TransactionProvider for MockChainTxProvider {
    fn send_tx(&self, tx: JsonTransaction) -> Option<ckb_jsonrpc_types::Byte32> {
        let mut chain = self.chain.borrow_mut();
        let inner_tx = tx.inner;
        let inner_tx = ckb_types::packed::Transaction::from(inner_tx);
        let converted_tx_view = inner_tx.as_advanced_builder().build();
        let tx = chain.complete_tx(converted_tx_view);
        if let Ok(hash) = chain.receive_tx(&tx) {
            let tx_hash: ckb_jsonrpc_types::Byte32 = hash.into();
            Some(tx_hash)
        } else {
            None
        }
    }

    fn verify_tx(&self, tx: JsonTransaction) -> bool {
        let mut chain = self.chain.borrow_mut();
        let inner_tx = tx.inner;
        let inner_tx = ckb_types::packed::Transaction::from(inner_tx);
        let converted_tx_view = inner_tx.as_advanced_builder().build();
        let tx = chain.complete_tx(converted_tx_view);
        let result = chain.verify_tx(&tx, MAX_CYCLES);
        match result {
            Ok(_) => true,
            Err(e) => {
                println!("Error in tx verify: {:?}", e);
                false
            }
        }
    }
}

impl QueryProvider for MockChainTxProvider {
    fn query(&self, query: CellQuery) -> Option<Vec<ckb_jsonrpc_types::OutPoint>> {
        let CellQuery { _query, _limit } = query;
        println!("QUERY FROM QUERY PROVIDER: {:?}", _query);
        match _query {
            QueryStatement::Single(query_attr) => match query_attr {
                CellQueryAttribute::LockHash(hash) => {
                    let cells = self.chain.borrow().get_cells_by_lock_hash(hash.into());
                    Some(
                        cells
                            .unwrap()
                            .into_iter()
                            .map(|outp| outp.into())
                            .collect::<Vec<ckb_jsonrpc_types::OutPoint>>(),
                    )
                }
                CellQueryAttribute::LockScript(script) => {
                    let script = ckb_types::packed::Script::from(script);
                    let cells = self
                        .chain
                        .borrow()
                        .get_cells_by_lock_hash(script.calc_script_hash());
                    Some(
                        cells
                            .unwrap()
                            .into_iter()
                            .map(|outp| outp.into())
                            .collect::<Vec<ckb_jsonrpc_types::OutPoint>>(),
                    )
                }
                CellQueryAttribute::TypeScript(script) => {
                    let script = ckb_types::packed::Script::from(script);
                    let cells = self
                        .chain
                        .borrow()
                        .get_cells_by_type_hash(script.calc_script_hash());
                    Some(
                        cells
                            .unwrap()
                            .into_iter()
                            .map(|outp| outp.into())
                            .collect::<Vec<ckb_jsonrpc_types::OutPoint>>(),
                    )
                }
                _ => panic!("Capacity based queries currently unsupported!"),
            },
            _ => panic!("Compund queries currently unsupported!"),
        }
    }
}
