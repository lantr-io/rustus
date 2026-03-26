use serde::{Deserialize, Serialize};

use crate::data::Data;
use crate::module::AnnotationsDecl;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeVar {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opt_id: Option<i64>,
    #[serde(default)]
    pub is_builtin: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeBinding {
    pub name: String,
    pub tp: SIRType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstrDecl {
    pub name: String,
    pub params: Vec<TypeBinding>,
    pub type_params: Vec<TypeVar>,
    pub parent_type_args: Vec<SIRType>,
    pub annotations: AnnotationsDecl,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataDecl {
    pub name: String,
    pub constructors: Vec<ConstrDecl>,
    pub type_params: Vec<TypeVar>,
    pub annotations: AnnotationsDecl,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SIRType {
    // Primitives
    Integer,
    Boolean,
    ByteString,
    String,
    Unit,
    // Plutus Data
    Data,
    // Function type
    Fun {
        from: Box<SIRType>,
        to: Box<SIRType>,
    },
    // Sum type — references DataDecl by name in the symbol table
    SumCaseClass {
        decl_name: String,
        type_args: Vec<SIRType>,
    },
    // Single constructor case class — references its ConstrDecl and parent DataDecl by name
    CaseClass {
        constr_name: String,
        decl_name: String,
        type_args: Vec<SIRType>,
    },
    // Type variable (polymorphism)
    TypeVar {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        opt_id: Option<i64>,
        #[serde(default)]
        is_builtin: bool,
    },
    // Placeholder for unresolved types — filled in by the typing pass
    Unresolved,
}

impl SIRType {
    /// Substitute type variables using a map from opt_id → concrete type.
    pub fn substitute(&self, subst: &std::collections::HashMap<i64, SIRType>) -> SIRType {
        if subst.is_empty() {
            return self.clone();
        }
        match self {
            SIRType::TypeVar {
                opt_id: Some(id), ..
            } => subst.get(id).cloned().unwrap_or_else(|| self.clone()),
            SIRType::Fun { from, to } => SIRType::Fun {
                from: Box::new(from.substitute(subst)),
                to: Box::new(to.substitute(subst)),
            },
            SIRType::SumCaseClass {
                decl_name,
                type_args,
            } => SIRType::SumCaseClass {
                decl_name: decl_name.clone(),
                type_args: type_args.iter().map(|a| a.substitute(subst)).collect(),
            },
            SIRType::CaseClass {
                constr_name,
                decl_name,
                type_args,
            } => SIRType::CaseClass {
                constr_name: constr_name.clone(),
                decl_name: decl_name.clone(),
                type_args: type_args.iter().map(|a| a.substitute(subst)).collect(),
            },
            // Primitives, Data, Unresolved, TypeVar without id — unchanged
            _ => self.clone(),
        }
    }

    /// Check if this type or any nested type contains Unresolved.
    pub fn has_unresolved(&self) -> bool {
        match self {
            SIRType::Unresolved => true,
            SIRType::Fun { from, to } => from.has_unresolved() || to.has_unresolved(),
            SIRType::SumCaseClass { type_args, .. } | SIRType::CaseClass { type_args, .. } => {
                type_args.iter().any(|a| a.has_unresolved())
            }
            _ => false,
        }
    }
}

/// Trait for Rust types that have a corresponding SIR type representation.
pub trait HasSIRType {
    /// Returns the SIRType for this Rust type.
    fn sir_type() -> SIRType;

    /// Returns the DataDecl for this type, if it is a user-defined data type.
    fn sir_data_decl() -> Option<DataDecl> {
        None
    }
}

// Built-in HasSIRType implementations for primitive types

impl HasSIRType for bool {
    fn sir_type() -> SIRType {
        SIRType::Boolean
    }
}

impl HasSIRType for i64 {
    fn sir_type() -> SIRType {
        SIRType::Integer
    }
}

impl HasSIRType for Vec<u8> {
    fn sir_type() -> SIRType {
        SIRType::ByteString
    }
}

impl HasSIRType for String {
    fn sir_type() -> SIRType {
        SIRType::String
    }
}

impl HasSIRType for () {
    fn sir_type() -> SIRType {
        SIRType::Unit
    }
}

impl HasSIRType for num_bigint::BigInt {
    fn sir_type() -> SIRType {
        SIRType::Integer
    }
}

impl HasSIRType for Data {
    fn sir_type() -> SIRType {
        SIRType::Data
    }
}

/// Marker type for type parameters in generic DataDecls.
/// `TypeParam<1>` implements HasSIRType by returning `TypeVar("A", Some(1))`.
/// Used as the dummy type argument when registering generic types,
/// so that `sir_type()` naturally propagates TypeVars through type-application.
pub struct TypeParam<const ID: i64>;

impl<const ID: i64> HasSIRType for TypeParam<ID> {
    fn sir_type() -> SIRType {
        let name = ((b'A' + (ID - 1) as u8) as char).to_string();
        SIRType::TypeVar {
            name,
            opt_id: Some(ID),
            is_builtin: false,
        }
    }
}

impl<const ID: i64> crate::data::ToData for TypeParam<ID> {
    fn to_data(&self) -> crate::data::Data {
        unreachable!("TypeParam is a phantom type for SIR generation only")
    }
}

impl<const ID: i64> crate::data::FromData for TypeParam<ID> {
    fn from_data(_data: &crate::data::Data) -> Result<Self, crate::data::DataError> {
        unreachable!("TypeParam is a phantom type for SIR generation only")
    }
}
