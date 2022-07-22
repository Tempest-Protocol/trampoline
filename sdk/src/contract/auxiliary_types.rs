use super::schema::*;
use std::prelude::v1::*;

use crate::ckb_types::bytes::Bytes;
use crate::ckb_types::packed::{CellInput, Uint64};
use crate::types::{cell::CellOutputWithData, transaction::CellMetaTransaction};

use ckb_jsonrpc_types::OutPoint;
use ckb_types::core::cell::CellMeta;

use crate::types::bytes::Bytes as TBytes;
use crate::types::cell::{Cell, CellError, CellResult};

use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum ContractSource {
    LocalPath(PathBuf),
    Immediate(Bytes),
    Chain(OutPoint),
}

impl ContractSource {
    pub fn load_from_path(path: PathBuf) -> std::io::Result<Bytes> {
        let file = fs::read(path)?;
        println!("SUDT CODE SIZE: {}", file.len());
        Ok(Bytes::from(file))
    }
}

impl TryFrom<ContractSource> for Cell {
    type Error = CellError;
    fn try_from(src: ContractSource) -> CellResult<Cell> {
        match src {
            ContractSource::LocalPath(p) => {
                let data = ContractSource::load_from_path(p)?;
                let data = TBytes::from(data);
                Ok(Cell::with_data(data))
            }
            ContractSource::Immediate(b) => {
                let data = TBytes::from(b);
                Ok(Cell::with_data(data))
            }
            ContractSource::Chain(outp) => {
                let mut cell = Cell::default();
                cell.set_outpoint(outp.into())?;
                Ok(cell)
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum ContractField {
    Args,
    Data,
    LockScript,
    TypeScript,
    Capacity,
}

#[derive(Clone, PartialEq)]
pub enum TransactionField {
    ResolvedInputs,
    Inputs,
    Outputs,
    Dependencies,
}

#[derive(PartialEq)]
pub enum RuleScope {
    ContractField(ContractField),
    TransactionField(TransactionField),
}

impl From<ContractField> for RuleScope {
    fn from(f: ContractField) -> Self {
        Self::ContractField(f)
    }
}

impl From<TransactionField> for RuleScope {
    fn from(f: TransactionField) -> Self {
        Self::TransactionField(f)
    }
}

#[derive(Clone)]
pub struct RuleContext {
    inner: CellMetaTransaction,
    pub idx: usize,
    pub curr_field: TransactionField,
}

impl RuleContext {
    pub fn new(tx: impl Into<CellMetaTransaction>) -> Self {
        Self {
            inner: tx.into(),
            idx: 0,
            curr_field: TransactionField::Outputs,
        }
    }
    pub fn tx(mut self, tx: impl Into<CellMetaTransaction>) -> Self {
        self.inner = tx.into();
        self
    }

    pub fn get_tx(&self) -> CellMetaTransaction {
        self.inner.clone()
    }
    pub fn idx(&mut self, idx: usize) {
        self.idx = idx;
    }

    pub fn curr_field(&mut self, field: TransactionField) {
        self.curr_field = field;
    }

    pub fn load<A, D>(&self, scope: impl Into<RuleScope>) -> ContractCellField<A, D>
    where
        D: JsonByteConversion + MolConversion + BytesConversion + Clone + Default,
        A: JsonByteConversion + MolConversion + BytesConversion + Clone,
    {
        match scope.into() {
            RuleScope::ContractField(field) => match field {
                ContractField::Args => todo!(),
                ContractField::Data => match self.curr_field {
                    TransactionField::Outputs => {
                        let data_reader = self.inner.outputs_data();
                        let data_reader = data_reader.as_reader();
                        let data = data_reader.get(self.idx);
                        if let Some(data) = data {
                            ContractCellField::Data(D::from_bytes(data.raw_data().to_vec().into()))
                        } else {
                            ContractCellField::Data(D::default())
                        }
                    }
                    _ => ContractCellField::Data(D::default()),
                },
                ContractField::LockScript => todo!(),
                ContractField::TypeScript => todo!(),
                ContractField::Capacity => todo!(),
            },
            RuleScope::TransactionField(field) => match field {
                TransactionField::Inputs => ContractCellField::Inputs(
                    self.inner.inputs().into_iter().collect::<Vec<CellInput>>(),
                ),
                TransactionField::Outputs => ContractCellField::Outputs(
                    self.inner
                        .outputs_with_data_iter()
                        .collect::<Vec<CellOutputWithData>>(),
                ),
                TransactionField::Dependencies => ContractCellField::CellDeps(
                    self.inner
                        .cell_deps_iter()
                        .collect::<Vec<crate::ckb_types::packed::CellDep>>(),
                ),
                TransactionField::ResolvedInputs => {
                    ContractCellField::ResolvedInputs(self.inner.inputs.clone())
                }
            },
        }
    }
}

pub struct OutputRule<A, D> {
    pub scope: RuleScope,
    pub rule: Box<dyn Fn(RuleContext) -> ContractCellField<A, D>>,
}

impl<A, D> OutputRule<A, D> {
    pub fn new<F>(scope: impl Into<RuleScope>, rule: F) -> Self
    where
        F: 'static + Fn(RuleContext) -> ContractCellField<A, D>,
    {
        OutputRule {
            scope: scope.into(),
            rule: Box::new(rule),
        }
    }
    pub fn exec(&self, ctx: &RuleContext) -> ContractCellField<A, D> {
        self.rule.as_ref()(ctx.clone()) //call((ctx,))
    }
}
pub enum ContractType {
    Type,
    Lock,
}

#[derive(Clone)]
pub enum ContractCellField<A, D> {
    Args(A),
    Data(D),
    LockScript(ckb_types::packed::Script),
    TypeScript(ckb_types::packed::Script),
    Capacity(Uint64),
    Inputs(Vec<CellInput>),
    ResolvedInputs(Vec<CellMeta>),
    Outputs(Vec<CellOutputWithData>),
    CellDeps(Vec<ckb_types::packed::CellDep>),
}
