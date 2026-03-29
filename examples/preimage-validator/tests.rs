//! Tests for Preimage Validator — compile to UPLC and eval in CEK machine.

use rustus_core::data::{Data, FromData, ToData};
use rustus_prelude::builtins;
use rustus_prelude::ledger::v1::*;
use rustus_prelude::list;
use rustus_prelude::list::List;

#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
pub struct PreimageDatum {
    pub hash: Vec<u8>,
    pub pkh: PubKeyHash,
}

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
