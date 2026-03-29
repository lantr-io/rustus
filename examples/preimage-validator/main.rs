//! Preimage Validator — analog of scalus PreimageValidator.
//!
//! Validates that a preimage hashes to a given hash and that
//! the transaction is signed by a specific public key hash.

use rustus_core::data::{Data, FromData};
use rustus_prelude::builtins;
use rustus_prelude::ledger::v1::{PubKeyHash, ScriptContext};
use rustus_prelude::list;

/// The datum: expected hash + authorized public key hash.
#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
pub struct PreimageDatum {
    pub hash: Vec<u8>,
    pub pkh: PubKeyHash,
}

/// V1 spending validator: check preimage + signatory.
#[rustus::compile]
fn preimage_validator(datum: Data, redeemer: Data, ctx: Data) {
    let d: PreimageDatum = FromData::from_data(&datum).unwrap();
    let preimage: Vec<u8> = FromData::from_data(&redeemer).unwrap();
    let script_ctx: ScriptContext = FromData::from_data(&ctx).unwrap();
    let signed: bool = list::contains(script_ctx.tx_info.signatories, d.pkh);
    rustus_prelude::require!(signed, "Not signed");
    let computed_hash: Vec<u8> = builtins::sha2_256(&preimage);
    rustus_prelude::require!(computed_hash == d.hash, "Wrong preimage");
}

fn main() {
    match rustus::compile_module("preimage_validator") {
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
