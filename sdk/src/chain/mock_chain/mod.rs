use crate::chain::*;
use crate::contract::generator::{
    CellQuery, CellQueryAttribute, QueryProvider, QueryStatement, TransactionProvider,
};
use crate::contract::schema::{BytesConversion, JsonByteConversion, JsonConversion, MolConversion};
use ckb_chain_spec::consensus::{Consensus, ConsensusBuilder};
use ckb_error::Error as CKBError;
use ckb_jsonrpc_types::TransactionView as JsonTransaction;
use ckb_resource::BUNDLED;
use ckb_script::{TransactionScriptsVerifier, TxVerifyEnv};
use ckb_traits::{CellDataProvider, HeaderProvider};
use ckb_types::core::{BlockBuilder, BlockView, HeaderBuilder, TransactionBuilder};
use ckb_types::packed::{CellInput, CellOutputBuilder, ScriptOptBuilder};
use ckb_types::{
    bytes::Bytes,
    core::{
        cell::{CellMeta, CellMetaBuilder, ResolvedTransaction},
        hardfork::HardForkSwitch,
        Capacity, Cycle, DepType, EpochExt, EpochNumberWithFraction, HeaderView, ScriptHashType,
        TransactionInfo, TransactionView,
    },
    packed::{Byte32, CellDep, CellOutput, OutPoint, Script},
};
use ckb_util::LinkedHashSet;
use rand::{thread_rng, Rng};

use ckb_always_success_script::ALWAYS_SUCCESS;
use ckb_system_scripts::BUNDLED_CELL;

use std::sync::{Arc, Mutex};
use std::{cell::RefCell, collections::HashMap};
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

pub type CellOutputWithData = (CellOutput, Bytes);

#[derive(Clone, Debug)]
pub struct MockChain {
    pub cells: HashMap<OutPoint, CellOutputWithData>,
    pub outpoint_txs: HashMap<OutPoint, TransactionInfo>,
    pub headers: HashMap<Byte32, HeaderView>,
    pub epoches: HashMap<Byte32, EpochExt>,
    pub cells_by_data_hash: HashMap<Byte32, OutPoint>,
    pub cells_by_lock_hash: HashMap<Byte32, Vec<OutPoint>>,
    pub cells_by_type_hash: HashMap<Byte32, Vec<OutPoint>>,
    pub genesis_info: Option<GenesisInfo>,
    pub default_lock: Option<OutPoint>,
    pub debug: bool,
    messages: Arc<Mutex<Vec<Message>>>,
}

impl Default for MockChain {
    fn default() -> Self {
        let mut chain = Self {
            cells: Default::default(),
            outpoint_txs: Default::default(),
            headers: Default::default(),
            epoches: Default::default(),
            cells_by_data_hash: Default::default(),
            cells_by_lock_hash: Default::default(),
            cells_by_type_hash: Default::default(),
            genesis_info: None,
            default_lock: None,
            debug: Default::default(),
            messages: Default::default(),
        };

        // Deploy always success script as default lock script
        // This is required to deploy a random cell during the genesis event
        let default_lock = chain.deploy_cell_with_data(Bytes::from(ALWAYS_SUCCESS.to_vec()));
        chain.default_lock = Some(default_lock);

        // Deploy system scripts to the chain

        // Run genesis event on the mockchain
        genesis_event(&mut chain);

        

        // Return chain
        chain
    }
}

impl PartialEq for MockChain {
    // Simple equality check for testing purposes
    // Curves around genesis info not implementing PartialEq
    fn eq(&self, other: &Self) -> bool {
        self.cells == other.cells &&
        self.default_lock == other.default_lock &&
        self.outpoint_txs == other.outpoint_txs &&
        self.headers == other.headers &&
        self.epoches == other.epoches &&
        self.cells_by_data_hash == other.cells_by_data_hash &&
        self.cells_by_lock_hash == other.cells_by_lock_hash &&
        self.cells_by_type_hash == other.cells_by_type_hash &&  
        self.debug == other.debug &&

        // Compare GenesisInfo
        self.genesis_info.as_ref().unwrap().sighash_data_hash() == other.genesis_info.as_ref().unwrap().sighash_data_hash() &&
        self.genesis_info.as_ref().unwrap().sighash_type_hash() == other.genesis_info.as_ref().unwrap().sighash_type_hash() &&
        self.genesis_info.as_ref().unwrap().multisig_data_hash() == other.genesis_info.as_ref().unwrap().multisig_data_hash() &&
        self.genesis_info.as_ref().unwrap().multisig_type_hash() == other.genesis_info.as_ref().unwrap().multisig_type_hash() &&
        self.genesis_info.as_ref().unwrap().dao_data_hash() == other.genesis_info.as_ref().unwrap().dao_data_hash() &&
        self.genesis_info.as_ref().unwrap().dao_type_hash() == other.genesis_info.as_ref().unwrap().dao_type_hash()
    }
}

