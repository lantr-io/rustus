use rustus_core::data::Data;
use rustus_core::data::ToData;
use rustus_prelude::list;

#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
enum Color {
    Red,
    Green,
    Blue,
}

#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
struct Datum {
    owner: Vec<u8>,
    color: Color,
}

#[rustus_macros::compile]
fn validator(datum: Datum, _redeemer: Data, _ctx: Data) -> bool {
    match datum.color {
        Color::Red => true,
        _ => false,
    }
}

fn main() {
    // Test Rust functions
    let datum = Datum {
        owner: vec![1, 2, 3],
        color: Color::Red,
    };
    println!("validator(Red) = {}", validator(datum.clone(), Data::unit(), Data::unit()));

    let datum2 = Datum {
        owner: vec![4, 5, 6],
        color: Color::Blue,
    };
    println!("validator(Blue) = {}", validator(datum2, Data::unit(), Data::unit()));

    // Test prelude function in Rust
    let test_list = rustus_prelude::List::from_vec(vec![
        Data::I { value: 42.into() },
        Data::I { value: 99.into() },
    ]);
    println!("list::is_empty = {}", list::is_empty(test_list.clone()));
    println!("list::head = {:?}", list::head(test_list));

    // Phase 2: build SIR module
    let module = rustus_core::registry::build_module("my_validator");

    let json = serde_json::to_string_pretty(&module).unwrap();
    std::fs::write("my_validator.sir.json", &json).unwrap();
    println!("\nWrote my_validator.sir.json ({} bytes)", json.len());

    println!("\nModule '{}' v{}.{}", module.name, module.version.0, module.version.1);
    println!("Data declarations: {:?}", module.data_decls.keys().collect::<Vec<_>>());
    println!(
        "Bindings: {:?}",
        module
            .defs
            .iter()
            .map(|b| format!(
                "{}{}",
                b.module_name
                    .as_ref()
                    .map(|m| format!("{}/", m))
                    .unwrap_or_default(),
                b.name
            ))
            .collect::<Vec<_>>()
    );

    println!("\nDatum.to_data() = {:?}", datum.to_data());
}
