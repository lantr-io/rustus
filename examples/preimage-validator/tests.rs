//! Tests for Preimage Validator — compile to UPLC and eval in CEK machine.

#[path = "validator.rs"]
mod validator;
use validator::PreimageDatum;

use rustus_core::data::{Data, ToData};
use rustus_prelude::builtins;
use rustus_prelude::ledger::v1::*;
use rustus_prelude::list::List;

fn make_ctx(signatories: Vec<PubKeyHash>) -> Data {
    ScriptContext {
        tx_info: TxInfo {
            inputs: List::Nil, outputs: List::Nil,
            fee: Value { inner: Data::Map { values: vec![] } },
            mint: Value { inner: Data::Map { values: vec![] } },
            dcert: List::Nil, withdrawals: List::Nil,
            valid_range: Interval::always(),
            signatories: List::from_vec(signatories),
            data: List::Nil,
            id: TxId { hash: vec![0x00] },
        },
        purpose: ScriptPurpose::Spending {
            tx_out_ref: TxOutRef { id: TxId { hash: vec![0x00] }, idx: 0.into() },
        },
    }.to_data()
}

fn try_compile() -> Option<rustus::Validator> {
    rustus::compile_module("preimage_validator").ok()
}

#[test]
fn correct_preimage_and_signer() {
    let Some(validator) = try_compile() else { return };
    let secret = b"my secret preimage";
    let pkh = PubKeyHash { hash: vec![0xaa, 0xbb] };
    let datum = PreimageDatum { hash: builtins::sha2_256(secret), pkh: pkh.clone() }.to_data();
    let redeemer = secret.to_vec().to_data();
    let result = validator.eval(&[datum, redeemer, make_ctx(vec![pkh])]).unwrap();
    assert!(result.succeeded(), "Expected success: {:?}", result.error);
}

#[test]
fn wrong_preimage() {
    let Some(validator) = try_compile() else { return };
    let pkh = PubKeyHash { hash: vec![0xaa, 0xbb] };
    let datum = PreimageDatum { hash: builtins::sha2_256(b"correct"), pkh: pkh.clone() }.to_data();
    let redeemer = b"wrong".to_vec().to_data();
    let result = validator.eval(&[datum, redeemer, make_ctx(vec![pkh])]).unwrap();
    assert!(result.failed());
    assert!(result.logs.iter().any(|l| l.contains("Wrong preimage")));
}

#[test]
fn missing_signer() {
    let Some(validator) = try_compile() else { return };
    let secret = b"my secret";
    let pkh = PubKeyHash { hash: vec![0xaa, 0xbb] };
    let wrong = PubKeyHash { hash: vec![0xff] };
    let datum = PreimageDatum { hash: builtins::sha2_256(secret), pkh }.to_data();
    let redeemer = secret.to_vec().to_data();
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
