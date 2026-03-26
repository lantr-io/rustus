use rustus_core::num_bigint::BigInt;
use rustus_prelude::List;

#[rustus::compile]
fn list_head_or_zero(list: List<BigInt>) -> BigInt {
    match list {
        List::Cons { head, .. } => head,
        List::Nil => BigInt::from(0),
    }
}

fn main() {
    // Test Rust side
    let my_list = List::from_vec(vec![BigInt::from(42), BigInt::from(99)]);
    println!("list_head_or_zero([42,99]) = {}", list_head_or_zero(my_list));

    let empty: List<BigInt> = List::Nil;
    println!("list_head_or_zero([]) = {}", list_head_or_zero(empty));

    // Build module and inspect typing
    let module = rustus_core::registry::build_module("typing_test");

    // Find our binding
    let binding = module.defs.iter().find(|b| b.name == "list_head_or_zero");
    if let Some(b) = binding {
        println!("\nBinding type: {:?}", b.tp);
        println!("SIR JSON:");
        println!("{}", serde_json::to_string_pretty(&b.value).unwrap());
    }

    // Verify no Unresolved remains
    for b in &module.defs {
        match rustus_core::typing::verify_complete(&b.value) {
            Ok(()) => println!("{}: typing complete ✓", b.name),
            Err(errors) => {
                println!("{}: TYPING INCOMPLETE", b.name);
                for e in errors {
                    println!("  - {}", e);
                }
            }
        }
    }
}
