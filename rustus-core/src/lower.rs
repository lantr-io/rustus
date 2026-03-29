//! Lowering pass: converts PreSIR → SIR using TypeDict and ResolutionContext.
//!
//! This pass runs at builder time (phase 2) when:
//! - All DataDecls are registered (pass 1 complete)
//! - TypeDict is populated with variable types and type info
//! - ResolutionContext has function signatures for cross-module resolution
//!
//! The lowering pass resolves types that the macro couldn't determine:
//! - Variable types from TypeDict
//! - Function call types from function signatures
//! - Field access types from DataDecl lookups
//! - Equality builtins from operand types
//! - Typeclass arguments from concrete arg types

use std::collections::HashMap;

use crate::constant::UplcConstant;
use crate::default_fun::DefaultFun;
use crate::module::AnnotationsDecl;
use crate::pre_sir::*;
use crate::registry::ResolutionContext;
use crate::sir::{self, SIR};
use crate::sir_type::{DataDecl, SIRType, TypeEnv, TypeVar};

fn type_env_from_type_dict(td: &TypeDict) -> TypeEnv {
    let mut env = TypeEnv::new();
    for (name, tp) in &td.vars {
        env.insert(name.clone(), tp.clone());
    }
    env
}

// ---------------------------------------------------------------------------
// LowerCtx: the lowering context
// ---------------------------------------------------------------------------

pub struct LowerCtx<'a> {
    ctx: &'a ResolutionContext,
    type_dict: &'a TypeDict,
    env: TypeEnv,
    /// Name of the typeclass equality variable (e.g., "__eq") if in a generic function.
    typeclass_eq_var: Option<String>,
    /// Current function's Rust name (for detecting recursive calls).
    current_fn_name: Option<String>,
    /// Current function's SIR name (for recursive call resolution).
    current_sir_name: Option<String>,
}

impl<'a> LowerCtx<'a> {
    fn new(ctx: &'a ResolutionContext, type_dict: &'a TypeDict) -> Self {
        LowerCtx {
            ctx,
            type_dict,
            env: type_env_from_type_dict(type_dict),
            typeclass_eq_var: None,
            current_fn_name: None,
            current_sir_name: None,
        }
    }

    fn resolve_type_hint(&self, hint: &TypeHint) -> SIRType {
        resolve_type_hint(hint, self.type_dict, &self.ctx.data_decls)
    }

    // -----------------------------------------------------------------------
    // Expression lowering
    // -----------------------------------------------------------------------

