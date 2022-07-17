#[cfg(all(feature = "rpc", test))]
mod test {
    use std::{
        collections::HashMap,
        process::{Command, Output},
        str::FromStr,
        thread,
        time::Duration,
    };

    use ckb_sdk::traits::DefaultCellDepResolver;
    use serial_test::serial;

    use ckb_always_success_script::ALWAYS_SUCCESS;
    use ckb_types::{
        packed::CellOutput,
        prelude::{Pack, Unpack},
        H256,
    };

    use trampoline_sdk::{
        bytes::Bytes,
        cell::Cell,
        chain::{CellInputs, Chain, RpcChain, TransactionBuilder},
        script::Script,
    };

    use trampoline_utils::{
        account::Account,
        lock::{create_secp_sighash_unlocker, Lock, SigHashAllLock},
        transaction::TransactionHelper,
    };

    use ckb_verification::ContextualWithoutScriptTransactionVerifier;

    // This tests assumes local node and indexer are running on URLs below
    const CKB_URL: &str = "http://localhost:8114";
    const INDEXER_URL: &str = "http://localhost:8116";

    fn default_chain() -> RpcChain {
        RpcChain::new(CKB_URL, INDEXER_URL)
    }

    fn fresh_chain() -> RpcChain {
        let chain = RpcChain::new(CKB_URL, INDEXER_URL);
        chain
            .reset()
            .expect("Failed to reset chain to genesis block");
        thread::sleep(Duration::from_secs(1));
        restart_indexer();
        thread::sleep(Duration::from_secs(2));
        chain
    }

    // Creates default Account
    // Password: trampoline
    // Privkey controls default devchain account
    fn default_account() -> Account {
        let password = b"trampoline";
        let pk = "d00c06bfd800d27397002dca6fb0993d5ba6399b4238b2f29ee9deb97593d2bc";
        Account::from_secret(pk.into(), password).expect("Failed creating default test Account")
    }

    struct DevAccount {
        pub account: Account,
    }

    impl Default for DevAccount {
        fn default() -> Self {
            let account = default_account();
            Self { account: account }
        }
    }

    fn restart_indexer() {
        Command::new("docker")
            .arg("stop")
            .arg("trampoline-test-indexer")
            .output()
            .expect("Failed to stop indexer container");

        Command::new("docker")
            .arg("run")
            .arg("-d")
            .arg("--rm")
            .arg("--network")
            .arg("host")
            .arg("--name")
            .arg("trampoline-test-indexer")
            .arg("nervos/ckb-indexer")
            .arg("-s")
            .arg("data")
            .output()
            .expect("Failed to start indexer container");
    }

    #[test]
    #[serial]
    fn test_rpc_client_get_tip() {
        let chain = fresh_chain();
        let tip = chain.get_tip();
        assert!(tip.is_some());
    }

    #[test]
    #[serial]
    fn test_mine_one_block() {
        let chain = fresh_chain();
        let first_header = chain.get_tip().expect("Failed to get tip from chain");
        let mined_hash = chain.mine_once().expect("Failed to mine a block");
        let second_header = chain.get_tip().expect("Failed to get tip from chain");
        assert_eq!(
            first_header.inner.number.value() + 1,
            second_header.inner.number.value() as u64
        );
        assert_eq!(second_header.hash, mined_hash);
    }

    #[test]
    #[ignore]
    fn test_verify_valid_tx() {
        // Given a default chain, default sender and random receiver account
        let chain = fresh_chain();

        let dev = DevAccount::default();
        let sender = dev.account;
        let password = b"trampoline";
        let destination = Account::new(b"trampoline").expect("Failed to generate random account");

        // When deploying a capacity transfer transaction
        let amount = 6100000000u64;
        let tx_helper = TransactionHelper::new(CKB_URL, INDEXER_URL);
        let tx = tx_helper
            .capacity_transfer(&sender, password, amount, &destination)
            .expect("Failed to create capacity transfer transaction");

        let verification = chain.verify_tx(tx);
        assert!(verification.is_ok());
    }

