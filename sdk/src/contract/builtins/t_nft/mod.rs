use std::prelude::v1::*;

pub mod mol_defs;
use crate::ckb_types::{bytes::Bytes, prelude::*};

use crate::contract::Contract;
use crate::{
    contract::schema::SchemaPrimitiveType,
    contract::schema::{BytesConversion, JsonByteConversion, JsonBytes, MolConversion},
    impl_entity_unpack, impl_pack_for_fixed_byte_array, impl_primitive_reader_unpack,
};

pub use mol_defs::*;

pub trait NftContentHasher {
    fn hash(content: impl AsRef<[u8]>) -> mol_defs::Byte32;
}

impl_pack_for_fixed_byte_array!([u8; 32], Byte32);
impl_primitive_reader_unpack!([u8; 32], Byte32Reader, 32, from);
impl_entity_unpack!([u8; 32], Byte32);

pub type GenesisId = SchemaPrimitiveType<[u8; 32], Byte32>;
pub type ContentId = SchemaPrimitiveType<[u8; 32], Byte32>;

#[derive(Debug, Clone, Default)]
pub struct TrampolineNFT {
    pub genesis_id: GenesisId,
    pub cid: ContentId,
}

impl BytesConversion for TrampolineNFT {
    fn from_bytes(bytes: Bytes) -> Self {
        let nft_mol = NFT::from_compatible_slice(&bytes.to_vec()).unwrap();
        Self {
            genesis_id: GenesisId::new(nft_mol.genesis_id().unpack()),
            cid: ContentId::new(nft_mol.content_id().unpack()),
        }
    }

    fn to_bytes(&self) -> Bytes {
        NFTBuilder::default()
            .content_id(self.cid.to_mol())
            .genesis_id(self.genesis_id.to_mol())
            .build()
            .as_bytes()
    }
}

impl JsonByteConversion for TrampolineNFT {
    fn to_json_bytes(&self) -> JsonBytes {
        todo!()
    }

    fn from_json_bytes(_bytes: JsonBytes) -> Self {
        todo!()
    }
}

impl MolConversion for TrampolineNFT {
    type MolType = NFT;

    fn to_mol(&self) -> Self::MolType {
        NFTBuilder::default()
            .content_id(self.cid.inner.pack())
            .genesis_id(self.genesis_id.inner.pack())
            .build()
    }

    fn from_mol(entity: Self::MolType) -> Self {
        Self {
            genesis_id: GenesisId::new(entity.genesis_id().unpack()),
            cid: ContentId::new(entity.content_id().unpack()),
        }
    }
}

pub type TrampolineNFTContract =
    Contract<SchemaPrimitiveType<Bytes, ckb_types::packed::Bytes>, TrampolineNFT>;
