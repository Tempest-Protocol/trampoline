use std::fmt;
use std::prelude::v1::*;
use std::str::FromStr;

use super::script::Script;
use crate::ckb_types::packed::Script as PackedScript;
use crate::ckb_types::H160;
use ckb_sdk::NetworkType;
use ckb_sdk::{Address as CKBAddress, AddressPayload};
use secp256k1::PublicKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    inner: CKBAddress,
}

impl Address {
    pub fn new(data: impl Into<CKBAddress>) -> Self {
        Self { inner: data.into() }
    }

    pub fn inner(&self) -> &CKBAddress {
        &self.inner
    }
}

impl From<Address> for CKBAddress {
    fn from(addr: Address) -> CKBAddress {
        addr.inner().clone()
    }
}

impl From<Address> for Script {
    fn from(a: Address) -> Script {
        Script::from(PackedScript::from(a.inner()))
    }
}

// TODO: Set cargo config var for determining network type at top level

impl From<Script> for Address {
    fn from(script: Script) -> Self {
        let packed: PackedScript = script.into();
        let ckb_addr_payload = AddressPayload::from(packed);
        let ckb_addr = CKBAddress::new(NetworkType::Dev, ckb_addr_payload, true);
        Self { inner: ckb_addr }
    }
}
impl From<AddressPayload> for Address {
    fn from(p: AddressPayload) -> Self {
        let ckb_addr = CKBAddress::new(NetworkType::Dev, p, true);
        Self { inner: ckb_addr }
    }
}

impl From<H160> for Address {
    fn from(h: H160) -> Self {
        let payload = AddressPayload::from_pubkey_hash(h);
        payload.into()
    }
}

impl From<&PublicKey> for Address {
    fn from(key: &PublicKey) -> Self {
        let payload = AddressPayload::from_pubkey(key);

        payload.into()
    }
}

impl FromStr for Address {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let inner = CKBAddress::from_str(s)?;
        Ok(Self { inner })
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.inner().fmt(f)
    }
}
