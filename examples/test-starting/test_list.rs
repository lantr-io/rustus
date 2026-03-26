use rustus_core::data::{Data, FromData, ToData};
use rustus_core::num_bigint::BigInt;
use rustus_core::sir_type::HasSIRType;
use rustus_prelude::list::{self, List};

fn main() {
    // Test with Data elements
    let my_list: List<Data> = List::from_vec(vec![
        Data::I { value: BigInt::from(10) },
        Data::I { value: BigInt::from(20) },
    ]);
    println!("List<Data>: {:?}", my_list);
    println!("is_empty = {}", list::is_empty(my_list.clone()));
    println!("head = {:?}", list::head(my_list.clone()));

    // Test with i64 elements
    let int_list: List<i64> = List::from_vec(vec![1, 2, 3]);
    println!("\nList<i64>: {:?}", int_list);
    let data = int_list.to_data();
    println!("to_data = {:?}", data);
    let back: List<i64> = List::from_data(&data).unwrap();
    assert_eq!(int_list, back);
    println!("roundtrip OK");

    // Check SIR types
    println!("\n--- SIR Types ---");

    // List<Data> — type-application: List applied to Data
    let data_list_type = <List<Data>>::sir_type();
    println!("List<Data> sir_type: {:?}", data_list_type);

    // List<i64> — type-application: List applied to Integer
    let int_list_type = <List<i64>>::sir_type();
    println!("List<i64> sir_type: {:?}", int_list_type);

    // DataDecl — always the same, with TypeVars
    let decl = <List<Data>>::sir_data_decl().unwrap();
    println!("\nDataDecl name: {}", decl.name);
    println!("DataDecl type_params: {:?}", decl.type_params);
    for c in &decl.constructors {
        println!("  {} (type_params: {:?}, parent_type_args: {:?})", c.name, c.type_params, c.parent_type_args);
        for p in &c.params {
            println!("    {}: {:?}", p.name, p.tp);
        }
    }

    // Check via build_module — registration uses TypeParam, so DataDecl should have TypeVars
    let module = rustus_core::registry::build_module("test_list");
    if let Some(list_decl) = module.data_decls.get("scalus.cardano.onchain.plutus.prelude.List") {
        println!("\n--- DataDecl from registry (uses TypeParam) ---");
        println!("DataDecl name: {}", list_decl.name);
        println!("type_params: {:?}", list_decl.type_params);
        for c in &list_decl.constructors {
            println!("  {}", c.name);
            for p in &c.params {
                println!("    {}: {:?}", p.name, p.tp);
            }
        }
    }

    println!("\nAll OK!");
}
