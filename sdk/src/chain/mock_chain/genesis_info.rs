use ckb_system_scripts::BUNDLED_CELL;
use ckb_types::core::{BlockBuilder, BlockView, TransactionBuilder};
use ckb_types::{bytes::Bytes, packed::Byte32, prelude::*};
use std::prelude::v1::*;

use super::MockChain;
pub struct GenesisScripts {
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

// Deploy every system script from a genesis script to a MockChain
pub fn genesis_event(chain: &mut MockChain) {
    // Deploy scripts
    deploy_genesis_scripts(chain, None);
    // Create genesis block
    let block = genesis_block_from_chain(chain);
    // Insert block header
    chain.insert_header(block.header());
}

pub fn deploy_genesis_scripts(chain: &mut MockChain, scripts: Option<GenesisScripts>) {
    // Generate default scripts if no scripts were passed
    let scripts = scripts.unwrap_or_default();

    // Deploy every script to the chain
    chain.deploy_cell_with_data(scripts.secp256k1_data.clone());
    chain.deploy_cell_with_data(scripts.secp256k1_blake160_sighash_all.clone());
    chain.deploy_cell_with_data(scripts.secp256k1_blake160_multisig_all.clone());
    chain.deploy_cell_with_data(scripts.dao);
}

pub fn genesis_block_from_chain(chain: &mut MockChain) -> BlockView {
    let block: BlockBuilder = BlockBuilder::default();

    let tx = TransactionBuilder::default();

    let secp256k1_data_code_hash_bytes =
        Byte32::from_slice(&ckb_system_scripts::CODE_HASH_SECP256K1_DATA).unwrap();
    let secp256k1_data_outpoint = chain
        .get_cell_by_data_hash(&secp256k1_data_code_hash_bytes)
        .unwrap();
    let secp256k1_data = chain.get_cell(&secp256k1_data_outpoint).unwrap();
    let tx = tx.output(secp256k1_data.0.clone());
    let tx = tx.output_data(secp256k1_data.1.pack());

    let blake_sighash_all_code_hash_bytes =
        Byte32::from_slice(&ckb_system_scripts::CODE_HASH_SECP256K1_BLAKE160_SIGHASH_ALL).unwrap();
    let blake_sighash_all_outpoint = chain
        .get_cell_by_data_hash(&blake_sighash_all_code_hash_bytes)
        .unwrap();
    let blake_sighash_all = chain.get_cell(&blake_sighash_all_outpoint).unwrap();
    let tx = tx.output(blake_sighash_all.0.clone());
    let tx = tx.output_data(blake_sighash_all.1.pack());

    let dao_code_hash_bytes = Byte32::from_slice(&ckb_system_scripts::CODE_HASH_DAO).unwrap();
    let dao_outpoint = chain.get_cell_by_data_hash(&dao_code_hash_bytes).unwrap();
    let dao = chain.get_cell(&dao_outpoint).unwrap();
    let tx = tx.output(dao.0.clone());
    let tx = tx.output_data(dao.1.pack());

    // Some cell without data or scripts to complete the genesis block and respect the script order
    let random_cell_outpoint = chain.deploy_random_cell_with_default_lock(100000, None);
    let random_cell = chain.get_cell(&random_cell_outpoint).unwrap();
    let tx = tx.output(random_cell.0.clone());
    let tx = tx.output_data(random_cell.1.pack());

    let blake_multisig_code_hash_bytes =
        Byte32::from_slice(&ckb_system_scripts::CODE_HASH_SECP256K1_BLAKE160_MULTISIG_ALL).unwrap();
    let blake_multisig_outpoint = chain
        .get_cell_by_data_hash(&blake_multisig_code_hash_bytes)
        .unwrap();
    let blake_multisig = chain.get_cell(&blake_multisig_outpoint).unwrap();
    let tx = tx.output(blake_multisig.0.clone());
    let tx = tx.output_data(blake_multisig.1.pack());

    let block = block.transaction(tx.build());

    block.build()
}
