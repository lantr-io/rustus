use std::collections::{BTreeMap, HashMap};

use crate::module::{self, AnnotationsDecl, Module};
use crate::sir::SIR;
use crate::sir_type::{DataDecl, SIRType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    TypeDecl,
    Function,
}

pub struct PreSirEntry {
    pub name: &'static str,
    pub module: Option<&'static str>,
    pub kind: EntryKind,
    pub builder: fn(&mut ResolutionContext),
}

inventory::collect!(PreSirEntry);

/// Registered function info for cross-module resolution.
#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub module_name: String,
    pub fn_name: String,
    pub tp: SIRType,
}

pub struct ResolutionContext {
    pub data_decls: BTreeMap<String, DataDecl>,
    pub bindings: Vec<module::Binding>,
    /// Functions indexed by (rust_path) → FunctionDef for cross-module resolution.
    pub functions: HashMap<String, FunctionDef>,
}

impl ResolutionContext {
    pub fn new() -> Self {
        ResolutionContext {
            data_decls: BTreeMap::new(),
            bindings: Vec::new(),
            functions: HashMap::new(),
        }
    }

    pub fn register_data_decl(&mut self, name: &str, decl: DataDecl) {
        self.data_decls.insert(name.to_string(), decl);
    }

    pub fn get_data_decl(&self, name: &str) -> Option<&DataDecl> {
        self.data_decls.get(name)
    }

    /// Pre-register a function's type so recursive calls can resolve via resolve_call.
    /// Called before building the SIR body.
    pub fn pre_register_function(
        &mut self,
        rust_name: &str,
        sir_name: &str,
        module: Option<&str>,
        tp: SIRType,
    ) {
        let module_name = module.unwrap_or("").to_string();
        let fdef = FunctionDef {
            module_name,
            fn_name: sir_name.to_string(),
            tp,
        };
        self.functions.insert(rust_name.to_string(), fdef.clone());
        if rust_name != sir_name {
            self.functions.insert(sir_name.to_string(), fdef);
        }
    }

    /// Register a binding with optional module name.
    pub fn register_binding_in_module(
        &mut self,
        module_name: &str,
        fn_name: &str,
        rust_path: &str,
        tp: SIRType,
        value: SIR,
    ) {
        let fdef = FunctionDef {
            module_name: module_name.to_string(),
            fn_name: fn_name.to_string(),
            tp: tp.clone(),
        };
        // Register under both Rust path and SIR name for lookup
        self.functions.insert(rust_path.to_string(), fdef.clone());
        if rust_path != fn_name {
            self.functions.insert(fn_name.to_string(), fdef);
        }
        self.bindings.push(module::Binding {
            name: format!("{}.{}", module_name, fn_name),
            module_name: Some(module_name.to_string()),
            tp,
            value,
        });
    }

    /// Register a binding without module info (backwards compat).
    pub fn register_binding(&mut self, name: &str, tp: SIRType, value: SIR) {
        self.bindings.push(module::Binding {
            name: name.to_string(),
            module_name: None,
            tp,
            value,
        });
    }

    /// Resolve a function call by Rust path (e.g. "List::head").
    /// Returns ExternalVar if found, or Var as fallback.
    /// Resolve a function call by Rust path (e.g. "list::head").
    /// Tries full path first, then just the function name part.
    /// Returns ExternalVar if found, or Var as fallback.
    pub fn resolve_call(&self, rust_path: &str, fallback_tp: SIRType) -> SIR {
        // Try full rust path first, then just the function name
        let fdef = self.functions.get(rust_path).or_else(|| {
            let fn_part = rust_path.rsplit("::").next().unwrap_or(rust_path);
            self.functions.get(fn_part)
        });

        if let Some(fdef) = fdef {
            let full_name = if fdef.module_name.is_empty() {
                fdef.fn_name.clone()
            } else {
                format!("{}.{}", fdef.module_name, fdef.fn_name)
            };
            SIR::ExternalVar {
                module_name: fdef.module_name.clone(),
                name: full_name,
                tp: fdef.tp.clone(),
                anns: AnnotationsDecl::empty(),
            }
        } else {
            SIR::Var {
                name: rust_path.to_string(),
                tp: fallback_tp,
                anns: AnnotationsDecl::empty(),
            }
        }
    }

    pub fn into_module(self, name: &str) -> Module {
        Module {
            version: module::SIR_VERSION,
            name: name.to_string(),
            linked: false,
            require_backend: None,
            data_decls: self.data_decls,
            defs: self.bindings,
            anns: AnnotationsDecl::empty(),
        }
    }
}

/// Collect all registered PreSirEntry builders, execute them in two passes
/// (types first, then functions), and assemble into a Module.
pub fn build_module(name: &str) -> Module {
    let mut ctx = ResolutionContext::new();

    let entries: Vec<&PreSirEntry> = inventory::iter::<PreSirEntry>().collect();

    // Pass 1: type declarations — populate the symbol table
    for e in entries.iter().filter(|e| e.kind == EntryKind::TypeDecl) {
        (e.builder)(&mut ctx);
    }

    // Pass 2: functions — build SIR using resolved types from the symbol table
    for e in entries.iter().filter(|e| e.kind == EntryKind::Function) {
        (e.builder)(&mut ctx);
    }

    let mut module = ctx.into_module(name);

    // Pass 3: renumber TypeVar opt_ids to be globally unique
    crate::typing::renumber_type_vars(&mut module.data_decls);

    // Pass 4: typing — resolve all Unresolved types
    for binding in &mut module.defs {
        if let Err(errors) = crate::typing::type_sir(&mut binding.value, &module.data_decls) {
            eprintln!(
                "Typing errors in {}: {}",
                binding.name,
                errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    module
}
