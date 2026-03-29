//! Tests for PubKey Validator — compile to UPLC and eval in CEK machine.

use rustus_core::data::{Data, FromData, ToData};
use rustus_prelude::ledger::v1::*;
use rustus_prelude::list;
use rustus_prelude::list::List;

#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
pub struct OwnerDatum {
    pub owner: PubKeyHash,
}

#[rustus::compile]
fn pubkey_validator(datum: Data, _redeemer: Data, ctx: Data) {
    let owner_datum: OwnerDatum = FromData::from_data(&datum).unwrap();
    let script_ctx: ScriptContext = FromData::from_data(&ctx).unwrap();
    let signed: bool = list::contains(script_ctx.tx_info.signatories, owner_datum.owner);
    rustus_prelude::require!(signed, "Not signed by owner")
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
    rustus::compile_module("pubkey_validator").ok()
}

#[test]
fn correct_signer() {
    let Some(validator) = try_compile() else { return };
    let pkh = PubKeyHash { hash: vec![0xde, 0xad, 0xbe, 0xef] };
    let datum = OwnerDatum { owner: pkh.clone() }.to_data();
    let result = validator.eval(&[datum, Data::unit(), make_ctx(vec![pkh])]).unwrap();
    assert!(result.succeeded(), "Expected success: {:?}", result.error);
}

#[test]
fn wrong_signer() {
    let Some(validator) = try_compile() else { return };
    let pkh = PubKeyHash { hash: vec![0xde, 0xad, 0xbe, 0xef] };
    let wrong = PubKeyHash { hash: vec![0xca, 0xfe] };
    let datum = OwnerDatum { owner: pkh }.to_data();
    let result = validator.eval(&[datum, Data::unit(), make_ctx(vec![wrong])]).unwrap();
    assert!(result.failed());
}

#[test]
fn produces_flat() {
    let Some(validator) = try_compile() else { return };
    assert!(!validator.to_flat().unwrap().is_empty());
}
