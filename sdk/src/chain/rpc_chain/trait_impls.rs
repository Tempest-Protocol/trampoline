use super::*;
use crate::chain::{CellInputs, Chain};
use crate::ckb_types::prelude::{Builder, Entity};
use ckb_sdk::{rpc::ckb_indexer::Order, traits::CellQueryOptions};
use ckb_types::packed::CellOutput;
use ckb_types::packed::Script as CkbScript;
use provider::RpcProvider;
use std::prelude::v1::*;

impl Chain for RpcChain {
    type Inner = RpcProvider;

    fn inner(&self) -> Self::Inner {
        RpcProvider::new(self.clone())
    }

    fn deploy_cell(
        &mut self,
        cell: &crate::types::cell::Cell,
        unlockers: crate::chain::Unlockers,
        inputs: &crate::chain::CellInputs,
    ) -> crate::chain::ChainResult<OutPoint> {
        let inputs_script = match inputs {
            CellInputs::ScriptQuery(script) => script,
            CellInputs::Empty => {
                return Err(crate::chain::ChainError::InvalidInputs(inputs.clone()))
            }
        };

        let builder = TransactionBuilder::default()
            .add_output(cell.clone())
            .balance(inputs_script.clone().into(), None, self)
            .expect("Failed to balance transaction");

        let (unlocked_tx, locked_groups) = builder
            .unlock(&unlockers, self)
            .expect("Failed to unlock transaction");

        match locked_groups.len() {
            0 => {
                let tx = unlocked_tx.build();
                println!(
                    "Deploying single cell in transaction: {:?} \n",
                    tx.inner.outputs
                );
                // println!("Deploying single cell in transaction outputs data: {:?} \n", tx.inner.outputs_data);
                let tx_hash = self.send_tx(tx)?;
                Ok(OutPoint::new(tx_hash, 0))
            }
            _ => Err(ChainError::DeployCellTxHasLockedGroups),
        }
    }

    fn deploy_cells(
        &mut self,
        cells: &Vec<crate::types::cell::Cell>,
        unlockers: crate::chain::Unlockers,
        inputs: &crate::chain::CellInputs,
    ) -> crate::chain::ChainResult<Vec<OutPoint>> {
        let inputs_script = match inputs {
            CellInputs::ScriptQuery(script) => script,
            CellInputs::Empty => {
                return Err(crate::chain::ChainError::InvalidInputs(inputs.clone()))
            }
        };

        let mut builder = TransactionBuilder::default();
        builder = builder.add_outputs(cells.clone());
        builder
            .balance(inputs_script.clone().into(), None, self)
            .expect("Failed to balance transaction");

        let (unlocked_tx, locked_groups) = builder
            .unlock(&unlockers, self)
            .expect("Failed to unlock transaction");

        match locked_groups.len() {
            0 => {
                let tx = unlocked_tx.build();
                println!(
                    "Deploying multiple cells in transaction: {:?} \n",
                    tx.inner.outputs
                );
                // println!("Deploying multiple cells in transaction outputs data: {:?} \n", tx.inner.outputs_data);
                let tx_hash = self.send_tx(tx.clone())?;
                let out_points = {
                    let mut out_points = Vec::new();
                    for (i, _output) in tx.inner.outputs.iter().enumerate() {
                        out_points.push(OutPoint::new(tx_hash.clone(), i as u32));
                    }
                    out_points
                };
                Ok(out_points)
            }
            _ => Err(ChainError::DeployCellTxHasLockedGroups),
        }
    }

    fn set_default_lock(&mut self, lock: Cell) -> Result<(), ChainError> {
        // Check if script is already deployed
        let deployer_lock = lock.lock_script().unwrap();
        let mut indexer = IndexerRpcClient::new(self.indexer_url.as_str());
        let query = CellQueryOptions::new_lock(deployer_lock.into());
        let search_key = query.into();
        let deployed_cells = indexer.get_cells(search_key, Order::Desc, 100.into(), None);

        match deployed_cells {
            Ok(cell_page) => {
                let cells = cell_page.objects;
                match cells.into_iter().find(|cell| {
                    let cell_data_hash = CellOutput::calc_data_hash(cell.output_data.as_bytes());
                    lock.data_hash()
                        == H256::from_slice(cell_data_hash.as_slice())
                            .expect("Failed to hash cell data")
                }) {
                    // If some cell is found, set it as default lock
                    Some(cell) => {
                        self.default_lock = Some(cell.out_point.into());
                        Ok(())
                    }
                    // If none is found, deploy it
                    None => Err(ChainError::LockScriptCellNotFound(lock)),
                }
            }

            Err(err) => Err(ChainError::RpcError(err)),
        }
    }

    fn generate_cell_with_default_lock(&self, lock_args: crate::types::bytes::Bytes) -> Cell {
        let cell = self
            .inner()
            .get_cell_with_data(self.default_lock.as_ref().unwrap());
        match cell {
            Ok(cell) => {
                let (_, contract_data) = cell;
                let data_hash = CellOutput::calc_data_hash(&contract_data);
                let script = CkbScript::new_builder()
                    .code_hash(data_hash)
                    .hash_type(ckb_types::core::ScriptHashType::Data1.into())
                    .args(lock_args.into())
                    .build();
                Cell::with_lock(Script::from(script))
            }
            Err(e) => panic!("Failed to get cell with default lock: {}", e),
        }
    }
}
