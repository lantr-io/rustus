use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Data {
    Constr { tag: i64, args: Vec<Data> },
    Map { values: Vec<(Data, Data)> },
    List { values: Vec<Data> },
    I { value: BigInt },
    B { value: Vec<u8> },
}

impl Data {
    pub fn unit() -> Self {
        Data::Constr {
            tag: 0,
            args: vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub enum DataError {
    UnexpectedTag { expected: &'static str, got: i64 },
    UnexpectedVariant { expected: &'static str },
    MissingField { index: usize },
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataError::UnexpectedTag { expected, got } => {
                write!(f, "expected {expected}, got tag {got}")
            }
            DataError::UnexpectedVariant { expected } => {
                write!(f, "expected {expected} variant")
            }
            DataError::MissingField { index } => {
                write!(f, "missing field at index {index}")
            }
        }
    }
}

impl std::error::Error for DataError {}

pub trait ToData {
    fn to_data(&self) -> Data;
}

pub trait FromData: Sized {
    fn from_data(data: &Data) -> Result<Self, DataError>;
}

// Built-in ToData/FromData for primitives

impl ToData for bool {
    fn to_data(&self) -> Data {
        Data::Constr {
            tag: if *self { 1 } else { 0 },
            args: vec![],
        }
    }
}

impl FromData for bool {
    fn from_data(data: &Data) -> Result<Self, DataError> {
        match data {
            Data::Constr { tag: 0, .. } => Ok(false),
            Data::Constr { tag: 1, .. } => Ok(true),
            Data::Constr { tag, .. } => Err(DataError::UnexpectedTag {
                expected: "bool (0 or 1)",
                got: *tag,
            }),
            _ => Err(DataError::UnexpectedVariant {
                expected: "Constr",
            }),
        }
    }
}

impl ToData for BigInt {
    fn to_data(&self) -> Data {
        Data::I {
            value: self.clone(),
        }
    }
}

impl FromData for BigInt {
    fn from_data(data: &Data) -> Result<Self, DataError> {
        match data {
            Data::I { value } => Ok(value.clone()),
            _ => Err(DataError::UnexpectedVariant { expected: "I" }),
        }
    }
}

impl ToData for i64 {
    fn to_data(&self) -> Data {
        Data::I {
            value: BigInt::from(*self),
        }
    }
}

impl FromData for i64 {
    fn from_data(data: &Data) -> Result<Self, DataError> {
        match data {
            Data::I { value } => {
                use num_bigint::ToBigInt;
                Ok(i64::try_from(value).map_err(|_| DataError::UnexpectedVariant {
                    expected: "I (fits in i64)",
                })?)
            }
            _ => Err(DataError::UnexpectedVariant { expected: "I" }),
        }
    }
}

impl ToData for Vec<u8> {
    fn to_data(&self) -> Data {
        Data::B {
            value: self.clone(),
        }
    }
}

impl FromData for Vec<u8> {
    fn from_data(data: &Data) -> Result<Self, DataError> {
        match data {
            Data::B { value } => Ok(value.clone()),
            _ => Err(DataError::UnexpectedVariant { expected: "B" }),
        }
    }
}

impl ToData for Data {
    fn to_data(&self) -> Data {
        self.clone()
    }
}

impl FromData for Data {
    fn from_data(data: &Data) -> Result<Self, DataError> {
        Ok(data.clone())
    }
}

// Box<T> is transparent for ToData/FromData
impl<T: ToData> ToData for Box<T> {
    fn to_data(&self) -> Data {
        (**self).to_data()
    }
}

impl<T: FromData> FromData for Box<T> {
    fn from_data(data: &Data) -> Result<Self, DataError> {
        T::from_data(data).map(Box::new)
    }
}
