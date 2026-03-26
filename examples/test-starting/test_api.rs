use rustus::data::Data;
use rustus::num_bigint::BigInt;

#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
enum Color { Red, Green, Blue }

#[derive(Debug, Clone, PartialEq, rustus::ToData, rustus::FromData)]
struct Datum { owner: Vec<u8>, color: Color }

#[rustus::compile]
fn add(a: BigInt, b: BigInt) -> BigInt {
    a + b
}

fn main() {
    println!("add(1,2) = {}", add(BigInt::from(1), BigInt::from(2)));
    println!("OK — rustus::compile and rustus::ToData work!");
}
