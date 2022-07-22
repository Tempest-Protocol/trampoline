use std::prelude::v1::*;

mod core_schema {
    use super::*;
    use crate::ckb_types::{bytes::Bytes, prelude::*};
    use core::cmp::{Eq, PartialEq};
    use core::marker::PhantomData;
    use no_std_compat::fmt::Debug;
    use no_std_compat::hash::Hash;

    #[derive(Clone, Debug, Default)]
    pub struct SchemaPrimitiveType<T, M> {
        pub inner: T,
        _entity_type: std::marker::PhantomData<M>,
    }

    impl<T, M> PartialEq for SchemaPrimitiveType<T, M>
    where
        T: PartialEq,
    {
        fn eq(&self, other: &Self) -> bool {
            self.inner == other.inner
        }
    }
    impl<T, M> Eq for SchemaPrimitiveType<T, M> where T: Eq {}

    impl<T, M> SchemaPrimitiveType<T, M>
    where
        M: Entity + Unpack<T>,
        T: Pack<M>,
    {
        pub fn from_slice(s: impl AsRef<[u8]>) -> Self {
            let s_1 = s.as_ref();
            let s_1 = s_1.to_vec();
            Self::from_bytes(Bytes::from(s_1))
        }
    }

    impl<T, M> Hash for SchemaPrimitiveType<T, M>
    where
        M: Entity + Unpack<T>,
        T: Pack<M> + Hash,
    {
        fn hash<H: no_std_compat::hash::Hasher>(&self, state: &mut H) {
            self.inner.hash(state);
            self._entity_type.hash(state);
        }
    }

    pub trait MolConversion {
        type MolType: Entity;

        fn to_mol(&self) -> Self::MolType;

        fn from_mol(entity: Self::MolType) -> Self;
    }

    pub trait BytesConversion: MolConversion {
        fn from_bytes(bytes: Bytes) -> Self;

        fn to_bytes(&self) -> Bytes;
    }

    impl<T, M> SchemaPrimitiveType<T, M>
    where
        M: Entity + Unpack<T>,
        T: Pack<M>,
    {
        pub fn byte_size(&self) -> usize {
            self.to_mol().as_builder().expected_length()
        }
    }

    impl<T, M> SchemaPrimitiveType<T, M> {
        pub fn new(inner: T) -> Self {
            Self {
                inner,
                _entity_type: std::marker::PhantomData::<M>,
            }
        }

        pub fn from(native_type: T) -> Self {
            SchemaPrimitiveType::new(native_type)
        }

        pub fn into(self) -> T {
            self.inner
        }
    }

    impl<T, M> MolConversion for SchemaPrimitiveType<T, M>
    where
        M: Entity + Unpack<T>,
        T: Pack<M>,
    {
        type MolType = M;
        fn to_mol(&self) -> Self::MolType {
            self.inner.pack()
        }

        fn from_mol(entity: Self::MolType) -> Self {
            Self {
                inner: entity.unpack(),
                _entity_type: std::marker::PhantomData::<M>,
            }
        }
    }

    impl<T, M> BytesConversion for SchemaPrimitiveType<T, M>
    where
        M: Entity + Unpack<T>,
        T: Pack<M>,
    {
        fn from_bytes(bytes: Bytes) -> Self {
            Self {
                inner: M::from_compatible_slice(bytes.as_ref())
                    .expect("Unable to build primitive type from bytes")
                    .unpack(),
                _entity_type: PhantomData::<M>,
            }
        }

        fn to_bytes(&self) -> Bytes {
            self.to_mol().as_bytes()
        }
    }

    pub trait TrampolineBaseSchema: BytesConversion + MolConversion {}

    impl<T, M> TrampolineBaseSchema for SchemaPrimitiveType<T, M>
    where
        M: Entity + Unpack<T>,
        T: Pack<M>,
    {
    }

    impl<T, M> From<crate::bytes::Bytes> for SchemaPrimitiveType<T, M>
    where
        M: Entity + Unpack<T>,
        T: Pack<M>,
    {
        fn from(b: crate::bytes::Bytes) -> Self {
            Self::from_bytes(b.into())
        }
    }
    impl<T, M> From<Bytes> for SchemaPrimitiveType<T, M>
    where
        M: Entity + Unpack<T>,
        T: Pack<M>,
    {
        fn from(b: Bytes) -> Self {
            Self::from_bytes(b)
        }
    }
}

pub use core_schema::*;
#[cfg(all(feature = "std", not(feature = "script")))]
mod extension {
    use super::core_schema::*;
    use crate::ckb_types::prelude::*;
    use ckb_jsonrpc_types::JsonBytes;
    pub trait JsonByteConversion {
        fn to_json_bytes(&self) -> JsonBytes;
        fn from_json_bytes(bytes: JsonBytes) -> Self;
    }

    pub trait JsonConversion {
        type JsonType;
        fn to_json(&self) -> Self::JsonType;

        fn from_json(json: Self::JsonType) -> Self;
    }

    impl<T, M> JsonByteConversion for SchemaPrimitiveType<T, M>
    where
        M: Entity + Unpack<T>,
        T: Pack<M>,
    {
        fn to_json_bytes(&self) -> JsonBytes {
            self.to_mol().as_bytes().pack().into()
        }

        fn from_json_bytes(bytes: JsonBytes) -> Self {
            Self::from_bytes(bytes.into_bytes())
        }
    }

    impl<T, M> JsonConversion for SchemaPrimitiveType<T, M>
    where
        M: Entity + Unpack<T>,
        T: Pack<M>,
    {
        type JsonType = JsonBytes;

        fn to_json(&self) -> Self::JsonType {
            self.to_json_bytes()
        }

        fn from_json(json: Self::JsonType) -> Self {
            Self::from_json_bytes(json)
        }
    }

    pub trait TrampolineSchema: TrampolineBaseSchema + JsonConversion + JsonByteConversion {}

    impl<T, M> TrampolineSchema for SchemaPrimitiveType<T, M>
    where
        M: Entity + Unpack<T>,
        T: Pack<M>,
    {
    }
}

#[cfg(all(feature = "std", not(feature = "script")))]
pub use extension::*;
