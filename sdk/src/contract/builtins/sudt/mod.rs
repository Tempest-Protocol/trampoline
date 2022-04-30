use std::prelude::v1::*;

use ckb_types::H256;

use crate::ckb_types::packed::{Byte32, Uint128};
use crate::contract::{Contract, TContract};

use crate::contract::schema::{SchemaPrimitiveType, TrampolineSchema};

#[derive(Debug, Clone, Default)]
struct InnerOwnerLockHash([u8; 32]);

#[derive(Debug, Clone, Default)]
struct InnerSudtAmount(u128);

pub type OwnerLockHash = SchemaPrimitiveType<H256, Byte32>;
pub type SudtAmount = SchemaPrimitiveType<u128, Uint128>;

impl TrampolineSchema for OwnerLockHash {}
impl TrampolineSchema for SudtAmount {}
pub type SudtContract = Contract<OwnerLockHash, SudtAmount>;

pub type SudtTrampolineContract = TContract<OwnerLockHash, SudtAmount>;