    pub fn lower_expr(&mut self, pre: &PreSIR) -> SIR {
        match pre {
            PreSIR::Var { name, anns } => {
                let tp = self
                    .env
                    .lookup(name)
                    .cloned()
                    .unwrap_or(SIRType::Unresolved);
                SIR::Var {
                    name: name.clone(),
                    tp,
                    anns: anns.clone(),
                }
            }

            PreSIR::Const { value, anns } => {
                let tp = const_type(value);
                SIR::Const {
                    uplc_const: value.clone(),
                    tp,
                    anns: anns.clone(),
                }
            }

            PreSIR::Call {
                func_path,
                args,
                anns,
            } => self.lower_call(func_path, args, anns),

            PreSIR::BinOp {
                op,
                left,
                right,
                anns,
            } => self.lower_binop(op, left, right, anns),

            PreSIR::Let {
                name,
                type_hint,
                value,
                body,
                is_rec,
                anns,
            } => {
                let value_sir = self.lower_expr(value);
                let binding_tp = match type_hint {
                    TypeHint::Infer => crate::typing::sir_type(&value_sir),
                    _ => self.resolve_type_hint(type_hint),
                };
                self.env.push_scope();
                self.env.insert(name.clone(), binding_tp.clone());
                let body_sir = self.lower_expr(body);
                self.env.pop_scope();
                SIR::Let {
                    bindings: vec![sir::Binding {
                        name: name.clone(),
                        tp: binding_tp,
                        value: value_sir,
                    }],
                    body: Box::new(body_sir),
                    flags: sir::LetFlags {
                        is_rec: *is_rec,
                        is_lazy: false,
                    },
                    anns: anns.clone(),
                }
            }

            PreSIR::Match {
                scrutinee,
                arms,
                anns,
            } => self.lower_match(scrutinee, arms, anns),

            PreSIR::IfThenElse {
                cond,
                then_branch,
                else_branch,
                anns,
            } => {
                let cond_sir = self.lower_expr(cond);
                let then_sir = self.lower_expr(then_branch);
                let else_sir = match else_branch {
                    Some(e) => self.lower_expr(e),
                    None => SIR::Const {
                        uplc_const: UplcConstant::Unit,
                        tp: SIRType::Unit,
                        anns: AnnotationsDecl::empty(),
                    },
                };
                let tp = crate::typing::sir_type(&then_sir);
                SIR::IfThenElse {
                    cond: Box::new(cond_sir),
                    t: Box::new(then_sir),
                    f: Box::new(else_sir),
                    tp,
                    anns: anns.clone(),
                }
            }

            PreSIR::Construct {
                type_name,
                constr_name,
                args,
                anns,
            } => self.lower_construct(type_name, constr_name, args, anns),

            PreSIR::FieldAccess {
                base,
                field,
                anns,
            } => {
                let base_sir = self.lower_expr(base);
                let base_tp = crate::typing::sir_type(&base_sir);
                let field_tp = resolve_field_type(&base_tp, field, &self.ctx.data_decls, self.type_dict)
                    .unwrap_or(SIRType::Unresolved);
                SIR::Select {
                    scrutinee: Box::new(base_sir),
                    field: field.clone(),
                    tp: field_tp,
                    anns: anns.clone(),
                }
            }

            PreSIR::FromData {
                arg,
                target_type,
                anns: _,
            } => {
                let arg_sir = self.lower_expr(arg);
                let target_tp = self.resolve_type_hint(target_type);
                SIR::Apply {
                    f: Box::new(SIR::ExternalVar {
                        module_name:
                            "scalus.uplc.builtin.internal.UniversalDataConversion$".to_string(),
                        name: "scalus.uplc.builtin.internal.UniversalDataConversion$.fromData"
                            .to_string(),
                        tp: SIRType::Fun {
                            from: Box::new(SIRType::Data),
                            to: Box::new(target_tp.clone()),
                        },
                        anns: AnnotationsDecl::empty(),
                    }),
                    arg: Box::new(arg_sir),
                    tp: target_tp,
                    anns: AnnotationsDecl::with_from_data(),
                }
            }

            PreSIR::ToData {
                arg,
                source_type,
                anns: _,
            } => {
                let arg_sir = self.lower_expr(arg);
                let source_tp = match source_type {
                    TypeHint::Infer => crate::typing::sir_type(&arg_sir),
                    _ => self.resolve_type_hint(source_type),
                };
                SIR::Apply {
                    f: Box::new(SIR::ExternalVar {
                        module_name:
                            "scalus.uplc.builtin.internal.UniversalDataConversion$".to_string(),
                        name: "scalus.uplc.builtin.internal.UniversalDataConversion$.toData"
                            .to_string(),
                        tp: SIRType::Fun {
                            from: Box::new(source_tp),
                            to: Box::new(SIRType::Data),
                        },
                        anns: AnnotationsDecl::empty(),
                    }),
                    arg: Box::new(arg_sir),
                    tp: SIRType::Data,
                    anns: AnnotationsDecl::with_to_data(),
                }
            }

            PreSIR::Error { message, anns } => SIR::Error {
                msg: Box::new(SIR::Const {
                    uplc_const: UplcConstant::String {
                        value: message.clone(),
                    },
                    tp: SIRType::String,
                    anns: AnnotationsDecl::empty(),
                }),
                anns: anns.clone(),
            },

            PreSIR::Require {
                cond,
                message,
                anns,
            } => {
                let cond_sir = self.lower_expr(cond);
                SIR::IfThenElse {
                    cond: Box::new(cond_sir),
                    t: Box::new(SIR::Const {
                        uplc_const: UplcConstant::Unit,
                        tp: SIRType::Unit,
                        anns: AnnotationsDecl::empty(),
                    }),
                    f: Box::new(SIR::Error {
                        msg: Box::new(SIR::Const {
                            uplc_const: UplcConstant::String {
                                value: message.clone(),
                            },
                            tp: SIRType::String,
                            anns: AnnotationsDecl::empty(),
                        }),
                        anns: AnnotationsDecl::empty(),
                    }),
                    tp: SIRType::Unit,
                    anns: anns.clone(),
                }
            }

            PreSIR::Negate { expr, anns } => {
                let inner = self.lower_expr(expr);
                let zero = SIR::Const {
                    uplc_const: UplcConstant::Integer {
                        value: num_bigint::BigInt::from(0i64),
                    },
                    tp: SIRType::Integer,
                    anns: AnnotationsDecl::empty(),
                };
                make_builtin_apply2(
                    DefaultFun::SubtractInteger,
                    SIRType::Integer,
                    SIRType::Integer,
                    zero,
                    inner,
                    anns,
                )
            }
        }
    }

