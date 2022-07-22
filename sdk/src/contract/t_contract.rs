use std::prelude::v1::*;
use std::sync::{Arc, Mutex};

use super::auxiliary_types::*;
use super::generator::GeneratorMiddleware;
use super::schema::TrampolineSchema;
use crate::ckb_types::packed::CellOutput;
use crate::ckb_types::{bytes::Bytes, packed, prelude::*};
use crate::types::bytes::Bytes as TBytes;
use crate::types::cell::{Cell, CellError, CellResult};
use crate::types::query::CellQuery;
use crate::types::script::Script as TScript;
use crate::types::{cell::CellOutputWithData, transaction::CellMetaTransaction};

use crate::ckb_types::core::TransactionView;

use crate::ckb_types::{core::TransactionBuilder, H256};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TContractError {
    #[error(transparent)]
    CellError(#[from] CellError),
    #[error("Cannot convert contract to CellDep. Please set Contract::Source to `Chain` or set inner code cell's outpoint")]
    MissingOutpointOnCellDep,
}
pub type TContractResult<T> = Result<T, TContractError>;
// Replacement for Contract

// pub struct OutputRule<ArgEntity, ArgStruct, DataEntity, DataStruct> {
//     pub scope: RuleScope,
//     pub rule: Box<dyn Fn(RuleContext) -> ContractCellField<A, D>>,
// }

// impl<A, D> OutputRule<A, D> {
//     pub fn new<F>(scope: impl Into<RuleScope>, rule: F) -> Self
//     where
//         F: 'static + Fn(RuleContext) -> ContractCellField<A, D>,
//     {
//         OutputRule {
//             scope: scope.into(),
//             rule: Box::new(rule),
//         }
//     }
//     pub fn exec(&self, ctx: &RuleContext) -> ContractCellField<A, D> {
//         self.rule.as_ref()(ctx.clone()) //call((ctx,))
//     }
// }
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
        let code_cell = Cell::with_data(bytes.clone());
        let mut caller_cell = Cell::default();
        let script = TScript::from(&code_cell);

        assert!(caller_cell.set_type_script(script).is_ok());

        Self {
            source: Some(ContractSource::Immediate((&bytes).into())),
            inner_code_cell: code_cell,
            inner_usage_cell: caller_cell,
            output_rules: Default::default(),
            input_rules: Default::default(),
            outputs_count: 1,
            contract_type: ContractType::Type,
        }
    }
}

enum InnerCellType {
    Code,
    Caller,
}

