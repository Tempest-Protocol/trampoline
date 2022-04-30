

use ckb_types::prelude::*;
use ckb_types::{H256, bytes::Bytes as CkBytes};
use ckb_types::core::{
    ScriptHashType,
    Capacity, CapacityError
    
};
use super::bytes::Bytes;
use super::cell::Cell;
use super::constants::{CODE_HASH_SIZE_BYTES};
use ckb_types::packed::{
    Script as PackedScript, Bytes as PackedBytes
};
use ckb_jsonrpc_types::{
    Script as JsonScript, 
    ScriptHashType as JsonScriptHashType,

    JsonBytes
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error(transparent)]
    ScriptCapacityError(#[from] CapacityError),
    #[error("Calculated script hash {0} does not match stored script hash {1}")]
    MismatchedScriptHash(H256, H256),
}

pub type ScriptResult<T> = Result<T, ScriptError>;

#[derive(Debug, Clone, Default)]
pub struct Script {
    pub (crate) args: Bytes,
    pub (crate) code_hash: H256,
    pub (crate) hash_type: JsonScriptHashType,

}


impl Script {
 
    
    pub fn set_args(&mut self, args: impl Into<Bytes>) {
        self.args = args.into();
    }

    pub fn set_hash_type(&mut self, typ: JsonScriptHashType) {
        self.hash_type = typ;
    }
    pub fn set_code_hash(&mut self, code_hash: H256) {
        self.code_hash = code_hash;
    }
    pub fn size_bytes(&self) -> usize {
        // Args bytes size + code_hash + hash_type (which is one byte)
        // script_hash is not included in this calculation since it is not present
        // in on-chain script structure. 
        self.args.len() + CODE_HASH_SIZE_BYTES + 1
    }

    pub fn calc_script_hash(&self) -> H256 {
        let packed: PackedScript = self.clone().into();
        packed.calc_script_hash().unpack()

    }
    /// Validate that script hash is correct
    pub fn validate(&self) -> ScriptResult<H256> {
        let packed: PackedScript = self.clone().into();
        let calc_hash = packed.calc_script_hash().unpack();
        if calc_hash != self.calc_script_hash() {
            Err(ScriptError::MismatchedScriptHash(calc_hash, self.calc_script_hash()))
        } else {
            Ok(calc_hash)
        }
    }
    pub fn required_capacity(&self) -> ScriptResult<Capacity> {
        Capacity::bytes(self.size_bytes())
            .map_err(|e| ScriptError::ScriptCapacityError(e))
    }
    pub fn code_hash(&self) -> H256 {
        self.code_hash.clone()
    }

    pub fn hash_type_json(&self) -> JsonScriptHashType {
        self.hash_type.clone()
    }

    pub fn hash_type_raw(&self) -> ScriptHashType {
        self.hash_type.clone().into()
    }

    pub fn args(&self) -> Bytes {
        self.args.clone()
    }

    pub fn args_json(&self) -> JsonBytes {
        self.args.clone().into()
    }

    pub fn args_raw(&self) -> CkBytes{
        self.args.clone().into()
    }

    // PackedBytes is ckb_types::packed::Bytes which is a wrapper struct around molecule::bytes::Bytes.
    // molecule::bytes::Bytes is either a Bytes(Vec<u8>) wrapper struct (in no_std) OR
    // bytes::Bytes (from bytes crate) in std (even though bytes::Bytes is no_std compatible)
    // PackedBytes of course implemented ckb_types::packed::prelude::Entity
    pub fn args_packed(&self) -> PackedBytes {
        self.args.clone().into()
    }

    
}
impl From<JsonScript> for Script {
    fn from(j: JsonScript) -> Self {
       
        let hash_type = j.hash_type.clone();
        let code_hash = j.code_hash.clone();
        let args = j.args.clone().into();

      
        Self {
            args,
            code_hash: code_hash,
            hash_type,
     

        }
    }
}

impl From<PackedScript> for Script {
    fn from(s: PackedScript) -> Self {
        let reader = s.as_reader();
        let args = reader.args().to_entity();
        let hash_type = ScriptHashType::try_from(reader.hash_type().to_entity()).unwrap();
        let code_hash = reader.code_hash().to_entity().unpack();

        Self {
            args: args.into(),
            code_hash,
            hash_type: hash_type.into(),

        }
    }
}

impl From<Script> for JsonScript {
    fn from(s: Script) -> Self {
        let Script {code_hash, hash_type, args} = s;
        JsonScript {
            code_hash,
            hash_type,
            args: args.into(),
        }
    }
}

impl From<Script> for PackedScript {
    fn from(s: Script) -> Self {
        let Script {code_hash, hash_type, args} = s;

        PackedScript::new_builder()
            .args(args.into())
            .code_hash(code_hash.pack())
            .hash_type(ScriptHashType::from(hash_type).into())
            .build()
    }
}

impl From<Cell> for Script {
    fn from(c: Cell) -> Self {
        let mut s = Self::default();
        s.set_code_hash(c.data_hash());
   
        s
    }
}

impl From<&Cell> for Script {
    fn from(c: &Cell) -> Self {
        let mut s = Self::default();
        s.set_code_hash(c.data_hash());
        s
    }
}
