//! Pre-SIR: expression-oriented IR emitted by the `#[compile]` macro.
//!
//! PreSIR is a simple expression tree with no SIRType annotations, no Apply chains,
//! and no DataDecl embedding. The macro generates PreSIR + TypeDict registrations.
//! A lowering pass (lower.rs) converts PreSIR → SIR using the TypeDict and ResolutionContext.

use std::collections::HashMap;

use crate::constant::UplcConstant;
use crate::module::AnnotationsDecl;
use crate::sir_type::{DataDecl, SIRType};

// ---------------------------------------------------------------------------
// TypeHint: lightweight type descriptor the macro CAN determine from syntax
// ---------------------------------------------------------------------------

/// A type hint extracted from Rust syntax. Unlike SIRType, this does NOT require
/// trait resolution or runtime calls — it's purely syntactic.
#[derive(Debug, Clone)]
pub enum TypeHint {
    Bool,
    Integer,
    ByteString,
    String,
    Unit,
    Data,
    /// Generic type parameter (e.g., T) with 1-based index
    TypeParam { name: String, index: i64 },
    /// Named user type with optional generic args: e.g., "List" with [TypeParam("T",1)]
    Named {
        rust_path: String,
        type_args: Vec<TypeHint>,
    },
    /// Function type: from → to
    Fun {
        from: Box<TypeHint>,
        to: Box<TypeHint>,
    },
    /// No type info available (let without annotation, etc.)
    Infer,
}

// ---------------------------------------------------------------------------
// TypeDict: type information populated at builder time
// ---------------------------------------------------------------------------

/// Type dictionary populated at builder time via `HasSIRType` trait dispatch.
/// The lowering pass uses this to resolve types that the macro can't determine.
///
/// The macro emits calls like:
///   `td.register_var("x", <OwnerDatum as HasSIRType>::sir_type())`
/// These execute at builder time where Rust's type system is available,
/// and the results are stored here for the lowering pass.
#[derive(Debug, Clone)]
pub struct TypeDict {
    /// Variable name → SIRType (from fn params + let annotations).
    /// Variable names are unique (shadowing eliminated by renumbering).
    pub vars: HashMap<String, SIRType>,
    /// Rust type path → SIRType (from HasSIRType::sir_type())
    pub type_map: HashMap<String, SIRType>,
    /// Rust type path → DataDecl (from HasSIRType::sir_data_decl())
    pub decl_map: HashMap<String, DataDecl>,
}

impl TypeDict {
    pub fn new() -> Self {
        TypeDict {
            vars: HashMap::new(),
            type_map: HashMap::new(),
            decl_map: HashMap::new(),
        }
    }

    /// Register a variable's resolved SIRType.
    pub fn register_var(&mut self, name: &str, tp: SIRType) {
        self.vars.insert(name.to_string(), tp);
    }

    /// Register a Rust type's SIRType and optional DataDecl.
    pub fn register_type_info(
        &mut self,
        rust_name: &str,
        tp: SIRType,
        decl: Option<DataDecl>,
    ) {
        self.type_map.insert(rust_name.to_string(), tp);
        if let Some(d) = decl {
            self.decl_map.insert(rust_name.to_string(), d);
        }
    }

    /// Look up a DataDecl by Rust type name.
    pub fn data_decl(&self, rust_name: &str) -> Option<&DataDecl> {
        self.decl_map.get(rust_name)
    }

    /// Look up a SIRType by Rust type name.
    pub fn sir_type(&self, rust_name: &str) -> Option<&SIRType> {
        self.type_map.get(rust_name)
    }
}

// ---------------------------------------------------------------------------
// PreSIR: the expression tree
// ---------------------------------------------------------------------------

/// Binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Eq,
    Lt,
    Le,
}

/// Pre-SIR expression. No SIRType fields, no Apply chains, no DataDecl embedding.
#[derive(Debug, Clone)]
pub enum PreSIR {
    /// Variable reference (name is unique due to renumbering).
    Var {
        name: String,
        anns: AnnotationsDecl,
    },

    /// Literal constant (type is unambiguous from the value).
    Const {
        value: UplcConstant,
        anns: AnnotationsDecl,
    },

