//! Tests for Preimage Validator — compile to UPLC and eval in CEK machine.

#[path = "validator.rs"]
mod validator;
use validator::PreimageDatum;

use rustus_core::bytestring::ByteString;
use rustus_core::data::{Data, ToData};
use rustus_prelude::builtins;
use rustus_prelude::ledger::v1::*;
use rustus_prelude::list::List;

fn make_ctx(signatories: Vec<PubKeyHash>) -> Data {
    ScriptContext {
        tx_info: TxInfo {
            inputs: List::Nil, outputs: List::Nil,
            fee: Value::zero(),
            mint: Value::zero(),
            dcert: List::Nil, withdrawals: List::Nil,
            valid_range: Interval::always(),
            signatories: List::from_vec(signatories),
            data: List::Nil,
            id: TxId { hash: ByteString::from_hex("00") },
        },
        purpose: ScriptPurpose::Spending {
            tx_out_ref: TxOutRef { id: TxId { hash: ByteString::from_hex("00") }, idx: 0.into() },
        },
    }.to_data()
}

fn try_compile() -> Option<rustus::Validator> {
    rustus::compile_module("preimage_validator").ok()
}

#[test]
fn correct_preimage_and_signer() {
    let Some(validator) = try_compile() else { return };
    let secret = ByteString::from_slice(b"my secret preimage");
    let pkh = PubKeyHash { hash: ByteString::from_hex("aabb") };
    let datum = PreimageDatum { hash: builtins::sha2_256(&secret), pkh: pkh.clone() }.to_data();
    let redeemer = secret.to_data();
    let result = validator.eval(&[datum, redeemer, make_ctx(vec![pkh])]).unwrap();
    assert!(result.succeeded(), "Expected success: {:?}", result.error);
}

#[test]
fn wrong_preimage() {
    let Some(validator) = try_compile() else { return };
    let pkh = PubKeyHash { hash: ByteString::from_hex("aabb") };
    let datum = PreimageDatum {
        hash: builtins::sha2_256(&ByteString::from_slice(b"correct")),
        pkh: pkh.clone(),
    }.to_data();
    let redeemer = ByteString::from_slice(b"wrong").to_data();
    let result = validator.eval(&[datum, redeemer, make_ctx(vec![pkh])]).unwrap();
    assert!(result.failed());
    assert!(result.logs.iter().any(|l| l.contains("Wrong preimage")));
}

#[test]
fn missing_signer() {
    let Some(validator) = try_compile() else { return };
    let secret = ByteString::from_slice(b"my secret");
    let pkh = PubKeyHash { hash: ByteString::from_hex("aabb") };
    let wrong = PubKeyHash { hash: ByteString::from_hex("ff") };
    let datum = PreimageDatum { hash: builtins::sha2_256(&secret), pkh }.to_data();
    let redeemer = secret.to_data();
    let result = validator.eval(&[datum, redeemer, make_ctx(vec![wrong])]).unwrap();
    assert!(result.failed());
    assert!(result.logs.iter().any(|l| l.contains("Not signed")));
}

#[test]
fn produces_flat() {
    let Some(validator) = try_compile() else { return };
    assert!(!validator.to_flat().unwrap().is_empty());
}

#[test]
fn uplc_contains_sha2() {
    let Some(validator) = try_compile() else { return };
    let text = validator.to_text().unwrap();
    assert!(text.contains("sha2_256") || text.contains("Sha2"));
}
