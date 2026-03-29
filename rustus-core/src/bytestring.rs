use serde::{Deserialize, Serialize};
use std::fmt;

use crate::data::{Data, DataError, FromData, ToData};
use crate::sir_type::{HasSIRType, SIRType};

/// On-chain ByteString type, matching `scalus.uplc.builtin.ByteString`.
///
/// A wrapper around `Vec<u8>` that only exposes bytestring-relevant operations.
/// Used for hashes, public keys, token names, policy IDs, etc.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ByteString(Vec<u8>);

impl ByteString {
    pub const fn new() -> Self {
        ByteString(Vec::new())
    }

    pub fn from_vec(v: Vec<u8>) -> Self {
        ByteString(v)
    }

    pub fn from_slice(s: &[u8]) -> Self {
        ByteString(s.to_vec())
    }

    pub fn from_hex(s: &str) -> Self {
        assert!(s.len() % 2 == 0, "hex string must have even length, got {}", s.len());
        let bytes = (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("invalid hex"))
            .collect();
        ByteString(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn to_vec(self) -> Vec<u8> {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl Default for ByteString {
    fn default() -> Self {
        ByteString::new()
    }
}

impl From<Vec<u8>> for ByteString {
    fn from(v: Vec<u8>) -> Self {
        ByteString(v)
    }
}

impl From<&[u8]> for ByteString {
    fn from(s: &[u8]) -> Self {
        ByteString(s.to_vec())
    }
}

impl From<ByteString> for Vec<u8> {
    fn from(bs: ByteString) -> Self {
        bs.0
    }
}

impl AsRef<[u8]> for ByteString {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for ByteString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ByteString(\"{}\")", self.to_hex())
    }
}

impl fmt::Display for ByteString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

// --- Data encoding ---

impl ToData for ByteString {
    fn to_data(&self) -> Data {
        Data::B {
            value: self.0.clone(),
        }
    }
}

impl FromData for ByteString {
    fn from_data(data: &Data) -> Result<Self, DataError> {
        match data {
            Data::B { value } => Ok(ByteString(value.clone())),
            _ => Err(DataError::UnexpectedVariant {
                expected: "B (ByteString)",
            }),
        }
    }
}

// --- SIR type ---

impl HasSIRType for ByteString {
    fn sir_type() -> SIRType {
        SIRType::ByteString
    }
}

// --- On-chain equality ---

impl crate::typeclasses::OnchainPartialEq for ByteString {
    fn sir_eq() -> crate::sir::SIR {
        crate::typeclasses::make_binary_builtin(
            crate::default_fun::DefaultFun::EqualsByteString,
            SIRType::ByteString,
        )
    }
}

impl crate::typeclasses::OnchainPartialOrd for ByteString {
    fn sir_ord() -> crate::sir::SIR {
        crate::typeclasses::make_binary_builtin(
            crate::default_fun::DefaultFun::LessThanByteString,
            SIRType::ByteString,
        )
    }
}
