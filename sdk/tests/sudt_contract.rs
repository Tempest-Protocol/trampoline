extern crate ckb_always_success_script;
extern crate trampoline_sdk;

use ckb_types::packed::CellOutputBuilder;
use trampoline_sdk::chain::{MockChain, MockChainTxProvider as ChainRpc};
use trampoline_sdk::contract::*;
use trampoline_sdk::contract::{builtins::sudt::*, generator::*, schema::*};

use ckb_types::{
    bytes::Bytes,
    core::{TransactionBuilder, TransactionView},
    packed::{Byte32, CellOutput},
    prelude::*,
    H256,
};

use ckb_hash::blake2b_256;
use ckb_jsonrpc_types::JsonBytes;

use std::path::Path;
use std::sync::{Arc, Mutex};

// Generated from ckb-cli util blake2b --binary-path /path/to/builtins/bins/simple_udt
const EXPECTED_SUDT_HASH: &str =
    "0xe1e354d6d643ad42724d40967e334984534e0367405c5ae42a9d7d63d77df419";

fn gen_sudt_contract(
    minter_lock: Option<ckb_types::packed::Script>,
    initial_supply: Option<u128>,
) -> SudtContract {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();

    let path_to_sudt_bin = Path::new(&out_dir).join("simple_udt");

    let lock = {
        if let Some(lock_script) = minter_lock {
            Some(JsonBytes::from_bytes(
                lock_script.calc_script_hash().as_bytes(),
            ))
        } else {
            Some(JsonBytes::from_bytes(Byte32::default().as_bytes()))
        }
    };

    let init_supply = {
        if let Some(supply) = initial_supply {
            let supply = supply.to_le_bytes();
            let mut bytes_buf = [0u8; 16];
            bytes_buf.copy_from_slice(&supply);
            Some(JsonBytes::from_vec(bytes_buf.to_vec()))
        } else {
            let supply = 0_u128.to_le_bytes();
            let mut bytes_buf = [0u8; 16];
            bytes_buf.copy_from_slice(&supply);
            Some(JsonBytes::from_vec(bytes_buf.to_vec()))
        }
    };

    let sudt_src = ContractSource::load_from_path(path_to_sudt_bin).unwrap();

    SudtContract {
        args: OwnerLockHash::from_json_bytes(lock.unwrap()),
        data: SudtAmount::from_json_bytes(init_supply.unwrap()),
        source: Some(ContractSource::Immediate(sudt_src.clone())),
        lock: None,
        type_: None,
        code: Some(JsonBytes::from_bytes(sudt_src)),
        output_rules: vec![],
        input_rules: vec![],
        outputs_count: 1,
    }
}

fn generate_always_success_lock(
    args: Option<ckb_types::packed::Bytes>,
) -> ckb_types::packed::Script {
    let data: Bytes = ckb_always_success_script::ALWAYS_SUCCESS.to_vec().into();
    let data_hash = H256::from(blake2b_256(data.to_vec().as_slice()));
    ckb_types::packed::Script::default()
        .as_builder()
        .args(args.unwrap_or([0u8].pack()))
        .code_hash(data_hash.pack())
        .hash_type(ckb_types::core::ScriptHashType::Data1.into())
        .build()
}
fn generate_simple_udt_cell(sudt_contract: &SudtContract) -> CellOutput {
    let lock = sudt_contract
        .lock
        .clone()
        .unwrap_or(generate_always_success_lock(None).into());
    CellOutput::new_builder()
        .capacity(100_u64.pack())
        .type_(
            Some(ckb_types::packed::Script::from(
                sudt_contract.as_script().unwrap(),
            ))
            .pack(),
        )
        .lock(lock.into())
        .build()
}

fn generate_mock_tx(
    outputs: Vec<CellOutput>,
    outputs_data: Vec<ckb_types::packed::Bytes>,
) -> TransactionView {
    TransactionBuilder::default()
        .outputs(outputs)
        .outputs_data(outputs_data)
        .build()
}

