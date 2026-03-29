use rustus_core::num_bigint::BigInt;
use rustus_core::data::Data;

// A helper that exists in Rust but is NOT registered with #[compile]
fn my_helper(x: BigInt) -> BigInt {
    x + BigInt::from(1)
}

// Uses a single-name call — the macro compiles it as a Var, which the typing pass can't resolve
#[rustus::compile]
fn calls_unregistered(x: BigInt) -> BigInt {
    my_helper(x)
}

// Uses a method call that's not in the registry
#[rustus::compile]
fn uses_clone(x: Data) -> Data {
    x.clone()
}

fn main() {
    let module = rustus_core::registry::build_module("error_test");

    println!("--- Typing verification ---");
    for b in &module.defs {
        match rustus_core::typing::verify_complete(&b.value) {
            Ok(()) => println!("  {}: OK", b.name),
            Err(errors) => {
                println!("  {}: {} error(s)", b.name, errors.len());
                for e in &errors {
                    println!("    error: {}", e);
                }
            }
        }
    }
}
