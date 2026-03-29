//! PubKey Validator — analog of scalus PubKeyValidator.
//!
//! V1 style: 3 Data arguments, explicit fromData conversions.

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
fn pubkey_validator(datum: Data, _redeemer: Data, ctx: Data) {
    let owner_datum: OwnerDatum = FromData::from_data(&datum).unwrap();
    let script_ctx: ScriptContext = FromData::from_data(&ctx).unwrap();
    let signed: bool = list::contains(script_ctx.tx_info.signatories, owner_datum.owner);
    rustus_prelude::require!(signed, "Not signed by owner")
}

fn main() {
    match rustus::compile_module("pubkey_validator") {
        Ok(validator) => {
            println!("{}", validator.to_text().unwrap());
            println!("\nFlat: {} bytes", validator.to_flat().unwrap().len());
            let hash = validator.hash().unwrap();
            println!(
                "Script hash: {}",
                hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
            );
        }
        Err(e) => eprintln!("Error: {e}"),
    }
}