    // -----------------------------------------------------------------------
    // Call lowering
    // -----------------------------------------------------------------------

    fn lower_call(&mut self, func_path: &str, args: &[PreSIR], anns: &AnnotationsDecl) -> SIR {
        let lowered_args: Vec<SIR> = args.iter().map(|a| self.lower_expr(a)).collect();

        // Detect recursive call: same function, pass typeclass var from scope
        let is_recursive = self
            .current_fn_name
            .as_ref()
            .map(|n| func_path == n)
            .unwrap_or(false)
            || self
                .current_sir_name
                .as_ref()
                .map(|n| func_path == n)
                .unwrap_or(false);

        // Resolve function
        let func_sir = self.ctx.resolve_call(func_path, SIRType::Unresolved);
        let func_tp = crate::typing::sir_type(&func_sir);

        // Build Apply chain with types from function signature
        let mut result = func_sir;
        let mut remaining_tp = func_tp;
        for arg_sir in &lowered_args {
            let apply_tp = peel_fun_result(&remaining_tp);
            result = SIR::Apply {
                f: Box::new(result),
                arg: Box::new(arg_sir.clone()),
                tp: apply_tp.clone(),
                anns: anns.clone(),
            };
            remaining_tp = apply_tp;
        }

        // Append typeclass args
        if is_recursive && self.typeclass_eq_var.is_some() {
            // Recursive call: pass the typeclass var from our scope
            let eq_var = self.typeclass_eq_var.as_ref().unwrap();
            let eq_tp = self
                .env
                .lookup(eq_var)
                .cloned()
                .unwrap_or(SIRType::Unresolved);
            let apply_tp = peel_fun_result(&remaining_tp);
            result = SIR::Apply {
                f: Box::new(result),
                arg: Box::new(SIR::Var {
                    name: eq_var.clone(),
                    tp: eq_tp,
                    anns: AnnotationsDecl::empty(),
                }),
                tp: apply_tp,
                anns: anns.clone(),
            };
        } else {
            // Non-recursive: resolve typeclass args from concrete arg types
            let fdef = self.ctx.lookup_function(func_path);

            if let Some(fdef) = fdef {
                for bound in &fdef.typeclass_bounds {
                    if bound.typeclass == "PartialEq" {
                        // Determine concrete element type from the lowered arg
                        let elem_tp = if bound.elem_arg_index < lowered_args.len() {
                            crate::typing::sir_type(&lowered_args[bound.elem_arg_index])
                        } else {
                            SIRType::Data
                        };
                        let eq_sir = make_equality_builtin(&elem_tp);
                        let apply_tp = peel_fun_result(&remaining_tp);
                        result = SIR::Apply {
                            f: Box::new(result),
                            arg: Box::new(eq_sir),
                            tp: apply_tp.clone(),
                            anns: anns.clone(),
                        };
                        remaining_tp = apply_tp;
                    }
                }
            }
        }

        result
    }

    // -----------------------------------------------------------------------
    // BinOp lowering
    // -----------------------------------------------------------------------

