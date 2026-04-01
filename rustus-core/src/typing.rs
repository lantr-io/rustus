//! Typing pass: walks the SIR tree and resolves `SIRType::Unresolved` types.
//!
//! The pass maintains a type environment (variable name → SIRType) and uses
//! the symbol table (DataDecl registry) to look up constructor field types.
//!
//! Before typing, `renumber_type_vars` assigns globally unique opt_ids to all
//! TypeVars across all DataDecls, so they don't collide during unification.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use crate::sir::{Case, Pattern, SIR};
use crate::sir_type::{DataDecl, SIRType};

// ---------------------------------------------------------------------------
// TypeVar renumbering: assign globally unique opt_ids across all DataDecls
// ---------------------------------------------------------------------------

/// Assign globally unique opt_ids to all TypeVars in the symbol table.
/// Also rewrites any SIRType references within DataDecls (constructor params,
/// parent_type_args) to use the new IDs.
pub fn renumber_type_vars(data_decls: &mut BTreeMap<String, DataDecl>) {
    let mut counter: i64 = 1;

    for decl in data_decls.values_mut() {
        if decl.type_params.is_empty() {
            continue;
        }

        // Build remap: old_id → new_id for this DataDecl's type params
        let mut remap: HashMap<i64, i64> = HashMap::new();
        for tp in &mut decl.type_params {
            if let Some(old_id) = tp.opt_id {
                let new_id = counter;
                counter += 1;
                remap.insert(old_id, new_id);
                tp.opt_id = Some(new_id);
            }
        }

        if remap.is_empty() {
            continue;
        }

        // Apply remap to all constructors
        for constr in &mut decl.constructors {
            for tp in &mut constr.type_params {
                if let Some(old_id) = tp.opt_id {
                    if let Some(&new_id) = remap.get(&old_id) {
                        tp.opt_id = Some(new_id);
                    }
                }
            }
            for pta in &mut constr.parent_type_args {
                remap_sir_type(pta, &remap);
            }
            for param in &mut constr.params {
                remap_sir_type(&mut param.tp, &remap);
            }
        }
    }
}

/// Remap opt_ids in a SIRType tree.
fn remap_sir_type(tp: &mut SIRType, remap: &HashMap<i64, i64>) {
    match tp {
        SIRType::TypeVar { opt_id, .. } => {
            if let Some(old_id) = opt_id {
                if let Some(&new_id) = remap.get(old_id) {
                    *opt_id = Some(new_id);
                }
            }
        }
        SIRType::Fun { from, to } => {
            remap_sir_type(from, remap);
            remap_sir_type(to, remap);
        }
        SIRType::SumCaseClass { type_args, .. } | SIRType::CaseClass { type_args, .. } => {
            for arg in type_args {
                remap_sir_type(arg, remap);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone)]
pub struct TypingError {
    pub kind: TypingErrorKind,
    pub location: String,
}

#[derive(Debug, Clone)]
pub enum TypingErrorKind {
    UnresolvedType { context: String },
    UnknownVariable { name: String },
    UnknownDataDecl { name: String },
    UnknownConstructor { constr_name: String, decl_name: String },
    NotAFunctionType { tp: SIRType },
    NotADataType { tp: SIRType, context: String },
}

impl fmt::Display for TypingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let loc = if self.location.is_empty() {
            String::new()
        } else {
            format!(" at {}", self.location)
        };
        match &self.kind {
            TypingErrorKind::UnresolvedType { context } => {
                write!(f, "unresolved type in {}{}", context, loc)
            }
            TypingErrorKind::UnknownVariable { name } => {
                write!(f, "unknown variable '{}'{}", name, loc)
            }
            TypingErrorKind::UnknownDataDecl { name } => {
                write!(f, "type '{}' is not registered with #[derive(ToData)]{}", name, loc)
            }
            TypingErrorKind::UnknownConstructor {
                constr_name,
                decl_name,
            } => write!(
                f,
                "unknown constructor '{}' in type '{}'{}", constr_name, decl_name, loc
            ),
            TypingErrorKind::NotAFunctionType { tp } => {
                write!(f, "expected function type, got {:?}{}", tp, loc)
            }
            TypingErrorKind::NotADataType { tp, context } => {
                write!(f, "expected data type in {}, got {:?}{}", context, tp, loc)
            }
        }
    }
}

