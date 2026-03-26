use rustus_core::typeclasses::OnchainPartialEq;
use rustus_prelude::ledger::v1::PubKeyHash;
use rustus_core::num_bigint::BigInt;
use rustus_core::data::Data;

fn main() {
    println!("BigInt eq: {:?}", BigInt::sir_eq());
    println!("Vec<u8> eq: {:?}", Vec::<u8>::sir_eq());
    println!("Data eq: {:?}", Data::sir_eq());
    println!("PubKeyHash eq: {:?}", PubKeyHash::sir_eq());
}
