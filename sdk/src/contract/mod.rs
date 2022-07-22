// Modules usable within on-chain scripts
pub mod schema;
pub use schema::*;
pub mod builtins;
pub use builtins::*;

#[cfg(all(feature = "std", not(feature = "script")))]
pub mod auxiliary_types;
#[cfg(all(feature = "std", not(feature = "script")))]
pub mod generator;

#[cfg(all(feature = "std", not(feature = "script")))]
pub mod t_contract;
#[cfg(all(feature = "std", not(feature = "script")))]
mod contract_gen_1 {
    use super::{
        auxiliary_types::{ContractSource, RuleContext},
        generator::GeneratorMiddleware,
    };
    use crate::ckb_types::{
        bytes::Bytes,
        core::{TransactionBuilder, TransactionView},
        packed::{self, CellOutput, CellOutputBuilder},
        prelude::*,
        H256,
    };
    use ckb_hash::blake2b_256;
    use ckb_jsonrpc_types::{CellDep, DepType, JsonBytes, OutPoint, Script};
    use std::sync::{Arc, Mutex};

    use super::auxiliary_types::*;
    use super::schema::*;
    use crate::types::{
        cell::CellOutputWithData, query::CellQuery, transaction::CellMetaTransaction,
    };
    use std::prelude::v1::*;
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
        #[cfg(all(feature = "std", not(feature = "script")))]
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
                                Some(ckb_types::packed::Script::from(self.as_script().unwrap()))
                                    .pack(),
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
        #[cfg(all(feature = "std", not(feature = "script")))]
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

            let outputs =
                tx.clone()
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
}

#[cfg(all(feature = "std", not(feature = "script")))]
pub use contract_gen_1::*;
#[cfg(all(feature = "std", not(feature = "script")))]
pub use t_contract::*;
