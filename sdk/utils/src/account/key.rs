use ckb_fixed_hash::H512;
use ckb_types::{H160, H256};
use lazy_static::lazy_static;
use rand::Rng;
use std::{
    fmt,
    fmt::{Debug, Formatter},
    ops::Deref,
    str::FromStr,
};

use crate::account::error::AccountError;

lazy_static! {
    static ref SECP256K1: secp256k1::Secp256k1<secp256k1::All> = secp256k1::Secp256k1::new();
}

pub type LockArg = H160;

#[derive(Default)]
pub struct Secret {
    inner: H256,
}

impl Secret {
    pub fn from_slice(data: &[u8]) -> Result<Secret, AccountError> {
        if data.len() != 32 {
            return Err(AccountError::InvalidKeyLength);
        }
        let mut h = [0u8; 32];
        h.copy_from_slice(&data[..32]);
        Ok(Secret { inner: h.into() })
    }

    pub fn public_key(&self) -> Result<Public, AccountError> {
        let pk = secp256k1::PublicKey::from_secret_key(&SECP256K1, &self.to_secp256k1_secret()?);
        Ok(pk.into())
    }

    fn to_secp256k1_secret(&self) -> Result<secp256k1::SecretKey, AccountError> {
        secp256k1::SecretKey::from_slice(self.inner.as_bytes()).map_err(Into::into)
    }
}

impl FromStr for Secret {
    type Err = AccountError;

    fn from_str(s: &str) -> Result<Secret, Self::Err> {
        let sk = secp256k1::SecretKey::from_str(s)?;
        Ok(sk.into())
    }
}

impl fmt::LowerHex for Secret {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.inner, f)
    }
}

impl Deref for Secret {
    type Target = H256;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl From<secp256k1::SecretKey> for Secret {
    fn from(key: secp256k1::SecretKey) -> Self {
        let mut h = [0u8; 32];
        h.copy_from_slice(&key[0..32]);
        Secret { inner: h.into() }
    }
}

pub struct Public {
    inner: H512,
}

impl Public {
    #[allow(dead_code)]
    pub fn from_slice(data: &[u8]) -> Result<Public, AccountError> {
        Ok(secp256k1::PublicKey::from_slice(data)?.into())
    }

    pub fn to_lock_arg(&self) -> Result<LockArg, AccountError> {
        let pk = self.to_secp256k1_public()?;
        let lock_arg = H160::from_slice(&ckb_hash::blake2b_256(&pk.serialize()[..])[0..20])
            .map_err(|_| AccountError::InvalidKeyLength)?;
        Ok(lock_arg)
    }

    fn to_secp256k1_public(&self) -> Result<secp256k1::PublicKey, AccountError> {
        // uncompressed key prefix is 4
        let mut data = [4u8; 65];
        data[1..65].copy_from_slice(self.as_bytes());
        secp256k1::PublicKey::from_slice(&data).map_err(Into::into)
    }
}

impl fmt::LowerHex for Public {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.inner, f)
    }
}

impl From<secp256k1::PublicKey> for Public {
    fn from(key: secp256k1::PublicKey) -> Self {
        let serialized = key.serialize_uncompressed();
        let mut pubkey = [0u8; 64];
        pubkey.copy_from_slice(&serialized[1..65]);
        Public {
            inner: pubkey.into(),
        }
    }
}

impl Deref for Public {
    type Target = H512;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub struct KeyPair {
    secret: Secret,
    public: Public,
}

impl KeyPair {
    pub fn from_secret_slice(sk: &[u8]) -> Result<Self, AccountError> {
        let secret = Secret::from_slice(sk)?;
        let public = secret.public_key()?;
        Ok(KeyPair { secret, public })
    }

    pub fn from_secret_str(sk: String) -> Result<Self, AccountError> {
        let secret = Secret::from_str(&sk)?;
        let public = secret.public_key()?;
        Ok(KeyPair { secret, public })
    }

    #[allow(dead_code)]
    pub fn secret(&self) -> &Secret {
        &self.secret
    }

    #[allow(dead_code)]
    pub fn public(&self) -> &Public {
        &self.public
    }

    pub fn lock_arg(&self) -> Result<LockArg, AccountError> {
        self.public.to_lock_arg()
    }
}

#[derive(Debug, Default)]
pub struct Generator;

impl Generator {
    pub fn generate(&self) -> KeyPair {
        let sk = self.generate_secret();
        let pk = secp256k1::PublicKey::from_secret_key(&SECP256K1, &sk);

        KeyPair {
            secret: sk.into(),
            public: pk.into(),
        }
    }

    fn generate_secret(&self) -> secp256k1::SecretKey {
        let mut seed = vec![0; 32];
        let mut rng = rand::thread_rng();
        loop {
            rng.fill(seed.as_mut_slice());
            if let Ok(key) = secp256k1::SecretKey::from_slice(&seed) {
                return key;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::str::FromStr;

    #[test]
    fn test_key() {
        let sk: Secret =
            Secret::from_str("009c0df368efef6084ba35ded33f05ef2a5f4b25d7841bce77e2449be9311dba")
                .unwrap();

        assert!(sk.public_key().is_ok())
    }

    #[test]
    fn test_invalid_key() {
        let sk = Secret::from_slice(&[0u8; 31]);
        assert!(sk.is_err());

        let invalid_hex = "009c0df368efef6084ba35ded00000ef2a5f4b25d7841bce77e2449be9311dbx";
        let sk = Secret::from_str(invalid_hex);
        assert!(sk.is_err())
    }
}
