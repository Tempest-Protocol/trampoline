use std::prelude::v1::*;
pub mod builtins;
pub mod schema;
use self::schema::*;

use crate::ckb_types::packed::{CellInput, CellOutput, CellOutputBuilder, Uint64};
use crate::ckb_types::{bytes::Bytes, packed, prelude::*};
use crate::types::{
    transaction::CellMetaTransaction,
    cell::CellOutputWithData,
};
pub mod generator;

use self::generator::{CellQuery, GeneratorMiddleware};


use crate::ckb_types::core::TransactionView;

use crate::ckb_types::{core::TransactionBuilder, H256};

use ckb_hash::blake2b_256;

use ckb_jsonrpc_types::{CellDep, DepType, JsonBytes, OutPoint, Script};
use ckb_types::core::cell::CellMeta;
use thiserror::Error;
use crate::types::cell::{Cell, CellError, CellResult};
use crate::types::bytes::{Bytes as TBytes};
use crate::types::script::{Script as TScript, ScriptResult, ScriptError};
use std::fs;

use std::path::PathBuf;

use std::sync::{Arc, Mutex};

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

            },
            ContractSource::Immediate(b) => {
                let data = TBytes::from(b);
                Ok(Cell::with_data(data))
            },
            ContractSource::Chain(outp) =>  {
                let mut cell = Cell::default();
                cell.set_outpoint(outp.into())?;
                Ok(cell)
            },
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
#[derive(Debug, Error)]
pub enum TContractError {
    #[error(transparent)]
    CellError(#[from] CellError),
}
pub type TContractResult<T> = Result<T, TContractError>;
// Replacement for Contract

pub struct TContract<A: Default, D: Default> {
    pub source: Option<ContractSource>,
    inner_code_cell: Cell,
    inner_usage_cell: Cell,
    pub output_rules: Vec<OutputRule<A, D>>,
    pub input_rules: Vec<Box<dyn Fn(TransactionView) -> CellQuery>>,
    pub outputs_count: usize,
    pub contract_type: ContractType,
}

impl<A, D> Default for TContract<A, D>
where
    A: Default + Into<TBytes>,
    D: Default + Into<TBytes>,
{
    fn default() -> Self {
        Self {
            source: Default::default(),
            inner_code_cell: Default::default(),
            inner_usage_cell: Default::default(),
            output_rules: Default::default(),
            input_rules: Default::default(),
            outputs_count: 1,
            contract_type: ContractType::Type,
        }
    }
}


impl<A, D> From<TBytes> for TContract<A, D>
where
    A: Default,
    D: Default,
{
    
    fn from(bytes: TBytes) -> Self {
        Self {
            source: Some(ContractSource::Immediate((&bytes).into())),
            inner_code_cell: Cell::with_data(bytes),
            inner_usage_cell: Default::default(),
            output_rules: Default::default(),
            input_rules: Default::default(),
            outputs_count: 1,
            contract_type: ContractType::Type,
        }
    }
}

enum InnerCellType {
    Code,
    Caller
}

impl<A,D> TContract<A,D>
where
    A: Into<TBytes> + Default,
    D: Into<TBytes> + Default,
{

    fn safe_cell_update(&mut self, cell: Cell, cell_type: InnerCellType) -> TContractResult<()> {
        // In the case where the code cell hash been updated, ensure that
        // the usage cell's relevant script (lock or type) uses the correct code_hash
        match cell_type {
            InnerCellType::Caller => {
                self.inner_usage_cell = cell;
                Ok(())
            },
            InnerCellType::Code => {
               let new_code_hash = cell.data_hash();
               self.inner_code_cell = cell;
               match self.contract_type {
                    ContractType::Type => {
                        let script = self.inner_usage_cell.type_script()?;
                        if let Some(mut script) = script {
                            script.set_code_hash(new_code_hash);
                            self.inner_usage_cell.set_type_script(script).map_err(|e| e.into())
                        } else {
                            Ok(())
                        }
                    
                    },
                    ContractType::Lock => {
                        let mut script = self.inner_usage_cell.lock_script()?;
                        script.set_code_hash(new_code_hash);
                        self.inner_usage_cell.set_lock_script(script).map_err(|e| e.into())
                    },
                }
            }
        }
    }
    fn update_inner_cells<F>(&mut self, update: F, cell_type: InnerCellType)  -> TContractResult<()> 
        where F: FnOnce(Cell) -> CellResult<Cell>, 
    {
        let cell_to_update = match &cell_type {
            InnerCellType::Caller => {
                update(self.inner_usage_cell.clone())
            },
            InnerCellType::Code => {
                update(self.inner_code_cell.clone())
            }
        }?;
        self.safe_cell_update(cell_to_update, cell_type)?;
        // check if self.script_hash() == self.inner_usage_cell.lock_script_hash() or type_script_hash()
        Ok(())
    }

    // unfortunate clones here
    pub fn set_lock(&mut self, lock: impl Into<TScript>) -> TContractResult<()>{
        //let lock: TScript = lock.into();
        self.update_inner_cells(|mut cell| {
            //let mut cell = cell.clone();
            cell.set_lock_script(lock)?;
            Ok(cell)
        }, InnerCellType::Code)
    }

    pub fn set_type(&mut self, type_: impl Into<TScript>) -> TContractResult<()> {
       // let type_ = type_.into();
        self.update_inner_cells(|mut cell| {
            //let mut cell = cell.clone();
            cell.set_type_script(type_)?;
            Ok(cell)
        }, InnerCellType::Code)
    }

    pub fn set_caller_cell_data(&mut self, data: D) -> TContractResult<()> {
        // let data:TBytes = data.into();
        self.update_inner_cells(move |cell| {
            let mut cell = cell.clone();
            cell.set_data(data)?;
            Ok(cell)
        }, InnerCellType::Caller)
    }

    pub fn set_caller_cell_args(&mut self, args: A) -> TContractResult<()> {
        match self.contract_type {
            ContractType::Type => {
               self.inner_usage_cell.set_type_args(args)?;
               Ok(())
                
            },
            ContractType::Lock => {
                self.inner_usage_cell.set_lock_args(args)?;
                Ok(())
            },
        }
    }

    pub fn code_hash(&self) -> H256 {
        self.inner_code_cell.data_hash()
    }


    pub fn script_hash(&self) ->Option<H256> {
        match self.contract_type {
            ContractType::Type => {
                self.inner_usage_cell.type_hash().ok().unwrap_or_default()            
            },
            ContractType::Lock => {
                self.inner_usage_cell.lock_hash().ok()
            },
        }        
    }

    pub fn caller_cell_data_hash(&self) -> H256 {
        todo!()
    }

    // Can be used to retrieve cell output, cell output with data, cell input, etc...
    pub fn as_caller_cell<C: From<Cell>>(&self) -> TContractResult<C> {
        let cell = self.inner_usage_cell.clone();
        cell.validate()?;
        Ok(cell.into())
    }

    pub fn as_code_cell<C: From<Cell>>(&self) -> TContractResult<C> {
        let cell = self.inner_code_cell.clone();
        cell.validate()?;
        Ok(cell.into())
    }


    // Can be used to retrieve packed script or json script struct
    pub fn as_script<S: From<TScript>>(&self) -> TContractResult<Option<S>> 
    {
        match self.contract_type {
            ContractType::Type => {
                Ok(self.inner_usage_cell.type_script()?.map(|s| s.into()))
                
            },
            ContractType::Lock => {
                Ok(Some(self.inner_usage_cell.lock_script()?.into()))
            },
        }
    }

    // Get code cell as a cell dep
    pub fn as_cell_dep(&self) -> TContractResult<CellDep> {
        todo!()
    }

    // Get caller cell as a cell output
    pub fn as_cell_output(&self) -> TContractResult<CellOutput> {
        todo!()
    }


}
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

pub struct Contract<A, D> {
    pub source: Option<ContractSource>,
    pub data: D,
    pub args: A,
    pub lock: Option<Script>,
    pub type_: Option<Script>,
    pub code: Option<JsonBytes>,
    #[allow(clippy::type_complexity)]
    pub output_rules: Vec<OutputRule<A, D>>,
    pub input_rules: Vec<Box<dyn Fn(TransactionView) -> CellQuery>>,
    pub outputs_count: usize,
}

impl<A, D> Default for Contract<A, D>
where
    D: JsonByteConversion + MolConversion + BytesConversion + Clone + Default,
    A: JsonByteConversion + MolConversion + BytesConversion + Clone + Default,
{
    fn default() -> Self {
        Self {
            source: Default::default(),
            data: Default::default(),
            args: Default::default(),
            lock: Default::default(),
            type_: Default::default(),
            code: Default::default(),
            output_rules: Default::default(),
            input_rules: Default::default(),
            outputs_count: 1,
        }
    }
}

impl<A, D> Contract<A, D>
where
    D: JsonByteConversion + MolConversion + BytesConversion + Clone,
    A: JsonByteConversion + MolConversion + BytesConversion + Clone,
{
    // The lock script of the cell containing contract code
    pub fn lock(mut self, lock: Script) -> Self {
        self.lock = Some(lock);
        self
    }

    // The type script of the cell containing contract code
    pub fn type_(mut self, type_: Script) -> Self {
        self.type_ = Some(type_);
        self
    }

    pub fn data_hash(&self) -> Option<H256> {
        let data = self.data.to_mol();
        let data = data.as_slice();
        let raw_hash = blake2b_256(&data);
        H256::from_slice(&raw_hash).ok()
    }
    pub fn code_hash(&self) -> Option<H256> {
        if let Some(data) = &self.code {
            let byte_slice = data.as_bytes();

            let raw_hash = blake2b_256(&byte_slice);
            H256::from_slice(&raw_hash).ok()
        } else {
            None
        }
    }

    // Returns a script structure which can be used as a lock or type script on other cells.
    // This is an easy way to let other cells use this contract
    pub fn as_script(&self) -> Option<ckb_jsonrpc_types::Script> {
        self.code_hash().map(|data_hash| {
            Script::from(
                packed::ScriptBuilder::default()
                    .args(self.args.to_bytes().pack())
                    .code_hash(data_hash.pack())
                    .hash_type(ckb_types::core::ScriptHashType::Data1.into())
                    .build(),
            )
        })
    }

    // Return a CellOutputWithData which is the code cell storing this contract's logic
    pub fn as_code_cell(&self) -> CellOutputWithData {
        let data: Bytes = self.code.clone().unwrap_or_default().into_bytes();
        let type_script = self.type_.clone().unwrap_or_default();
        let type_script = {
            if self.type_.is_some() {
                Some(ckb_types::packed::Script::from(type_script))
            } else {
                None
            }
        };

        let cell_output = CellOutputBuilder::default()
            .capacity((data.len() as u64).pack())
            .lock(self.lock.clone().unwrap_or_default().into())
            .type_(type_script.pack())
            .build();
        (cell_output, data)
    }

    pub fn script_hash(&self) -> Option<ckb_jsonrpc_types::Byte32> {
        let script: ckb_types::packed::Script = self.as_script().unwrap().into();
        Some(script.calc_script_hash().into())
    }

    pub fn as_cell_dep(&self, out_point: OutPoint) -> CellDep {
        CellDep {
            out_point,
            dep_type: DepType::Code,
        }
    }

    // Set data of a cell that will *reference* (i.e., use) this contract
    pub fn set_raw_data(&mut self, data: impl Into<JsonBytes>) {
        self.data = D::from_json_bytes(data.into());
    }

    pub fn set_data(&mut self, data: D) {
        self.data = data;
    }

    // Set args of a cell that will *reference* (i.e., use) this contract
    pub fn set_raw_args(&mut self, args: impl Into<JsonBytes>) {
        self.args = A::from_json_bytes(args.into());
    }

    pub fn set_args(&mut self, args: A) {
        self.args = args;
    }

    pub fn read_data(&self) -> D {
        self.data.clone()
    }

    pub fn read_args(&self) -> A {
        self.args.clone()
    }

    pub fn read_raw_data(&self, data: Bytes) -> D {
        D::from_bytes(data)
    }

    pub fn read_raw_args(&self, args: Bytes) -> A {
        A::from_bytes(args)
    }

    pub fn add_output_rule<F>(&mut self, scope: impl Into<RuleScope>, transform_func: F)
    where
        F: Fn(RuleContext) -> ContractCellField<A, D> + 'static,
    {
        self.output_rules
            .push(OutputRule::new(scope.into(), transform_func));
    }

    pub fn add_input_rule<F>(&mut self, query_func: F)
    where
        F: Fn(TransactionView) -> CellQuery + 'static,
    {
        self.input_rules.push(Box::new(query_func))
    }

    pub fn output_count(&mut self, count: usize) {
        self.outputs_count = count;
    }
    pub fn tx_template(&self) -> TransactionView {
        let arg_size = self.args.to_mol().as_builder().expected_length() as u64;
        let data_size = self.data.to_mol().as_builder().expected_length() as u64;
        println!("DATA SIZE EXPECTED: {:?}", data_size);
        let mut data = Vec::with_capacity(data_size as usize);
        (0..data_size as usize).into_iter().for_each(|_| {
            data.push(0u8);
        });
        let mut tx = TransactionBuilder::default();

        for _ in 0..self.outputs_count {
            tx = tx
                .output(
                    CellOutput::new_builder()
                        .capacity((data_size + arg_size).pack())
                        .type_(
                            Some(ckb_types::packed::Script::from(self.as_script().unwrap())).pack(),
                        )
                        .build(),
                )
                .output_data(data.pack());
        }

        if let Some(ContractSource::Chain(outp)) = self.source.clone() {
            tx = tx.cell_dep(self.as_cell_dep(outp).into());
        }

        tx.build()
    }
}

impl<A, D> GeneratorMiddleware for Contract<A, D>
where
    D: JsonByteConversion + MolConversion + BytesConversion + Clone,
    A: JsonByteConversion + MolConversion + BytesConversion + Clone,
{
    fn update_query_register(
        &self,
        tx: CellMetaTransaction,
        query_register: Arc<Mutex<Vec<CellQuery>>>,
    ) {
        let queries = self.input_rules.iter().map(|rule| rule(tx.clone().tx));

        query_register.lock().unwrap().extend(queries);
    }
    fn pipe(
        &self,
        tx_meta: CellMetaTransaction,
        _query_queue: Arc<Mutex<Vec<CellQuery>>>,
    ) -> CellMetaTransaction {
        type OutputWithData = (CellOutput, Bytes);

        let tx = tx_meta.tx.clone();
        let tx_template = self.tx_template();

        let total_deps = tx
            .cell_deps()
            .as_builder()
            .extend(tx_template.cell_deps_iter())
            .build();
        let total_outputs = tx
            .outputs()
            .as_builder()
            .extend(tx_template.outputs())
            .build();
        let total_inputs = tx
            .inputs()
            .as_builder()
            .extend(tx_template.inputs())
            .build();
        let total_outputs_data = tx
            .outputs_data()
            .as_builder()
            .extend(tx_template.outputs_data())
            .build();
        let tx = tx
            .as_advanced_builder()
            .set_cell_deps(
                total_deps
                    .into_iter()
                    .collect::<Vec<crate::ckb_types::packed::CellDep>>(),
            )
            .set_outputs(
                total_outputs
                    .into_iter()
                    .collect::<Vec<crate::ckb_types::packed::CellOutput>>(),
            )
            .set_inputs(
                total_inputs
                    .into_iter()
                    .collect::<Vec<crate::ckb_types::packed::CellInput>>(),
            )
            .set_outputs_data(
                total_outputs_data
                    .into_iter()
                    .collect::<Vec<crate::ckb_types::packed::Bytes>>(),
            )
            .build();

        let outputs = tx
            .clone()
            .outputs()
            .into_iter()
            .enumerate()
            .filter_map(|(idx, output)| {
                let self_script_hash: ckb_types::packed::Byte32 =
                    self.script_hash().unwrap().into();

                if let Some(type_) = output.type_().to_opt() {
                    if type_.calc_script_hash() == self_script_hash {
                        return Some((idx, tx.output_with_data(idx).unwrap()));
                    }
                }

                if output.lock().calc_script_hash() == self_script_hash {
                    return Some((idx, tx.output_with_data(idx).unwrap()));
                }

                None
            });

        let mut ctx = RuleContext::new(tx_meta.clone());

        let outputs = outputs
            .map(|output_with_idx| {
                ctx.idx(output_with_idx.0);
                let processed = self.output_rules.iter().fold(output_with_idx.1, |output, rule| {
                    let data = self.read_raw_data(output.1.clone());
                    println!("Data before update {:?}", data.to_mol());
                    let updated_field = rule.exec(&ctx);
                    match updated_field {
                        ContractCellField::Args(_) => todo!(),
                        ContractCellField::Data(d) => {
                            if rule.scope != ContractField::Data.into() {
                                panic!("Error, mismatch of output rule scope and returned field");
                            }
                            let updated_tx = ctx.get_tx();
                            let inner_tx_view = updated_tx.tx.clone();
                            let updated_outputs_data = inner_tx_view.outputs_with_data_iter()
                                .enumerate().map(|(i, output)| {
                                    if i == ctx.idx {
                                       (output.0, d.to_bytes())
                                    } else {
                                        output
                                    }
                                }).collect::<Vec<CellOutputWithData>>();
                            let updated_inner_tx = inner_tx_view.as_advanced_builder()
                                .set_outputs(updated_outputs_data.iter().map(|o| o.0.clone()).collect::<Vec<_>>())
                                .set_outputs_data(updated_outputs_data.iter().map(|o| o.1.pack()).collect::<Vec<_>>())
                                .build();
                            let updated_tx = updated_tx.tx(updated_inner_tx);
                            ctx = ctx.clone().tx(updated_tx);
                            (output.0, d.to_bytes())
                        },
                        ContractCellField::LockScript(_) => todo!(),
                        ContractCellField::TypeScript(_) => todo!(),
                        ContractCellField::Capacity(_) => todo!(),
                        _ => {
                            panic!("Error: Contract-level rule attempted transaction-level update.")
                        }
                    }
                });
                println!("Output bytes of processed output: {:?}", processed.1.pack());
                processed
            })
            .collect::<Vec<OutputWithData>>();

        let final_inner_tx = tx
            .as_advanced_builder()
            .set_outputs(
                outputs
                    .iter()
                    .map(|out| out.0.clone())
                    .collect::<Vec<CellOutput>>(),
            )
            .set_outputs_data(
                outputs
                    .iter()
                    .map(|out| out.1.clone().pack())
                    .collect::<Vec<ckb_types::packed::Bytes>>(),
            )
            .build();
        tx_meta.tx(final_inner_tx)
    }
}
