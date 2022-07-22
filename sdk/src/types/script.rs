use std::prelude::v1::*;

#[cfg(all(feature = "std", not(feature = "script")))]
mod script_error {
    use super::*;
    use crate::ckb_types::{core::CapacityError, H256};
    use thiserror::Error;
    #[derive(Debug, Error)]
    pub enum ScriptError {
        #[error(transparent)]
        ScriptCapacityError(#[from] CapacityError),
        #[error("Calculated script hash {0} does not match stored script hash {1}")]
        MismatchedScriptHash(H256, H256),
    }
    pub type ScriptResult<T> = Result<T, ScriptError>;
}

#[cfg(feature = "script")]
mod script_error {
    use super::*;
    use crate::ckb_types::core::{Capacity, CapacityError, ScriptHashType};
    use crate::ckb_types::H256;

    use core::fmt;
    pub enum ScriptError {
        ScriptCapacityError(CapacityError),
        MismatchedScriptHash(H256, H256),
    }

    impl core::fmt::Debug for ScriptError {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "invalid first item to double")
        }
    }

    pub type ScriptResult<T> = Result<T, ScriptError>;
}

pub use script_error::*;
mod core_script {
    use super::*;
    pub const CODE_HASH_SIZE_BYTES: usize = 32;

    use crate::{bytes::Bytes, cell::Cell};

    use crate::ckb_hash::blake2b_256;
    use crate::ckb_types::core::{Capacity, ScriptHashType};
    use crate::ckb_types::packed::{Bytes as PackedBytes, Script as PackedScript};
    use crate::ckb_types::prelude::*;
    use crate::ckb_types::{bytes::Bytes as CkBytes, H256};

    #[cfg_attr(all(feature = "std", not(feature = "script")), derive(Debug))]
    #[derive(Clone)]
    pub struct Script {
        pub(crate) args: Bytes,
        pub(crate) code_hash: [u8; 32],
        pub(crate) hash_type: ScriptHashType,
    }

    impl Default for Script {
        fn default() -> Self {
            Self {
                args: Default::default(),
                code_hash: Default::default(),
                hash_type: ScriptHashType::Data,
            }
        }
    }
    impl Script {
        pub fn set_args(&mut self, args: impl Into<Bytes>) {
            self.args = args.into();
        }

        pub fn set_hash_type(&mut self, typ: impl Into<ScriptHashType>) {
            self.hash_type = typ.into();
        }
        pub fn set_code_hash(&mut self, code_hash: impl Into<[u8; 32]>) {
            self.code_hash = code_hash.into();
        }
        pub fn size_bytes(&self) -> usize {
            // Args bytes size + code_hash + hash_type (which is one byte)
            // script_hash is not included in this calculation since it is not present
            // in on-chain script structure.
            self.args.len() + CODE_HASH_SIZE_BYTES + 1
        }

        #[cfg(feature = "script")]
        pub fn calc_script_hash(&self) -> H256 {
            let packed: PackedScript = self.clone().into();
            let packed = packed.as_reader().as_slice().to_vec();
            blake2b_256(CkBytes::from(packed)).into()
        }

        /// Validate that script hash is correct
        pub fn validate(&self) -> ScriptResult<H256> {
            let packed: PackedScript = self.clone().into();
            let packed = packed.as_reader().as_slice().to_vec();
            let calc_hash = blake2b_256(CkBytes::from(packed)).into();

            if calc_hash != self.calc_script_hash() {
                Err(ScriptError::MismatchedScriptHash(
                    calc_hash,
                    self.calc_script_hash(),
                ))
            } else {
                Ok(calc_hash)
            }
        }
        pub fn required_capacity(&self) -> ScriptResult<Capacity> {
            Capacity::bytes(self.size_bytes()).map_err(ScriptError::ScriptCapacityError)
        }
        pub fn code_hash(&self) -> H256 {
            self.code_hash.into()
        }

        pub fn hash_type_raw(&self) -> ScriptHashType {
            self.hash_type
        }

        pub fn args(&self) -> Bytes {
            self.args.clone()
        }

        pub fn args_raw(&self) -> CkBytes {
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

    impl From<PackedScript> for Script {
        fn from(s: PackedScript) -> Self {
            let reader = s.as_reader();
            let args = reader.args().to_entity();
            let hash_type = ScriptHashType::try_from(reader.hash_type().to_entity()).unwrap();
            let code_hash = reader.code_hash().to_entity().unpack().into();

            Self {
                args: args.into(),
                code_hash,
                hash_type,
            }
        }
    }

    impl From<Script> for PackedScript {
        fn from(s: Script) -> Self {
            let Script {
                code_hash,
                hash_type,
                args,
            } = s;

            PackedScript::new_builder()
                .args(args.into())
                .code_hash(code_hash.pack())
                .hash_type(hash_type.into())
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

    #[cfg(all(feature = "std", not(feature = "script")))]
    mod extended {
        use super::*;
        use ckb_jsonrpc_types::{
            JsonBytes, Script as JsonScript, ScriptHashType as JsonScriptHashType,
        };

        impl Script {
            pub fn hash_type_json(&self) -> JsonScriptHashType {
                self.hash_type.into()
            }

            pub fn args_json(&self) -> JsonBytes {
                self.args.clone().into()
            }

            pub fn calc_script_hash(&self) -> H256 {
                let packed: PackedScript = self.clone().into();
                packed.calc_script_hash().unpack()
            }
        }

        impl From<Script> for JsonScript {
            fn from(s: Script) -> Self {
                let Script {
                    code_hash,
                    hash_type,
                    args,
                } = s;
                JsonScript {
                    code_hash: code_hash.into(),
                    hash_type: hash_type.into(),
                    args: args.into(),
                }
            }
        }

        impl From<JsonScript> for Script {
            fn from(j: JsonScript) -> Self {
                let hash_type = j.hash_type.clone().into();
                let code_hash = j.code_hash.clone().into();
                let args = j.args.into();

                Self {
                    args,
                    code_hash,
                    hash_type,
                }
            }
        }
    }

    #[cfg(all(feature = "std", not(feature = "script")))]
    pub use extended::*;
}

pub use self::core_script::*;
