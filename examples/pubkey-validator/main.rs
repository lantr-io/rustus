//! PubKey Validator — analog of scalus PubKeyValidator.
//!
//! V1 style: 3 Data arguments, explicit fromData conversions.

use rustus_core::data::{Data, FromData, ToData};
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
    let signed: bool = list::contains(script_ctx.tx_info.signatories, owner_datum.owner.to_data());
    rustus_prelude::require!(signed, "Not signed by owner")
}

fn main() {
    let module = rustus_core::registry::build_module("pubkey_validator");

    println!("Module: {}", module.name);
    println!("\nBindings:");
    for b in &module.defs {
        let prefix = b.module_name.as_ref().map(|m| format!("{}/", m)).unwrap_or_default();
        println!("  {}{}", prefix, b.name);
    }

    println!("\nTyping:");
    for b in &module.defs {
        match rustus_core::typing::verify_complete(&b.value) {
            Ok(()) => println!("  {}: OK", b.name),
            Err(errors) => {
                println!("  {}: {} error(s)", b.name, errors.len());
                for e in &errors {
                    println!("    {}", e);
                }
            }
        }
    }

    let json = serde_json::to_string_pretty(&module).unwrap();
    std::fs::write("pubkey_validator.sir.json", &json).unwrap();
    println!("\nWrote pubkey_validator.sir.json ({} bytes)", json.len());
}