fn format_location(anns: &crate::module::AnnotationsDecl) -> String {
    let pos = &anns.pos;
    if pos.file.is_empty() && pos.start_line == 0 {
        String::new()
    } else {
        format!("{}:{}:{}", pos.file, pos.start_line + 1, pos.start_column)
    }
}

fn make_error(kind: TypingErrorKind, anns: &crate::module::AnnotationsDecl) -> TypingError {
    TypingError {
        kind,
        location: format_location(anns),
    }
}

fn make_error_no_loc(kind: TypingErrorKind) -> TypingError {
    TypingError {
        kind,
        location: String::new(),
    }
}

use crate::sir_type::TypeEnv;

/// Run the typing pass on a SIR tree, resolving all Unresolved types.
pub fn type_sir(
    sir: &mut SIR,
    symbol_table: &BTreeMap<String, DataDecl>,
) -> Result<(), Vec<TypingError>> {
    let mut env = TypeEnv::new();
    let mut errors = Vec::new();
    type_node(sir, &mut env, symbol_table, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Verify that no Unresolved types remain in the SIR tree.
pub fn verify_complete(sir: &SIR) -> Result<(), Vec<TypingError>> {
    let mut errors = Vec::new();
    check_node(sir, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Get the type of a SIR node.
pub fn sir_type(sir: &SIR) -> SIRType {
    match sir {
        SIR::Var { tp, .. } => tp.clone(),
        SIR::ExternalVar { tp, .. } => tp.clone(),
        SIR::Const { tp, .. } => tp.clone(),
        SIR::LamAbs { param, term, .. } => {
            let param_tp = sir_type(param);
            let body_tp = sir_type(term);
            SIRType::Fun {
                from: Box::new(param_tp),
                to: Box::new(body_tp),
            }
        }
        SIR::Apply { tp, .. } => tp.clone(),
        SIR::Let { body, .. } => sir_type(body),
        SIR::Constr { tp, .. } => tp.clone(),
        SIR::Match { tp, .. } => tp.clone(),
        SIR::IfThenElse { tp, .. } => tp.clone(),
        SIR::Builtin { tp, .. } => tp.clone(),
        SIR::Error { .. } => SIRType::TypeNothing,
        SIR::Decl { term, .. } => sir_type(term),
        SIR::Select { tp, .. } => tp.clone(),
    }
}

/// Type a single SIR node, mutating it in place.
fn type_node(
    sir: &mut SIR,
    env: &mut TypeEnv,
    st: &BTreeMap<String, DataDecl>,
    errors: &mut Vec<TypingError>,
) {
    match sir {
        SIR::Var { name, tp, anns, .. } => {
            if *tp == SIRType::Unresolved {
                if let Some(resolved) = env.lookup(name) {
                    *tp = resolved.clone();
                } else {
                    errors.push(make_error(
                        TypingErrorKind::UnknownVariable { name: name.clone() },
                        anns,
                    ));
                }
            }
        }

        SIR::ExternalVar { .. } | SIR::Const { .. } | SIR::Builtin { .. } => {
            // Already typed
        }

        SIR::LamAbs {
            param, term, ..
        } => {
            // Type param (should already be typed from macro)
            type_node(param, env, st, errors);
            // Add param to env
            if let SIR::Var { name, tp, .. } = param.as_ref() {
                env.push_scope();
                env.insert(name.clone(), tp.clone());
                type_node(term, env, st, errors);
                env.pop_scope();
            } else {
                type_node(term, env, st, errors);
            }
        }

        SIR::Apply { f, arg, tp, anns, .. } => {
            type_node(f, env, st, errors);
            type_node(arg, env, st, errors);

            if *tp == SIRType::Unresolved {
                let f_tp = sir_type(f);
                match &f_tp {
                    SIRType::Fun { to, .. } => *tp = *to.clone(),
                    _ => {
                        if f_tp != SIRType::Unresolved {
                            errors.push(make_error(
                                TypingErrorKind::NotAFunctionType { tp: f_tp },
                                anns,
                            ));
                        }
                    }
                }
            }
        }

        SIR::Let {
            bindings,
            body,
            ..
        } => {
            env.push_scope();
            for binding in bindings.iter_mut() {
                type_node(&mut binding.value, env, st, errors);
                if binding.tp == SIRType::Unresolved {
                    binding.tp = sir_type(&binding.value);
                }
                env.insert(binding.name.clone(), binding.tp.clone());
            }
            type_node(body, env, st, errors);
            env.pop_scope();
        }

        SIR::Match {
            scrutinee,
            cases,
            tp,
            ..
        } => {
            type_node(scrutinee, env, st, errors);
            let scrutinee_tp = sir_type(scrutinee);

            // Build substitution map from scrutinee's type_args
            let subst = build_substitution(&scrutinee_tp, st);

            for case in cases.iter_mut() {
                type_case(case, &scrutinee_tp, &subst, env, st, errors);
            }

            // Infer match result type from first non-bottom arm
            if *tp == SIRType::Unresolved {
                for case in cases.iter() {
                    let case_tp = sir_type(&case.body);
                    if case_tp != SIRType::Unresolved && case_tp != SIRType::TypeNothing {
                        *tp = case_tp;
                        break;
                    }
                }
            }
        }

        SIR::IfThenElse { cond, t, f, tp, .. } => {
            type_node(cond, env, st, errors);
            type_node(t, env, st, errors);
            type_node(f, env, st, errors);
            if *tp == SIRType::Unresolved {
                let t_tp = sir_type(t);
                if t_tp != SIRType::Unresolved && t_tp != SIRType::TypeNothing {
                    *tp = t_tp;
                } else {
                    let f_tp = sir_type(f);
                    if f_tp != SIRType::Unresolved && f_tp != SIRType::TypeNothing {
                        *tp = f_tp;
                    }
                }
            }
        }

        SIR::Select {
            scrutinee,
            field,
            tp,
            anns,
            ..
        } => {
            type_node(scrutinee, env, st, errors);
            if *tp == SIRType::Unresolved {
                let scr_tp = sir_type(scrutinee);
                if let Some(field_tp) = resolve_field_type(&scr_tp, field, st) {
                    *tp = field_tp;
                } else {
                    errors.push(make_error(
                        TypingErrorKind::NotADataType {
                            tp: scr_tp,
                            context: format!("select .{}", field),
                        },
                        anns,
                    ));
                }
            }
        }

        SIR::Constr { args, .. } => {
            for arg in args.iter_mut() {
                type_node(arg, env, st, errors);
            }
        }

        SIR::Error { msg, .. } => {
            type_node(msg, env, st, errors);
        }

        SIR::Decl { term, .. } => {
            type_node(term, env, st, errors);
        }
    }
}

/// Type a match case: add pattern bindings to env, type the body.
fn type_case(
    case: &mut Case,
    scrutinee_tp: &SIRType,
    subst: &HashMap<i64, SIRType>,
    env: &mut TypeEnv,
    st: &BTreeMap<String, DataDecl>,
    errors: &mut Vec<TypingError>,
) {
    env.push_scope();

    match &case.pattern {
        Pattern::Constr {
            constr_name,
            decl_name,
            bindings,
            ..
        } => {
            // Look up DataDecl — try exact match first, then use scrutinee's decl
            let resolved_decl = st.get(decl_name).or_else(|| {
                // Fallback: use the scrutinee's DataDecl (covers short name mismatch)
                match scrutinee_tp {
                    SIRType::SumCaseClass { decl_name: scr_decl, .. } => st.get(scr_decl),
                    SIRType::CaseClass { decl_name: scr_decl, .. } => st.get(scr_decl),
                    _ => None,
                }
            });
            if let Some(decl) = resolved_decl {
                // Find constructor: try exact name, then suffix match (e.g. "List::Cons" matches "...List$.Cons")
                let constr = decl.constructors.iter()
                    .find(|c| c.name == *constr_name)
                    .or_else(|| {
                        let suffix = constr_name.rsplit("::").next().unwrap_or(constr_name);
                        decl.constructors.iter().find(|c| c.name.ends_with(&format!("$.{}", suffix)))
                    });
                if let Some(constr) = constr {
                    // Map bindings to constructor param types.
                    // Try by name first (named fields), fall back to positional (tuple fields).
                    for (i, binding_name) in bindings.iter().enumerate() {
                        if binding_name == "_" {
                            continue;
                        }
                        // Try by field name first
                        let param = constr
                            .params
                            .iter()
                            .find(|p| p.name == *binding_name)
                            .or_else(|| constr.params.get(i));
                        if let Some(param) = param {
                            let param_tp = param.tp.substitute(subst);
                            env.insert(binding_name.clone(), param_tp);
                        }
                    }
                } else {
                    errors.push(make_error(
                        TypingErrorKind::UnknownConstructor {
                            constr_name: constr_name.clone(),
                            decl_name: decl_name.clone(),
                        },
                        &case.anns,
                    ));
                }
            } else if !decl_name.is_empty() {
                errors.push(make_error(
                    TypingErrorKind::UnknownDataDecl {
                        name: decl_name.clone(),
                    },
                    &case.anns,
                ));
            }
        }
        Pattern::Wildcard => {}
    }

    type_node(&mut case.body, env, st, errors);
    env.pop_scope();
}

fn build_substitution(
    tp: &SIRType,
    st: &BTreeMap<String, DataDecl>,
) -> HashMap<i64, SIRType> {
    let mut subst = HashMap::new();
    let (decl_name, type_args) = match tp {
        SIRType::SumCaseClass {
            decl_name,
            type_args,
        } => (decl_name, type_args),
        SIRType::CaseClass {
            decl_name,
            type_args,
            ..
        } => (decl_name, type_args),
        _ => return subst,
    };

    if let Some(decl) = st.get(decl_name) {
        for (type_param, type_arg) in decl.type_params.iter().zip(type_args.iter()) {
            if let Some(id) = type_param.opt_id {
                subst.insert(id, type_arg.clone());
            }
        }
    }
    subst
}

/// Resolve a field's type given the scrutinee type.
fn resolve_field_type(
    scrutinee_tp: &SIRType,
    field_name: &str,
    st: &BTreeMap<String, DataDecl>,
) -> Option<SIRType> {
    let (decl_name, type_args) = match scrutinee_tp {
        SIRType::SumCaseClass {
            decl_name,
            type_args,
        } => (decl_name, type_args),
        SIRType::CaseClass {
            decl_name,
            type_args,
            ..
        } => (decl_name, type_args),
        _ => return None,
    };

    let decl = st.get(decl_name)?;

    // Build substitution
    let mut subst = HashMap::new();
    for (tp, ta) in decl.type_params.iter().zip(type_args.iter()) {
        if let Some(id) = tp.opt_id {
            subst.insert(id, ta.clone());
        }
    }

    // Find the field in any constructor
    for constr in &decl.constructors {
        for param in &constr.params {
            if param.name == field_name {
                return Some(param.tp.substitute(&subst));
            }
        }
    }
    None
}

/// Walk the tree checking for remaining Unresolved types.
fn check_node(sir: &SIR, errors: &mut Vec<TypingError>) {
    match sir {
        SIR::Var { tp, name, anns } => {
            if tp.has_unresolved() {
                errors.push(make_error(
                    TypingErrorKind::UnresolvedType { context: format!("variable '{}'", name) },
                    anns,
                ));
            }
        }
        SIR::ExternalVar { tp, name, module_name, anns } => {
            // Skip type check for UniversalDataConversion (fromData/toData) —
            // the linker handles their types
            if module_name != "scalus.uplc.builtin.internal.UniversalDataConversion$"
                && tp.has_unresolved()
            {
                errors.push(make_error(
                    TypingErrorKind::UnresolvedType { context: format!("external variable '{}'", name) },
                    anns,
                ));
            }
        }
        SIR::Const { tp, anns, .. } => {
            if tp.has_unresolved() {
                errors.push(make_error(
                    TypingErrorKind::UnresolvedType { context: "constant".to_string() },
                    anns,
                ));
            }
        }
        SIR::LamAbs { param, term, .. } => {
            check_node(param, errors);
            check_node(term, errors);
        }
        SIR::Apply { f, arg, tp, anns } => {
            // Skip type check for fromData/toData Apply nodes (linker handles them)
            let is_data_conversion = matches!(f.as_ref(),
                SIR::ExternalVar { module_name, .. }
                if module_name == "scalus.uplc.builtin.internal.UniversalDataConversion$"
            );
            if !is_data_conversion && tp.has_unresolved() {
                let f_desc = match f.as_ref() {
                    SIR::Var { name, .. } => format!("call to '{}'", name),
                    SIR::ExternalVar { name, .. } => format!("call to '{}'", name),
                    SIR::Builtin { builtin_fun, .. } => format!("builtin '{:?}'", builtin_fun),
                    _ => "function application".to_string(),
                };
                errors.push(make_error(
                    TypingErrorKind::UnresolvedType { context: f_desc },
                    anns,
                ));
            }
            check_node(f, errors);
            check_node(arg, errors);
        }
        SIR::Let { bindings, body, .. } => {
            for b in bindings {
                if b.tp.has_unresolved() {
                    errors.push(make_error_no_loc(
                        TypingErrorKind::UnresolvedType { context: format!("let binding '{}'", b.name) },
                    ));
                }
                check_node(&b.value, errors);
            }
            check_node(body, errors);
        }
        SIR::Match { scrutinee, cases, tp, anns } => {
            if tp.has_unresolved() {
                errors.push(make_error(
                    TypingErrorKind::UnresolvedType { context: "match result".to_string() },
                    anns,
                ));
            }
            check_node(scrutinee, errors);
            for c in cases {
                check_node(&c.body, errors);
            }
        }
        SIR::IfThenElse { cond, t, f, tp, anns } => {
            if tp.has_unresolved() {
                errors.push(make_error(
                    TypingErrorKind::UnresolvedType { context: "if/else result".to_string() },
                    anns,
                ));
            }
            check_node(cond, errors);
            check_node(t, errors);
            check_node(f, errors);
        }
        SIR::Select { scrutinee, tp, field, anns } => {
            if tp.has_unresolved() {
                errors.push(make_error(
                    TypingErrorKind::UnresolvedType { context: format!("field access '.{}'", field) },
                    anns,
                ));
            }
            check_node(scrutinee, errors);
        }
        SIR::Builtin { tp, anns, .. } => {
            if tp.has_unresolved() {
                errors.push(make_error(
                    TypingErrorKind::UnresolvedType { context: "builtin".to_string() },
                    anns,
                ));
            }
        }
        SIR::Constr { args, .. } => {
            for a in args {
                check_node(a, errors);
            }
        }
        SIR::Error { msg, .. } => check_node(msg, errors),
        SIR::Decl { term, .. } => check_node(term, errors),
    }
}
