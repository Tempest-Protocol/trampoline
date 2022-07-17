mod invoice;

use ckb_types::{
    bytes::Bytes,
    packed::WitnessArgs,
    prelude::{Builder, Entity, Pack},
};
use invoice::*;

use crate::{
    account::Account,
    error::HelperError,
    lock::{create_secp_sighash_unlocker, Lock, SigHashAllLock},
    rpc::{RpcInfo, RpcProvider},
};
use ckb_sdk::tx_builder::{CapacityBalancer, TxBuilder};
use std::collections::HashMap;

pub struct TransactionHelper {
    ckb_url: String,
    indexer_url: String,
}

impl TransactionHelper {
    pub fn new(ckb_url: &str, indexer_url: &str) -> Self {
        Self {
            ckb_url: ckb_url.into(),
            indexer_url: indexer_url.into(),
        }
    }

    pub fn capacity_transfer(
        &self,
        sender: &Account,
        password: &[u8],
        amount: u64,
        destination: &Account,
    ) -> Result<ckb_types::core::TransactionView, HelperError> {
        let rpc_info = RpcInfo::from((self.ckb_url.clone(), self.indexer_url.clone()));
        // Setup providers
        let provider = RpcProvider::new(rpc_info);

        let tx_builder = DefaultInvoice::new_tx_builder(destination, &amount);

        let unlockers = {
            let (script_id, unlocker) = create_secp_sighash_unlocker(sender, password);
            let mut unlockers = HashMap::new();
            unlockers.insert(script_id, unlocker);
            unlockers
        };

        let balancer = create_balancer(sender, 1000);

        // Build providers
        let mut cell_collector = provider.cell_collector();
        let cell_dep_resolver = provider.cell_dep_resolver();
        let header_dep_resolver = provider.header_dep_resolver();
        let tx_dep_provider = provider.tx_dep_provider();

        let builder_result = tx_builder.build_unlocked(
            &mut cell_collector,
            // Maybe get some .clone() getters for each of these and use them instead of borrowing
            &cell_dep_resolver,
            &header_dep_resolver,
            &tx_dep_provider,
            &balancer,
            &unlockers,
        );

        match builder_result {
            Ok((tx, locked_group)) => {
                if locked_group.is_empty() {
                    Ok(tx)
                } else {
                    Err(HelperError::LockedGroupNotEmpty(locked_group))
                }
            }
            Err(e) => Err(HelperError::BuildError(e)),
        }
    }
}

fn create_balancer(account: &Account, fee_rate: u64) -> CapacityBalancer {
    let sender_lock = SigHashAllLock::from_account(account).as_script();
    let placeholder_witness = WitnessArgs::new_builder()
        .lock(Some(Bytes::from(vec![0u8; 65])).pack())
        .build();
    CapacityBalancer::new_simple(sender_lock, placeholder_witness, fee_rate)
}
