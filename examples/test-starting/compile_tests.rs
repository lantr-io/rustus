//! Scalus compilation coverage for the example modules that have no eval tests.
//!
//! The examples are the corpus that exercises the SIR emitters, but most of them
//! only build SIR Rust-side. These drive them all the way through scalus, so a
//! backend change that breaks lowering shows up in `cargo test` rather than the
//! next time someone runs an example by hand.
//!
//! Only examples with no colliding type declarations can share one binary: the
//! registry is process-global, so every `#[compile]` item linked in here lands in
//! the same module.

#![allow(dead_code, unused_imports)]

#[path = "validator.rs"]
mod validator;

#[path = "with_prelude.rs"]
mod with_prelude;

#[path = "test_bigint.rs"]
mod test_bigint;

fn compile(module: &str) -> rustus::Validator {
    rustus::compile_module(module).unwrap_or_else(|e| panic!("compiling {module}: {e}"))
}

/// Enum match + `from_data` on a derived struct.
#[test]
fn validator_compiles() {
    assert!(!compile("validator").to_flat().unwrap().is_empty());
}

/// Prelude call (`list::is_empty`) resolved through an external module binding.
#[test]
fn with_prelude_compiles() {
    assert!(!compile("check_first_element").to_flat().unwrap().is_empty());
}

/// Integer builtins.
#[test]
fn test_bigint_compiles() {
    assert!(!compile("add_values").to_flat().unwrap().is_empty());
}
