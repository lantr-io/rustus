//! Tests for `==`.
//!
//! On primitives `==` is a builtin. On any other type Rustus emits the SIR the Scalus
//! compiler emits for `===`: the type's `Eq` instance applied to both operands, marked
//! as an application of `Eq`. Scalus's lowering then compares the operands in whatever
//! representation they are held in, without calling the instance.

use rustus_core::bytestring::ByteString;
use rustus_core::constant::UplcConstant;
use rustus_core::data::{Data, FromData, ToData};
use rustus_core::module::Module;
use rustus_core::num_bigint::BigInt;
use rustus_core::sir::{Pattern, SIR};
use rustus_prelude::ledger::v1::PubKeyHash;
use rustus_prelude::list::{self, List};
use rustus_prelude::order::Order;

#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
enum Suit {
    Clubs,
    Hearts,
    Spades,
}

#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
struct Range {
    from: BigInt,
    to: BigInt,
}

/// An enum held as Data.
#[rustus::compile]
fn eq_suit(a_data: Data, b_data: Data) {
    let x: Suit = FromData::from_data(&a_data).unwrap();
    let y: Suit = FromData::from_data(&b_data).unwrap();
    rustus_prelude::require!(x == y, "differ")
}

/// A struct held as Data.
#[rustus::compile]
fn eq_range(a_data: Data, b_data: Data) {
    let x: Range = FromData::from_data(&a_data).unwrap();
    let y: Range = FromData::from_data(&b_data).unwrap();
    rustus_prelude::require!(x == y, "differ")
}

/// A one_element wrapper.
#[rustus::compile]
fn eq_pkh(a_data: Data, b_data: Data) {
    let x: PubKeyHash = FromData::from_data(&a_data).unwrap();
    let y: PubKeyHash = FromData::from_data(&b_data).unwrap();
    rustus_prelude::require!(x == y, "differ")
}

/// A `uplc_constr` enum built on-chain, so both operands are native `constr` terms.
#[rustus::compile]
fn eq_order(a_data: Data, b_data: Data) {
    let a: BigInt = FromData::from_data(&a_data).unwrap();
    let b: BigInt = FromData::from_data(&b_data).unwrap();
    let x: Order = if a < BigInt::from(0) { Order::Less } else { Order::Greater };
    let y: Order = if b < BigInt::from(0) { Order::Less } else { Order::Greater };
    rustus_prelude::require!(x == y, "differ")
}

#[rustus::compile]
fn eq_bool(a_data: Data, b_data: Data) {
    let a: BigInt = FromData::from_data(&a_data).unwrap();
    let b: BigInt = FromData::from_data(&b_data).unwrap();
    let x: bool = a < BigInt::from(0);
    let y: bool = b < BigInt::from(0);
    rustus_prelude::require!(x == y, "differ")
}

/// Equality reached through `list::contains`, which Scalus replaces with its own
/// intrinsic: that compares structurally too, and drops the `PartialEq` argument.
#[rustus::compile]
fn contains_suit(list_data: Data, suit_data: Data) {
    let suits: List<Suit> = FromData::from_data(&list_data).unwrap();
    let suit: Suit = FromData::from_data(&suit_data).unwrap();
    let found: bool = list::contains(suits, suit);
    rustus_prelude::require!(found, "differ")
}

#[rustus::compile]
fn contains_order(list_data: Data, order_data: Data) {
    let orders: List<Order> = FromData::from_data(&list_data).unwrap();
    let order: Order = FromData::from_data(&order_data).unwrap();
    let found: bool = list::contains(orders, order);
    rustus_prelude::require!(found, "differ")
}

// ---------------------------------------------------------------------------
// The SIR, against what the Scalus compiler emits
// ---------------------------------------------------------------------------

