use crate::ckb_types::{
    core::{Capacity, DepType},
    packed::{CellDep, CellOutput, OutPoint},
    prelude::*,
    H256,
};
use std::prelude::v1::*;

use no_std_compat::borrow::Borrow;

use crate::bytes::Bytes;
use crate::script::Script;

pub type CellOutputWithData = (CellOutput, crate::ckb_types::bytes::Bytes);

#[cfg(all(feature = "std", not(feature = "script")))]
pub mod cell_error {

    use crate::ckb_types::core::CapacityError;
    use crate::{bytes::BytesError, script::ScriptError};
    use std::io::Error as IoError;
    use thiserror::Error;
    #[derive(Debug, Error)]
    pub enum CellError {
        #[error("Capacity not enough for cell size")]
        CapacityNotEnough,
        #[error(transparent)]
        CapacityCalcError(#[from] CapacityError),
        #[error(transparent)]
        ScriptError(#[from] ScriptError),
        #[error(transparent)]
        BytesError(#[from] BytesError),
        #[error(transparent)]
        IoError(#[from] IoError),
        #[error("Type script is currently None")]
        MissingTypeScript,
        #[error("Cannot convert cell to CellDep: no outpoint")]
        MissingOutpoint,
    }
}

#[cfg(feature = "script")]
pub mod cell_error {
    use crate::ckb_types::core::CapacityError;
    use crate::{bytes::BytesError, script::ScriptError};
    use core::fmt::{self, write};

    pub enum CellError {
        CapacityNotEnough,
        CapacityCalcError,
        UnknownCapacityError,
        ScriptError(ScriptError),
        BytesError,
        MissingTypeScript,
        MissingOutpoint,
    }

    impl From<CapacityError> for CellError {
        fn from(e: CapacityError) -> Self {
            match e {
                CapacityError::Overflow => CellError::CapacityCalcError,
                _ => CellError::UnknownCapacityError,
            }
        }
    }
    impl From<ScriptError> for CellError {
        fn from(e: ScriptError) -> Self {
            CellError::ScriptError(e)
        }
    }
    impl From<BytesError> for CellError {
        fn from(_e: BytesError) -> Self {
            CellError::BytesError
        }
    }

    impl core::fmt::Debug for CellError {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let err_str = match self {
                CellError::CapacityNotEnough => "Capacity Not Enough",
                CellError::CapacityCalcError => "Capacity Calc Error",
                CellError::ScriptError(_) => "Script Error",
                CellError::BytesError => "Bytes Error",
                CellError::MissingTypeScript => "Missing type script",
                CellError::MissingOutpoint => "Missing OutPoint",
                CellError::UnknownCapacityError => "Unknown err capacity",
            };
            write!(f, "{}", err_str)
        }
    }
}
pub use cell_error::*;
pub type CellResult<T> = Result<T, CellError>;

#[cfg_attr(all(feature = "std", not(feature = "script")), derive(Debug))]
#[derive(Clone, Default)]
pub struct Cell {
    data: Bytes,
    outpoint: Option<OutPoint>,
    capacity: Capacity,
    lock_script: Script,
    type_script: Option<Script>,
}

// impl Default for Cell {
//     fn default() -> Self {
//         Self {
//             data: Default::default(),
//             outpoint: Default::default(),
//             capacity: Default::default(),
//             lock_script: Default::default(),
//             type_script: Default::default(),
//         }
//     }
// }

impl Cell {
    pub fn with_data(data: impl Into<Bytes>) -> Self {
        let data: Bytes = data.into();

        let mut cell = Self {
            data,
            outpoint: None,
            capacity: Capacity::zero(),
            lock_script: Script::default(),
            type_script: None,
        };
        let cell_capacity = cell.required_capacity().unwrap();
        println!("Cell WITH DATA REQ CAPACITY {}", cell_capacity.as_u64());
        assert!(cell.set_capacity_shannons(cell_capacity.as_u64()).is_ok());
        cell
    }

    pub fn with_lock(script: impl Borrow<Script>) -> Self {
        Self {
            data: Default::default(),
            outpoint: None,
            capacity: Capacity::zero(),
            lock_script: script.borrow().clone(),
            type_script: None,
        }
    }

    pub fn required_capacity(&self) -> CellResult<Capacity> {
        let type_script_size = match &self.type_script {
            Some(script) => {
                // script.validate()?;
                script.required_capacity()?
            }
            None => Capacity::zero(),
        };
        //self.lock_script.validate()?;
        let lock_script_size = self.lock_script.required_capacity()?;
        let other_fields_size = self.data.required_capacity()?;
        let capacity_field_req = Capacity::bytes(8)?;
        let total_size = type_script_size
            .safe_add(lock_script_size)?
            .safe_add(other_fields_size)?
            .safe_add(capacity_field_req)?;
        Ok(total_size)
    }
    /// Ensure the total cell size <= min required capacity
    /// Ensure that the capacity in the cell >= min required capacity
    pub fn validate(&self) -> CellResult<Capacity> {
        match &self.type_script {
            Some(script) => {
                script.validate()?;
            }
            None => {}
        };
        self.lock_script.validate()?;
        self.required_capacity()
    }

    pub fn lock_hash(&self) -> CellResult<H256> {
        self.lock_script.validate().map_err(CellError::ScriptError)
    }

    pub fn type_hash(&self) -> CellResult<Option<H256>> {
        if let Some(script) = &self.type_script {
            script.validate().map_err(CellError::ScriptError).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn type_script(&self) -> CellResult<Option<Script>> {
        Ok(self.type_script.clone())
    }

    pub fn lock_script(&self) -> CellResult<Script> {
        Ok(self.lock_script.clone())
    }
    pub fn capacity(&self) -> Capacity {
        self.capacity
    }

    pub fn data_size(&self) -> usize {
        self.data.len()
    }

    pub fn outpoint(&self) -> Option<OutPoint> {
        self.outpoint.clone()
    }

    pub fn data_hash(&self) -> H256 {
        self.data.hash_256()
    }

    pub fn data(&self) -> Bytes {
        self.data.clone()
    }

    pub fn set_lock_script(&mut self, script: impl Into<Script>) -> CellResult<()> {
        self.lock_script = script.into();
        Ok(())
    }

    pub fn set_type_script(&mut self, script: impl Into<Script>) -> CellResult<()> {
        self.type_script = Some(script.into());
        Ok(())
    }

    pub fn set_lock_args(&mut self, bytes: impl Into<Bytes>) -> CellResult<()> {
        self.lock_script.set_args(bytes);
        Ok(())
    }

    pub fn set_type_args(&mut self, bytes: impl Into<Bytes>) -> CellResult<()> {
        if let Some(script) = &mut self.type_script {
            script.set_args(bytes);
            Ok(())
        } else {
            Err(CellError::MissingTypeScript)
        }
    }

    pub fn set_data(&mut self, data: impl Into<Bytes>) -> CellResult<()> {
        self.data = data.into();
        Ok(())
    }

    pub fn set_outpoint(&mut self, outp: OutPoint) -> CellResult<()> {
        self.outpoint = Some(outp);
        Ok(())
    }

    pub fn set_capacity_ckbytes(&mut self, capacity: u64) -> CellResult<()> {
        self.capacity = Capacity::bytes(capacity as usize)?;
        Ok(())
    }

    pub fn set_capacity_shannons(&mut self, capacity: u64) -> CellResult<()> {
        self.capacity = Capacity::shannons(capacity);
        Ok(())
    }

    pub fn as_cell_dep(&self, _dep_type: DepType) -> CellResult<CellDep> {
        if let Some(outp) = self.outpoint() {
            Ok(CellDep::new_builder()
                .dep_type(_dep_type.into())
                .out_point(outp)
                .build())
        } else {
            Err(CellError::MissingOutpoint)
        }
    }
}

impl TryFrom<Cell> for CellDep {
    type Error = CellError;

    fn try_from(value: Cell) -> Result<Self, Self::Error> {
        value.as_cell_dep(DepType::Code)
    }
}

impl From<Cell> for CellOutput {
    fn from(cell: Cell) -> Self {
        CellOutput::new_builder()
            .capacity(cell.capacity.as_u64().pack())
            .lock(cell.lock_script.into())
            .type_(
                cell.type_script
                    .map(crate::ckb_types::packed::Script::from)
                    .pack(),
            )
            .build()
    }
}

impl From<CellOutput> for Cell {
    fn from(celloutput: CellOutput) -> Self {
        let mut cell = Cell::default();
        cell.set_lock_script(celloutput.lock()).ok();
        if let Some(typ) = celloutput.type_().to_opt() {
            cell.set_type_script(typ).ok();
        }
        cell.set_capacity_shannons(celloutput.capacity().unpack())
            .ok();
        cell
    }
}

impl From<Cell> for CellOutputWithData {
    fn from(cell: Cell) -> Self {
        let outp: CellOutput = cell.clone().into();
        let data = cell.data;
        (outp, data.into())
    }
}

impl TryFrom<&Cell> for CellDep {
    type Error = CellError;
    fn try_from(value: &Cell) -> Result<Self, Self::Error> {
        value.as_cell_dep(DepType::Code)
    }
}

impl From<&Cell> for CellOutput {
    fn from(cell: &Cell) -> Self {
        cell.clone().into()
    }
}

impl From<&Cell> for CellOutputWithData {
    fn from(cell: &Cell) -> Self {
        cell.clone().into()
    }
}
