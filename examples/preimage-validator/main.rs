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
    // Check that the transaction is signed by the public key hash
    let signed: bool = list::contains(script_ctx.tx_info.signatories, d.pkh);
    rustus_prelude::require!(signed, "Not signed");
    // Check that the preimage hashes to the expected hash
    let computed_hash: Vec<u8> = builtins::sha2_256(&preimage);
    rustus_prelude::require!(computed_hash == d.hash, "Wrong preimage");
}

fn main() {
    let module = rustus_core::registry::build_module("preimage_validator");

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
    std::fs::write("preimage_validator.sir.json", &json).unwrap();

    // Compile to UPLC via JVM
    println!("\n--- JVM Compilation ---");
    match rustus::compile_module("preimage_validator") {
        Ok(validator) => {
            println!("Flat bytes: {} bytes", validator.to_flat().unwrap().len());
            let hash = validator.hash().unwrap();
            println!(
                "Script hash: {}",
                hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
            );

            // Test: correct preimage + correct signer → success
            use rustus_core::data::ToData;
            use rustus_prelude::ledger::v1::*;
            use rustus_prelude::list::List;

            let secret = b"my secret preimage";
            let expected_hash = builtins::sha2_256(secret);
            let pkh = PubKeyHash { hash: vec![0xaa, 0xbb, 0xcc] };

            let datum = PreimageDatum {
                hash: expected_hash.clone(),
                pkh: pkh.clone(),
            }.to_data();
            let redeemer = secret.to_vec().to_data(); // ByteString

            let ctx = ScriptContext {
                tx_info: TxInfo {
                    inputs: List::Nil, outputs: List::Nil,
                    fee: Value { inner: Data::Map { values: vec![] } },
                    mint: Value { inner: Data::Map { values: vec![] } },
                    dcert: List::Nil, withdrawals: List::Nil,
                    valid_range: Interval::always(),
                    signatories: List::from_vec(vec![pkh.clone()]),
                    data: List::Nil,
                    id: TxId { hash: vec![0x00] },
                },
                purpose: ScriptPurpose::Spending {
                    tx_out_ref: TxOutRef { id: TxId { hash: vec![0x00] }, idx: 0.into() },
                },
            }.to_data();

            println!("\n--- CEK Eval (correct preimage + signer) ---");
            let result = validator.eval(&[datum.clone(), redeemer.clone(), ctx]).unwrap();
            println!("Success: {}", result.success);
            println!("CPU: {}, MEM: {}", result.cpu, result.mem);
            if let Some(err) = &result.error {
                println!("Error: {err}");
            }
            for log in &result.logs {
                println!("Log: {log}");
            }

            // Test: wrong preimage → should fail
            let wrong_redeemer = b"wrong preimage".to_vec().to_data();
            let ctx2 = ScriptContext {
                tx_info: TxInfo {
                    inputs: List::Nil, outputs: List::Nil,
                    fee: Value { inner: Data::Map { values: vec![] } },
                    mint: Value { inner: Data::Map { values: vec![] } },
                    dcert: List::Nil, withdrawals: List::Nil,
                    valid_range: Interval::always(),
                    signatories: List::from_vec(vec![pkh]),
                    data: List::Nil,
                    id: TxId { hash: vec![0x00] },
                },
                purpose: ScriptPurpose::Spending {
                    tx_out_ref: TxOutRef { id: TxId { hash: vec![0x00] }, idx: 0.into() },
                },
            }.to_data();

            println!("\n--- CEK Eval (wrong preimage) ---");
            let result = validator.eval(&[datum, wrong_redeemer, ctx2]).unwrap();
            println!("Success: {}", result.success);
            if let Some(err) = &result.error {
                println!("Error: {err}");
            }
            for log in &result.logs {
                println!("Log: {log}");
            }
        }
        Err(e) => eprintln!("JVM compilation skipped: {e}"),
    }
}
