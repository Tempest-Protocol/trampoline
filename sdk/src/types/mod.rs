use std::prelude::v1::*;

use crate::ckb_types::bytes::Bytes as CkBytes;
use crate::ckb_types::core::Capacity;
use crate::ckb_types::packed::Bytes as PackedBytes;

pub mod bytes;
pub mod cell;
pub mod constants;
pub mod script;

#[cfg(all(feature = "std", not(feature = "script")))]
pub mod address;
#[cfg(all(feature = "std", not(feature = "script")))]
pub mod query;
#[cfg(all(feature = "std", not(feature = "script")))]
pub mod transaction;

// TO DO: Implement this trait for all types

pub trait TrampolineBaseType: Into<CkBytes> + Into<PackedBytes> {
    type Error;
    fn validate(&self) -> Result<(), Self::Error>;

    fn required_capacity(&self) -> Result<Capacity, Self::Error>;

    fn size_bytes(&self) -> Result<usize, Self::Error>;
}

#[cfg(test)]
mod tests {

    use super::*;
    use ckb_always_success_script::ALWAYS_SUCCESS;
    use ckb_hash::blake2b_256;
    use ckb_types::core::ScriptHashType;
    use ckb_types::packed::{CellOutput, Script};
    use ckb_types::prelude::*;
    use ckb_types::H256;

    #[test]
    fn test_cell_data_hash() {
        let first = blake2b_256(ALWAYS_SUCCESS);
        let cell = cell::Cell::with_data(ALWAYS_SUCCESS.as_ref());
        let cell_calc_hash = cell.data_hash();
        let first_calc_hash: H256 = first.into();
        let cell_output_calc_hash: H256 = CellOutput::calc_data_hash(ALWAYS_SUCCESS).unpack();
        assert_eq!(cell_calc_hash, first_calc_hash);
        assert_eq!(cell_calc_hash, cell_output_calc_hash);
    }

    #[test]
    fn test_cell_lock_hash() {
        let packed_script = Script::new_builder()
            .args(vec![0].pack())
            .code_hash(blake2b_256(ALWAYS_SUCCESS).pack())
            .hash_type(ScriptHashType::Data1.into())
            .build();
        let hash_1: H256 = packed_script.calc_script_hash().unpack();
        let mut cell_with_lock = cell::Cell::default();
        assert!(cell_with_lock.set_lock_script(packed_script).is_ok());
        let hash_2 = cell_with_lock.lock_hash().unwrap();
        assert_eq!(hash_1, hash_2);
    }

    // #[test]
    // fn validate_fails_before_set_due_to_capacity() {
    //     let cell = Cell::with_data(ALWAYS_SUCCESS.as_ref());
    //     let validate_res = cell.validate();
    //     match validate_res {
    //         Ok(_) => assert!(false, "Cell validate returned Ok when it shouldn't"),
    //         Err(e) => {
    //             match e {
    //                 CellError::CapacityNotEnough => assert!(true),
    //                 _ => assert!(false, "CellError does not match! Got {:?}--Expected {:?}", e, CellError::CapacityNotEnough)
    //             }
    //         }
    //     };

    // }
}