    fn lower_binop(
        &mut self,
        op: &BinOp,
        left: &PreSIR,
        right: &PreSIR,
        anns: &AnnotationsDecl,
    ) -> SIR {
        let left_sir = self.lower_expr(left);
        let right_sir = self.lower_expr(right);

        match op {
            BinOp::Eq => {
                // In generic function with PartialEq: use typeclass var
                if let Some(ref eq_var) = self.typeclass_eq_var {
                    let eq_tp = self
                        .env
                        .lookup(eq_var)
                        .cloned()
                        .unwrap_or(SIRType::Unresolved);
                    let eq_var_sir = SIR::Var {
                        name: eq_var.clone(),
                        tp: eq_tp,
                        anns: AnnotationsDecl::empty(),
                    };
                    return SIR::Apply {
                        f: Box::new(SIR::Apply {
                            f: Box::new(eq_var_sir),
                            arg: Box::new(left_sir),
                            tp: SIRType::Unresolved,
                            anns: anns.clone(),
                        }),
                        arg: Box::new(right_sir),
                        tp: SIRType::Boolean,
                        anns: anns.clone(),
                    };
                }
                // Concrete: resolve from operand type
                let left_tp = crate::typing::sir_type(&left_sir);
                let (builtin, operand_tp) = match &left_tp {
                    SIRType::Integer => (DefaultFun::EqualsInteger, SIRType::Integer),
                    SIRType::ByteString => (DefaultFun::EqualsByteString, SIRType::ByteString),
                    SIRType::String => (DefaultFun::EqualsString, SIRType::String),
                    _ => (DefaultFun::EqualsData, SIRType::Data),
                };
                make_builtin_apply2(
                    builtin,
                    operand_tp,
                    SIRType::Boolean,
                    left_sir,
                    right_sir,
                    anns,
                )
            }
            BinOp::Add => make_builtin_apply2(
                DefaultFun::AddInteger,
                SIRType::Integer,
                SIRType::Integer,
                left_sir,
                right_sir,
                anns,
            ),
            BinOp::Sub => make_builtin_apply2(
                DefaultFun::SubtractInteger,
                SIRType::Integer,
                SIRType::Integer,
                left_sir,
                right_sir,
                anns,
            ),
            BinOp::Mul => make_builtin_apply2(
                DefaultFun::MultiplyInteger,
                SIRType::Integer,
                SIRType::Integer,
                left_sir,
                right_sir,
                anns,
            ),
            BinOp::Lt => make_builtin_apply2(
                DefaultFun::LessThanInteger,
                SIRType::Integer,
                SIRType::Boolean,
                left_sir,
                right_sir,
                anns,
            ),
            BinOp::Le => make_builtin_apply2(
                DefaultFun::LessThanEqualsInteger,
                SIRType::Integer,
                SIRType::Boolean,
                left_sir,
                right_sir,
                anns,
            ),
        }
    }

    // -----------------------------------------------------------------------
    // Match lowering
    // -----------------------------------------------------------------------

    fn lower_match(
        &mut self,
        scrutinee: &PreSIR,
        arms: &[PreMatchArm],
        anns: &AnnotationsDecl,
    ) -> SIR {
        let scrutinee_sir = self.lower_expr(scrutinee);
        let scrutinee_tp = crate::typing::sir_type(&scrutinee_sir);

        // Build substitution from scrutinee type args (for generic type params)
        let subst = build_substitution(&scrutinee_tp, &self.ctx.data_decls, self.type_dict);

        let cases: Vec<sir::Case> = arms
            .iter()
            .map(|arm| self.lower_match_arm(arm, &scrutinee_tp, &subst))
            .collect();

        // Infer result type from first arm
        let tp = cases
            .first()
            .map(|c| crate::typing::sir_type(&c.body))
            .unwrap_or(SIRType::Unresolved);

        SIR::Match {
            scrutinee: Box::new(scrutinee_sir),
            cases,
            tp,
            anns: anns.clone(),
        }
    }

