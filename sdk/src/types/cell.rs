use ckb_types::{
    H256,
    core::{
        cell::{
            CellMeta,
            CellMetaBuilder,
            CellProvider,
            CellStatus,
            
        },
        Capacity,
        
        capacity_bytes,
        CapacityResult,
        CapacityError, DepType,
    },
    packed::{Byte32, CellOutput, OutPoint, CellInput, Bytes as PackedBytes, CellDep},
    prelude::*,
};



use ckb_jsonrpc_types::{CellOutput as JsonCellOutput, 
    CellDep as JsonCellDep, 
    CellData, 
    CellInput as JsonCellInput, 
    CellWithStatus, 
    JsonBytes, 
    CellInfo as CellWithData,
    Byte32 as JsonByte32,
    OutPoint as JsonOutPoint,
    
};
use std::io::Error as IoError;
use thiserror::Error;

use super::script::{Script, ScriptError};
use super::bytes::{Bytes, BytesError};

pub type CellOutputWithData = (CellOutput, ckb_types::bytes::Bytes);

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
}

pub type CellResult<T> = Result<T, CellError>;

#[derive(Debug, Clone, Default)]
pub struct Cell {
    data: Bytes,
    outpoint: Option<OutPoint>,
    capacity: Capacity,
    lock_script: Script,
    type_script: Option<Script>,
}

impl Cell {

    pub fn with_data(data: impl Into<Bytes>) -> Self {
        Self {
            data: data.into(),
            outpoint: None,
            capacity: Capacity::zero(),
            lock_script: Script::default(),
            type_script: None,
        }
    }
    /// Ensure the total cell size <= min required capacity
    /// Ensure that the capacity in the cell >= min required capacity
    pub fn validate(&self) -> CellResult<Capacity> {
       let type_script_size = match &self.type_script {
           Some(script) => {
               script.validate()?;
               script.required_capacity()?
           },
           None => Capacity::zero()
       };
       self.lock_script.validate()?;
       let lock_script_size = self.lock_script.required_capacity()?;
       let other_fields_size = self.data.required_capacity()?;
       let capacity_field_req = Capacity::bytes(8)?;
       let total_size = 
            type_script_size
            .safe_add(lock_script_size)?
            .safe_add(other_fields_size)?
            .safe_add(capacity_field_req)?;
        if self.capacity < total_size {
            Err(CellError::CapacityNotEnough)
        } else {
            Ok(total_size)
        }
    }

    pub fn lock_hash(&self) -> CellResult<H256> {
        self.lock_script.validate()
            .map_err(|e| CellError::ScriptError(e))
    }

    pub fn type_hash(&self) -> CellResult<Option<H256>> {
        if let Some(script) = &self.type_script {
            script.validate()
                .map_err(|e| CellError::ScriptError(e))
                .map(|hash| Some(hash))
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
    
    pub fn as_cell_dep(&self, dep_type: DepType) -> CellResult<CellDep> {
        todo!()
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
            .type_(cell
                    .type_script.map(|script| ckb_types::packed::Script::from(script))
                    .pack()
            )
            .build()
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