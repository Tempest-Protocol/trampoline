use std::prelude::v1::*;

// When in no-std mode, both CKBytes and PackedBytes are the same
use crate::ckb_types::core::CapacityError;

// use molecule::bytes::Bytes as MolBytes; // This is equivalent to CkBytes when in std mode
// Molecule bytes are newtype around Vec<u8> when no_std is enabled and is bytes::Bytes when std is enabled
// CkBytes is equivalent to molecule bytes when "script" feature is enabled because ckb_types will refer to ckb_standalone_types::bytes::Bytes which
// is a re-export of molecule::bytes::Bytes
// packed::Bytes is a newtype around molecule::bytes::Bytes in both std and no_std mode

// In summary:
// When in no_std:
//    Molecule::bytes::Bytes   = Bytes(Vec<u8>)
//    ckb_types::packed::Bytes = Bytes(Molecule::bytes::Bytes)
//    ckb_types::bytes::Bytes  = molecule::bytes::Bytes

// When in std:
//    Molecule::bytes::Bytes = bytes::Bytes (an efficient byte implementation)
//    ckb_types::packed::Bytes = Bytes(Molecule::bytes::Bytes)
//    ckb_types::bytes::Bytes = bytes::Bytes = Molecule::bytes::Bytes

#[cfg(all(feature = "std", not(feature = "script")))]
pub mod bytes_error {
    use super::*;
    use thiserror::Error;
    #[derive(Debug, Error)]
    pub enum BytesError {
        #[error(transparent)]
        CapacityCalcError(#[from] CapacityError),
    }
}

#[cfg(feature = "script")]
pub mod bytes_error {
    use super::*;
    use core::fmt;
    pub enum BytesError {
        CapacityCalcError(CapacityError),
    }

    impl From<CapacityError> for BytesError {
        fn from(e: CapacityError) -> Self {
            Self::CapacityCalcError(e)
        }
    }

    impl fmt::Debug for BytesError {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let err_str = match self {
                BytesError::CapacityCalcError(cap_err) => format!("{:?}", cap_err),
            };
            write!(f, "{}", err_str)
        }
    }
}

pub use bytes_error::*;
pub type BytesResult<T> = Result<T, BytesError>;

mod core_bytes {

    // When in no-std mode, both CKBytes and PackedBytes are the same
    use crate::ckb_hash::blake2b_256;
    use crate::ckb_types::bytes::Bytes as CkBytes;
    use crate::ckb_types::packed::Byte;
    use crate::ckb_types::packed::Bytes as PackedBytes;
    use crate::ckb_types::prelude::*;
    use crate::ckb_types::{core::Capacity, H256};
    use crate::contract::schema::{BytesConversion, SchemaPrimitiveType};

    use super::*;
    #[derive(Clone, Debug, Default)]
    pub struct Bytes(pub(crate) Vec<u8>);

    impl Bytes {
        pub fn hash_256(&self) -> H256 {
            let raw_bytes: CkBytes = self.clone().into();
            H256(blake2b_256(raw_bytes))
        }

        pub fn len(&self) -> usize {
            let packed = PackedBytes::from(self.clone());
            packed.len()
        }

        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }

        pub fn required_capacity(&self) -> BytesResult<Capacity> {
            Capacity::bytes(self.len()).map_err(BytesError::CapacityCalcError)
        }
    }

    impl From<CkBytes> for Bytes {
        fn from(ckbytes: CkBytes) -> Self {
            Self(ckbytes.to_vec())
        }
    }

    impl From<PackedBytes> for Bytes {
        fn from(packed_bytes: PackedBytes) -> Self {
            Self(packed_bytes.unpack())
        }
    }

    impl From<Vec<u8>> for Bytes {
        fn from(bytes: Vec<u8>) -> Self {
            Self(bytes)
        }
    }

    impl From<&[u8]> for Bytes {
        fn from(slice: &[u8]) -> Self {
            Self(slice.to_vec())
        }
    }

    impl From<Bytes> for Vec<u8> {
        fn from(b: Bytes) -> Vec<u8> {
            b.0
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
            CkBytes::from(b.0)
        }
    }

    impl From<Bytes> for PackedBytes {
        fn from(b: Bytes) -> Self {
            PackedBytes::new_builder()
                .extend(b.0.iter().map(|byte| Byte::new(*byte)))
                .build()
        }
    }

    impl From<&Bytes> for CkBytes {
        fn from(b: &Bytes) -> Self {
            b.0.clone().into()
        }
    }

    impl From<&Bytes> for PackedBytes {
        fn from(b: &Bytes) -> Self {
            b.clone().into()
        }
    }

    impl<T, M> From<SchemaPrimitiveType<T, M>> for Bytes
    where
        M: Entity + Unpack<T>,
        T: Pack<M>,
    {
        fn from(schema_obj: SchemaPrimitiveType<T, M>) -> Self {
            schema_obj.to_bytes().into()
        }
    }
}

#[cfg(all(feature = "std", not(feature = "script")))]
mod extended {
    use super::core_bytes::*;
    use ckb_jsonrpc_types::JsonBytes;
    impl From<JsonBytes> for Bytes {
        fn from(json_bytes: JsonBytes) -> Self {
            Self(json_bytes.into_bytes().to_vec())
        }
    }

    impl From<Bytes> for JsonBytes {
        fn from(b: Bytes) -> Self {
            JsonBytes::from_bytes(b.into())
        }
    }

    impl From<&Bytes> for JsonBytes {
        fn from(b: &Bytes) -> Self {
            b.clone().into()
        }
    }
}

pub use core_bytes::*;
#[cfg(all(feature = "std", not(feature = "script")))]
pub use extended::*;
