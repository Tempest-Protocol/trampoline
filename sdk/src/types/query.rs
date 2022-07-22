use ckb_jsonrpc_types::{Byte32, Capacity, Script};
use std::prelude::v1::*;

// TODO: implement from for CellQueryAttribute on json_types and packed types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CellQueryAttribute {
    LockHash(Byte32),
    LockScript(Script),
    TypeScript(Script),
    MinCapacity(Capacity),
    MaxCapacity(Capacity),
    DataHash(Byte32),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QueryStatement {
    Single(CellQueryAttribute),
    FilterFrom(CellQueryAttribute, CellQueryAttribute),
    Any(Vec<CellQueryAttribute>),
    All(Vec<CellQueryAttribute>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CellQuery {
    pub _query: QueryStatement,
    pub _limit: u64,
}
