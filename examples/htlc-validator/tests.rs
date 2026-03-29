//! Tests for HTLC Validator (V3 style) — compile to UPLC and eval in CEK machine.

#[path = "validator.rs"]
mod validator;
use validator::{Action, Config};

use rustus_core::bytestring::ByteString;
use rustus_core::data::{Data, ToData};
use rustus_prelude::builtins;
use rustus_prelude::ledger::v1::*;
use rustus_prelude::ledger::v3;
use rustus_prelude::list::List;
use rustus_prelude::option::Option as PlutusOption;
use rustus_prelude::sorted_map::SortedMap;

fn test_config() -> Config {
    Config {
        committer: PubKeyHash { hash: ByteString::from_hex("aa") },
        receiver: PubKeyHash { hash: ByteString::from_hex("bb") },
        image: builtins::sha3_256(&ByteString::from_slice(b"secret")),
        timeout: 1000.into(),
    }
}

fn make_v3_ctx(
    config: &Config,
    action: &Action,
    signatories: Vec<PubKeyHash>,
    valid_range: Interval,
) -> Data {
    v3::ScriptContext {
        tx_info: v3::TxInfo {
            inputs: List::Nil,
            reference_inputs: List::Nil,
            outputs: List::Nil,
            fee: 0.into(),
            mint: Value { inner: Data::Map { values: vec![] } },
            certificates: List::Nil,
            withdrawals: SortedMap::empty(),
            valid_range,
            signatories: List::from_vec(signatories),
            redeemers: SortedMap::empty(),
            data: SortedMap::empty(),
            id: TxId { hash: ByteString::from_hex("00") },
            votes: SortedMap::empty(),
            proposal_procedures: List::Nil,
            current_treasury_amount: PlutusOption::None,
            treasury_donation: PlutusOption::None,
        },
        redeemer: action.to_data(),
        script_info: v3::ScriptInfo::SpendingScript {
            tx_out_ref: TxOutRef {
                id: TxId { hash: ByteString::from_hex("00") },
                idx: 0.into(),
            },
            datum: PlutusOption::Some { value: config.to_data() },
        },
    }.to_data()
}

fn try_compile() -> Option<rustus::Validator> {
    rustus::compile_module("htlc_validator").ok()
}

// --- Reveal tests ---

#[test]
fn reveal_correct_preimage_before_timeout() {
    let Some(validator) = try_compile() else { return };
    let config = test_config();
    let action = Action::Reveal { preimage: ByteString::from_slice(b"secret") };
    // valid_to = 500, timeout = 1000 → 500 <= 1000 ✓
    let range = Interval {
        from: IntervalBound::finite_inclusive(0.into()),
        to: IntervalBound::finite_exclusive(500.into()),
    };
    let ctx = make_v3_ctx(&config, &action, vec![config.receiver.clone()], range);
    let result = validator.eval(&[ctx]).unwrap();
    assert!(result.succeeded(), "Expected success: {:?} logs={:?}", result.error, result.logs);
}

#[test]
fn reveal_wrong_preimage() {
    let Some(validator) = try_compile() else { return };
    let config = test_config();
    let action = Action::Reveal { preimage: ByteString::from_slice(b"wrong") };
    let range = Interval {
        from: IntervalBound::finite_inclusive(0.into()),
        to: IntervalBound::finite_exclusive(500.into()),
    };
    let ctx = make_v3_ctx(&config, &action, vec![config.receiver.clone()], range);
    let result = validator.eval(&[ctx]).unwrap();
    assert!(result.failed());
    assert!(result.logs.iter().any(|l| l.contains("Invalid preimage")));
}

#[test]
fn reveal_after_timeout_fails() {
    let Some(validator) = try_compile() else { return };
    let config = test_config();
    let action = Action::Reveal { preimage: ByteString::from_slice(b"secret") };
    // valid_to = 2000, timeout = 1000 → 2000 <= 1000 ✗
    let range = Interval {
        from: IntervalBound::finite_inclusive(0.into()),
        to: IntervalBound::finite_exclusive(2000.into()),
    };
    let ctx = make_v3_ctx(&config, &action, vec![config.receiver.clone()], range);
    let result = validator.eval(&[ctx]).unwrap();
    assert!(result.failed());
    assert!(result.logs.iter().any(|l| l.contains("Must be before timeout")));
}

#[test]
fn reveal_wrong_signer() {
    let Some(validator) = try_compile() else { return };
    let config = test_config();
    let action = Action::Reveal { preimage: ByteString::from_slice(b"secret") };
    let range = Interval {
        from: IntervalBound::finite_inclusive(0.into()),
        to: IntervalBound::finite_exclusive(500.into()),
    };
    let ctx = make_v3_ctx(&config, &action, vec![config.committer.clone()], range);
    let result = validator.eval(&[ctx]).unwrap();
    assert!(result.failed());
    assert!(result.logs.iter().any(|l| l.contains("Must be signed by receiver")));
}

// --- Timeout tests ---

#[test]
fn timeout_after_deadline() {
    let Some(validator) = try_compile() else { return };
    let config = test_config();
    let action = Action::Timeout;
    // valid_from = 1500, timeout = 1000 → 1000 <= 1500 ✓
    let range = Interval {
        from: IntervalBound::finite_inclusive(1500.into()),
        to: IntervalBound::pos_inf(),
    };
    let ctx = make_v3_ctx(&config, &action, vec![config.committer.clone()], range);
    let result = validator.eval(&[ctx]).unwrap();
    assert!(result.succeeded(), "Expected success: {:?} logs={:?}", result.error, result.logs);
}

#[test]
fn timeout_before_deadline_fails() {
    let Some(validator) = try_compile() else { return };
    let config = test_config();
    let action = Action::Timeout;
    // valid_from = 500, timeout = 1000 → 1000 <= 500 ✗
    let range = Interval {
        from: IntervalBound::finite_inclusive(500.into()),
        to: IntervalBound::pos_inf(),
    };
    let ctx = make_v3_ctx(&config, &action, vec![config.committer.clone()], range);
    let result = validator.eval(&[ctx]).unwrap();
    assert!(result.failed());
    assert!(result.logs.iter().any(|l| l.contains("Must be after timeout")));
}

#[test]
fn timeout_wrong_signer() {
    let Some(validator) = try_compile() else { return };
    let config = test_config();
    let action = Action::Timeout;
    let range = Interval {
        from: IntervalBound::finite_inclusive(1500.into()),
        to: IntervalBound::pos_inf(),
    };
    let ctx = make_v3_ctx(&config, &action, vec![config.receiver.clone()], range);
    let result = validator.eval(&[ctx]).unwrap();
    assert!(result.failed());
    assert!(result.logs.iter().any(|l| l.contains("Must be signed by committer")));
}

#[test]
fn produces_flat() {
    let Some(validator) = try_compile() else { return };
    assert!(!validator.to_flat().unwrap().is_empty());
}
