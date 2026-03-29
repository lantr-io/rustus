/// Test custom functional_typeclass macro.
use rustus_core::typeclasses;

// Bare — everything derived from trait
#[rustus::functional_typeclass]
pub trait OnChainHashable {
    fn sir_hash() -> rustus_core::sir::SIR;
}

// With explicit scalus name override
#[rustus::functional_typeclass(name = "my.app.CustomOrd")]
pub trait OnChainCustomOrd {
    fn sir_cmp() -> rustus_core::sir::SIR;
}

fn main() {
    let registry = typeclasses::typeclass_registry();

    println!("Registered typeclasses:");
    for tc in &registry {
        println!("  {} → {} (method: {})", tc.rust_trait_name, tc.scalus_name, tc.method_name);
    }

    // Built-ins
    let eq = registry.iter().find(|tc| tc.rust_trait_name == "PartialEq").unwrap();
    assert_eq!(eq.scalus_name, "scalus.cardano.onchain.plutus.prelude.Eq");
    assert_eq!(eq.method_name, "sir_eq");

    let ord = registry.iter().find(|tc| tc.rust_trait_name == "PartialOrd").unwrap();
    assert_eq!(ord.scalus_name, "scalus.cardano.onchain.plutus.prelude.Ord");
    assert_eq!(ord.method_name, "sir_ord");

    // Custom bare — name = trait name, method from trait
    let hashable = registry.iter().find(|tc| tc.rust_trait_name == "OnChainHashable").unwrap();
    assert_eq!(hashable.scalus_name, "OnChainHashable");
    assert_eq!(hashable.method_name, "sir_hash");

    // Custom with name override — method still from trait
    let custom = registry.iter().find(|tc| tc.rust_trait_name == "OnChainCustomOrd").unwrap();
    assert_eq!(custom.scalus_name, "my.app.CustomOrd");
    assert_eq!(custom.method_name, "sir_cmp");

    println!("\nAll checks passed!");
}
