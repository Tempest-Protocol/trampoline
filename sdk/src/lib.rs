#![no_std]
extern crate no_std_compat as std;

#[cfg(all(feature = "std", not(feature = "script")))]
pub mod account;

#[cfg(all(feature = "std", not(feature = "script")))]
pub mod chain;

#[cfg(all(feature = "std", not(feature = "script")))]
pub mod rpc;

pub mod contract;

pub(crate) mod types;

pub use types::{bytes, cell, constants, script};

#[cfg(all(feature = "std", not(feature = "script")))]
pub use types::{address, query, transaction};

#[cfg(feature = "script")]
#[macro_export]
macro_rules! impl_std_convert {
    ($name:ident, $bytes_size:expr) => {
        impl ::core::convert::AsRef<[u8]> for $name {
            #[inline]
            fn as_ref(&self) -> &[u8] {
                &self.0[..]
            }
        }
        impl ::core::convert::AsMut<[u8]> for $name {
            #[inline]
            fn as_mut(&mut self) -> &mut [u8] {
                &mut self.0[..]
            }
        }
        impl ::core::convert::From<[u8; $bytes_size]> for $name {
            #[inline]
            fn from(bytes: [u8; $bytes_size]) -> Self {
                $name(bytes)
            }
        }
        impl ::core::convert::From<$name> for [u8; $bytes_size] {
            #[inline]
            fn from(hash: $name) -> Self {
                hash.0
            }
        }
    };
}

#[cfg(feature = "script")]
pub mod ckb_hash {
    use crate::ckb_types::bytes::Bytes;
    use blake2b_ref::{blake2b, Blake2b, Blake2bBuilder};
    use std::prelude::v1::*;

    #[doc(hidden)]
    pub const BLAKE2B_KEY: &[u8] = &[];
    /// Output digest size.
    pub const BLAKE2B_LEN: usize = 32;
    /// Blake2b personalization.
    pub const CKB_HASH_PERSONALIZATION: &[u8] = b"ckb-default-hash";

    /// ```
    pub const BLANK_HASH: [u8; 32] = [
        68, 244, 198, 151, 68, 213, 248, 197, 93, 100, 32, 98, 148, 157, 202, 228, 155, 196, 231,
        239, 67, 211, 136, 197, 161, 47, 66, 181, 99, 61, 22, 62,
    ];

    pub fn new_blake2b() -> Blake2b {
        Blake2bBuilder::new(32)
            .personal(CKB_HASH_PERSONALIZATION)
            .build()
    }

    pub fn blake2b_256(s: Bytes) -> [u8; 32] {
        if s.is_empty() {
            return BLANK_HASH;
        }
        inner_blake2b_256(s)
    }

    fn inner_blake2b_256(s: Bytes) -> [u8; 32] {
        let s = s.to_vec();
        let mut result = [0u8; 32];
        let mut blake2b = new_blake2b();
        blake2b.update(&s);
        blake2b.finalize(&mut result);
        result
    }
}
#[cfg(all(feature = "std", not(feature = "script")))]
pub mod ckb_hash {
    pub use ckb_hash::*;
}
pub mod ckb_types {
    #[cfg(feature = "script")]
    pub use ckb_standalone_types::prelude::{Builder, Entity, Pack, PackVec, Reader, Unpack};
    #[cfg(feature = "script")]
    pub use ckb_standalone_types::{self, bytes, packed, prelude};

    #[cfg(feature = "script")]
    pub mod core {
        use super::prelude::*;
        use crate::impl_std_convert;
        use ckb_standalone_types::core as ckb_core;
        pub use ckb_standalone_types::packed::*;
        mod capacity {
            use core::fmt;
            use std::prelude::v1::*;

            /// CKB capacity.
            ///
            /// It is encoded as the amount of `Shannons` internally.
            #[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
            pub struct Capacity(u64);

            /// Represents the ratio `numerator / denominator`, where `numerator` and `denominator` are both
            /// unsigned 64-bit integers.
            #[derive(Clone, PartialEq, Debug, Eq, Copy)]
            pub struct Ratio {
                /// Numerator.
                numer: u64,
                /// Denominator.
                denom: u64,
            }

            impl Ratio {
                /// Creates a ratio numer / denom.
                pub const fn new(numer: u64, denom: u64) -> Self {
                    Self { numer, denom }
                }

                /// The numerator in ratio numerator / denominator.
                pub fn numer(&self) -> u64 {
                    self.numer
                }

                /// The denominator in ratio numerator / denominator.
                pub fn denom(&self) -> u64 {
                    self.denom
                }
            }

            /// Conversion into `Capacity`.
            pub trait IntoCapacity {
                /// Converts `self` into `Capacity`.
                fn into_capacity(self) -> Capacity;
            }

