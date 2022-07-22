use std::path::Path;

use ckb_hash::blake2b_256;

use trampoline_sdk::ckb_types::packed::{CellInput, CellOutput};

use ckb_jsonrpc_types::JsonBytes;
use trampoline_sdk::chain::{MockChain, MockChainTxProvider as ChainRpc};
use trampoline_sdk::ckb_types::{
    self,
    bytes::Bytes,
    core::{TransactionBuilder, TransactionView},
    error::Error,
    packed::*,
    prelude::*,
    H256,
};
use trampoline_sdk::contract::auxiliary_types::*;
use trampoline_sdk::contract::*;
use trampoline_sdk::contract::{builtins::t_nft::*, generator::*};
use trampoline_sdk::query::*;
// TO DO
// Should just add a Bytes type to trampoline which provides a single interface for all these
// Various byte types

// ALSO: Make generator pipeline able to handle empty data so it doesn't have to be set

fn _assert_script_error(err: Error, err_code: i8) {
    let error_string = err.to_string();
    assert!(
        error_string.contains(format!("error code {} ", err_code).as_str()),
        "error_string: {}, expected_error_code: {}",
        error_string,
        err_code
    );
}

fn _generate_always_success_lock(
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

fn gen_nft_contract() -> TrampolineNFTContract {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();

    let path_to_nft_bin = Path::new(&out_dir).join("trampoline-nft");
    let bin = ContractSource::load_from_path(path_to_nft_bin).unwrap();
    let mut contract = TrampolineNFTContract::default();
    contract.code = Some(JsonBytes::from_bytes(bin));
    contract
}

fn _gen_tnft_cell_output(contract: &TrampolineNFTContract) -> CellOutput {
    let lock = contract
        .lock
        .clone()
        .unwrap_or(_generate_always_success_lock(None).into());

    CellOutput::new_builder()
        .capacity(200_u64.pack())
        .type_(
            Some(ckb_types::packed::Script::from(
                contract.as_script().unwrap(),
            ))
            .pack(),
        )
        .lock(lock.into())
        .build()
}

fn _generate_mock_tx(
    inputs: Vec<CellInput>,
    outputs: Vec<CellOutput>,
    outputs_data: Vec<ckb_types::packed::Bytes>,
) -> TransactionView {
    TransactionBuilder::default()
        .inputs(inputs)
        .outputs(outputs)
        .outputs_data(outputs_data)
        .build()
}

fn genesis_id_from(input: OutPoint) -> GenesisId {
    let seed_tx_hash = input.tx_hash();
    let seed_idx = input.index();
    let mut seed = Vec::with_capacity(36);
    seed.extend_from_slice(seed_tx_hash.as_slice());
    seed.extend_from_slice(seed_idx.as_slice());
    let hash = blake2b_256(&seed);

    GenesisId::from_mol(hash.pack())
}

type NftArgs = SchemaPrimitiveType<Bytes, ckb_types::packed::Bytes>;
type NftField = ContractCellField<NftArgs, TrampolineNFT>;
#[test]
fn test_success_deploy() {
    let mut tnft_contract = gen_nft_contract();
    let mut chain = MockChain::default();
    let minter_lock_cell = chain.get_default_script_outpoint();
    let minter_lock_script = chain.build_script(&minter_lock_cell, vec![1_u8].into());

    let tx_input_cell = chain.deploy_random_cell_with_default_lock(2000, Some(vec![1_u8].into()));

    let tnft_code_cell = tnft_contract.as_code_cell();

    let tnft_code_cell_outpoint = chain.create_cell(tnft_code_cell.0, tnft_code_cell.1);
    tnft_contract.source = Some(ContractSource::Chain(tnft_code_cell_outpoint.into()));

    let genesis_seed = genesis_id_from(tx_input_cell);

    tnft_contract.add_input_rule(move |_tx| -> CellQuery {
        CellQuery {
            _query: QueryStatement::Single(CellQueryAttribute::LockHash(
                minter_lock_script
                    .clone()
                    .unwrap()
                    .calc_script_hash()
                    .into(),
            )),
            _limit: 1,
        }
    });

    tnft_contract.add_output_rule(ContractField::Data, move |ctx| -> NftField {
        let nft: NftField = ctx.load(ContractField::Data);
        if let ContractCellField::Data(nft_data) = nft {
            let mut t_nft_data = nft_data;
            t_nft_data.genesis_id = genesis_seed.clone();
            NftField::Data(t_nft_data)
        } else {
            nft
        }
    });

    let chain_rpc = ChainRpc::new(chain.clone());
    let generator = Generator::new()
        .chain_service(&chain)
        .query_service(&chain_rpc)
        .pipeline(vec![&tnft_contract]);
    let new_mint_tx = generator.generate(); //generator.pipe(tx_skeleton, Arc::new(Mutex::new(vec![])));
    let is_valid = chain_rpc.verify_tx(new_mint_tx.tx.into());
    assert!(is_valid);
}

#[test]
fn test_invalid_mismatched_genesis_id() {
    let mut tnft_contract = gen_nft_contract();
    let mut chain = MockChain::default();
    let minter_lock_cell = chain.get_default_script_outpoint();
    let minter_lock_script = chain.build_script(&minter_lock_cell, vec![1_u8].into());

    let _tx_input_cell = chain.deploy_random_cell_with_default_lock(2000, Some(vec![1_u8].into()));

    let genesis_id_seed_cell =
        chain.deploy_random_cell_with_default_lock(2000, Some(vec![2_u8].into()));

    let tnft_code_cell = tnft_contract.as_code_cell();

    let tnft_code_cell_outpoint = chain.create_cell(tnft_code_cell.0, tnft_code_cell.1);
    tnft_contract.source = Some(ContractSource::Chain(tnft_code_cell_outpoint.into()));
    let genesis_seed = genesis_id_from(genesis_id_seed_cell);

    tnft_contract.add_input_rule(move |_tx| -> CellQuery {
        CellQuery {
            _query: QueryStatement::Single(CellQueryAttribute::LockHash(
                minter_lock_script
                    .clone()
                    .unwrap()
                    .calc_script_hash()
                    .into(),
            )),
            _limit: 1,
        }
    });

    tnft_contract.add_output_rule(ContractField::Data, move |ctx| -> NftField {
        let nft: NftField = ctx.load(ContractField::Data);
        if let ContractCellField::Data(nft_data) = nft {
            let mut t_nft_data = nft_data;
            t_nft_data.genesis_id = genesis_seed.clone();
            NftField::Data(t_nft_data)
        } else {
            nft
        }
    });

    let chain_rpc = ChainRpc::new(chain.clone());
    let generator = Generator::new()
        .chain_service(&chain)
        .query_service(&chain_rpc)
        .pipeline(vec![&tnft_contract]);
    let new_mint_tx = generator.generate(); //generator.pipe(tx_skeleton, Arc::new(Mutex::new(vec![])));
    let is_valid = chain_rpc.verify_tx(new_mint_tx.tx.into());
    assert!(!is_valid);
}

// TO DO: Finish Test; currently builds a tx identical to mint
#[test]
fn test_invalid_mint_of_pre_existing_tnft() {
    let mut tnft_contract = gen_nft_contract();
    let mut chain = MockChain::default();
    let minter_lock_cell = chain.get_default_script_outpoint();
    let minter_lock_script = chain.build_script(&minter_lock_cell, vec![1_u8].into());

    let _tx_input_cell = chain.deploy_random_cell_with_default_lock(2000, Some(vec![1_u8].into()));
    let input_tnft_seed = chain.deploy_random_cell_with_default_lock(2000, Some(vec![2_u8].into()));

    let tnft_code_cell = tnft_contract.as_code_cell();

    let tnft_code_cell_outpoint = chain.create_cell(tnft_code_cell.0, tnft_code_cell.1);
    tnft_contract.source = Some(ContractSource::Chain(tnft_code_cell_outpoint.into()));

    let tnft_input_cell = CellOutput::new_builder()
        .lock(minter_lock_script.clone().unwrap())
        .capacity(150_u64.pack())
        .type_(Some(Script::from(tnft_contract.as_script().unwrap())).pack())
        .build();
    let tnft_input_cell_data = TrampolineNFT {
        genesis_id: genesis_id_from(input_tnft_seed),
        cid: Default::default(),
    };

    let _tnft_input_outpoint =
        chain.deploy_cell_output(tnft_input_cell_data.to_bytes(), tnft_input_cell);

    // Create two tnft output cells with same data as tnft input cell
    // Add input rule to grab the tnft_input_cell
    tnft_contract.add_input_rule(move |_tx| -> CellQuery {
        CellQuery {
            _query: QueryStatement::Single(CellQueryAttribute::LockHash(
                minter_lock_script
                    .clone()
                    .unwrap()
                    .calc_script_hash()
                    .into(),
            )),
            _limit: 1,
        }
    });

    tnft_contract.add_output_rule(ContractField::Data, move |ctx| -> NftField {
        let nft: NftField = ctx.load(ContractField::Data);
        if let NftField::ResolvedInputs(inputs) = ctx.load(TransactionField::ResolvedInputs) {
            if let ContractCellField::Data(nft_data) = nft {
                let mut t_nft_data = nft_data;
                let genesis_id = genesis_id_from(inputs.first().unwrap().out_point.clone());
                t_nft_data.genesis_id = genesis_id;
                NftField::Data(t_nft_data)
            } else {
                nft
            }
        } else {
            nft
        }
    });

    let chain_rpc = ChainRpc::new(chain.clone());
    let generator = Generator::new()
        .chain_service(&chain)
        .query_service(&chain_rpc)
        .pipeline(vec![&tnft_contract]);
    let new_mint_tx = generator.generate(); //generator.pipe(tx_skeleton, Arc::new(Mutex::new(vec![])));
    let is_valid = chain_rpc.verify_tx(new_mint_tx.tx.into());
    assert!(is_valid);
}