    /// Function call. Lowering resolves to ExternalVar + Apply chain.
    Call {
        func_path: String,
        args: Vec<PreSIR>,
        anns: AnnotationsDecl,
    },

    /// Binary operation. Lowering resolves `Eq` to the correct Equals* builtin.
    BinOp {
        op: BinOp,
        left: Box<PreSIR>,
        right: Box<PreSIR>,
        anns: AnnotationsDecl,
    },

    /// Let binding.
    Let {
        name: String,
        type_hint: TypeHint,
        value: Box<PreSIR>,
        body: Box<PreSIR>,
        is_rec: bool,
        anns: AnnotationsDecl,
    },

    /// Match expression.
    Match {
        scrutinee: Box<PreSIR>,
        arms: Vec<PreMatchArm>,
        anns: AnnotationsDecl,
    },

    /// If-then-else. `else_branch: None` means unit (if without else).
    IfThenElse {
        cond: Box<PreSIR>,
        then_branch: Box<PreSIR>,
        else_branch: Option<Box<PreSIR>>,
        anns: AnnotationsDecl,
    },

    /// Constructor application: `List::Cons(head, tail)`.
    Construct {
        type_name: String,
        constr_name: String,
        args: Vec<PreSIR>,
        anns: AnnotationsDecl,
    },

    /// Field access: `expr.field`.
    FieldAccess {
        base: Box<PreSIR>,
        field: String,
        anns: AnnotationsDecl,
    },

    /// `FromData::from_data(&x).unwrap()` — data conversion.
    FromData {
        arg: Box<PreSIR>,
        target_type: TypeHint,
        anns: AnnotationsDecl,
    },

    /// `.to_data()` — data conversion.
    ToData {
        arg: Box<PreSIR>,
        source_type: TypeHint,
        anns: AnnotationsDecl,
    },

    /// `panic!("msg")` or error.
    Error {
        message: String,
        anns: AnnotationsDecl,
    },

    /// `require!(cond, "msg")` — if cond then () else error(msg).
    Require {
        cond: Box<PreSIR>,
        message: String,
        anns: AnnotationsDecl,
    },

    /// Negation: `-expr`.
    Negate {
        expr: Box<PreSIR>,
        anns: AnnotationsDecl,
    },
}

// ---------------------------------------------------------------------------
// Pattern matching
// ---------------------------------------------------------------------------

/// A match arm in PreSIR.
#[derive(Debug, Clone)]
pub struct PreMatchArm {
    pub pattern: PrePattern,
    pub body: PreSIR,
    pub anns: AnnotationsDecl,
}

/// A pattern in PreSIR.
#[derive(Debug, Clone)]
pub enum PrePattern {
    /// Constructor pattern: `Type::Variant { field1, field2 }` or `Type::Variant(a, b)`.
    Constr {
        type_name: String,
        constr_name: String,
        bindings: Vec<String>,
    },
    /// Wildcard: `_` or catch-all.
    Wildcard,
}

// ---------------------------------------------------------------------------
// Function definition
// ---------------------------------------------------------------------------

/// A typeclass bound on a generic parameter.
#[derive(Debug, Clone)]
pub struct TypeclassBound {
    /// The trait name (e.g., "PartialEq").
    pub trait_name: String,
    /// 1-based index of the type parameter this constrains.
    pub type_param_index: i64,
}

/// A function parameter.
#[derive(Debug, Clone)]
pub struct PreParam {
    pub name: String,
    pub type_hint: TypeHint,
    /// Original Rust type path string (for DataDecl lookup in TypeDict).
    pub rust_type_path: String,
}

/// A complete function definition as emitted by the `#[compile]` macro.
#[derive(Debug, Clone)]
pub struct PreFnDef {
    pub rust_name: String,
    pub sir_name: String,
    pub module: Option<String>,
    pub params: Vec<PreParam>,
    pub ret_type: TypeHint,
    pub generic_params: Vec<String>,
    pub typeclass_bounds: Vec<TypeclassBound>,
    pub body: PreSIR,
    /// Type dictionary populated at builder time.
    pub type_dict: TypeDict,
}