/// A SIR tree as indented text: node kinds, names, and annotation keys.
fn shape(sir: &SIR, indent: usize, out: &mut String) {
    let marks = |anns: &rustus_core::module::AnnotationsDecl| {
        let mut keys: Vec<&String> = anns.data.keys().collect();
        keys.sort();
        keys.iter().map(|k| format!(" @{k}")).collect::<String>()
    };
    let pad = " ".repeat(indent);
    let mut line = |text: String| out.push_str(&format!("{pad}{text}\n"));
    match sir {
        SIR::Decl { term, .. } => shape(term, indent, out),
        SIR::Var { name, .. } => line(format!("Var {name}")),
        SIR::ExternalVar { name, .. } => line(format!("ExternalVar {name}")),
        SIR::Const { uplc_const, .. } => line(format!("Const {uplc_const:?}")),
        SIR::Builtin { builtin_fun, .. } => line(format!("Builtin {builtin_fun:?}")),
        SIR::LamAbs { param, term, .. } => {
            let SIR::Var { name, .. } = &**param else { panic!("LamAbs param is not a Var") };
            line(format!("LamAbs {name}"));
            shape(term, indent + 2, out);
        }
        SIR::Apply { f, arg, anns, .. } => {
            line(format!("Apply{}", marks(anns)));
            shape(f, indent + 2, out);
            shape(arg, indent + 2, out);
        }
        SIR::And { a, b, .. } => {
            line("And".to_string());
            shape(a, indent + 2, out);
            shape(b, indent + 2, out);
        }
        SIR::Select { scrutinee, field, .. } => {
            line(format!("Select .{field}"));
            shape(scrutinee, indent + 2, out);
        }
        SIR::Match { scrutinee, cases, anns, .. } => {
            line(format!("Match{}", marks(anns)));
            shape(scrutinee, indent + 2, out);
            for case in cases {
                let pattern = match &case.pattern {
                    Pattern::Constr { constr_name, bindings, .. } => {
                        format!("{constr_name}({})", bindings.join(", "))
                    }
                    Pattern::Wildcard => "_".to_string(),
                };
                out.push_str(&format!("{pad}  case {pattern}\n"));
                shape(&case.body, indent + 4, out);
            }
        }
        other => panic!("node not expected in an Eq instance or its use: {other:?}"),
    }
}

fn shape_of(sir: &SIR) -> String {
    let mut out = String::new();
    shape(sir, 0, &mut out);
    out
}

fn binding<'a>(module: &'a Module, name: &str) -> &'a rustus_core::module::Binding {
    module
        .defs
        .iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("no binding {name}"))
}

/// The application of an `Eq` instance in `sir`: the outer of its two `Apply` nodes.
fn eq_application(sir: &SIR) -> Option<&SIR> {
    let is_eq = |sir: &SIR| matches!(sir, SIR::Apply { anns, f, .. }
        if anns.data.contains_key("functionalInterfaceType") && matches!(**f, SIR::Apply { .. }));
    if is_eq(sir) {
        return Some(sir);
    }
    match sir {
        SIR::Decl { term, .. } | SIR::LamAbs { term, .. } => eq_application(term),
        SIR::Let { bindings, body, .. } => bindings
            .iter()
            .find_map(|b| eq_application(&b.value))
            .or_else(|| eq_application(body)),
        SIR::IfThenElse { cond, t, f, .. } => {
            eq_application(cond).or_else(|| eq_application(t)).or_else(|| eq_application(f))
        }
        _ => None,
    }
}

/// `x == y` is `given_Eq_Suit(x)(y)`, with both applications marked as `Eq`.
#[test]
fn use_site_applies_the_instance() {
    let module = rustus_core::registry::build_module("eq_suit");
    let application = eq_application(&binding(&module, "eq_suit").value).expect("no Eq application");
    assert_eq!(
        shape_of(application),
        "\
Apply @functionalInterfaceType
  Apply @functionalInterfaceType
    ExternalVar Suit$.given_Eq_Suit
    Var x
  Var y
"
    );
    let SIR::Apply { anns, .. } = application else { unreachable!() };
    let SIR::Const { uplc_const: UplcConstant::String { value }, .. } =
        &anns.data["functionalInterfaceType"]
    else {
        panic!("the marker is not a string constant")
    };
    assert_eq!(value, "scalus.cardano.onchain.plutus.prelude.Eq");
}

/// The instance of an enum, as `Eq.derived` writes it.
#[test]
fn enum_instance() {
    let module = rustus_core::registry::build_module("eq_suit");
    let instance = binding(&module, "Suit$.given_Eq_Suit");
    assert_eq!(instance.module_name.as_deref(), Some("Suit$"));
    assert_eq!(
        shape_of(&instance.value),
        "\
LamAbs lhs
  LamAbs rhs
    Match @unchecked
      Var lhs
      case Suit::Clubs()
        Match
          Var rhs
          case Suit::Clubs()
            Const Bool { value: true }
          case _
            Const Bool { value: false }
      case Suit::Hearts()
        Match
          Var rhs
          case Suit::Hearts()
            Const Bool { value: true }
          case _
            Const Bool { value: false }
      case Suit::Spades()
        Match
          Var rhs
          case Suit::Spades()
            Const Bool { value: true }
          case _
            Const Bool { value: false }
"
    );
}

