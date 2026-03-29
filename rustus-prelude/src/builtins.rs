//! Rust implementations of UPLC builtin functions.
//!
//! These serve two purposes:
//! 1. Off-chain testing — run the same logic in Rust without the CEK machine
//! 2. The `#[compile]` macro recognizes calls to these and emits SIR Builtin nodes

use rustus_core::bytestring::ByteString;
use rustus_core::num_bigint::BigInt;

// ---------------------------------------------------------------------------
// Integer operations
// ---------------------------------------------------------------------------

pub fn add_integer(a: BigInt, b: BigInt) -> BigInt {
    a + b
}

pub fn subtract_integer(a: BigInt, b: BigInt) -> BigInt {
    a - b
}

pub fn multiply_integer(a: BigInt, b: BigInt) -> BigInt {
    a * b
}

pub fn divide_integer(a: BigInt, b: BigInt) -> BigInt {
    // Floored division (toward negative infinity), matching Plutus/Haskell `div`
    let (q, r) = (&a / &b, &a % &b);
    if (r != BigInt::from(0)) && ((r < BigInt::from(0)) != (b < BigInt::from(0))) {
        q - 1
    } else {
        q
    }
}

pub fn quotient_integer(a: BigInt, b: BigInt) -> BigInt {
    &a / &b
}

pub fn remainder_integer(a: BigInt, b: BigInt) -> BigInt {
    &a % &b
}

pub fn mod_integer(a: BigInt, b: BigInt) -> BigInt {
    ((a % &b) + &b) % &b
}

pub fn equals_integer(a: BigInt, b: BigInt) -> bool {
    a == b
}

pub fn less_than_integer(a: BigInt, b: BigInt) -> bool {
    a < b
}

pub fn less_than_equals_integer(a: BigInt, b: BigInt) -> bool {
    a <= b
}

// ---------------------------------------------------------------------------
// ByteString operations
// ---------------------------------------------------------------------------

pub fn append_bytestring(a: ByteString, b: ByteString) -> ByteString {
    let mut result = a.to_vec();
    result.extend(b.as_bytes());
    ByteString::from_vec(result)
}

pub fn cons_bytestring(byte: u8, bs: ByteString) -> ByteString {
    let mut result = vec![byte];
    result.extend(bs.as_bytes());
    ByteString::from_vec(result)
}

pub fn slice_bytestring(start: i64, len: i64, bs: ByteString) -> ByteString {
    let s = start as usize;
    let l = len as usize;
    let bytes = bs.as_bytes();
    if s >= bytes.len() {
        ByteString::new()
    } else {
        let end = (s + l).min(bytes.len());
        ByteString::from_slice(&bytes[s..end])
    }
}

pub fn length_of_bytestring(bs: &ByteString) -> BigInt {
    BigInt::from(bs.len())
}

pub fn index_bytestring(bs: &ByteString, index: i64) -> u8 {
    bs.as_bytes()[index as usize]
}

pub fn equals_bytestring(a: &ByteString, b: &ByteString) -> bool {
    a == b
}

pub fn less_than_bytestring(a: &ByteString, b: &ByteString) -> bool {
    a < b
}

pub fn less_than_equals_bytestring(a: &ByteString, b: &ByteString) -> bool {
    a <= b
}

// ---------------------------------------------------------------------------
// String operations
// ---------------------------------------------------------------------------

pub fn append_string(a: String, b: String) -> String {
    a + &b
}

pub fn equals_string(a: &str, b: &str) -> bool {
    a == b
}

pub fn encode_utf8(s: &str) -> ByteString {
    ByteString::from_vec(s.as_bytes().to_vec())
}

pub fn decode_utf8(bs: &ByteString) -> String {
    String::from_utf8(bs.as_bytes().to_vec()).expect("invalid UTF-8")
}

// ---------------------------------------------------------------------------
// Cryptographic hash functions
// ---------------------------------------------------------------------------

use sha2::Digest as _;

pub fn sha2_256(data: &ByteString) -> ByteString {
    ByteString::from_vec(sha2::Sha256::digest(data.as_bytes()).to_vec())
}

pub fn sha3_256(data: &ByteString) -> ByteString {
    ByteString::from_vec(sha3::Sha3_256::digest(data.as_bytes()).to_vec())
}

pub fn blake2b_256(data: &ByteString) -> ByteString {
    use blake2::digest::Digest;
    ByteString::from_vec(
        blake2::Blake2b::<blake2::digest::consts::U32>::digest(data.as_bytes()).to_vec(),
    )
}

pub fn blake2b_224(data: &ByteString) -> ByteString {
    use blake2::digest::Digest;
    ByteString::from_vec(
        blake2::Blake2b::<blake2::digest::consts::U28>::digest(data.as_bytes()).to_vec(),
    )
}

// ---------------------------------------------------------------------------
// Data operations
// ---------------------------------------------------------------------------

use rustus_core::data::Data;

pub fn constr_data(tag: BigInt, args: Vec<Data>) -> Data {
    Data::Constr {
        tag: tag.try_into().expect("tag too large"),
        args,
    }
}

pub fn map_data(values: Vec<(Data, Data)>) -> Data {
    Data::Map { values }
}

pub fn list_data(values: Vec<Data>) -> Data {
    Data::List { values }
}

pub fn i_data(value: BigInt) -> Data {
    Data::I { value }
}

pub fn b_data(value: ByteString) -> Data {
    Data::B {
        value: value.to_vec(),
    }
}

pub fn un_constr_data(data: &Data) -> (BigInt, Vec<Data>) {
    match data {
        Data::Constr { tag, args } => (BigInt::from(*tag), args.clone()),
        _ => panic!("un_constr_data: expected Constr"),
    }
}

pub fn un_map_data(data: &Data) -> Vec<(Data, Data)> {
    match data {
        Data::Map { values } => values.clone(),
        _ => panic!("un_map_data: expected Map"),
    }
}

pub fn un_list_data(data: &Data) -> Vec<Data> {
    match data {
        Data::List { values } => values.clone(),
        _ => panic!("un_list_data: expected List"),
    }
}

pub fn un_i_data(data: &Data) -> BigInt {
    match data {
        Data::I { value } => value.clone(),
        _ => panic!("un_i_data: expected I"),
    }
}

pub fn un_b_data(data: &Data) -> ByteString {
    match data {
        Data::B { value } => ByteString::from_vec(value.clone()),
        _ => panic!("un_b_data: expected B"),
    }
}

pub fn equals_data(a: &Data, b: &Data) -> bool {
    a == b
}

// ---------------------------------------------------------------------------
// Trace
// ---------------------------------------------------------------------------

pub fn trace<T>(msg: &str, value: T) -> T {
    eprintln!("[trace] {}", msg);
    value
}
