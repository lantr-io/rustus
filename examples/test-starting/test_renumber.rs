use rustus_core::num_bigint::BigInt;
use rustus_prelude::List;

// A second generic type to test ID uniqueness
#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
#[rustus(name = "test.Pair")]
struct Pair<A, B> {
    first: A,
    second: B,
}

fn main() {
    let module = rustus_core::registry::build_module("renumber_test");

    // Check TypeVar IDs in each generic DataDecl
    for (name, decl) in &module.data_decls {
        if !decl.type_params.is_empty() {
            let ids: Vec<_> = decl.type_params.iter()
                .map(|tp| format!("{}={:?}", tp.name, tp.opt_id))
                .collect();
            println!("{}: type_params [{}]", name, ids.join(", "));

            for c in &decl.constructors {
                for p in &c.params {
                    if let rustus_core::sir_type::SIRType::TypeVar { name: n, opt_id, .. } = &p.tp {
                        println!("  {}.{}: TypeVar({}, {:?})", c.name, p.name, n, opt_id);
                    }
                }
            }
        }
    }
}
