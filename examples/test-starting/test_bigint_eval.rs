//! UPLC evaluation tests for integers.

use rustus_core::data::{Data, FromData, ToData};
use rustus_core::num_bigint::BigInt;

/// Compares an argument with a negative constant, so both have to keep their sign
/// on the way to Scalus.
#[rustus::compile]
fn below_minus_five(n_data: Data) {
    let n: BigInt = FromData::from_data(&n_data).unwrap();
    rustus_prelude::require!(n < BigInt::from(-5), "not below -5")
}

fn eval(n: i64) -> rustus::EvalResult {
    let validator = rustus::compile_module("below_minus_five")
        .unwrap_or_else(|e| panic!("compiling below_minus_five: {e}"));
    validator.eval(&[BigInt::from(n).to_data()]).unwrap()
}

#[test]
fn negative_below_the_constant() {
    let result = eval(-6);
    assert!(result.succeeded(), "-6 < -5 failed: {:?}", result.error);
}

#[test]
fn negative_above_the_constant() {
    assert!(eval(-4).failed());
}

#[test]
fn positive_with_the_same_magnitude() {
    assert!(eval(6).failed());
}
