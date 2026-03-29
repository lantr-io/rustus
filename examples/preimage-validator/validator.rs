use rustus_core::bytestring::ByteString;
use rustus_core::data::{Data, FromData};
use rustus_prelude::builtins;
use rustus_prelude::ledger::v1::{PubKeyHash, ScriptContext};
use rustus_prelude::list;

/// The datum: expected hash + authorized public key hash.
#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
pub struct PreimageDatum {
    pub hash: ByteString,
    pub pkh: PubKeyHash,
}

/// V1 spending validator: check preimage + signatory.
#[rustus::compile]
pub fn preimage_validator(datum: Data, redeemer: Data, ctx: Data) {
    let d: PreimageDatum = FromData::from_data(&datum).unwrap();
    let preimage: ByteString = FromData::from_data(&redeemer).unwrap();
    let script_ctx: ScriptContext = FromData::from_data(&ctx).unwrap();
    let signed: bool = list::contains(script_ctx.tx_info.signatories, d.pkh);
    rustus_prelude::require!(signed, "Not signed");
    let computed_hash: ByteString = builtins::sha2_256(&preimage);
    rustus_prelude::require!(computed_hash == d.hash, "Wrong preimage");
}
