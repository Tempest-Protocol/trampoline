use super::genesis_info::genesis_event;
use crate::chain::mock_chain::MAX_CYCLES;
use crate::chain::*;
use crate::contract::generator::{QueryProvider, TransactionProvider};
use crate::query::{CellQuery, CellQueryAttribute, QueryStatement};

use ckb_always_success_script::ALWAYS_SUCCESS;
use ckb_jsonrpc_types::TransactionView as JsonTransaction;
use ckb_traits::{CellDataProvider, HeaderProvider};
use ckb_types::{
    bytes::Bytes,
    core::{
        cell::{CellMeta, CellMetaBuilder},
        HeaderView,
    },
    packed::{Byte32, CellOutput, OutPoint},
};
use std::cell::RefCell;

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

    fn deploy_cell(
        &mut self,
        cell: &Cell,
        _unlockers: Unlockers,
        _inputs: &CellInputs,
    ) -> ChainResult<OutPoint> {
        let (outp, data): CellOutputWithData = cell.into();
        Ok(self.deploy_cell_output(data, outp))
    }

    fn set_default_lock(&mut self, lock: OutPoint) -> Result<(), ChainError> {
        // let (outp, data) = (lock.clone().into(), lock.data());
        // let outpoint = self.deploy_cell_output(data.into(), outp);
        self.default_lock = Some(lock);
        Ok(())
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

    fn deploy_cells(
        &mut self,
        cells: &Vec<Cell>,
        _unlockers: Unlockers,
        _inputs: &CellInputs,
    ) -> ChainResult<Vec<OutPoint>> {
        Ok(cells
            .iter()
            .map(|c| {
                let (outp, data): CellOutputWithData = c.into();
                self.deploy_cell_output(data, outp)
            })
            .collect::<Vec<_>>())
    }
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_deploy_cell_and_fetch_cell() {
        // Setup chain
        let mut chain = MockChain::default();

        // Setup cell to deploy
        let lock_args = Bytes::from(b"test".to_vec());
        let cell = chain.generate_cell_with_default_lock(lock_args.into());

        // Setup cell inputs for the tx to include
        let inputs = CellInputs::Empty;

        // Deploy cell and get outpoint
        let outpoint = chain.deploy_cell(&cell, HashMap::new(), &inputs).unwrap();

        // Retrieve the deployed cell
        let fetched_cell = chain.get_cell(&outpoint).unwrap();

        let cells = chain.cells;
        println!("{:?}", cells.get(&outpoint).unwrap().0.lock().args());

        assert_eq!(
            format!("{:?}", CellOutputWithData::from(cell).0.lock().args()),
            format!("{:?}", fetched_cell.0.lock().args())
        );
    }

    #[test]
    fn test_deploy_two_cells_and_fetch_them() {
        let mut chain = MockChain::default();
        let lock_args = Bytes::from(b"test".to_vec());
        let cell = chain.generate_cell_with_default_lock(lock_args.clone().into());
        let cell2 = chain.generate_cell_with_default_lock(lock_args.into());
        let inputs = CellInputs::Empty;
        let outpoint = chain.deploy_cell(&cell, HashMap::new(), &inputs).unwrap();
        let outpoint2 = chain.deploy_cell(&cell2, HashMap::new(), &inputs).unwrap();
        let fetched_cell = chain.get_cell(&outpoint).unwrap();
        let fetched_cell2 = chain.get_cell(&outpoint2).unwrap();
        assert_eq!(
            format!("{:?}", cell),
            format!("{:?}", Cell::from(fetched_cell.0))
        );
        assert_eq!(
            format!("{:?}", cell2),
            format!("{:?}", Cell::from(fetched_cell2.0))
        );
    }
}