impl MockChain {
    pub fn deploy_cell_with_data(&mut self, data: Bytes) -> OutPoint {
        let data_hash = CellOutput::calc_data_hash(&data);
        if let Some(out_point) = self.cells_by_data_hash.get(&data_hash) {
            return out_point.to_owned();
        }
        let tx_hash = random_hash();
        let out_point = OutPoint::new(tx_hash, 0);
        let cell_builder = CellOutput::new_builder()
            .capacity(Capacity::bytes(data.len()).expect("Data Capacity").pack())
            .type_(
                ScriptOptBuilder::default()
                    .set(Some(
                        Script::new_builder().code_hash(data_hash.clone()).build(),
                    ))
                    .build(),
            );

        let cell = cell_builder.build();

        self.cells.insert(out_point.clone(), (cell, data));
        self.cells_by_data_hash.insert(data_hash, out_point.clone());
        out_point
    }

    pub fn deploy_cell_output(&mut self, data: Bytes, output: CellOutput) -> OutPoint {
        let data_hash = CellOutput::calc_data_hash(&data);
        if let Some(out_point) = self.cells_by_data_hash.get(&data_hash) {
            return out_point.to_owned();
        }
        let tx_hash = random_hash();
        let out_point = OutPoint::new(tx_hash, 0);
        self.create_cell_with_outpoint(out_point.clone(), output, data);
        out_point
    }

    pub fn get_default_script_outpoint(&self) -> OutPoint {
        self.default_lock.clone().unwrap()
    }

    pub fn deploy_random_cell_with_default_lock(
        &mut self,
        capacity: usize,
        args: Option<Bytes>,
    ) -> OutPoint {
        let script = {
            if let Some(args) = args {
                self.build_script(&self.get_default_script_outpoint(), args)
            } else {
                self.build_script(&self.get_default_script_outpoint(), Bytes::default())
            }
        }
        .unwrap();
        let tx_hash = random_hash();
        let out_point = OutPoint::new(tx_hash, 0);
        let cell = CellOutput::new_builder()
            .capacity(Capacity::bytes(capacity).expect("Data Capacity").pack())
            .lock(script)
            .build();
        self.create_cell_with_outpoint(out_point.clone(), cell, Bytes::default());
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
        &self,
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

    pub fn build_script(&self, outp: &OutPoint, args: Bytes) -> Option<Script> {
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

        // for i in tx.input_pts_iter() {
        //     if let Some((cell, _data)) = self.cells.get(&i) {
        //         let dep = self.find_cell_dep_for_script(&cell.lock());
        //         cell_deps.insert(dep);

        //         if let Some(script) = cell.type_().to_opt() {
        //             if script.code_hash() != TYPE_ID_CODE_HASH.pack()
        //                 || script.hash_type() != ScriptHashType::Type.into()
        //             {
        //                 let dep = self.find_cell_dep_for_script(&script);
        //                 cell_deps.insert(dep);
        //             }
        //         }
        //     }
        // }

        // for (cell, _data) in tx.outputs_with_data_iter() {
        //     if let Some(script) = cell.type_().to_opt() {
        //         if script.code_hash() != TYPE_ID_CODE_HASH.pack()
        //             || script.hash_type() != ScriptHashType::Type.into()
        //         {
        //             let dep = self.find_cell_dep_for_script(&script);
        //             cell_deps.insert(dep);
        //         }
        //     }
        // }

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
        println!("RESOLVED CELL DEPS: {:#?}", resolved_cell_deps);
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
    ///always
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
        println!(
            "TX AFTER CHAIN COMPLETE {:#?}",
            ckb_jsonrpc_types::TransactionView::from(tx.clone())
        );
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
    fn query_cell_meta(&self, query: CellQuery) -> Option<Vec<CellMeta>> {
        if let Some(outpoints) = self.query(query) {
            println!("OUTPOINTS TO CREATE CELL META: {:?}", outpoints);
            Some(
                outpoints
                    .iter()
                    .map(|outp| {
                        let outp = ckb_types::packed::OutPoint::from(outp.clone());
                        let cell_output = self.chain.borrow().get_cell(&outp).unwrap();
                        CellMetaBuilder::from_cell_output(cell_output.0, cell_output.1)
                            .out_point(outp)
                            .build()
                    })
                    .collect(),
            )
        } else {
            println!("NO OUTPOINTS TO RESOLVE IN QUERY CELL META");
            None
        }
    }
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
                CellQueryAttribute::DataHash(hash) => Some(vec![self
                    .chain
                    .borrow()
                    .get_cell_by_data_hash(&hash.into())
                    .unwrap()
                    .into()]),
                _ => panic!("Capacity based queries currently unsupported!"),
            },
            _ => panic!("Compund queries currently unsupported!"),
        }
    }
}

