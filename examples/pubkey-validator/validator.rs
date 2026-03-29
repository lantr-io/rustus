use rustus_core::data::{Data, FromData};
use rustus_prelude::ledger::v1::{PubKeyHash, ScriptContext};
use rustus_prelude::list;

/// The datum: owner's public key hash.
#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
pub struct OwnerDatum {
    pub owner: PubKeyHash,
}

/// V1 spending validator: all args are Data, conversions inside.
#[rustus::compile]
pub fn pubkey_validator(datum: Data, _redeemer: Data, ctx: Data) {
    let owner_datum: OwnerDatum = FromData::from_data(&datum).unwrap();
    let script_ctx: ScriptContext = FromData::from_data(&ctx).unwrap();
    let signed: bool = list::contains(script_ctx.tx_info.signatories, owner_datum.owner);
    rustus_prelude::require!(signed, "Not signed by owner")
}
