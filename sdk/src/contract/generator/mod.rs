use ckb_jsonrpc_types::{Byte32, OutPoint, TransactionView as JsonTransaction};
use ckb_types::packed::CellDepBuilder;
use std::prelude::v1::*;

use crate::types::query::*;
use crate::types::transaction::CellMetaTransaction;
use crate::{
    chain::Chain,
    ckb_types::{
        core::{cell::CellMeta, TransactionBuilder},
        packed::CellInputBuilder,
        prelude::*,
    },
};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

// Note: Uses ckb_jsonrpc_types
pub trait TransactionProvider {
    fn send_tx(&self, tx: JsonTransaction) -> Option<Byte32>;

    fn verify_tx(&self, tx: JsonTransaction) -> bool;
}

// Note: Uses ckb_types::core::TransactionView; not ckb_jsonrpc_types::TransactionView
pub trait GeneratorMiddleware {
    fn pipe(
        &self,
        tx: CellMetaTransaction,
        query_register: Arc<Mutex<Vec<CellQuery>>>,
    ) -> CellMetaTransaction;

    fn update_query_register(
        &self,
        tx: CellMetaTransaction,
        query_register: Arc<Mutex<Vec<CellQuery>>>,
    );
}

pub trait QueryProvider {
    fn query(&self, query: CellQuery) -> Option<Vec<OutPoint>>;
    fn query_cell_meta(&self, query: CellQuery) -> Option<Vec<CellMeta>>;
}

#[derive(Default)]
pub struct Generator<'a, 'b, C: Chain> {
    middleware: Vec<&'a dyn GeneratorMiddleware>,
    chain_service: Option<&'b C>,
    query_service: Option<&'b dyn QueryProvider>,
    tx: Option<CellMetaTransaction>,
    query_queue: Arc<Mutex<Vec<CellQuery>>>,
}

impl<'a, 'b, C> Generator<'a, 'b, C>
where
    C: Chain,
{
    pub fn new() -> Self {
        Generator {
            middleware: vec![],
            chain_service: None,
            query_service: None,
            tx: Some(TransactionBuilder::default().build().into()),
            query_queue: Arc::new(Mutex::new(vec![])),
        }
    }

    pub fn pipeline(mut self, pipes: Vec<&'a dyn GeneratorMiddleware>) -> Self {
        self.middleware = pipes;
        self
    }

    pub fn chain_service(mut self, chain_service: &'b C) -> Self {
        self.chain_service = Some(chain_service);
        self
    }

    pub fn query_service(mut self, query_service: &'b dyn QueryProvider) -> Self {
        self.query_service = Some(query_service);
        self
    }

    pub fn query(&self, query: CellQuery) -> Option<Vec<CellMeta>> {
        // println!(
        //     "Res in generator.query for cell_query {:?} is {:?}",
        //     query, res
        // );
        self.query_service.unwrap().query_cell_meta(query)
    }

    pub fn generate(&self) -> CellMetaTransaction {
        self.pipe(self.tx.as_ref().unwrap().clone(), self.query_queue.clone())
    }

    pub fn resolve_queries(&self, query_register: Arc<Mutex<Vec<CellQuery>>>) -> Vec<CellMeta> {
        query_register
            .lock()
            .unwrap()
            .iter()
            .flat_map(|query| self.query(query.to_owned()).unwrap())
            .collect::<Vec<_>>()
    }
}

impl<C: Chain> GeneratorMiddleware for Generator<'_, '_, C> {
    fn update_query_register(
        &self,
        tx: CellMetaTransaction,
        query_register: Arc<Mutex<Vec<CellQuery>>>,
    ) {
        self.middleware
            .iter()
            .for_each(|m| m.update_query_register(tx.clone(), query_register.clone()));
    }
    fn pipe(
        &self,
        tx: CellMetaTransaction,
        query_register: Arc<Mutex<Vec<CellQuery>>>,
    ) -> CellMetaTransaction {
        self.update_query_register(tx.clone(), query_register.clone());
        let inputs = self.resolve_queries(query_register.clone());
        // println!("RESOLVED INPUTS IN GENERATOR PIPE: {:?}", inputs);
        let inner_tx = tx
            .as_advanced_builder()
            .set_inputs(
                inputs
                    .iter()
                    .map(|inp| {
                        CellInputBuilder::default()
                            .previous_output(inp.out_point.clone())
                            .build()
                    })
                    .collect::<Vec<_>>(),
            )
            .build();
        let tx = tx.tx(inner_tx).with_inputs(inputs);
        let tx = self.middleware.iter().fold(tx, |tx, middleware| {
            middleware.pipe(tx, query_register.clone())
        });
        #[allow(clippy::mutable_key_type)]
        let mut queries = HashSet::new();
        tx.inputs.iter().for_each(|cell| {
            if let Some(script) = cell.cell_output.type_().to_opt() {
                let query = CellQuery {
                    _query: QueryStatement::Single(CellQueryAttribute::DataHash(
                        script.code_hash().into(),
                    )),
                    _limit: 1,
                };
                queries.insert(query);
            }
            queries.insert(CellQuery {
                _query: QueryStatement::Single(CellQueryAttribute::DataHash(
                    cell.cell_output.lock().code_hash().into(),
                )),
                _limit: 1,
            });
        });
        let deps = queries.into_iter().flat_map(|q| {
            self.query(q).unwrap().into_iter().map(|cell_dep_meta| {
                CellDepBuilder::default()
                    .out_point(cell_dep_meta.out_point)
                    .build()
            })
        });

        let inner_tx = tx
            .as_advanced_builder()
            .cell_deps(tx.cell_deps().as_builder().extend(deps).build())
            .build();
        // TO DO: Resolve cell deps of inputs
        //         Will have to accommodate some cells being deptype of depgroup

        println!(
            "FINAL TX GENERATED: {:#?}",
            ckb_jsonrpc_types::TransactionView::from(tx.clone().tx)
        );
        tx.tx(inner_tx)
    }
}