impl Chain for MockChain {
    type Inner = MockChainTxProvider;

    fn inner(&self) -> Self::Inner {
        MockChainTxProvider::new(self.clone())
    }

    fn deploy_cell(&mut self, cell: &Cell) -> ChainResult<OutPoint> {
        let (outp, data): CellOutputWithData = cell.into();
        Ok(self.deploy_cell_output(data, outp))
    }

    // Check how the genesis block is deployed on actual chains
    fn genesis_info(&self) -> Option<GenesisInfo> {
        self.genesis_info.clone()
    }

    fn set_genesis_info(&mut self, genesis_info: GenesisInfo) {
        self.genesis_info = Some(genesis_info);
    }

    fn set_default_lock<A, D>(&mut self, lock: Contract<A, D>)
    where
        D: JsonByteConversion + MolConversion + BytesConversion + Clone + Default,
        A: JsonByteConversion + MolConversion + BytesConversion + Clone + Default,
    {
        let (outp, data) = lock.as_code_cell();
        let outpoint = self.deploy_cell_output(data, outp);
        self.default_lock = Some(outpoint);
    }

    fn generate_cell_with_default_lock(&self, lock_args: crate::types::bytes::Bytes) -> Cell {
        let script = self
            .build_script(
                &self.get_default_script_outpoint(),
                lock_args.clone().into(),
            )
            .unwrap();
        let mut cell = Cell::default();
        cell.set_lock_script(script).unwrap();
        cell.set_lock_args(lock_args).unwrap();
        cell
    }
}

// // Deploy system scripts from ckb-system-scripts bundled cell
// fn genesis_event(chain: &mut MockChain) {
//     todo!()
//     // let bundle = BUNDLED_CELLS
// }

// for script in BUNDLED_CELL.file_names() {
//             let data = BUNDLED_CELL.get(script).unwrap();
//             let out_point = chain.deploy_cell_with_data(Bytes::from(data.to_vec()));
// }

// fn deploy_system_scripts(chain: &mut MockChain, cell: &Cell) -> ChainResult<OutPoint> {
//     let (outp, data): CellOutputWithData = cell.into();
//     let script = chain.build_script(&outp, data.clone().into()).unwrap();
//     let outpoint = chain.deploy_cell_output(data, outp);
//     Ok(outpoint)
// }

struct GenesisScripts {
    secp256k1_data: Bytes,
    secp256k1_blake160_sighash_all: Bytes,
    secp256k1_blake160_multisig_all: Bytes,
    dao: Bytes,
}

impl Default for GenesisScripts {
    fn default() -> Self {
        let bundle = &BUNDLED_CELL;
        GenesisScripts {
            secp256k1_data: Bytes::from(bundle.get("specs/cells/secp256k1_data").unwrap().to_vec()),
            secp256k1_blake160_sighash_all: Bytes::from(
                bundle
                    .get("specs/cells/secp256k1_blake160_sighash_all")
                    .unwrap()
                    .to_vec(),
            ),
            secp256k1_blake160_multisig_all: Bytes::from(
                bundle
                    .get("specs/cells/secp256k1_blake160_multisig_all")
                    .unwrap()
                    .to_vec(),
            ),
            dao: Bytes::from(bundle.get("specs/cells/dao").unwrap().to_vec()),
        }
    }
}

// Deploy every system script from a genesis script to a MockChain return GenesisInfo
fn genesis_event(
    chain: &mut MockChain,
) -> GenesisInfo {
    // Does nothing if the genesis scripts are already deployed
    match &chain.genesis_info {
        // Should return the existing genesis info
        Some(genesis_info) => {
            genesis_info.clone()
        }

        // Run genesis event
        // Deploy genesis scripts, create a block, set the genesis info and return it
        None => {
            // Deploy scripts
            deploy_genesis_scripts(chain, None);
            // Create genesis block
            let block = genesis_block_from_chain(chain);
            // Insert block header
            chain.insert_header(block.clone().header());
            // Return genesis info
            GenesisInfo::from_block(&block).expect("Failed to create genesis info from genesis block")
        }
    }
}