    #[test]
    #[serial]
    fn test_send_tx_get_tx() {
        // Given a default chain, default sender and random receiver account
        let chain = fresh_chain();

        let dev = DevAccount::default();
        let sender = dev.account;
        let password = b"trampoline";
        let destination = Account::new(b"trampoline").expect("Failed to generate random account");

        // When deploying a capacity transfer transaction
        // WHY TF THIS AMOUNT
        // Check if change is required
        let amount = 6100000000u64;
        let tx_helper = TransactionHelper::new(CKB_URL, INDEXER_URL);
        let tx = tx_helper
            .capacity_transfer(&sender, password, amount, &destination)
            .expect("Failed to create capacity transfer transaction");

        let sent_tx = tx.clone();

        let hash = chain.send_tx(tx);
        println!("Result from send TX is {:?}", &hash);
        let hash = hash.expect("Failed sending TX");
        let hash = H256::from(hash.unpack());

        // And retrieving the deployed transaction from chain
        let tx_in_chain = chain.get_tx(hash).expect("Failed to get TX from chain");
        let tx_in_chain = tx_in_chain.expect("TX not found");
        let tx_in_chain = tx_in_chain
            .transaction
            .expect("TransactionWithStatus has no inner TX");
        // Then both transactions should be the same
        // let tx_in_chain = tx_in_chain.inner;

        // Transform both TXs into Trampoline::Transaction

        let sent_tx = trampoline_sdk::transaction::Transaction::from(sent_tx);
        let fetched_tx = trampoline_sdk::transaction::Transaction::from(tx_in_chain);

        assert_eq!(sent_tx, fetched_tx);
    }

    #[test]
    #[serial]
    fn new_rpc_dev_chain_has_sighash_all_as_default_lock() {
        let chain = fresh_chain();
        assert!(chain.default_lock.is_some());
        let cell = chain
            .inner()
            .get_cell_with_data(&chain.default_lock.unwrap())
            .expect("Failed to get default lock cell from chain");
        let data_hash = CellOutput::calc_data_hash(&cell.1);
        let sighash_all_code_hash = ckb_system_scripts::CODE_HASH_SECP256K1_BLAKE160_SIGHASH_ALL;

        assert_eq!(data_hash, sighash_all_code_hash.pack());
    }

    #[test]
    #[serial]
    fn create_cell_with_default_lock_from_default_rpcchain_has_sighashall_lock() {
        let chain = fresh_chain();
        let cell = chain.generate_cell_with_default_lock(Bytes::default());
        let code_hash = cell.lock_script().unwrap().code_hash();
        let sighash_all_code_hash =
            H256::from(ckb_system_scripts::CODE_HASH_SECP256K1_BLAKE160_SIGHASH_ALL);
        assert_eq!(code_hash, sighash_all_code_hash);
    }

    #[test]
    #[serial]
    fn deploy_ass_and_set_as_default_lock() {
        let mut chain = fresh_chain();

        // let _reset = chain.reset().expect("Failed to reset chain");
        let dev = DevAccount::default();
        let dev_account_lock = SigHashAllLock::from_account(&dev.account).as_script();
        let password = b"trampoline";

        // Create AlwaysSuccessScript cell
        let ass_script_bin = ALWAYS_SUCCESS;
        let mut ass_cell = Cell::with_data(ass_script_bin.to_vec());
        ass_cell
            .set_lock_script(dev_account_lock.clone())
            .expect("Failed to set lock script for ASL cell");

        let unlockers = {
            let (script_id, unlocker) = create_secp_sighash_unlocker(&dev.account, password);
            let mut unlockers = HashMap::new();
            unlockers.insert(script_id, unlocker);
            unlockers
        };

        // Create inputs
        let inputs = CellInputs::from(Script::from(dev_account_lock));

        let deploy_outpoint = chain
            .clone()
            .deploy_cell(&ass_cell, unlockers, &inputs)
            .expect("Failed to deploy AlwaysSuccessLock cell");

        // Mine block and wait for indexer to catch up
        let _mined_block_hash = chain.mine_once().expect("Failed to mine block");
        let _mined_block_hash = chain.mine_once().expect("Failed to mine block");
        let _mined_block_hash = chain.mine_once().expect("Failed to mine block");
        let _mined_block_hash = chain.mine_once().expect("Failed to mine block");
        thread::sleep(Duration::from_secs(1)); // 20 seconds
                                               // let _mined_block_hash = chain.mine_once();
                                               // thread::sleep(Duration::from_secs(1)); // 20 seconds
                                               // let _mined_block_hash = chain.mine_once();
                                               // thread::sleep(Duration::from_secs(1)); // 20 seconds

        chain
            .set_default_lock(deploy_outpoint)
            .expect("Failed to set default lock");

        // Check that default lock outpoint and deploy output are the same
        let default_lock_outpoint = chain.default_lock().unwrap();
        println!(
            "Default lock cell is \nTxHash: {:?}\nIndex: {:?}",
            default_lock_outpoint.tx_hash(),
            default_lock_outpoint.index()
        );

        let ass_script_cell = chain
            .inner()
            .get_cell_with_data(&default_lock_outpoint)
            .expect("Failed to get script cell");

        let from_chain_data_hash = Bytes::from(ass_script_cell.1).hash_256();
        let from_deploy_data_hash = Bytes::from(ass_script_bin.to_vec()).hash_256();
        assert_eq!(from_chain_data_hash, from_deploy_data_hash);
    }
}
