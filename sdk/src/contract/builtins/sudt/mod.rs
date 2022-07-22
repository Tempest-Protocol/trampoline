use std::prelude::v1::*;

use crate::ckb_types::H256;

use crate::ckb_types::packed::{Byte32, Uint128};

#[cfg(all(feature = "std", not(feature = "script")))]
use crate::contract::{Contract, TContract};

use crate::contract::schema::SchemaPrimitiveType;

#[derive(Debug, Clone, Default)]
struct InnerOwnerLockHash([u8; 32]);

#[derive(Debug, Clone, Default)]
struct InnerSudtAmount(u128);

pub type OwnerLockHash = SchemaPrimitiveType<H256, Byte32>;
pub type SudtAmount = SchemaPrimitiveType<u128, Uint128>;

#[cfg(all(feature = "std", not(feature = "script")))]
pub type SudtContract = Contract<OwnerLockHash, SudtAmount>;
#[cfg(all(feature = "std", not(feature = "script")))]
pub type SudtTrampolineContract = TContract<OwnerLockHash, SudtAmount>;
