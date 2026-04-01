use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::sir::SIR;
use crate::sir_type::{DataDecl, SIRType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourcePos {
    pub file: String,
    pub start_line: i32,
    pub start_column: i32,
    pub end_line: i32,
    pub end_column: i32,
}

impl SourcePos {
    pub fn empty() -> Self {
        SourcePos {
            file: String::new(),
            start_line: 0,
            start_column: 0,
            end_line: 0,
            end_column: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationsDecl {
    pub pos: SourcePos,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub data: HashMap<String, crate::sir::SIR>,
}

impl AnnotationsDecl {
    pub fn empty() -> Self {
        AnnotationsDecl {
            pos: SourcePos::empty(),
            comment: None,
            data: HashMap::new(),
        }
    }

    pub fn with_from_data() -> Self {
        let mut data = HashMap::new();
        data.insert(
            "fromData".to_string(),
            crate::sir::SIR::Const {
                uplc_const: crate::constant::UplcConstant::Bool { value: true },
                tp: crate::sir_type::SIRType::Boolean,
                anns: Self::empty(),
            },
        );
        AnnotationsDecl {
            pos: SourcePos::empty(),
            comment: None,
            data,
        }
    }

    pub fn with_to_data() -> Self {
        let mut data = HashMap::new();
        data.insert(
            "toData".to_string(),
            crate::sir::SIR::Const {
                uplc_const: crate::constant::UplcConstant::Bool { value: true },
                tp: crate::sir_type::SIRType::Boolean,
                anns: Self::empty(),
            },
        );
        AnnotationsDecl {
            pos: SourcePos::empty(),
            comment: None,
            data,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_name: Option<String>,
    pub tp: SIRType,
    pub value: SIR,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub redirect_to_scalus: bool,
}

pub const SIR_VERSION: (i32, i32) = (5, 0);

/// Compiler options passed to the Scalus backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompilerOptions {
    /// Target Cardano protocol version. 9=changPV, 11=vanRossemPV (pv11).
    pub target_protocol_version: i32,
    /// Include error trace messages in compiled UPLC.
    pub generate_error_traces: bool,
    /// Strip all trace calls from compiled UPLC.
    pub remove_traces: bool,
    /// Run UPLC optimizer.
    pub optimize_uplc: bool,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        CompilerOptions {
            target_protocol_version: 11,
            generate_error_traces: true,
            remove_traces: false,
            optimize_uplc: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    pub version: (i32, i32),
    pub name: String,
    pub linked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_backend: Option<String>,
    pub data_decls: BTreeMap<String, DataDecl>,
    pub defs: Vec<Binding>,
    pub anns: AnnotationsDecl,
    #[serde(default)]
    pub options: CompilerOptions,
}