    fn lower_match_arm(
        &mut self,
        arm: &PreMatchArm,
        scrutinee_tp: &SIRType,
        subst: &HashMap<i64, SIRType>,
    ) -> sir::Case {
        self.env.push_scope();

        let pattern = match &arm.pattern {
            PrePattern::Constr {
                type_name,
                constr_name,
                bindings,
            } => {
                // Resolve DataDecl for this type
                let decl = find_data_decl(type_name, scrutinee_tp, &self.ctx.data_decls, self.type_dict);

                if let Some(decl) = decl {
                    // Find the constructor and bind variables
                    let constr = decl
                        .constructors
                        .iter()
                        .find(|c| c.name == *constr_name)
                        .or_else(|| {
                            let suffix = constr_name.rsplit("::").next().unwrap_or(constr_name);
                            decl.constructors
                                .iter()
                                .find(|c| c.name.ends_with(&format!("$.{}", suffix)))
                        });

                    if let Some(constr) = constr {
                        for (i, binding_name) in bindings.iter().enumerate() {
                            if binding_name == "_" {
                                continue;
                            }
                            let param = constr
                                .params
                                .iter()
                                .find(|p| p.name == *binding_name)
                                .or_else(|| constr.params.get(i));
                            if let Some(param) = param {
                                let param_tp = param.tp.substitute(subst);
                                self.env.insert(binding_name.clone(), param_tp);
                            }
                        }
                    }
                }

                // Use the decl_name for the pattern (scalus needs qualified names)
                let decl_name_resolved = decl
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| type_name.clone());

                sir::Pattern::Constr {
                    constr_name: constr_name.clone(),
                    decl_name: decl_name_resolved,
                    bindings: bindings.clone(),
                    type_params_bindings: vec![],
                }
            }
            PrePattern::Wildcard => sir::Pattern::Wildcard,
        };

        let body = self.lower_expr(&arm.body);
        self.env.pop_scope();