#[test]
fn test_failed_issuance_tx_no_permissions() {
    let mut chain = MockChain::default();

    // Create always success lock cell and add to chain
    let minter_lock_code_cell_data: Bytes =
        ckb_always_success_script::ALWAYS_SUCCESS.to_vec().into();
    let minter_lock_cell = chain.deploy_cell_with_data(minter_lock_code_cell_data);
    let minter_lock_script = chain.build_script(&minter_lock_cell, vec![1_u8].into());
    let non_minter_lock = chain.build_script(&minter_lock_cell, vec![200_u8].into());
    let non_minter_lock_hash = non_minter_lock.clone().unwrap().calc_script_hash();

    chain.create_cell(
        CellOutput::new_builder()
            .capacity(2000_u64.pack())
            .lock(non_minter_lock.unwrap())
            .build(),
        Default::default(),
    );

    // Deploy SUDT to chain
    let mut sudt_contract = gen_sudt_contract(minter_lock_script, Some(1500));
    let sudt_code_cell = sudt_contract.as_code_cell();
    let sudt_code_cell_outpoint = chain.create_cell(sudt_code_cell.0, sudt_code_cell.1);

    // Create Mint SUDT transaction, using as input a cell locked with a different user's lock script
    // Should fail because the user does not have mint permissions
    sudt_contract.source = Some(ContractSource::Chain(sudt_code_cell_outpoint.into()));
    //let fail_tx = TransactionBuilder::default().build();

    // Add rule to sudt output generation to increase the amount field.
    sudt_contract.add_output_rule(
        ContractField::Data,
        |ctx| -> ContractCellField<OwnerLockHash, SudtAmount> {
            let amount: ContractCellField<OwnerLockHash, SudtAmount> =
                ctx.load(ContractField::Data);
            if let ContractCellField::Data(amount) = amount {
                let amt: u128 = amount.into();
                ContractCellField::Data(SudtAmount::from(amt + 2000))
            } else {
                amount
            }
        },
    );

    sudt_contract.add_input_rule(move |_tx| -> CellQuery {
        CellQuery {
            _query: QueryStatement::Single(CellQueryAttribute::LockHash(
                non_minter_lock_hash.clone().into(),
            )),
            _limit: 1,
        }
    });

    // Instantiate chain rpc and tx generator
    let chain_rpc = ChainRpc::new(chain);
    let generator = Generator::new()
        .chain_service(&chain_rpc)
        .query_service(&chain_rpc)
        .pipeline(vec![&sudt_contract]);

    let new_fail_tx = generator.generate(); //generator.pipe(fail_tx, Arc::new(Mutex::new(vec![])));
                                            // Test that failure transaction failed
    let is_valid = chain_rpc.verify_tx(new_fail_tx.tx.into());
    assert!(!is_valid);
}

#[test]
fn test_sudt_issuance_tx_with_contract_pipeline() {
    let mut chain = MockChain::default();

    // Create always success lock cell and add to chain
    let minter_lock_code_cell_data: Bytes =
        ckb_always_success_script::ALWAYS_SUCCESS.to_vec().into();
    let minter_lock_cell = chain.deploy_cell_with_data(minter_lock_code_cell_data);
    let minter_lock_script = chain.build_script(&minter_lock_cell, vec![1_u8].into());
    let minter_lock_hash = minter_lock_script.clone().unwrap().calc_script_hash();
    chain.create_cell(
        CellOutput::new_builder()
            .capacity(2000_u64.pack())
            .lock(minter_lock_script.clone().unwrap())
            .build(),
        Default::default(),
    );

    // Deploy SUDT to chain
    let mut sudt_contract = gen_sudt_contract(minter_lock_script, Some(1500));
    let sudt_code_cell = sudt_contract.as_code_cell();
    let sudt_code_cell_outpoint = chain.create_cell(sudt_code_cell.0, sudt_code_cell.1);
    sudt_contract.source = Some(ContractSource::Chain(sudt_code_cell_outpoint.into()));
    // Create Mint SUDT transaction, using as input a cell locked with the minter's lock script
    // let tx = TransactionBuilder::default()
    //     .cell_dep(
    //         sudt_contract
    //             .as_cell_dep(sudt_code_cell_outpoint.into())
    //             .into(),
    //     )
    //     .cell_dep(chain.find_cell_dep_for_script(&minter_lock_script.unwrap()))
    //     .output(generate_simple_udt_cell(&sudt_contract))
    //     .outputs_data(vec![0_u128.to_le_bytes().pack()])
    //     .build();

    // Add rule to sudt output generation to increase the amount field.
    sudt_contract.add_output_rule(
        ContractField::Data,
        |ctx| -> ContractCellField<OwnerLockHash, SudtAmount> {
            let amount: ContractCellField<OwnerLockHash, SudtAmount> =
                ctx.load(ContractField::Data);
            if let ContractCellField::Data(amount) = amount {
                let amt: u128 = amount.into();
                ContractCellField::Data(SudtAmount::from(amt + 2000))
            } else {
                amount
            }
        },
    );
    sudt_contract.add_input_rule(move |_tx| -> CellQuery {
        CellQuery {
            _query: QueryStatement::Single(CellQueryAttribute::LockHash(
                minter_lock_hash.clone().into(),
            )),
            _limit: 1,
        }
    });

    // Instantiate chain rpc and tx generator
    let chain_rpc = ChainRpc::new(chain);
    let generator = Generator::new()
        .chain_service(&chain_rpc)
        .query_service(&chain_rpc)
        .pipeline(vec![&sudt_contract]);

    // Generate transaction
    let new_tx = generator.generate(); //generator.pipe(tx, Arc::new(Mutex::new(vec![])));

    // Test that success transaction succeeded & has correct sudt amount minted
    let new_tx_amt = new_tx.tx.output_with_data(0).unwrap().1;
    let new_tx_amt: u128 = sudt_contract.read_raw_data(new_tx_amt).to_mol().unpack();
    assert_eq!(new_tx_amt, 2000_u128);

    let is_valid = chain_rpc.verify_tx(new_tx.tx.into());
    assert!(is_valid);
}

