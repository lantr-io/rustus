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

    // Compile to UPLC via JVM (requires JAVA_HOME and uber-JAR)
    println!("\n--- JVM Compilation ---");
    match rustus::compile_module("pubkey_validator") {
        Ok(validator) => {
            println!(
                "Flat bytes: {} bytes",
                validator.to_flat().unwrap().len()
            );
            let hash = validator.hash().unwrap();
            println!(
                "Script hash: {}",
                hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
            );

            // Eval test using typed Rust structs
            use rustus_core::data::ToData;
            use rustus_prelude::ledger::v1::*;
            use rustus_prelude::list::List;
            use rustus_prelude::AssocMap;

            let owner_pkh = PubKeyHash { hash: vec![0xde, 0xad, 0xbe, 0xef] };
            let datum = OwnerDatum { owner: owner_pkh.clone() }.to_data();
            let redeemer = Data::unit();

            // Test AssocMap roundtrip
            let test_map: AssocMap<Data, Data> = AssocMap::from_vec(vec![
                (Data::B { value: vec![0x01] }, Data::I { value: 42.into() }),
                (Data::B { value: vec![0x02] }, Data::I { value: 99.into() }),
            ]);
            let map_data = test_map.to_data();
            println!("\nAssocMap to_data: {:?}", map_data);

            // Non-trivial Value: 2 ADA fee via AssocMap
            let lovelace_value = Value {
                inner: AssocMap::from_vec(vec![
                    (Data::B { value: vec![] }, AssocMap::from_vec(vec![
                        (Data::B { value: vec![] }, Data::I { value: 2_000_000.into() }),
                    ]).to_data()),
                ]).to_data(),
            };
            let empty_value = Value { inner: AssocMap::<Data, Data>::empty().to_data() };
            let interval = Interval::always();
            let purpose = ScriptPurpose::Spending {
                tx_out_ref: TxOutRef { id: TxId { hash: vec![0xbb] }, idx: 0.into() },
            };

            // Wrong signer — should fail
            let wrong_ctx = ScriptContext {
                tx_info: TxInfo {
                    inputs: List::Nil, outputs: List::Nil,
                    fee: lovelace_value.clone(), mint: empty_value.clone(),
                    dcert: List::Nil, withdrawals: List::Nil,
                    valid_range: interval.clone(),
                    signatories: List::from_vec(vec![PubKeyHash { hash: vec![0xca, 0xfe] }]),
                    data: List::Nil,
                    id: TxId { hash: vec![0xbb] },
                },
                purpose: purpose.clone(),
            }.to_data();

            println!("\n--- CEK Eval (wrong signer, should fail) ---");
            let result = validator.eval(&[datum.clone(), redeemer.clone(), wrong_ctx]).unwrap();
            println!("Success: {}", result.success);
            println!("CPU: {}, MEM: {}", result.cpu, result.mem);
            if let Some(err) = &result.error {
                println!("Error: {err}");
            }
            for log in &result.logs {
                println!("Log: {log}");
            }

            // Correct signer — should succeed
            let correct_ctx = ScriptContext {
                tx_info: TxInfo {
                    inputs: List::Nil, outputs: List::Nil,
                    fee: empty_value.clone(), mint: empty_value,
                    dcert: List::Nil, withdrawals: List::Nil,
                    valid_range: interval,
                    signatories: List::from_vec(vec![owner_pkh]),
                    data: List::Nil,
                    id: TxId { hash: vec![0xbb] },
                },
                purpose,
            }.to_data();

            println!("\n--- CEK Eval (correct signer, should succeed) ---");
            let result = validator.eval(&[datum, redeemer, correct_ctx]).unwrap();
            println!("Success: {}", result.success);
            println!("CPU: {}, MEM: {}", result.cpu, result.mem);
            if let Some(err) = &result.error {
                println!("Error: {err}");
            }
        }
        Err(e) => eprintln!("JVM compilation skipped: {e}"),
    }
}