        sir::Case {
            pattern,
            body,
            anns: arm.anns.clone(),
        }
    }

    // -----------------------------------------------------------------------
    // Constructor lowering
    // -----------------------------------------------------------------------

    fn lower_construct(
        &mut self,
        type_name: &str,
        constr_name: &str,
        args: &[PreSIR],
        anns: &AnnotationsDecl,
    ) -> SIR {
        let lowered_args: Vec<SIR> = args.iter().map(|a| self.lower_expr(a)).collect();

        // Find the DataDecl
        let decl = find_data_decl_by_name(type_name, &self.ctx.data_decls, self.type_dict);

        let (data, tp) = if let Some(decl) = decl {
            let tp = if decl.constructors.len() == 1 {
                SIRType::CaseClass {
                    constr_name: decl.constructors[0].name.clone(),
                    decl_name: decl.name.clone(),
                    type_args: vec![],
                }
            } else {
                SIRType::SumCaseClass {
                    decl_name: decl.name.clone(),
                    type_args: vec![],
                }
            };
            (decl.clone(), tp)
        } else {
            // Fallback: create a minimal DataDecl
            (
                DataDecl {
                    name: type_name.to_string(),
                    constructors: vec![],
                    type_params: vec![],
                    annotations: AnnotationsDecl::empty(),
                },
                SIRType::SumCaseClass {
                    decl_name: type_name.to_string(),
                    type_args: vec![],
                },
            )
        };

        SIR::Constr {
            name: constr_name.to_string(),
            data,
            args: lowered_args,
            tp,
            anns: anns.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Public API: lower a PreFnDef to SIR and register it
// ---------------------------------------------------------------------------

/// Resolve a TypeHint to SIRType using TypeDict and data_decls (standalone, no LowerCtx needed).
fn resolve_type_hint(
    hint: &TypeHint,
    type_dict: &TypeDict,
    data_decls: &std::collections::BTreeMap<String, DataDecl>,
) -> SIRType {
    match hint {
        TypeHint::Bool => SIRType::Boolean,
        TypeHint::Integer => SIRType::Integer,
        TypeHint::ByteString => SIRType::ByteString,
        TypeHint::String => SIRType::String,
        TypeHint::Unit => SIRType::Unit,
        TypeHint::Data => SIRType::Data,
        TypeHint::TypeParam { name, index } => SIRType::TypeVar {
            name: name.clone(),
            opt_id: Some(*index),
            is_builtin: false,
        },
        TypeHint::Named { rust_path, type_args } => {
            if type_args.is_empty() {
                if let Some(tp) = type_dict.sir_type(rust_path) {
                    return tp.clone();
                }
            }
            let resolved_args: Vec<SIRType> = type_args
                .iter()
                .map(|a| resolve_type_hint(a, type_dict, data_decls))
                .collect();
            if let Some(decl) = find_data_decl_by_name(rust_path, data_decls, type_dict) {
                if decl.constructors.len() == 1 {
                    SIRType::CaseClass {
                        constr_name: decl.constructors[0].name.clone(),
                        decl_name: decl.name.clone(),
                        type_args: resolved_args,
                    }
                } else {
                    SIRType::SumCaseClass {
                        decl_name: decl.name.clone(),
                        type_args: resolved_args,
                    }
                }
            } else {
                SIRType::Unresolved
            }
        }
        TypeHint::Fun { from, to } => SIRType::Fun {
            from: Box::new(resolve_type_hint(from, type_dict, data_decls)),
            to: Box::new(resolve_type_hint(to, type_dict, data_decls)),
        },
        TypeHint::Infer => SIRType::Unresolved,
    }
}

impl ResolutionContext {
    /// Lower a PreFnDef to SIR and register it as a binding.
    pub fn lower_fn_def(&mut self, def: PreFnDef) {
        // Phase 1: Resolve types and pre-register (needs &mut self, no LowerCtx yet)
        let param_types: Vec<SIRType> = def
            .params
            .iter()
            .map(|p| resolve_type_hint(&p.type_hint, &def.type_dict, &self.data_decls))
            .collect();
        let ret_type = resolve_type_hint(&def.ret_type, &def.type_dict, &self.data_decls);

        // Build function type: p1 → p2 → ... → [eq →] ... → ret
        let mut fn_type = ret_type;
        let mut eq_type_for_env: Option<SIRType> = None;

        // Add typeclass param types (innermost)
        for bound in def.typeclass_bounds.iter().rev() {
            if bound.trait_name == "PartialEq" {
                let idx = bound.type_param_index;
                let tp_name = def
                    .generic_params
                    .get((idx - 1) as usize)
                    .cloned()
                    .unwrap_or_else(|| "T".to_string());
                let tv = SIRType::TypeVar {
                    name: tp_name,
                    opt_id: Some(idx),
                    is_builtin: false,
                };
                let eq_type = SIRType::Fun {
                    from: Box::new(tv.clone()),
                    to: Box::new(SIRType::Fun {
                        from: Box::new(tv),
                        to: Box::new(SIRType::Boolean),
                    }),
                };
                fn_type = SIRType::Fun {
                    from: Box::new(eq_type.clone()),
                    to: Box::new(fn_type),
                };
                eq_type_for_env = Some(eq_type);
            }
        }

        // Add normal param types
        for tp in param_types.iter().rev() {
            fn_type = SIRType::Fun {
                from: Box::new(tp.clone()),
                to: Box::new(fn_type),
            };
        }

        // Pre-register for recursive calls
        let tc_bounds: Vec<crate::registry::TypeclassBound> = def
            .typeclass_bounds
            .iter()
            .map(|b| {
                let elem_idx = def
                    .params
                    .iter()
                    .position(|p| {
                        matches!(&p.type_hint, TypeHint::TypeParam { index, .. } if *index == b.type_param_index)
                    })
                    .unwrap_or(0);
                crate::registry::TypeclassBound {
                    typeclass: b.trait_name.clone(),
                    elem_arg_index: elem_idx,
                }
            })
            .collect();

        self.pre_register_function_with_bounds(
            &def.rust_name,
            &def.sir_name,
            def.module.as_deref(),
            fn_type.clone(),
            tc_bounds,
        );

        // Phase 2: Create LowerCtx and lower body (borrows &self immutably)
        let mut lower = LowerCtx::new(self, &def.type_dict);
        lower.current_fn_name = Some(def.rust_name.clone());
        lower.current_sir_name = Some(def.sir_name.clone());

        // Set up typeclass var
        if let Some(eq_type) = &eq_type_for_env {
            lower.typeclass_eq_var = Some("__eq".to_string());
            lower.env.insert("__eq".to_string(), eq_type.clone());
        }

        // Add params to env
        for (param, tp) in def.params.iter().zip(&param_types) {
            lower.env.insert(param.name.clone(), tp.clone());
        }

        // Lower body
        let sir_body = lower.lower_expr(&def.body);

        // Phase 3: Wrap in LamAbs chain (no longer needs LowerCtx)
        let mut sir_value = sir_body;

        // Typeclass param LamAbs (innermost)
        if let Some(eq_type) = eq_type_for_env {
            sir_value = SIR::LamAbs {
                param: Box::new(SIR::Var {
                    name: "__eq".to_string(),
                    tp: eq_type,
                    anns: AnnotationsDecl::empty(),
                }),
                term: Box::new(sir_value),
                type_params: vec![],
                anns: AnnotationsDecl::empty(),
            };
        }

        // Normal param LamAbs (outermost)
        for (i, (param, tp)) in def.params.iter().zip(&param_types).rev().enumerate() {
            let type_params = if i == def.params.len() - 1 && !def.generic_params.is_empty() {
                def.generic_params
                    .iter()
                    .enumerate()
                    .map(|(idx, name)| TypeVar {
                        name: name.clone(),
                        opt_id: Some((idx + 1) as i64),
                        is_builtin: false,
                    })
                    .collect()
            } else {
                vec![]
            };
            sir_value = SIR::LamAbs {
                param: Box::new(SIR::Var {
                    name: param.name.clone(),
                    tp: tp.clone(),
                    anns: AnnotationsDecl::empty(),
                }),
                term: Box::new(sir_value),
                type_params,
                anns: AnnotationsDecl::empty(),
            };
        }

        // Wrap with Decl nodes for DataDecls (from TypeDict, not trait calls)
        let mut seen_decls = std::collections::HashSet::new();
        for (_rust_name, decl) in &def.type_dict.decl_map {
            if seen_decls.insert(decl.name.clone()) {
                sir_value = SIR::Decl {
                    data: decl.clone(),
                    term: Box::new(sir_value),
                };
            }
        }
        // Also include DataDecls from ResolutionContext that params reference
        for param in &def.params {
            if let Some(decl) =
                find_data_decl_by_name(&param.rust_type_path, &self.data_decls, &def.type_dict)
            {
                if seen_decls.insert(decl.name.clone()) {
                    sir_value = SIR::Decl {
                        data: decl.clone(),
                        term: Box::new(sir_value),
                    };
                }
            }
        }

        // Phase 4: Register binding (needs &mut self)
        if let Some(ref module) = def.module {
            self.register_binding_in_module(
                module,
                &def.sir_name,
                &def.rust_name,
                fn_type,
                sir_value,
            );
        } else {
            self.register_binding(&def.sir_name, fn_type, sir_value);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get the type of a UplcConstant.
fn const_type(c: &UplcConstant) -> SIRType {
    match c {
        UplcConstant::Integer { .. } => SIRType::Integer,
        UplcConstant::ByteString { .. } => SIRType::ByteString,
        UplcConstant::String { .. } => SIRType::String,
        UplcConstant::Bool { .. } => SIRType::Boolean,
        UplcConstant::Unit => SIRType::Unit,
    }
}

/// Peel one layer from a Fun type, returning the result type.
fn peel_fun_result(tp: &SIRType) -> SIRType {
    match tp {
        SIRType::Fun { to, .. } => (**to).clone(),
        _ => SIRType::Unresolved,
    }
}

/// Build `Apply(Apply(Builtin(fun), left), right)` with proper types.
fn make_builtin_apply2(
    fun: DefaultFun,
    operand_tp: SIRType,
    result_tp: SIRType,
    left: SIR,
    right: SIR,
    anns: &AnnotationsDecl,
) -> SIR {
    let builtin_tp = SIRType::Fun {
        from: Box::new(operand_tp.clone()),
        to: Box::new(SIRType::Fun {
            from: Box::new(operand_tp.clone()),
            to: Box::new(result_tp.clone()),
        }),
    };
    let partial_tp = SIRType::Fun {
        from: Box::new(operand_tp),
        to: Box::new(result_tp.clone()),
    };
    SIR::Apply {
        f: Box::new(SIR::Apply {
            f: Box::new(SIR::Builtin {
                builtin_fun: fun,
                tp: builtin_tp,
                anns: AnnotationsDecl::empty(),
            }),
            arg: Box::new(left),
            tp: partial_tp,
            anns: anns.clone(),
        }),
        arg: Box::new(right),
        tp: result_tp,
        anns: anns.clone(),
    }
}

/// Create the correct equality builtin for a given SIRType.
fn make_equality_builtin(tp: &SIRType) -> SIR {
    let (fun, operand_tp) = match tp {
        SIRType::Integer => (DefaultFun::EqualsInteger, SIRType::Integer),
        SIRType::ByteString => (DefaultFun::EqualsByteString, SIRType::ByteString),
        SIRType::String => (DefaultFun::EqualsString, SIRType::String),
        _ => (DefaultFun::EqualsData, SIRType::Data),
    };
    SIR::Builtin {
        builtin_fun: fun,
        tp: SIRType::Fun {
            from: Box::new(operand_tp.clone()),
            to: Box::new(SIRType::Fun {
                from: Box::new(operand_tp),
                to: Box::new(SIRType::Boolean),
            }),
        },
        anns: AnnotationsDecl::empty(),
    }
}

/// Resolve a field's type from a scrutinee's type.
fn resolve_field_type(
    scrutinee_tp: &SIRType,
    field_name: &str,
    data_decls: &std::collections::BTreeMap<String, DataDecl>,
    type_dict: &TypeDict,
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

    // Find DataDecl from global registry or TypeDict
    let decl = data_decls
        .get(decl_name)
        .or_else(|| type_dict.decl_map.values().find(|d| d.name == *decl_name))?;

    // Build substitution
    let mut subst = HashMap::new();
    for (tp, ta) in decl.type_params.iter().zip(type_args.iter()) {
        if let Some(id) = tp.opt_id {
            subst.insert(id, ta.clone());
        }
    }

    // Find field
    for constr in &decl.constructors {
        for param in &constr.params {
            if param.name == field_name {
                return Some(param.tp.substitute(&subst));
            }
        }
    }
    None
}

/// Build a substitution map from a scrutinee type's type_args and its DataDecl.
fn build_substitution(
    tp: &SIRType,
    data_decls: &std::collections::BTreeMap<String, DataDecl>,
    type_dict: &TypeDict,
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

    let decl = data_decls
        .get(decl_name)
        .or_else(|| type_dict.decl_map.values().find(|d| d.name == *decl_name));

    if let Some(decl) = decl {
        for (type_param, type_arg) in decl.type_params.iter().zip(type_args.iter()) {
            if let Some(id) = type_param.opt_id {
                subst.insert(id, type_arg.clone());
            }
        }
    }
    subst
}

/// Find a DataDecl by short Rust name, checking TypeDict and global data_decls.
fn find_data_decl_by_name<'a>(
    rust_name: &str,
    data_decls: &'a std::collections::BTreeMap<String, DataDecl>,
    type_dict: &'a TypeDict,
) -> Option<&'a DataDecl> {
    // Try TypeDict first
    if let Some(decl) = type_dict.decl_map.get(rust_name) {
        return Some(decl);
    }
    // Try global data_decls by short name match
    for (decl_name, decl) in data_decls {
        let short = decl_name.rsplit('.').next().unwrap_or(decl_name);
        if short == rust_name {
            return Some(decl);
        }
    }
    None
}

/// Find a DataDecl for a type, using the scrutinee type to find the decl.
fn find_data_decl<'a>(
    type_name: &str,
    scrutinee_tp: &SIRType,
    data_decls: &'a std::collections::BTreeMap<String, DataDecl>,
    type_dict: &'a TypeDict,
) -> Option<&'a DataDecl> {
    // Try from scrutinee type's decl_name first
    let decl_name = match scrutinee_tp {
        SIRType::SumCaseClass { decl_name, .. } => Some(decl_name.as_str()),
        SIRType::CaseClass { decl_name, .. } => Some(decl_name.as_str()),
        _ => None,
    };
    if let Some(name) = decl_name {
        if let Some(decl) = data_decls.get(name) {
            return Some(decl);
        }
    }
    // Fallback to name lookup
    find_data_decl_by_name(type_name, data_decls, type_dict)
}
