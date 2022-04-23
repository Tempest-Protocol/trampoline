pub use crate::account::error::AccountError;
use crate::account::key::KeyPair;
use crate::{
    account::crypto::Crypto,
    account::key::{Generator, LockArg},
};
use anyhow::Result;
pub use ckb_crypto::secp::*;
pub use ckb_sdk::traits::{SecpCkbRawKeySigner, Signer};
pub use ckb_sdk::unlock::{ScriptSignError, ScriptSigner, SecpSighashScriptSigner};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{borrow::ToOwned, format, fs, path::Path, string::String};

mod crypto;
mod error;
mod key;

// TODO: more fields: id, address(mainnet, testnet)
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Account {
    pub crypto: Crypto,
    #[serde(skip)]
    pub lock_arg: LockArg,
}

impl Account {
    pub fn new(password: &[u8]) -> Result<Account, AccountError> {
        // TODO: maybe we do not need a KeyPair
        let keypair = Generator::default().generate();
        Account::from_keypair(keypair, password)
    }

    /// sk: hex format for the secret key
    pub fn from_secret(sk: String, password: &[u8]) -> Result<Account, AccountError> {
        let keypair = KeyPair::from_secret_str(sk)?;
        Account::from_keypair(keypair, password)
    }

    pub fn from_file<P: AsRef<Path>>(path: P, password: &[u8]) -> Result<Account, AccountError> {
        let json_str = fs::read_to_string(path)?;
        let json_value: Value = serde_json::from_str(&json_str)?;
        let crypto: Crypto = serde_json::from_value(json_value.get("crypto").unwrap().to_owned())?;
        let key = crypto.decrypt_key(password)?;
        let keypair = KeyPair::from_secret_slice(&key)?;
        Ok(Account {
            crypto,
            lock_arg: keypair.lock_arg()?,
        })
    }

    pub fn lock_arg_hex(&self) -> String {
        format!("{:x}", self.lock_arg)
    }

    fn from_keypair(keypair: KeyPair, password: &[u8]) -> Result<Account, AccountError> {
        let crypto = Crypto::encrypt_key(keypair.secret().as_bytes(), password);
        Ok(Account {
            crypto,
            lock_arg: keypair.lock_arg()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_from_file() {
        let password = b"testpass";
        let root = tempdir().unwrap();
        let path = root.path().join("keyfile");
        let account = Account::new(password).unwrap();
        fs::write(&path, serde_json::to_string_pretty(&account).unwrap()).unwrap();

        let account2 = Account::from_file(&path, password).unwrap();
        assert_eq!(account, account2);

        let account3 = Account::from_file(&path, b"invalid");
        assert!(account3.is_err());
    }
}
