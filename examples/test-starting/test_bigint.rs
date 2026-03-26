use rustus_core::num_bigint::BigInt;
use rustus_core::data::{ToData, FromData};
use rustus_core::sir_type::HasSIRType;

#[rustus_macros::compile]
fn add_values(a: BigInt, b: BigInt) -> BigInt {
    a + b
}

#[rustus_macros::compile]
fn is_positive(x: BigInt) -> bool {
    x > BigInt::from(0)
}

fn main() {
    // Test Rust side
    let a = BigInt::from(100);
    let b = BigInt::from(200);
    println!("add_values(100, 200) = {}", add_values(a.clone(), b.clone()));
    println!("is_positive(42) = {}", is_positive(BigInt::from(42)));
    println!("is_positive(-1) = {}", is_positive(BigInt::from(-1)));

    // Test ToData/FromData
    let data = a.to_data();
    println!("\n100.to_data() = {:?}", data);
    let back = BigInt::from_data(&data).unwrap();
    assert_eq!(back, BigInt::from(100));

    // Check SIR type
    println!("BigInt sir_type: {:?}", BigInt::sir_type());

    // Build module and check SIR
    let module = rustus_core::registry::build_module("bigint_test");
    let json = serde_json::to_string_pretty(&module).unwrap();
    std::fs::write("bigint_test.sir.json", &json).unwrap();

    // Show the add_values SIR
    for b in &module.defs {
        if b.name == "add_values" {
            println!("\nadd_values SIR:");
            println!("{}", serde_json::to_string_pretty(&b.value).unwrap());
        }
    }

    println!("\nAll OK!");
}
