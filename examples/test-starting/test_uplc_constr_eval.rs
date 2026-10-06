//! Tests for `repr = "uplc_constr"`: types Scalus holds as a native `constr` rather
//! than as Data. `Order` is the prelude type that uses it.

use rustus_core::constant::UplcConstant;
use rustus_core::data::{Data, FromData, ToData};
use rustus_core::num_bigint::BigInt;
use rustus_core::sir::SIR;
use rustus_core::sir_type::{DataDecl, HasSIRType};
use rustus_prelude::order::Order;

#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
#[rustus(repr = "uplc_constr")]
struct Span {
    from: BigInt,
    to: BigInt,
}

/// Builds an Order on-chain, decodes another from Data, and matches on both.
#[rustus::compile]
fn check_order(a_data: Data, b_data: Data, expected_data: Data) {
    let a: BigInt = FromData::from_data(&a_data).unwrap();
    let b: BigInt = FromData::from_data(&b_data).unwrap();
    let actual: Order = if a < b {
        Order::Less
    } else {
        if a == b { Order::Equal } else { Order::Greater }
    };
    let expected: Order = FromData::from_data(&expected_data).unwrap();
    let same: bool = match actual {
        Order::Less => match expected {
            Order::Less => true,
            _ => false,
        },
        Order::Equal => match expected {
            Order::Equal => true,
            _ => false,
        },
        Order::Greater => match expected {
            Order::Greater => true,
            _ => false,
        },
    };
    rustus_prelude::require!(same, "unexpected order")
}

#[rustus::compile]
fn check_span(span_data: Data) {
    let span: Span = FromData::from_data(&span_data).unwrap();
    rustus_prelude::require!(span.from <= span.to, "empty span")
}

fn compile(module: &str) -> rustus::Validator {
    rustus::compile_module(module).unwrap_or_else(|e| panic!("compiling {module}: {e}"))
}

fn eval_order(a: i64, b: i64, expected: Order) -> rustus::EvalResult {
    let args = [BigInt::from(a).to_data(), BigInt::from(b).to_data(), expected.to_data()];
    compile("check_order").eval(&args).unwrap()
}

fn uplc_repr(decl: &DataDecl) -> Option<&str> {
    match decl.annotations.data.get("uplcRepr") {
        Some(SIR::Const { uplc_const: UplcConstant::String { value }, .. }) => Some(value),
        _ => None,
    }
}

/// The derive records the repr on the declaration, for sums and products alike.
#[test]
fn repr_is_recorded_on_the_declaration() {
    for decl in [Order::sir_data_decl().unwrap(), Span::sir_data_decl().unwrap()] {
        assert_eq!(uplc_repr(&decl), Some("UplcConstr"), "{}", decl.name);
    }
}

#[test]
fn less() {
    let result = eval_order(1, 2, Order::Less);
    assert!(result.succeeded(), "1 vs 2 failed: {:?}", result.error);
}

#[test]
fn equal() {
    let result = eval_order(2, 2, Order::Equal);
    assert!(result.succeeded(), "2 vs 2 failed: {:?}", result.error);
}

#[test]
fn greater() {
    let result = eval_order(3, 2, Order::Greater);
    assert!(result.succeeded(), "3 vs 2 failed: {:?}", result.error);
}

#[test]
fn mismatch_fails() {
    let result = eval_order(1, 2, Order::Greater);
    assert!(result.failed());
    assert!(result.logs.iter().any(|l| l.contains("unexpected order")));
}

/// The annotation must reach Scalus: an Order built on-chain is a `constr` term,
/// which the Data representation never produces.
#[test]
fn order_is_native_constr() {
    let text = compile("check_order").to_text().unwrap();
    assert!(text.contains("(constr "), "no native constr in:\n{text}");
}

/// Scalus accepts the repr on a product type and reads its fields correctly.
#[test]
fn struct_fields() {
    let validator = compile("check_span");
    let ordered = Span { from: 1.into(), to: 5.into() }.to_data();
    let result = validator.eval(&[ordered]).unwrap();
    assert!(result.succeeded(), "1..5 failed: {:?}", result.error);

    let reversed = Span { from: 9.into(), to: 5.into() }.to_data();
    let result = validator.eval(&[reversed]).unwrap();
    assert!(result.failed());
    assert!(result.logs.iter().any(|l| l.contains("empty span")));
}