fn deploy_genesis_scripts(
    chain: &mut MockChain,
    scripts: Option<GenesisScripts>
) {
    // Generate default scripts if no scripts were passed
    let scripts = scripts.unwrap_or_default();

    // Deploy every script to the chain
    chain.deploy_cell_with_data(scripts.secp256k1_data.clone());
    chain.deploy_cell_with_data(scripts.secp256k1_blake160_sighash_all.clone());
    chain.deploy_cell_with_data(scripts.secp256k1_blake160_multisig_all.clone());
    chain.deploy_cell_with_data(scripts.dao.clone());
}



fn genesis_block_from_chain(chain: &mut MockChain) -> BlockView {
    let block: BlockBuilder = BlockBuilder::default();

    let tx = TransactionBuilder::default();

    let secp256k1_data_code_hash_bytes =
        Byte32::from_slice(&ckb_system_scripts::CODE_HASH_SECP256K1_DATA).unwrap();
    let secp256k1_data_outpoint = chain
        .get_cell_by_data_hash(&secp256k1_data_code_hash_bytes)
        .unwrap();
    let secp256k1_data = chain.get_cell(&secp256k1_data_outpoint).unwrap();
    let tx = tx.output(secp256k1_data.0.clone());
    let tx = tx.output_data(secp256k1_data.1.clone().pack());

    let blake_sighash_all_code_hash_bytes =
        Byte32::from_slice(&ckb_system_scripts::CODE_HASH_SECP256K1_BLAKE160_SIGHASH_ALL).unwrap();
    let blake_sighash_all_outpoint = chain
        .get_cell_by_data_hash(&blake_sighash_all_code_hash_bytes)
        .unwrap();
    let blake_sighash_all = chain.get_cell(&blake_sighash_all_outpoint).unwrap();
    let tx = tx.output(blake_sighash_all.0.clone());
    let tx = tx.output_data(blake_sighash_all.1.clone().pack());

    let dao_code_hash_bytes = Byte32::from_slice(&ckb_system_scripts::CODE_HASH_DAO).unwrap();
    let dao_outpoint = chain.get_cell_by_data_hash(&dao_code_hash_bytes).unwrap();
    let dao = chain.get_cell(&dao_outpoint).unwrap();
    let tx = tx.output(dao.0.clone());
    let tx = tx.output_data(dao.1.clone().pack());

    // Some cell without data or scripts to complete the genesis block and respect the script order
    let random_cell_outpoint = chain.deploy_random_cell_with_default_lock(100000, None);
    let random_cell = chain.get_cell(&random_cell_outpoint).unwrap();
    let tx = tx.output(random_cell.0.clone());
    let tx = tx.output_data(random_cell.1.clone().pack());

    let blake_multisig_code_hash_bytes =
        Byte32::from_slice(&ckb_system_scripts::CODE_HASH_SECP256K1_BLAKE160_MULTISIG_ALL).unwrap();
    let blake_multisig_outpoint = chain
        .get_cell_by_data_hash(&blake_multisig_code_hash_bytes)
        .unwrap();
    let blake_multisig = chain.get_cell(&blake_multisig_outpoint).unwrap();
    let tx = tx.output(blake_multisig.0.clone());
    let tx = tx.output_data(blake_multisig.1.clone().pack());

    let block = block.transaction(tx.build());

    block.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mockchain_setup() -> (mock_chain::MockChain, ckb_types::core::BlockView) {
        // Create a new mockchain
        let mut chain = MockChain::default();

        // Generate genesis block
        let genesis_block = genesis_block_from_chain(&mut chain);

        (chain, genesis_block)
    }

    #[test]
    fn default_mockchain_has_system_scripts_and_genesisinfo() {
        let (chain, _genesis_block) = mockchain_setup();

        // Check each script
        let multisig_code_hash_bytes =
            Byte32::from_slice(&ckb_system_scripts::CODE_HASH_SECP256K1_BLAKE160_MULTISIG_ALL)
                .unwrap();
        let multisig_outpoint = chain
            .get_cell_by_data_hash(&multisig_code_hash_bytes);
        assert!(multisig_outpoint.is_some());

        let blake160_sighash_all_code_hash_bytes =
            Byte32::from_slice(&ckb_system_scripts::CODE_HASH_SECP256K1_BLAKE160_SIGHASH_ALL)
                .unwrap();
        let blake160_sighash_all_outpoint = chain
            .get_cell_by_data_hash(&blake160_sighash_all_code_hash_bytes);
        assert!(blake160_sighash_all_outpoint.is_some());

        let dao = chain.get_cell_by_data_hash(&Byte32::from_slice(&ckb_system_scripts::CODE_HASH_DAO).unwrap());
        assert!(dao.is_some());

        let secp_data = chain.get_cell_by_data_hash(&Byte32::from_slice(&ckb_system_scripts::CODE_HASH_SECP256K1_DATA).unwrap());
        assert!(secp_data.is_some());
    }

    #[test]
    fn genesis_info_from_genesis_block_returns_ok() {
        let (chain, genesis_block) = mockchain_setup();

        let genesis_info = GenesisInfo::from_block(&genesis_block);

        assert!(genesis_info.is_ok());
    }

    #[test]
    fn test_genesis_block_has_dao_cell() {
        let (mut chain, genesis_block) = mockchain_setup();

        // Get the cell by hash
        let dao_code_hash_bytes = Byte32::from_slice(&ckb_system_scripts::CODE_HASH_DAO).unwrap();
        let dao_outpoint = chain.get_cell_by_data_hash(&dao_code_hash_bytes).unwrap();
        let dao_cell = chain.get_cell(&dao_outpoint).unwrap();
        let cell_by_hash = dao_cell.0;

        // Get cell by location
        let location = crate::types::constants::DAO_OUTPUT_LOC; // TX 0 OUTP 2
        let cell_by_location_in_block = genesis_block.transactions()[location.0]
            .outputs()
            .get(location.1)
            .unwrap();

        // Compare the two
        assert_eq!(cell_by_hash, cell_by_location_in_block);
    }

    #[test]
    fn test_genesis_block_has_secp_multisig_cell() {
        let (mut chain, genesis_block) = mockchain_setup();

        // Get the cell by hash
        let multisig_code_hash_bytes =
            Byte32::from_slice(&ckb_system_scripts::CODE_HASH_SECP256K1_BLAKE160_MULTISIG_ALL)
                .unwrap();
        let multisig_outpoint = chain
            .get_cell_by_data_hash(&multisig_code_hash_bytes)
            .unwrap();
        let multisig_cell = chain.get_cell(&multisig_outpoint).unwrap();
        let cell_by_hash = multisig_cell.0;

        // Get cell by location
        let location = crate::types::constants::MULTISIG_OUTPUT_LOC; // TX 0 OUTP 4
        let cell_by_location_in_block = genesis_block.transactions()[location.0]
            .outputs()
            .get(location.1)
            .unwrap();

        // Compare the two
        assert_eq!(cell_by_hash, cell_by_location_in_block);

        // Check the cell's data
        let data_hash = CellOutput::calc_data_hash(&multisig_cell.1);
        assert_eq!(
            ckb_resource::CODE_HASH_SECP256K1_BLAKE160_MULTISIG_ALL.pack(),
            data_hash
        );
    }

    #[test]
    fn test_genesis_block_has_secp_sighash_cell() {
        let (chain, genesis_block) = mockchain_setup();

        // Get the cell by hash
        let secp_sighash_outp = chain
            .get_cell_by_data_hash(
                &Byte32::from_slice(&ckb_system_scripts::CODE_HASH_SECP256K1_BLAKE160_SIGHASH_ALL)
                    .unwrap(),
            )
            .unwrap();
        let secp_sighash_cell = chain.get_cell(&secp_sighash_outp).unwrap();
        let cell_by_hash = secp_sighash_cell.0;

        let location = crate::types::constants::SIGHASH_OUTPUT_LOC; // TX 0 OUTP 1
        let cell_by_location_in_block = genesis_block.transactions()[location.0]
            .outputs()
            .get(location.1)
            .unwrap();

        assert_eq!(cell_by_location_in_block, cell_by_hash);
    }

    #[test]
    fn test_genesis_block_has_number_0() {
        // Create a new mockchain
        let mut chain = MockChain::default();

        // Create default genesis scripts
        let genesis_scripts = GenesisScripts::default();

        // Run genesis event on the mockchain with the scripts
        let scripts = genesis_event(&mut chain);

        // Generate genesis block
        let genesis_block = genesis_block_from_chain(&mut chain);

        // Check if genesis block has number 0
        assert_eq!(genesis_block.header().number(), 0);
    }
}
