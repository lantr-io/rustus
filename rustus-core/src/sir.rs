use serde::{Deserialize, Serialize};

use crate::constant::UplcConstant;
use crate::default_fun::DefaultFun;
use crate::module::AnnotationsDecl;
use crate::sir_type::{DataDecl, SIRType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LetFlags {
    #[serde(default)]
    pub is_rec: bool,
    #[serde(default)]
    pub is_lazy: bool,
}

impl LetFlags {
    pub fn none() -> Self {
        LetFlags {
            is_rec: false,
            is_lazy: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Pattern {
    Constr {
        constr_name: String,
        decl_name: String,
        bindings: Vec<String>,
        type_params_bindings: Vec<SIRType>,
    },
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Case {
    pub pattern: Pattern,
    pub body: SIR,
    pub anns: AnnotationsDecl,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub name: String,
    pub tp: SIRType,
    pub value: SIR,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SIR {
    Var {
        name: String,
        tp: SIRType,
        anns: AnnotationsDecl,
    },
    ExternalVar {
        module_name: String,
        name: String,
        tp: SIRType,
        anns: AnnotationsDecl,
    },
    Const {
        uplc_const: UplcConstant,
        tp: SIRType,
        anns: AnnotationsDecl,
    },
    LamAbs {
        param: Box<SIR>,
        term: Box<SIR>,
        type_params: Vec<crate::sir_type::TypeVar>,
        anns: AnnotationsDecl,
    },
    Apply {
        f: Box<SIR>,
        arg: Box<SIR>,
        tp: SIRType,
        anns: AnnotationsDecl,
    },
    Let {
        bindings: Vec<Binding>,
        body: Box<SIR>,
        flags: LetFlags,
        anns: AnnotationsDecl,
    },
    Constr {
        name: String,
        data: DataDecl,
        args: Vec<SIR>,
        tp: SIRType,
        anns: AnnotationsDecl,
    },
    Match {
        scrutinee: Box<SIR>,
        cases: Vec<Case>,
        tp: SIRType,
        anns: AnnotationsDecl,
    },
    IfThenElse {
        cond: Box<SIR>,
        t: Box<SIR>,
        f: Box<SIR>,
        tp: SIRType,
        anns: AnnotationsDecl,
    },
    Builtin {
        builtin_fun: DefaultFun,
        tp: SIRType,
        anns: AnnotationsDecl,
    },
    Error {
        msg: Box<SIR>,
        anns: AnnotationsDecl,
    },
    Decl {
        data: DataDecl,
        term: Box<SIR>,
    },
    Select {
        scrutinee: Box<SIR>,
        field: String,
        tp: SIRType,
        anns: AnnotationsDecl,
    },
}
