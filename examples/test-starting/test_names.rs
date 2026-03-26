use rustus_core::sir_type::HasSIRType;

// Without annotation — Rust-style names
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
enum Color {
    Red,
    Green,
    Blue,
}

// With annotation — scalus-style names
#[derive(Debug, Clone, PartialEq, rustus_macros::ToData, rustus_macros::FromData)]
#[rustus(name = "scalus.cardano.onchain.plutus.prelude.Maybe")]
enum Maybe {
    Nothing,
    Just(i64),
}

fn main() {
    // Color: Rust-style names
    let color_decl = Color::sir_data_decl().unwrap();
    println!("Color DataDecl name: {}", color_decl.name);
    for c in &color_decl.constructors {
        println!("  Constructor: {}", c.name);
    }

    println!();

    // Maybe: scalus-style names
    let maybe_decl = Maybe::sir_data_decl().unwrap();
    println!("Maybe DataDecl name: {}", maybe_decl.name);
    for c in &maybe_decl.constructors {
        println!("  Constructor: {}", c.name);
    }

    // Check SIRType
    println!("\nColor sir_type: {:?}", Color::sir_type());
    println!("Maybe sir_type: {:?}", Maybe::sir_type());
}
