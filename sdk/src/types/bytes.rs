use ckb_types::prelude::*;
use ckb_types::bytes::Bytes as CkBytes;
use ckb_types::packed::Bytes as PackedBytes;
use ckb_jsonrpc_types::JsonBytes;
use ckb_types::{H256, core::{Capacity, CapacityError}};
use ckb_hash::blake2b_256;
use crate::contract::schema::{SchemaPrimitiveType, BytesConversion};
use thiserror::Error;
// use molecule::bytes::Bytes as MolBytes; // This is equivalent to CkBytes when in std mode

#[derive(Debug, Error)]
pub enum BytesError {
    #[error(transparent)]
    CapacityCalcError(#[from] CapacityError),
}

pub type BytesResult<T> = Result<T, BytesError>;

#[derive(Clone, Debug, Default)]
pub struct Bytes(pub (crate) JsonBytes);


impl Bytes {
   pub fn hash_256(&self) -> H256 {
        let raw_bytes: CkBytes = self.clone().into();
        H256(blake2b_256(raw_bytes))
    }

    pub fn len(&self) -> usize {
        let packed = PackedBytes::from(self.clone());
        packed.len()
    }

    pub fn required_capacity(&self) -> BytesResult<Capacity> {
        Capacity::bytes(self.len())
            .map_err(|e| BytesError::CapacityCalcError(e))
    }
}

impl From<CkBytes> for Bytes {
    fn from(ckbytes: CkBytes) -> Self {
        Self(ckbytes.pack().into())
    }
}

impl From<PackedBytes> for Bytes {
    fn from(packed_bytes: PackedBytes) -> Self {
        Self(packed_bytes.into())
    }
}


impl From<JsonBytes> for Bytes {
    fn from(json_bytes: JsonBytes) -> Self {
        Self(json_bytes)
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(bytes: Vec<u8>) -> Self {
        let bytes = bytes.pack();
        bytes.into()
    }
}

impl From<&[u8]> for Bytes {
    fn from(slice: &[u8]) -> Self {
        slice.pack().into()
    }
}

// impl<T: AsRef<[u8]>> From<T> for Bytes {
//     fn from(bytes: T) -> Self {
//         let bytes = bytes.as_ref();
//         let bytes = bytes.pack();
//         Self(bytes.into())
//     }
// }

impl From<Bytes> for CkBytes {
    fn from(b: Bytes) -> Self {
        b.0.into_bytes()
    }
}

impl From<Bytes> for PackedBytes {
    fn from(b: Bytes) -> Self {
        b.0.into_bytes().pack()
    }
}

impl From<Bytes> for JsonBytes {
    fn from(b: Bytes) -> Self {
        b.0
    }
}

impl From<&Bytes> for CkBytes {
    fn from(b: &Bytes) -> Self {
        b.0.clone().into_bytes()
    }
}

impl From<&Bytes> for PackedBytes {
    fn from(b: &Bytes) -> Self {
        b.0.clone().into_bytes().pack()
    }
}

impl From<&Bytes> for JsonBytes {
    fn from(b: &Bytes) -> Self {
        b.0.clone()
    }
}

impl<T, M> From<SchemaPrimitiveType<T,M>> for Bytes 
where
    M: Entity + Unpack<T>,
    T: Pack<M>,
{
    fn from(schema_obj: SchemaPrimitiveType<T,M>) -> Self {
        schema_obj.to_bytes().into()
    }
}