/// The instance of a struct: its fields compared through their own instances.
#[test]
fn struct_instance() {
    let module = rustus_core::registry::build_module("eq_range");
    let big_int = "scalus.cardano.onchain.plutus.prelude.Eq$.given_Eq_BigInt";
    assert_eq!(
        shape_of(&binding(&module, "Range$.given_Eq_Range").value),
        format!(
            "\
LamAbs lhs
  LamAbs rhs
    And
      Apply @functionalInterfaceType
        Apply @functionalInterfaceType
          ExternalVar {big_int}
          Select .from
            Var lhs
        Select .from
          Var rhs
      Apply @functionalInterfaceType
        Apply @functionalInterfaceType
          ExternalVar {big_int}
          Select .to
            Var lhs
        Select .to
          Var rhs
"
        )
    );
    // The field instances are bindings too, under the names Scalus gives them.
    assert_eq!(
        shape_of(&binding(&module, big_int).value),
        "\
LamAbs x
  LamAbs y
    Apply
      Apply
        Builtin EqualsInteger
        Var x
      Var y
"
    );
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

fn compile(module: &str) -> rustus::Validator {
    rustus::compile_module(module).unwrap_or_else(|e| panic!("compiling {module}: {e}"))
}

/// Evaluates `module` on arguments that should compare equal and on ones that should not.
fn check(module: &str, same: [Data; 2], different: [Data; 2]) {
    let validator = compile(module);
    let result = validator.eval(&same).unwrap();
    assert!(result.succeeded(), "{module} on equal values: {:?}", result.error);

    let result = validator.eval(&different).unwrap();
    assert!(result.failed(), "{module} on different values succeeded");
    assert!(result.logs.iter().any(|l| l.contains("differ")), "{:?}", result.logs);
}

fn int(i: i64) -> Data {
    BigInt::from(i).to_data()
}

#[test]
fn enum_held_as_data() {
    check(
        "eq_suit",
        [Suit::Hearts.to_data(), Suit::Hearts.to_data()],
        [Suit::Hearts.to_data(), Suit::Spades.to_data()],
    );
}

#[test]
fn struct_held_as_data() {
    let range = |from: i64, to: i64| Range { from: from.into(), to: to.into() }.to_data();
    check("eq_range", [range(1, 5), range(1, 5)], [range(1, 5), range(1, 6)]);
}

#[test]
fn one_element_wrapper() {
    let pkh = |hex: &str| PubKeyHash { hash: ByteString::from_hex(hex) }.to_data();
    check("eq_pkh", [pkh("deadbeef"), pkh("deadbeef")], [pkh("deadbeef"), pkh("cafe")]);
}

#[test]
fn uplc_constr_enum() {
    check("eq_order", [int(-1), int(-7)], [int(-1), int(3)]);
}

#[test]
fn booleans() {
    check("eq_bool", [int(-1), int(-7)], [int(-1), int(3)]);
    check("eq_bool", [int(2), int(9)], [int(4), int(-3)]);
}

/// Scalus chooses the comparison from the representation: `equalsData` for values
/// held as Data, a native `case` for `constr` terms.
#[test]
fn comparison_follows_representation() {
    let held_as_data = compile("eq_suit").to_text().unwrap();
    assert!(held_as_data.contains("equalsData"), "no equalsData in:\n{held_as_data}");

    let native = compile("eq_order").to_text().unwrap();
    assert!(!native.contains("equalsData"), "equalsData in:\n{native}");
}

#[test]
fn contains_enum() {
    let suits = List::from_vec(vec![Suit::Clubs, Suit::Hearts]).to_data();
    check(
        "contains_suit",
        [suits.clone(), Suit::Hearts.to_data()],
        [suits, Suit::Spades.to_data()],
    );
}

#[test]
fn contains_uplc_constr_enum() {
    let orders = List::from_vec(vec![Order::Less, Order::Equal]).to_data();
    check(
        "contains_order",
        [orders.clone(), Order::Equal.to_data()],
        [orders, Order::Greater.to_data()],
    );
}
