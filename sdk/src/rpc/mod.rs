// use std::cell::RefCell;

// use ckb_sdk::{IndexerRpcClient, CkbRpcClient};
// use ckb_sdk::rpc::ckb_indexer::{SearchKey, SearchKeyFilter, ScriptType, Order, Pagination};
// use crate::contract::generator::{QueryProvider, TransactionProvider};
// use crate::types::query::{CellQuery, CellQueryAttribute, QueryStatement};
// use crate::chain::Chain;
// use ckb_types::core::cell::CellMeta;
// use ckb_jsonrpc_types::OutPoint;
mod clients;
pub use clients::*;

// pub struct TrampolineQueryClient {
//     inner_indexer: RefCell<IndexerRpcClient>,
//     inner_chain_rpc: CkbRpcClient,
//     _mercury: Option<()>,
// }

// impl QueryProvider for TrampolineQueryClient {
//     fn query(&self, query: CellQuery) -> Option<Vec<ckb_jsonrpc_types::OutPoint>> {
//         let CellQuery { _query, _limit } = query;
//         match _query {
//             QueryStatement::Single(query_attr) => match query_attr {
//                 CellQueryAttribute::LockHash(hash) => {
//                 todo!()
//                 }
//                 CellQueryAttribute::LockScript(script) => {
//                     if let Some(res) = self.inner_indexer.borrow_mut().get_cells(SearchKey { script, script_type: ScriptType::Lock, filter: None }, Order::Desc, 1.into(), None).ok() {
//                         return Some(res.objects.into_iter().map(|c| c.out_point).collect::<Vec<_>>());
//                     }
//                     return None;
//                 }
//                 CellQueryAttribute::TypeScript(script) => {
//                     if let Some(res) = self.inner_indexer.borrow_mut().get_cells(SearchKey { script, script_type: ScriptType::Type, filter: None }, Order::Desc, 1.into(), None).ok() {
//                         return Some(res.objects.into_iter().map(|c| c.out_point).collect::<Vec<_>>());
//                     }
//                     return None;
//                 }
//                 CellQueryAttribute::DataHash(hash) => {
//                     todo!()
//                     // if let Some(res) = self.inner_indexer.borrow_mut().get_cells(SearchKey { script, script_type: ScriptType::Lock, filter: None }, Order::Desc, 1.into(), None).ok() {
//                     //     return Some(res.objects.into_iter().map(|c| c.out_point).collect::<Vec<_>>());
//                     // }
//                     // return None;
//                 },
//                 _ => panic!("Capacity based queries currently unsupported!"),
//             },
//             _ => panic!("Compund queries currently unsupported!"),
//         }
//     }

//     fn query_cell_meta(&self, query: CellQuery) -> Option<Vec<CellMeta>> {
//         todo!()
//     }

// }
