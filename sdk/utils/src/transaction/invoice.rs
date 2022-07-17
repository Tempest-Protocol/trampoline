use ckb_sdk::tx_builder::transfer::CapacityTransferBuilder;
use ckb_types::{
    bytes::Bytes,
    packed::CellOutput,
    prelude::{Builder, Entity, Pack},
};

use crate::{
    account::Account,
    lock::{Lock, SigHashAllLock},
};

/// Invoice is a simple TX builder generator
/// It can be used to create a TX with a single output
/// from an Account an amount of CKB
/// (Account, u64) -> Invoice
pub struct DefaultInvoice;

impl DefaultInvoice {
    pub fn new_tx_builder(account: &Account, amount: &u64) -> CapacityTransferBuilder {
        let lock = SigHashAllLock::from_account(account);

        let output = CellOutput::new_builder()
            .lock(lock.as_script())
            .capacity(amount.pack())
            .build();
        CapacityTransferBuilder::new(vec![(output, Bytes::default())])
    }
}
