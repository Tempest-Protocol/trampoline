use ckb_sdk::{
    constants::SIGHASH_TYPE_HASH,
    traits::SecpCkbRawKeySigner,
    unlock::{ScriptUnlocker, SecpSighashScriptSigner, SecpSighashUnlocker},
    ScriptId,
};
use ckb_types::{
    core::ScriptHashType,
    packed::{Byte32, Script, ScriptBuilder},
    prelude::{Builder, Pack},
};

use crate::{account::Account, hex::parse_hex};

pub type UnlockerPair = (ScriptId, Box<dyn ScriptUnlocker>);

/// Create unlocker pair of ScriptId and ScriptUnlocker for a given account
pub fn create_secp_sighash_unlocker(account: &Account, password: &[u8]) -> UnlockerPair {
    // ATM this is the only way to access a decrypt key, it does not flush memory, nor is temporal
    // ckb_wallet crate implements TimedKeys which seem to be the way to go
    // Option B is implementing CKB in Wagyu, the cryptocurrency wallet framework
    let key = account.crypto.decrypt_key(password).unwrap();
    let secret = secp256k1::SecretKey::from_slice(&key).unwrap();
    // Create unlocker and script id pair
    let signer = SecpCkbRawKeySigner::new_with_secret_keys(vec![secret]);
    let sighash_signer = SecpSighashScriptSigner::new(Box::new(signer));
    let sighash_unlocker = SecpSighashUnlocker::new(sighash_signer);
    let sighash_script_id = ScriptId::new_type(SIGHASH_TYPE_HASH.clone());
    (
        sighash_script_id,
        Box::new(sighash_unlocker) as Box<dyn ScriptUnlocker>,
    )
}

// Lock Trait for creating diverse locks
pub trait Lock {
    fn as_script(&self) -> Script;
}

// SigHashAllLock
pub struct SigHashAllLock {
    hash_type: ScriptHashType,
    code_hash: Byte32,
    lock_arg: String,
}

impl SigHashAllLock {
    pub fn from_arg(arg_string: String) -> Self {
        Self {
            hash_type: ScriptHashType::Type,
            code_hash: SIGHASH_TYPE_HASH.pack(),
            lock_arg: arg_string,
        }
    }

    pub fn from_account(a: &Account) -> Self {
        let lock_arg = a.lock_arg_hex();
        Self {
            hash_type: ScriptHashType::Type,
            code_hash: SIGHASH_TYPE_HASH.pack(),
            lock_arg,
        }
    }
}

impl Lock for SigHashAllLock {
    fn as_script(&self) -> Script {
        let lock_arg = parse_hex(&self.lock_arg).unwrap().pack();
        ScriptBuilder::default()
            .hash_type(self.hash_type.into())
            .code_hash(self.code_hash.clone())
            .args(lock_arg)
            .build()
    }
}
