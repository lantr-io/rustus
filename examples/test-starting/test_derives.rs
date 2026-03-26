use rustus_core::data::{Data, FromData, ToData};
use rustus_core::sir_type::HasSIRType;

#[derive(Debug, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
enum Color {
    Red,
    Green,
    Blue,
}

#[derive(Debug, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
struct Datum {
    owner: Vec<u8>,
    color: Color,
}

fn main() {
    // Test ToData
    assert_eq!(
        Color::Red.to_data(),
        Data::Constr { tag: 0, args: vec![] }
    );
    assert_eq!(
        Color::Blue.to_data(),
        Data::Constr { tag: 2, args: vec![] }
    );
    println!("Color::Red.to_data() = {:?}", Color::Red.to_data());

    let datum = Datum {
        owner: vec![1, 2, 3],
        color: Color::Green,
    };
    let data = datum.to_data();
    println!("datum.to_data() = {:?}", data);
    assert_eq!(
        data,
        Data::Constr {
            tag: 0,
            args: vec![
                Data::B { value: vec![1, 2, 3] },
                Data::Constr { tag: 1, args: vec![] },
            ]
        }
    );

    // Test FromData
    let color_back = Color::from_data(&Data::Constr { tag: 2, args: vec![] }).unwrap();
    assert_eq!(color_back, Color::Blue);
    println!("FromData Color: {:?}", color_back);

    let datum_back = Datum::from_data(&data).unwrap();
    assert_eq!(datum_back, datum);
    println!("FromData Datum: {:?}", datum_back);

    // Test HasSIRType
    let color_type = Color::sir_type();
    println!("Color SIR type: {:?}", color_type);
    let color_decl = Color::sir_data_decl().unwrap();
    println!("Color DataDecl: {}", serde_json::to_string_pretty(&color_decl).unwrap());

    let datum_type = Datum::sir_type();
    println!("Datum SIR type: {:?}", datum_type);
    let datum_decl = Datum::sir_data_decl().unwrap();
    println!("Datum DataDecl: {}", serde_json::to_string_pretty(&datum_decl).unwrap());

    // Test inventory registration
    let module = rustus_core::registry::build_module("test");
    println!("Module data_decls: {:?}", module.data_decls.keys().collect::<Vec<_>>());
    assert!(module.data_decls.contains_key("Color"));
    assert!(module.data_decls.contains_key("Datum"));

    println!("\nAll tests passed!");
}
