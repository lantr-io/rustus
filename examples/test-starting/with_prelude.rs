use rustus_core::data::Data;
use rustus_prelude::list;
use rustus_prelude::List;

#[rustus_macros::compile]
fn check_first_element(input: List<Data>) -> bool {
    list::is_empty(input)
}

fn main() {
    // Test Rust side
    let empty_list: List<Data> = List::Nil;
    let non_empty = List::from_vec(vec![Data::I { value: 42.into() }]);
    println!("check_first_element(empty) = {}", check_first_element(empty_list));
    println!("check_first_element(non_empty) = {}", check_first_element(non_empty));

    // Build SIR module
    let module = rustus_core::registry::build_module("with_prelude");

    let json = serde_json::to_string_pretty(&module).unwrap();
    std::fs::write("with_prelude.sir.json", &json).unwrap();
    println!("\nWrote with_prelude.sir.json ({} bytes)", json.len());

    // Show bindings
    for b in &module.defs {
        let prefix = b
            .module_name
            .as_ref()
            .map(|m| format!("{}/", m))
            .unwrap_or_default();
        println!("  {}{}", prefix, b.name);
    }
}
