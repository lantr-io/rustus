//! Tests for PubKey Validator — compile to UPLC and eval in CEK machine.

#[path = "validator.rs"]
mod validator;
use validator::OwnerDatum;

use rustus_core::bytestring::ByteString;
use rustus_core::data::{Data, ToData};
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

fn compile() -> rustus::Validator {
    rustus::compile_module("pubkey_validator")
        .unwrap_or_else(|e| panic!("compiling pubkey_validator: {e}"))
}

#[test]
fn correct_signer() {
    let validator = compile();
    let pkh = PubKeyHash { hash: ByteString::from_hex("deadbeef") };
    let datum = OwnerDatum { owner: pkh.clone() }.to_data();
    let result = validator.eval(&[datum, Data::unit(), make_ctx(vec![pkh])]).unwrap();
    assert!(result.succeeded(), "Expected success: {:?}", result.error);
}

#[test]
fn wrong_signer() {
    let validator = compile();
    let pkh = PubKeyHash { hash: ByteString::from_hex("deadbeef") };
    let wrong = PubKeyHash { hash: ByteString::from_hex("cafe") };
    let datum = OwnerDatum { owner: pkh }.to_data();
    let result = validator.eval(&[datum, Data::unit(), make_ctx(vec![wrong])]).unwrap();
    assert!(result.failed());
}

#[test]
fn produces_flat() {
    let validator = compile();
    assert!(!validator.to_flat().unwrap().is_empty());
}
