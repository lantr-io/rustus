use num_bigint::BigInt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UplcConstant {
    Integer { value: BigInt },
    ByteString { value: Vec<u8> },
    String { value: String },
    Bool { value: bool },
    Unit,
}