#[test]
fn test_update_sudt_with_rule_pipeline() {
    // Load SUDT contract
    let mut sudt_contract = gen_sudt_contract(None, None);
    // Create SUDT Cell Output
    let sudt_cell = generate_simple_udt_cell(&sudt_contract);
    // Mock Transaction with a single output
    let transaction = generate_mock_tx(vec![sudt_cell], vec![2000_u128.to_le_bytes().pack()]);

    // Add output rule to sudt contract to increase balance by 17
    sudt_contract.add_output_rule(
        ContractField::Data,
        |ctx| -> ContractCellField<OwnerLockHash, SudtAmount> {
            let amount: ContractCellField<OwnerLockHash, SudtAmount> =
                ctx.load(ContractField::Data);
            if let ContractCellField::Data(amount) = amount {
                let amt: u128 = amount.into();
                ContractCellField::Data(SudtAmount::from(amt + 17))
            } else {
                amount
            }
        },
    );

    // Add output rule to sudt contract to increase balance by 20
    sudt_contract.add_output_rule(
        ContractField::Data,
        |ctx| -> ContractCellField<OwnerLockHash, SudtAmount> {
            let amount: ContractCellField<OwnerLockHash, SudtAmount> =
                ctx.load(ContractField::Data);
            if let ContractCellField::Data(amount) = amount {
                let amt: u128 = amount.into();
                ContractCellField::Data(SudtAmount::from(amt + 20))
            } else {
                amount
            }
        },
    );

    // Pipe transaction into sudt contract
    let new_tx = sudt_contract.pipe(transaction.into(), Arc::new(Mutex::new(vec![])));

    // Check that sudt contract updated correctly with a total balance increase of 37 (17 + 20)
    let new_tx_amt = new_tx.tx.output_with_data(0).unwrap().1;
    println!("New tx amt as bytes: {:?}", new_tx_amt.pack());
    let new_tx_amt: u128 = sudt_contract.read_raw_data(new_tx_amt).into();
    assert_eq!(new_tx_amt, 2037_u128);
}
#[test]
fn test_add_output_rule() {
    let mut sudt_contract = gen_sudt_contract(None, None);

    sudt_contract.add_output_rule(
        ContractField::Data,
        |ctx| -> ContractCellField<OwnerLockHash, SudtAmount> {
            let amount: ContractCellField<OwnerLockHash, SudtAmount> =
                ctx.load(ContractField::Data);

            if let ContractCellField::Data(amount) = amount {
                let amt: u128 = amount.into();
                ContractCellField::Data(SudtAmount::from(amt + 17))
            } else {
                amount
            }
        },
    );
}

#[test]
fn test_contract_pack_and_unpack_data() {
    let mut sudt_contract = gen_sudt_contract(None, None);

    sudt_contract.set_args(OwnerLockHash::from_mol(Byte32::default()));
    sudt_contract.set_data(SudtAmount::from_mol(1200_u128.pack()));

    let uint128_data: u128 = sudt_contract.read_data().to_mol().unpack();
    assert_eq!(uint128_data, 1200_u128);
}

#[test]
fn test_sudt_data_hash_gen_json() {
    let sudt_contract = gen_sudt_contract(None, None);

    let json_code_hash = ckb_jsonrpc_types::Byte32::from(sudt_contract.code_hash().unwrap().pack());

    let as_json_hex_str = serde_json::to_string(&json_code_hash).unwrap();

    assert_eq!(
        &format!("\"{}\"", EXPECTED_SUDT_HASH),
        as_json_hex_str.as_str()
    );
}

#[test]
fn test_sudt_code_hash_gen() {
    let mut sudt_contract = gen_sudt_contract(None, None);

    let code_hash = sudt_contract.code_hash().unwrap().pack();
    let hash_hex_str = format!("0x{}", hex::encode(&code_hash.raw_data()));
    assert_eq!(EXPECTED_SUDT_HASH, hash_hex_str.as_str());
}

#[test]
fn test_data_hash_is_accurate() {
    let mut sudt_contract = gen_sudt_contract(None, None);
    sudt_contract.set_data(SchemaPrimitiveType::from(123_u128));
    let data_hash = sudt_contract.data_hash().unwrap().pack();
    let cell_output = CellOutput::calc_data_hash(123_u128.pack().as_slice());
    assert_eq!(data_hash.as_slice(), cell_output.as_slice());
    sudt_contract.set_data(SchemaPrimitiveType::from(124_u128));
    let data_hash = sudt_contract.data_hash().unwrap().pack();
    assert_ne!(data_hash.as_slice(), cell_output.as_slice());
}
