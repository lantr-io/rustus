use rustus_core::data::{Data, FromData, ToData};
use rustus_core::num_bigint::BigInt;
use rustus_prelude::list::{self, List};

/// Validator-style: takes Data, indexes list, asserts result.
#[rustus::compile]
fn check_second(list_data: Data) {
    let list: List<Data> = FromData::from_data(&list_data).unwrap();
    let elem: Data = list::at(list, BigInt::from(1));
    // Just force evaluation — panics if out of bounds
    let _ = elem;
}

fn main() {
    // Test Rust-side List::at method
    let my_list: List<Data> = List::from_vec(vec![
        Data::I { value: BigInt::from(10) },
        Data::I { value: BigInt::from(20) },
        Data::I { value: BigInt::from(30) },
    ]);
    let result = my_list.at(&BigInt::from(1));
    println!("Rust-side: at(1) = {:?}", result);
    assert_eq!(result, Data::I { value: BigInt::from(20) });

    // Compile and check UPLC
    match rustus::compile_module("check_second") {
        Ok(validator) => {
            let text = validator.to_text().unwrap();

            if text.contains("dropList") || text.contains("DropList") {
                println!("Uses dropList builtin (pv11 intrinsic)");
            } else {
                println!("No dropList — recursive fallback (intrinsic not substituted)");
            }

            // Evaluate: pass Data-encoded list
            let arg = my_list.to_data();
            let result = validator.eval(&[arg]).unwrap();
            println!("CEK eval succeeded: {}", result.succeeded());
            if let Some(err) = &result.error {
                println!("Error: {}", err);
            }
        }
        Err(e) => eprintln!("Compile error: {e}"),
    }
}