            impl IntoCapacity for Capacity {
                fn into_capacity(self) -> Capacity {
                    self
                }
            }

            impl IntoCapacity for u64 {
                fn into_capacity(self) -> Capacity {
                    Capacity::shannons(self)
                }
            }

            impl IntoCapacity for u32 {
                fn into_capacity(self) -> Capacity {
                    Capacity::shannons(u64::from(self))
                }
            }

            impl IntoCapacity for u16 {
                fn into_capacity(self) -> Capacity {
                    Capacity::shannons(u64::from(self))
                }
            }

            impl IntoCapacity for u8 {
                fn into_capacity(self) -> Capacity {
                    Capacity::shannons(u64::from(self))
                }
            }

            // A `Byte` contains how many `Shannons`.
            const BYTE_SHANNONS: u64 = 100_000_000;

            // impl ::std::fmt::Display for Error {
            //     fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            //         write!(f, "OccupiedCapacity: overflow")
            //     }
            // }

            // Should include the str contents in this likely
            // Overflow variant should probably have the values involved in the overflow
            pub enum CapacityError {
                Overflow,
                ParseInt,
            }

            impl fmt::Debug for CapacityError {
                fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    let err_str = match self {
                        CapacityError::Overflow => "Capacity Error: Overflow",
                        CapacityError::ParseInt => "Capacity Error: ParseInt",
                    };
                    write!(f, "{}", err_str)
                }
            }
            /// Numeric operation result.
            pub type CapacityResult<T> = core::result::Result<T, CapacityError>;

            impl Capacity {
                /// Capacity of zero Shannons.
                pub const fn zero() -> Self {
                    Capacity(0)
                }

                /// Capacity of one Shannon.
                pub const fn one() -> Self {
                    Capacity(1)
                }

                /// Views the capacity as Shannons.
                pub const fn shannons(val: u64) -> Self {
                    Capacity(val)
                }

                /// Views the capacity as CKBytes.
                pub fn bytes(val: usize) -> CapacityResult<Self> {
                    (val as u64)
                        .checked_mul(BYTE_SHANNONS)
                        .map(Capacity::shannons)
                        .ok_or(CapacityError::Overflow)
                }

                /// Views the capacity as Shannons.
                pub fn as_u64(self) -> u64 {
                    self.0
                }

                /// Adds self and rhs and checks overflow error.
                pub fn safe_add<C: IntoCapacity>(self, rhs: C) -> CapacityResult<Self> {
                    self.0
                        .checked_add(rhs.into_capacity().0)
                        .map(Capacity::shannons)
                        .ok_or(CapacityError::Overflow)
                }

                /// Subtracts self and rhs and checks overflow error.
                pub fn safe_sub<C: IntoCapacity>(self, rhs: C) -> CapacityResult<Self> {
                    self.0
                        .checked_sub(rhs.into_capacity().0)
                        .map(Capacity::shannons)
                        .ok_or(CapacityError::Overflow)
                }

                /// Multiplies self and rhs and checks overflow error.
                pub fn safe_mul<C: IntoCapacity>(self, rhs: C) -> CapacityResult<Self> {
                    self.0
                        .checked_mul(rhs.into_capacity().0)
                        .map(Capacity::shannons)
                        .ok_or(CapacityError::Overflow)
                }

                /// Multiplies self with a ratio and checks overflow error.
                pub fn safe_mul_ratio(self, ratio: Ratio) -> CapacityResult<Self> {
                    self.0
                        .checked_mul(ratio.numer())
                        .and_then(|ret| ret.checked_div(ratio.denom()))
                        .map(Capacity::shannons)
                        .ok_or(CapacityError::Overflow)
                }
            }

            impl std::str::FromStr for Capacity {
                type Err = CapacityError;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    Ok(Capacity(
                        s.parse::<u64>().map_err(|e| CapacityError::ParseInt)?,
                    ))
                }
            }

            // impl ::std::fmt::Display for Capacity {
            //     fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            //         self.0.fmt(f)
            //     }
            // }