impl<A, D> TContract<A, D>
where
    A: Into<TBytes> + Default + TrampolineSchema,
    D: Into<TBytes> + Default + TrampolineSchema,
{
    fn safe_cell_update(&mut self, cell: Cell, cell_type: InnerCellType) -> TContractResult<()> {
        // In the case where the code cell hash been updated, ensure that
        // the usage cell's relevant script (lock or type) uses the correct code_hash
        match cell_type {
            InnerCellType::Caller => {
                self.inner_usage_cell = cell;
                Ok(())
            }
            InnerCellType::Code => {
                let new_code_hash = cell.data_hash();
                self.inner_code_cell = cell;
                match self.contract_type {
                    ContractType::Type => {
                        let script = self.inner_usage_cell.type_script()?;
                        if let Some(mut script) = script {
                            script.set_code_hash(new_code_hash);
                            self.inner_usage_cell
                                .set_type_script(script)
                                .map_err(|e| e.into())
                        } else {
                            Ok(())
                        }
                    }
                    ContractType::Lock => {
                        let mut script = self.inner_usage_cell.lock_script()?;
                        script.set_code_hash(new_code_hash);
                        self.inner_usage_cell
                            .set_lock_script(script)
                            .map_err(|e| e.into())
                    }
                }
            }
        }
    }
    fn update_inner_cells<F>(&mut self, update: F, cell_type: InnerCellType) -> TContractResult<()>
    where
        F: FnOnce(Cell) -> CellResult<Cell>,
    {
        let cell_to_update = match &cell_type {
            InnerCellType::Caller => update(self.inner_usage_cell.clone()),
            InnerCellType::Code => update(self.inner_code_cell.clone()),
        }?;
        self.safe_cell_update(cell_to_update, cell_type)?;
        // check if self.script_hash() == self.inner_usage_cell.lock_script_hash() or type_script_hash()
        Ok(())
    }

    // unfortunate clones here
    pub fn set_lock(&mut self, lock: impl Into<TScript>) -> TContractResult<()> {
        //let lock: TScript = lock.into();
        self.update_inner_cells(
            |mut cell| {
                //let mut cell = cell.clone();
                cell.set_lock_script(lock)?;
                Ok(cell)
            },
            InnerCellType::Code,
        )
    }

    pub fn set_type(&mut self, type_: impl Into<TScript>) -> TContractResult<()> {
        // let type_ = type_.into();
        self.update_inner_cells(
            |mut cell| {
                //let mut cell = cell.clone();
                cell.set_type_script(type_)?;
                Ok(cell)
            },
            InnerCellType::Code,
        )
    }

    pub fn set_caller_cell_data(&mut self, data: D) -> TContractResult<()> {
        let data: TBytes = data.to_bytes().into();
        println!("data in set caller cell data: {:?}", data);
        self.update_inner_cells(
            move |cell| {
                let mut cell = cell;
                cell.set_data(data)?;
                Ok(cell)
            },
            InnerCellType::Caller,
        )
    }

    pub fn set_caller_cell_args(&mut self, args: A) -> TContractResult<()> {
        match self.contract_type {
            ContractType::Type => {
                self.inner_usage_cell.set_type_args(args)?;
                Ok(())
            }
            ContractType::Lock => {
                self.inner_usage_cell.set_lock_args(args)?;
                Ok(())
            }
        }
    }

    pub fn code_hash(&self) -> H256 {
        self.inner_code_cell.data_hash()
    }

    pub fn script_hash(&self) -> Option<H256> {
        match self.contract_type {
            ContractType::Type => self.inner_usage_cell.type_hash().ok().unwrap_or_default(),
            ContractType::Lock => self.inner_usage_cell.lock_hash().ok(),
        }
    }

    pub fn caller_cell_data_hash(&self) -> H256 {
        self.inner_usage_cell.data_hash()
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
    pub fn as_script<S: From<TScript>>(&self) -> TContractResult<Option<S>> {
        match self.contract_type {
            ContractType::Type => Ok(self.inner_usage_cell.type_script()?.map(|s| s.into())),
            ContractType::Lock => Ok(Some(self.inner_usage_cell.lock_script()?.into())),
        }
    }

    // Get code cell as a cell dep
    pub fn as_code_cell_dep(&self) -> TContractResult<ckb_types::packed::CellDep> {
        match self
            .inner_code_cell
            .as_cell_dep(ckb_types::core::DepType::Code)
        {
            Ok(dep) => Ok(dep),
            Err(_) => {
                if let Some(src) = &self.source {
                    if let ContractSource::Chain(outp) = src {
                        Ok(packed::CellDep::new_builder()
                            .out_point(outp.clone().into())
                            .dep_type(ckb_types::core::DepType::Code.into())
                            .build())
                    } else {
                        Err(TContractError::MissingOutpointOnCellDep)
                    }
                } else {
                    Err(TContractError::MissingOutpointOnCellDep)
                }
            }
        }
    }

    // Get caller cell as a cell output
    pub fn as_cell_output(&self) -> TContractResult<CellOutput> {
        Ok(CellOutput::from(&self.inner_usage_cell))
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

    pub fn set_output_count(&mut self, count: usize) {
        self.outputs_count = count;
    }

    pub fn tx_template(&self) -> TContractResult<TransactionView> {
        let mut tx = TransactionBuilder::default();

        for _ in 0..self.outputs_count {
            tx = tx
                .output(self.as_cell_output().unwrap())
                .output_data(self.as_caller_cell::<Cell>()?.data().into());
        }

        if let Some(ContractSource::Chain(_)) = self.source.clone() {
            tx = tx.cell_dep(self.as_code_cell_dep().unwrap());
        }

        Ok(tx.build())
    }
}

impl<A, D> GeneratorMiddleware for TContract<A, D>
where
    A: Into<TBytes> + Clone + Default + TrampolineSchema,
    D: Into<TBytes> + Clone + Default + TrampolineSchema,
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
        let tx_template = {
            match tx_template {
                Ok(t) => t,
                Err(e) => {
                    panic!("Error occurred {:?}", e);
                }
            }
        };

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
                    self.script_hash().unwrap().pack();

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
                    let _data = D::from_bytes(output.1.clone()).to_mol();
                   // println!("Data before update {:?}", data.to_mol());
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