            // impl ::std::fmt::LowerHex for Capacity {
            //     fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            //         self.0.fmt(f)
            //     }
            // }
        }
        pub use capacity::*;

        #[derive(Clone, Copy, PartialEq, Eq)]
        pub enum ScriptHashType {
            Data = 0,
            Type = 1,
            Data1 = 2,
        }

        impl From<ScriptHashType> for u8 {
            fn from(s: ScriptHashType) -> Self {
                s as u8
            }
        }

        impl From<u8> for ScriptHashType {
            fn from(b: u8) -> Self {
                match b {
                    0 => ScriptHashType::Data,
                    1 => ScriptHashType::Type,
                    2 => ScriptHashType::Data1,
                    _ => ScriptHashType::Data,
                }
            }
        }

        impl From<ScriptHashType> for ckb_core::ScriptHashType {
            fn from(s: ScriptHashType) -> Self {
                match s {
                    ScriptHashType::Data => ckb_core::ScriptHashType::Data,
                    ScriptHashType::Type => ckb_core::ScriptHashType::Type,
                    ScriptHashType::Data1 => ckb_core::ScriptHashType::Data1,
                }
            }
        }

        impl From<ckb_core::ScriptHashType> for ScriptHashType {
            fn from(s: ckb_core::ScriptHashType) -> Self {
                match s {
                    ckb_core::ScriptHashType::Data => ScriptHashType::Data,
                    ckb_core::ScriptHashType::Type => ScriptHashType::Type,
                    ckb_core::ScriptHashType::Data1 => ScriptHashType::Data1,
                }
            }
        }

        impl From<molecule::prelude::Byte> for ScriptHashType {
            fn from(b: molecule::prelude::Byte) -> Self {
                let b: u8 = b.into();
                Self::from(b)
            }
        }

        impl From<ScriptHashType> for molecule::prelude::Byte {
            fn from(s: ScriptHashType) -> molecule::prelude::Byte {
                let b: u8 = s.into();
                Byte::new(b)
            }
        }

        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        pub enum DepType {
            Code = 0,
            DepGroup = 1,
        }

        impl Default for DepType {
            fn default() -> Self {
                DepType::Code
            }
        }

        impl TryFrom<super::packed::Byte> for DepType {
            type Error = ();

            fn try_from(v: super::packed::Byte) -> Result<Self, Self::Error> {
                match Into::<u8>::into(v) {
                    0 => Ok(DepType::Code),
                    1 => Ok(DepType::DepGroup),
                    _ => core::prelude::v1::Err(()),
                }
            }
        }

        impl Into<u8> for DepType {
            #[inline]
            fn into(self) -> u8 {
                self as u8
            }
        }

        impl Into<super::packed::Byte> for DepType {
            #[inline]
            fn into(self) -> super::packed::Byte {
                (self as u8).into()
            }
        }

        impl DepType {
            #[inline]
            pub(crate) fn verify_value(v: u8) -> bool {
                v <= 1
            }
        }
    }

    #[cfg(feature = "script")]
    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub struct H160(pub [u8; 20]);

    /// The 32-byte fixed-length binary data.
    ///
    /// The name comes from the number of bits in the data.
    ///
    /// In JSONRPC, it is encoded as a 0x-prefixed hex string.
    #[cfg(feature = "script")]
    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub struct H256(pub [u8; 32]);

    #[cfg(feature = "script")]
    impl_std_convert!(H160, 20);

    #[cfg(feature = "script")]
    impl_std_convert!(H256, 32);

    #[cfg(all(feature = "std", not(feature = "script")))]
    pub use ckb_types::*;
}

pub mod precompiled {
    pub const SUDT: &[u8] = include_bytes!("../binaries/simple_udt");
    pub const TNFT: &[u8] = include_bytes!("../binaries/trampoline-nft");
}

// From ckb_types::conversion
#[macro_export]
macro_rules! impl_entity_unpack {
    ($original:ty, $entity:ident) => {
        impl Unpack<$original> for $entity {
            fn unpack(&self) -> $original {
                self.as_reader().unpack()
            }
        }
    };
}
#[macro_export]
macro_rules! impl_primitive_reader_unpack {
    ($original:ty, $entity:ident, $size:literal, $byte_method:ident) => {
        impl Unpack<$original> for $entity<'_> {
            fn unpack(&self) -> $original {
                let mut b = [0u8; $size];
                b.copy_from_slice(self.as_slice());
                <$original>::$byte_method(b)
            }
        }
    };
    ($original:ty, $entity:ident, $size:literal) => {
        impl Unpack<$original> for $entity<'_> {
            fn unpack(&self) -> $original {
                let mut b = [0u8; $size];
                b.copy_from_slice(self.as_slice());
                <$original>::from_le_bytes(b)
            }
        }
    };
}
#[macro_export]
macro_rules! impl_pack_for_primitive {
    ($native_type:ty, $entity:ident) => {
        impl Pack<$entity> for $native_type {
            fn pack(&self) -> $entity {
                $entity::new_unchecked(Bytes::from(self.to_le_bytes().to_vec()))
            }
        }
    };
}

#[macro_export]
macro_rules! impl_pack_for_fixed_byte_array {
    ($native_type:ty, $entity:ident) => {
        impl Pack<$entity> for $native_type {
            fn pack(&self) -> $entity {
                $entity::new_unchecked(Bytes::from(self.to_vec()))
            }
        }
    };
}
