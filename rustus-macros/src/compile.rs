use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use std::collections::HashMap;
use syn::parse::Parser;
use syn::spanned::Spanned;
use syn::{parse_macro_input, Expr, FnArg, ItemFn, Pat, ReturnType, Stmt};

/// Emit `__anns(line, col)` call from a proc_macro2 Span.
/// Requires the `__anns` closure to be in scope (emitted in the builder function).
fn anns(span: Span) -> TokenStream2 {
    let start = span.start();
    let line = start.line as i32 - 1; // 0-based
    let col = start.column as i32;
    quote! { __anns(#line, #col) }
}

/// Emit empty annotations (for synthetic nodes with no source location).
fn anns_empty() -> TokenStream2 {
    quote! { rustus_core::module::AnnotationsDecl::empty() }
}

/// Context passed through expression compilation, tracking variable types.
struct CompileCtx {
    /// Map from variable name to SIRType-building expression
    var_types: HashMap<String, TokenStream2>,
    /// Map from variable name to original Rust type string (for field access inference)
    var_rust_types: HashMap<String, syn::Type>,
    /// Map from Rust function name to SIR name (for recursive calls with #[compile(name = "...")])
    name_remaps: HashMap<String, String>,
}

pub fn compile_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();
    let register_fn_name = format_ident!("__rustus_compile_{}", fn_name_str);

    // Parse optional attributes: #[compile(module = "...", name = "...")]
    let (module_attr, name_attr) = parse_compile_attrs(attr);
    let sir_name = name_attr.unwrap_or_else(|| fn_name_str.clone());

    // Extract parameter info: (name, type) pairs
    let params: Vec<(&syn::Ident, &syn::Type)> = input_fn
        .sig
        .inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(pat_type) = arg {
                if let Pat::Ident(pat_ident) = pat_type.pat.as_ref() {
                    return Some((&pat_ident.ident, pat_type.ty.as_ref()));
                }
            }
            None
        })
        .collect();

    // Build context with parameter types
    let mut ctx = CompileCtx {
        var_types: HashMap::new(),
        var_rust_types: HashMap::new(),
        name_remaps: HashMap::new(),
    };
    // If SIR name differs from Rust name, add remap for recursive calls
    if sir_name != fn_name_str {
        ctx.name_remaps.insert(fn_name_str.clone(), sir_name.clone());
    }
    for (name, ty) in &params {
        ctx.var_types
            .insert(name.to_string(), rust_type_to_sir_type_expr(ty));
        ctx.var_rust_types
            .insert(name.to_string(), (*ty).clone());
    }

    // Extract return type
    let ret_type_expr = match &input_fn.sig.output {
        ReturnType::Default => quote! { rustus_core::sir_type::SIRType::Unit },
        ReturnType::Type(_, ty) => rust_type_to_sir_type_expr(ty),
    };

    // Build the function type: param1 -> param2 -> ... -> ret
    let fn_type_expr = {
        let mut result = ret_type_expr.clone();
        for (_, ty) in params.iter().rev() {
            let param_type = rust_type_to_sir_type_expr(ty);
            result = quote! {
                rustus_core::sir_type::SIRType::Fun {
                    from: Box::new(#param_type),
                    to: Box::new(#result),
                }
            };
        }
        result
    };

    // Build the SIR body from the function body
    let body_sir = compile_fn_body(&input_fn, &ctx);

    // Build the LamAbs chain: \param1 -> \param2 -> ... -> body
    let sir_expr = {
        let mut result = body_sir;
        for (name, ty) in params.iter().rev() {
            let name_str = name.to_string();
            let param_type = rust_type_to_sir_type_expr(ty);
            result = quote! {
                rustus_core::sir::SIR::LamAbs {
                    param: Box::new(rustus_core::sir::SIR::Var {
                        name: #name_str.to_string(),
                        tp: #param_type,
                        anns: rustus_core::module::AnnotationsDecl::empty(),
                    }),
                    term: Box::new(#result),
                    type_params: vec![],
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            };
        }
        result
    };

    // Wrap with Decl nodes for all types that have DataDecls
    let decl_wraps = gen_decl_wraps(&params);

    let final_sir = if decl_wraps.is_empty() {
        sir_expr
    } else {
        let mut result = sir_expr;
        for decl_expr in decl_wraps {
            result = quote! {
                {
                    let __decl = #decl_expr;
                    if let Some(data) = __decl {
                        rustus_core::sir::SIR::Decl {
                            data,
                            term: Box::new(#result),
                        }
                    } else {
                        #result
                    }
                }
            };
        }
        result
    };

    let register_call = if let Some(ref module) = module_attr {
        let rust_path = fn_name_str.clone();
        quote! {
            ctx.register_binding_in_module(#module, #sir_name, #rust_path, fn_type, sir_value);
        }
    } else {
        quote! {
            ctx.register_binding(#sir_name, fn_type, sir_value);
        }
    };

    let module_literal = match &module_attr {
        Some(m) => quote! { Some(#m) },
        None => quote! { None },
    };

    let expanded = quote! {
        #input_fn

        fn #register_fn_name(ctx: &mut rustus_core::registry::ResolutionContext) {
            #[allow(unused)]
            let __anns = |line: i32, col: i32| rustus_core::module::AnnotationsDecl {
                pos: rustus_core::module::SourcePos {
                    file: file!().to_string(),
                    start_line: line,
                    start_column: col,
                    end_line: line,
                    end_column: col,
                },
                comment: None,
                data: std::collections::HashMap::new(),
            };
            // Pre-register function type so recursive/self calls can resolve via resolve_call
            let fn_type = #fn_type_expr;
            ctx.pre_register_function(#fn_name_str, #sir_name, #module_literal, fn_type.clone());
            let sir_value = #final_sir;
            #register_call
        }

        rustus_core::inventory::submit! {
            rustus_core::registry::PreSirEntry {
                name: #fn_name_str,
                module: #module_literal,
                kind: rustus_core::registry::EntryKind::Function,
                builder: #register_fn_name,
            }
        }
    };

    expanded.into()
}

/// Compile a function body (block of statements) to SIR-building code.
fn compile_fn_body(func: &ItemFn, ctx: &CompileCtx) -> TokenStream2 {
    compile_stmts(&func.block.stmts, ctx)
}

/// Compile a sequence of statements. The last expression is the result.
/// Preceding `let` bindings become `SIR::Let`.
fn compile_stmts(stmts: &[Stmt], ctx: &CompileCtx) -> TokenStream2 {
    if stmts.is_empty() {
        return quote! {
            rustus_core::sir::SIR::Const {
                uplc_const: rustus_core::constant::UplcConstant::Unit,
                tp: rustus_core::sir_type::SIRType::Unit,
                anns: rustus_core::module::AnnotationsDecl::empty(),
            }
        };
    }

    if stmts.len() == 1 {
        return match &stmts[0] {
            Stmt::Expr(expr, _) => compile_expr(expr, ctx),
            Stmt::Local(local) => compile_let_stmt(local, &[], ctx),
            _ => compile_unsupported("statement"),
        };
    }

    // Multiple statements: first is let binding, rest is body
    match &stmts[0] {
        Stmt::Local(local) => compile_let_stmt(local, &stmts[1..], ctx),
        Stmt::Expr(expr, _semi) => {
            // Expression statement followed by more — treat as let _ = expr; rest
            let value = compile_expr(expr, ctx);
            let body = compile_stmts(&stmts[1..], ctx);
            quote! {
                rustus_core::sir::SIR::Let {
                    bindings: vec![rustus_core::sir::Binding {
                        name: "_".to_string(),
                        tp: rustus_core::sir_type::SIRType::Unresolved,
                        value: #value,
                    }],
                    body: Box::new(#body),
                    flags: rustus_core::sir::LetFlags::none(),
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }
        _ => compile_unsupported("statement"),
    }
}

/// Compile `let name: Type = expr;` followed by remaining statements.
fn compile_let_stmt(local: &syn::Local, rest: &[Stmt], ctx: &CompileCtx) -> TokenStream2 {
    // Extract name and optional type annotation from pattern
    let (pat_name, type_expr) = match &local.pat {
        Pat::Ident(ident) => (
            ident.ident.to_string(),
            quote! { rustus_core::sir_type::SIRType::Unresolved },
        ),
        Pat::Type(pat_type) => {
            let name = if let Pat::Ident(ident) = pat_type.pat.as_ref() {
                ident.ident.to_string()
            } else {
                "_".to_string()
            };
            let ty = &pat_type.ty;
            let sir_type = rust_type_to_sir_type_expr(ty);
            (name, sir_type)
        }
        _ => (
            "_".to_string(),
            quote! { rustus_core::sir_type::SIRType::Unresolved },
        ),
    };

    let value = if let Some(init) = &local.init {
        // Check if value is a fromData pattern: FromData::from_data(&x).unwrap()
        // If so, use the type annotation to generate correct types
        if let Some(sir) = try_compile_from_data_with_type(&init.expr, &type_expr, ctx) {
            sir
        } else {
            compile_expr(&init.expr, ctx)
        }
    } else {
        compile_unsupported("let without initializer")
    };

    let body = compile_stmts(rest, ctx);

    quote! {
        rustus_core::sir::SIR::Let {
            bindings: vec![rustus_core::sir::Binding {
                name: #pat_name.to_string(),
                tp: #type_expr,
                value: #value,
            }],
            body: Box::new(#body),
            flags: rustus_core::sir::LetFlags::none(),
            anns: rustus_core::module::AnnotationsDecl::empty(),
        }
    }
}

/// Compile a single expression to SIR-building code.
fn compile_expr(expr: &Expr, ctx: &CompileCtx) -> TokenStream2 {
    match expr {
        Expr::Lit(lit) => compile_lit(lit),
        Expr::Match(m) => compile_match(m, ctx),
        Expr::Field(field) => compile_field_access(field, ctx),
        Expr::Path(path) => compile_path(path, ctx),
        Expr::Binary(binop) => compile_binop(binop, ctx),
        Expr::Call(call) => compile_call(call, ctx),
        Expr::If(if_expr) => compile_if(if_expr, ctx),

        // () — unit value
        Expr::Tuple(tuple) if tuple.elems.is_empty() => {
            quote! {
                rustus_core::sir::SIR::Const {
                    uplc_const: rustus_core::constant::UplcConstant::Unit,
                    tp: rustus_core::sir_type::SIRType::Unit,
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }

        // *expr — deref is transparent in SIR (no pointers on-chain)
        Expr::Unary(unary) => match &unary.op {
            syn::UnOp::Deref(_) => compile_expr(&unary.expr, ctx),
            syn::UnOp::Neg(_) => {
                // -x → SubtractInteger(0, x)
                let inner = compile_expr(&unary.expr, ctx);
                let zero = quote! {
                    rustus_core::sir::SIR::Const {
                        uplc_const: rustus_core::constant::UplcConstant::Integer {
                            value: rustus_core::num_bigint::BigInt::from(0i64),
                        },
                        tp: rustus_core::sir_type::SIRType::Integer,
                        anns: rustus_core::module::AnnotationsDecl::empty(),
                    }
                };
                quote! {
                    rustus_core::sir::SIR::Apply {
                        f: Box::new(rustus_core::sir::SIR::Apply {
                            f: Box::new(rustus_core::sir::SIR::Builtin {
                                builtin_fun: rustus_core::default_fun::DefaultFun::SubtractInteger,
                                tp: rustus_core::sir_type::SIRType::Fun {
                                    from: Box::new(rustus_core::sir_type::SIRType::Integer),
                                    to: Box::new(rustus_core::sir_type::SIRType::Fun {
                                        from: Box::new(rustus_core::sir_type::SIRType::Integer),
                                        to: Box::new(rustus_core::sir_type::SIRType::Integer),
                                    }),
                                },
                                anns: rustus_core::module::AnnotationsDecl::empty(),
                            }),
                            arg: Box::new(#zero),
                            tp: rustus_core::sir_type::SIRType::Fun {
                                from: Box::new(rustus_core::sir_type::SIRType::Integer),
                                to: Box::new(rustus_core::sir_type::SIRType::Integer),
                            },
                            anns: rustus_core::module::AnnotationsDecl::empty(),
                        }),
                        arg: Box::new(#inner),
                        tp: rustus_core::sir_type::SIRType::Integer,
                        anns: rustus_core::module::AnnotationsDecl::empty(),
                    }
                }
            }
            _ => compile_unsupported("unary operator"),
        },

        // (expr) — just unwrap parens
        Expr::Paren(paren) => compile_expr(&paren.expr, ctx),

        // &expr — transparent (no references in SIR)
        Expr::Reference(reference) => compile_expr(&reference.expr, ctx),

        // panic!("msg") → SIR::Error
        Expr::Macro(mac) => compile_macro(mac, ctx),

        // { stmts } — block expression
        Expr::Block(block) => compile_stmts(&block.block.stmts, ctx),

        // x.method(args) — method call
        Expr::MethodCall(mc) => compile_method_call(mc, ctx),

        _ => compile_unsupported("expression"),
    }
}

fn compile_lit(lit: &syn::ExprLit) -> TokenStream2 {
    match &lit.lit {
        syn::Lit::Bool(b) => {
            let val = b.value();
            quote! {
                rustus_core::sir::SIR::Const {
                    uplc_const: rustus_core::constant::UplcConstant::Bool { value: #val },
                    tp: rustus_core::sir_type::SIRType::Boolean,
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }
        syn::Lit::Int(i) => {
            let val: i64 = i.base10_parse().unwrap_or(0);
            quote! {
                rustus_core::sir::SIR::Const {
                    uplc_const: rustus_core::constant::UplcConstant::Integer {
                        value: rustus_core::num_bigint::BigInt::from(#val),
                    },
                    tp: rustus_core::sir_type::SIRType::Integer,
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }
        syn::Lit::Str(s) => {
            let val = s.value();
            quote! {
                rustus_core::sir::SIR::Const {
                    uplc_const: rustus_core::constant::UplcConstant::String { value: #val.to_string() },
                    tp: rustus_core::sir_type::SIRType::String,
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }
        _ => compile_unsupported("literal"),
    }
}

fn compile_binop(binop: &syn::ExprBinary, ctx: &CompileCtx) -> TokenStream2 {
    let left = compile_expr(&binop.left, ctx);
    let right = compile_expr(&binop.right, ctx);

    // Map Rust binary operators to SIR builtins.
    // For ==, we don't know the operand type at macro time — use Unresolved,
    // the typing pass will pick EqualsInteger / EqualsData / EqualsByteString.
    let (builtin, operand_tp, result_tp) = match &binop.op {
        syn::BinOp::Add(_) | syn::BinOp::AddAssign(_) => (
            quote! { rustus_core::default_fun::DefaultFun::AddInteger },
            quote! { rustus_core::sir_type::SIRType::Integer },
            quote! { rustus_core::sir_type::SIRType::Integer },
        ),
        syn::BinOp::Sub(_) | syn::BinOp::SubAssign(_) => (
            quote! { rustus_core::default_fun::DefaultFun::SubtractInteger },
            quote! { rustus_core::sir_type::SIRType::Integer },
            quote! { rustus_core::sir_type::SIRType::Integer },
        ),
        syn::BinOp::Mul(_) | syn::BinOp::MulAssign(_) => (
            quote! { rustus_core::default_fun::DefaultFun::MultiplyInteger },
            quote! { rustus_core::sir_type::SIRType::Integer },
            quote! { rustus_core::sir_type::SIRType::Integer },
        ),
        syn::BinOp::Eq(_) => (
            // Placeholder — typing pass resolves to correct Equals* based on operand type
            quote! { rustus_core::default_fun::DefaultFun::EqualsData },
            quote! { rustus_core::sir_type::SIRType::Unresolved },
            quote! { rustus_core::sir_type::SIRType::Boolean },
        ),
        syn::BinOp::Lt(_) => (
            quote! { rustus_core::default_fun::DefaultFun::LessThanInteger },
            quote! { rustus_core::sir_type::SIRType::Integer },
            quote! { rustus_core::sir_type::SIRType::Boolean },
        ),
        syn::BinOp::Le(_) => (
            quote! { rustus_core::default_fun::DefaultFun::LessThanEqualsInteger },
            quote! { rustus_core::sir_type::SIRType::Integer },
            quote! { rustus_core::sir_type::SIRType::Boolean },
        ),
        _ => return compile_unsupported("binary operator"),
    };

    let builtin_type = quote! {
        rustus_core::sir_type::SIRType::Fun {
            from: Box::new(#operand_tp),
            to: Box::new(rustus_core::sir_type::SIRType::Fun {
                from: Box::new(#operand_tp),
                to: Box::new(#result_tp),
            }),
        }
    };

    quote! {
        rustus_core::sir::SIR::Apply {
            f: Box::new(rustus_core::sir::SIR::Apply {
                f: Box::new(rustus_core::sir::SIR::Builtin {
                    builtin_fun: #builtin,
                    tp: #builtin_type,
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }),
                arg: Box::new(#left),
                tp: rustus_core::sir_type::SIRType::Fun {
                    from: Box::new(#operand_tp),
                    to: Box::new(#result_tp),
                },
                anns: rustus_core::module::AnnotationsDecl::empty(),
            }),
            arg: Box::new(#right),
            tp: #result_tp,
            anns: rustus_core::module::AnnotationsDecl::empty(),
        }
    }
}

fn compile_match(m: &syn::ExprMatch, ctx: &CompileCtx) -> TokenStream2 {
    let match_anns = anns(m.match_token.span);
    let scrutinee = compile_expr(&m.expr, ctx);

    let cases: Vec<TokenStream2> = m
        .arms
        .iter()
        .map(|arm| {
            let arm_anns = anns(arm.pat.span());
            let body = compile_expr(&arm.body, ctx);
            let pattern = compile_pattern(&arm.pat);
            quote! {
                rustus_core::sir::Case {
                    pattern: #pattern,
                    body: #body,
                    anns: #arm_anns,
                }
            }
        })
        .collect();

    quote! {
        rustus_core::sir::SIR::Match {
            scrutinee: Box::new(#scrutinee),
            cases: vec![#(#cases),*],
            tp: rustus_core::sir_type::SIRType::Unresolved,
            anns: #match_anns,
        }
    }
}

fn compile_pattern(pat: &Pat) -> TokenStream2 {
    match pat {
        Pat::Wild(_) => {
            quote! { rustus_core::sir::Pattern::Wildcard }
        }
        Pat::Path(pat_path) => {
            let path = &pat_path.path;
            let (decl_name, constr_name) = extract_enum_path(path);
            quote! {
                rustus_core::sir::Pattern::Constr {
                    constr_name: #constr_name.to_string(),
                    decl_name: #decl_name.to_string(),
                    bindings: vec![],
                    type_params_bindings: vec![],
                }
            }
        }
        Pat::TupleStruct(pat_ts) => {
            let path = &pat_ts.path;
            let (decl_name, constr_name) = extract_enum_path(path);
            let bindings: Vec<String> = pat_ts
                .elems
                .iter()
                .map(|p| {
                    if let Pat::Ident(ident) = p {
                        ident.ident.to_string()
                    } else {
                        "_".to_string()
                    }
                })
                .collect();
            quote! {
                rustus_core::sir::Pattern::Constr {
                    constr_name: #constr_name.to_string(),
                    decl_name: #decl_name.to_string(),
                    bindings: vec![#(#bindings.to_string()),*],
                    type_params_bindings: vec![],
                }
            }
        }
        Pat::Struct(pat_struct) => {
            // Named field pattern: Cons { head, tail } or Cons { head, .. }
            let path = &pat_struct.path;
            let (decl_name, constr_name) = extract_enum_path(path);
            let bindings: Vec<String> = pat_struct
                .fields
                .iter()
                .filter_map(|field| {
                    // Use the member name (field name) as the binding
                    if let syn::Member::Named(ident) = &field.member {
                        Some(ident.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            quote! {
                rustus_core::sir::Pattern::Constr {
                    constr_name: #constr_name.to_string(),
                    decl_name: #decl_name.to_string(),
                    bindings: vec![#(#bindings.to_string()),*],
                    type_params_bindings: vec![],
                }
            }
        }
        Pat::Ident(ident) => {
            let name = ident.ident.to_string();
            quote! {
                rustus_core::sir::Pattern::Constr {
                    constr_name: #name.to_string(),
                    decl_name: "".to_string(),
                    bindings: vec![#name.to_string()],
                    type_params_bindings: vec![],
                }
            }
        }
        _ => {
            quote! { rustus_core::sir::Pattern::Wildcard }
        }
    }
}

fn compile_field_access(field: &syn::ExprField, ctx: &CompileCtx) -> TokenStream2 {
    let field_anns = anns(field.span());
    let base = compile_expr(&field.base, ctx);
    let field_name = match &field.member {
        syn::Member::Named(ident) => ident.to_string(),
        syn::Member::Unnamed(index) => format!("_{}", index.index),
    };

    // Try to infer field type: if base is a known variable with a user type,
    // look up the field's SIRType from its DataDecl at runtime.
    let base_rust_type = get_base_rust_type(&field.base, ctx);
    let field_type_expr = if let Some(base_type) = &base_rust_type {
        let field_name_clone = field_name.clone();
        quote! {
            {
                let decl = <#base_type as rustus_core::sir_type::HasSIRType>::sir_data_decl();
                decl.as_ref().and_then(|d| {
                    d.constructors.iter()
                        .flat_map(|c| c.params.iter())
                        .find(|p| p.name == #field_name_clone)
                        .map(|p| p.tp.clone())
                }).unwrap_or(rustus_core::sir_type::SIRType::Unresolved)
            }
        }
    } else {
        quote! { rustus_core::sir_type::SIRType::Unresolved }
    };

    quote! {
        rustus_core::sir::SIR::Select {
            scrutinee: Box::new(#base),
            field: #field_name.to_string(),
            tp: #field_type_expr,
            anns: #field_anns,
        }
    }
}

/// Try to get the original Rust type for a base expression (used for field access).
fn get_base_rust_type(expr: &Expr, ctx: &CompileCtx) -> Option<syn::Type> {
    if let Expr::Path(path) = expr {
        let segments: Vec<String> = path
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        if segments.len() == 1 {
            return ctx.var_rust_types.get(&segments[0]).cloned();
        }
    }
    None
}

fn compile_path(path: &syn::ExprPath, ctx: &CompileCtx) -> TokenStream2 {
    let span_anns = anns(path.span());
    let segments: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();

    if segments.len() == 1 {
        let name = &segments[0];
        // Remap Rust name → SIR name (e.g. for recursive calls with #[compile(name = "...")])
        let sir_name = ctx.name_remaps.get(name).cloned().unwrap_or(name.clone());
        let type_expr = ctx
            .var_types
            .get(name)
            .cloned()
            .unwrap_or(quote! { rustus_core::sir_type::SIRType::Unresolved });
        quote! {
            rustus_core::sir::SIR::Var {
                name: #sir_name.to_string(),
                tp: #type_expr,
                anns: #span_anns,
            }
        }
    } else if segments.len() == 2 {
        let decl_name = &segments[0];
        let constr_name = format!("{}::{}", segments[0], segments[1]);
        quote! {
            rustus_core::sir::SIR::Constr {
                name: #constr_name.to_string(),
                data: <_ as rustus_core::sir_type::HasSIRType>::sir_data_decl()
                    .unwrap_or_else(|| panic!("no DataDecl for {}", #decl_name)),
                args: vec![],
                tp: rustus_core::sir_type::SIRType::SumCaseClass {
                    decl_name: #decl_name.to_string(),
                    type_args: vec![],
                },
                anns: rustus_core::module::AnnotationsDecl::empty(),
            }
        }
    } else {
        compile_unsupported("multi-segment path")
    }
}

fn compile_call(call: &syn::ExprCall, ctx: &CompileCtx) -> TokenStream2 {
    let call_anns = anns(call.span());
    if let Expr::Path(path) = call.func.as_ref() {
        let segments: Vec<String> = path
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();

        // Recognize BigInt::from(N) as an integer literal
        if segments.len() == 2
            && (segments[0] == "BigInt" || segments[0] == "num_bigint")
            && segments[1] == "from"
            && call.args.len() == 1
        {
            // Try to extract the literal value
            if let Some(Expr::Lit(lit)) = call.args.first() {
                if let syn::Lit::Int(int_lit) = &lit.lit {
                    let val: i64 = int_lit.base10_parse().unwrap_or(0);
                    return quote! {
                        rustus_core::sir::SIR::Const {
                            uplc_const: rustus_core::constant::UplcConstant::Integer {
                                value: rustus_core::num_bigint::BigInt::from(#val),
                            },
                            tp: rustus_core::sir_type::SIRType::Integer,
                            anns: rustus_core::module::AnnotationsDecl::empty(),
                        }
                    };
                }
                // BigInt::from(-N) via Unary neg
            }
            if let Some(Expr::Unary(unary)) = call.args.first() {
                if let syn::UnOp::Neg(_) = &unary.op {
                    if let Expr::Lit(lit) = unary.expr.as_ref() {
                        if let syn::Lit::Int(int_lit) = &lit.lit {
                            let val: i64 = -(int_lit.base10_parse::<i64>().unwrap_or(0));
                            return quote! {
                                rustus_core::sir::SIR::Const {
                                    uplc_const: rustus_core::constant::UplcConstant::Integer {
                                        value: rustus_core::num_bigint::BigInt::from(#val),
                                    },
                                    tp: rustus_core::sir_type::SIRType::Integer,
                                    anns: rustus_core::module::AnnotationsDecl::empty(),
                                }
                            };
                        }
                    }
                }
            }
        }

        // Error on bare T::from_data() or FromData::from_data() without .unwrap()
        if segments.len() == 2 && (segments[1] == "from_data") {
            return syn::Error::new_spanned(
                &call.func,
                "T::from_data() must be followed by .unwrap() in #[compile] functions",
            )
            .to_compile_error()
            .into();
        }

        // Route both Type::func() and bare func() through resolve_call
        if segments.len() == 1 || segments.len() == 2 {
            let rust_path = if segments.len() == 2 {
                format!("{}::{}", segments[0], segments[1])
            } else {
                // Apply name remap for recursive calls with #[compile(name = "...")]
                ctx.name_remaps.get(&segments[0]).cloned().unwrap_or(segments[0].clone())
            };
            let args: Vec<TokenStream2> =
                call.args.iter().map(|a| compile_expr(a, ctx)).collect();
            let mut result = quote! {
                ctx.resolve_call(#rust_path, rustus_core::sir_type::SIRType::Unresolved)
            };
            for arg in args {
                result = quote! {
                    rustus_core::sir::SIR::Apply {
                        f: Box::new(#result),
                        arg: Box::new(#arg),
                        tp: rustus_core::sir_type::SIRType::Unresolved,
                        anns: #call_anns,
                    }
                };
            }
            return result;
        }
    }

    // Fallback: generic call
    let func = compile_expr(&call.func, ctx);
    let args: Vec<TokenStream2> = call.args.iter().map(|a| compile_expr(a, ctx)).collect();

    let mut result = func;
    for arg in args {
        result = quote! {
            rustus_core::sir::SIR::Apply {
                f: Box::new(#result),
                arg: Box::new(#arg),
                tp: rustus_core::sir_type::SIRType::Unresolved,
                anns: #call_anns,
            }
        };
    }
    result
}

fn compile_if(if_expr: &syn::ExprIf, ctx: &CompileCtx) -> TokenStream2 {
    let cond = compile_expr(&if_expr.cond, ctx);
    let then_branch = {
        let stmts = &if_expr.then_branch.stmts;
        if let Some(Stmt::Expr(expr, _)) = stmts.last() {
            compile_expr(expr, ctx)
        } else {
            compile_unsupported("then branch")
        }
    };
    let else_branch = if let Some((_, else_expr)) = &if_expr.else_branch {
        compile_expr(else_expr, ctx)
    } else {
        quote! {
            rustus_core::sir::SIR::Const {
                uplc_const: rustus_core::constant::UplcConstant::Unit,
                tp: rustus_core::sir_type::SIRType::Unit,
                anns: rustus_core::module::AnnotationsDecl::empty(),
            }
        }
    };

    quote! {
        rustus_core::sir::SIR::IfThenElse {
            cond: Box::new(#cond),
            t: Box::new(#then_branch),
            f: Box::new(#else_branch),
            tp: rustus_core::sir_type::SIRType::Boolean,
            anns: rustus_core::module::AnnotationsDecl::empty(),
        }
    }
}

/// Compile panic!("msg") or todo!() → SIR::Error
fn compile_macro(mac: &syn::ExprMacro, ctx: &CompileCtx) -> TokenStream2 {
    let macro_name = mac
        .mac
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default();

    match macro_name.as_str() {
        "require" => {
            // require!(cond, "msg") → if cond then Unit else Error("msg")
            let tokens = mac.mac.tokens.to_string();
            // Parse: "cond , "msg""
            let parts: Vec<&str> = tokens.splitn(2, ',').collect();
            let cond_str = parts[0].trim();
            let msg = parts.get(1).map(|s| s.trim().trim_matches('"')).unwrap_or("require failed");
            // Parse the condition as an expression
            let cond_tokens: proc_macro2::TokenStream = cond_str.parse().unwrap();
            let cond_expr: Expr = syn::parse2(cond_tokens).unwrap();
            let cond_sir = compile_expr(&cond_expr, ctx);
            return quote! {
                rustus_core::sir::SIR::IfThenElse {
                    cond: Box::new(#cond_sir),
                    t: Box::new(rustus_core::sir::SIR::Const {
                        uplc_const: rustus_core::constant::UplcConstant::Unit,
                        tp: rustus_core::sir_type::SIRType::Unit,
                        anns: rustus_core::module::AnnotationsDecl::empty(),
                    }),
                    f: Box::new(rustus_core::sir::SIR::Error {
                        msg: Box::new(rustus_core::sir::SIR::Const {
                            uplc_const: rustus_core::constant::UplcConstant::String {
                                value: #msg.to_string(),
                            },
                            tp: rustus_core::sir_type::SIRType::String,
                            anns: rustus_core::module::AnnotationsDecl::empty(),
                        }),
                        anns: rustus_core::module::AnnotationsDecl::empty(),
                    }),
                    tp: rustus_core::sir_type::SIRType::Unit,
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            };
        }
        "panic" | "todo" | "unimplemented" => {
            // Try to extract the message string
            let msg = mac
                .mac
                .tokens
                .to_string()
                .trim_matches('"')
                .to_string();
            let msg = if msg.is_empty() {
                macro_name.clone()
            } else {
                msg
            };
            quote! {
                rustus_core::sir::SIR::Error {
                    msg: Box::new(rustus_core::sir::SIR::Const {
                        uplc_const: rustus_core::constant::UplcConstant::String {
                            value: #msg.to_string(),
                        },
                        tp: rustus_core::sir_type::SIRType::String,
                        anns: rustus_core::module::AnnotationsDecl::empty(),
                    }),
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }
        _ => compile_unsupported(&format!("macro {}", macro_name)),
    }
}

/// Compile x.method(args) → resolve as function call
fn compile_method_call(mc: &syn::ExprMethodCall, ctx: &CompileCtx) -> TokenStream2 {
    let mc_anns = anns(mc.span());
    let method_name = mc.method.to_string();

    // .unwrap() — check if receiver is T::from_data(expr), emit fromData Apply
    if method_name == "unwrap" {
        if let Some(from_data_sir) = try_compile_from_data_unwrap(&mc.receiver, ctx) {
            return from_data_sir;
        }
        // Otherwise just strip the unwrap
        return compile_expr(&mc.receiver, ctx);
    }

    // .to_data() — emit toData Apply with annotation
    if method_name == "to_data" {
        // Try to get the receiver's type from context (for known variables)
        let source_type = if let Expr::Field(field) = mc.receiver.as_ref() {
            // x.field.to_data() — try to infer field type
            if let Some(base_ty) = get_base_rust_type(&field.base, ctx) {
                let field_name = match &field.member {
                    syn::Member::Named(ident) => ident.to_string(),
                    syn::Member::Unnamed(idx) => format!("_{}", idx.index),
                };
                let field_name_clone = field_name.clone();
                quote! {
                    {
                        let decl = <#base_ty as rustus_core::sir_type::HasSIRType>::sir_data_decl();
                        decl.as_ref().and_then(|d| {
                            d.constructors.iter()
                                .flat_map(|c| c.params.iter())
                                .find(|p| p.name == #field_name_clone)
                                .map(|p| p.tp.clone())
                        }).unwrap_or(rustus_core::sir_type::SIRType::Data)
                    }
                }
            } else {
                quote! { rustus_core::sir_type::SIRType::Data }
            }
        } else if let Expr::Path(path) = mc.receiver.as_ref() {
            // x.to_data() — look up x's type from context
            let segments: Vec<String> = path.path.segments.iter().map(|s| s.ident.to_string()).collect();
            if segments.len() == 1 {
                ctx.var_types.get(&segments[0]).cloned()
                    .unwrap_or(quote! { rustus_core::sir_type::SIRType::Data })
            } else {
                quote! { rustus_core::sir_type::SIRType::Data }
            }
        } else {
            quote! { rustus_core::sir_type::SIRType::Data }
        };

        let arg = compile_expr(&mc.receiver, ctx);
        return quote! {
            rustus_core::sir::SIR::Apply {
                f: Box::new(rustus_core::sir::SIR::ExternalVar {
                    module_name: "scalus.uplc.builtin.internal.UniversalDataConversion$".to_string(),
                    name: "scalus.uplc.builtin.internal.UniversalDataConversion$.toData".to_string(),
                    tp: rustus_core::sir_type::SIRType::Fun {
                        from: Box::new(#source_type),
                        to: Box::new(rustus_core::sir_type::SIRType::Data),
                    },
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }),
                arg: Box::new(#arg),
                tp: rustus_core::sir_type::SIRType::Data,
                anns: rustus_core::module::AnnotationsDecl::with_to_data(),
            }
        };
    }

    // General method call
    let receiver = compile_expr(&mc.receiver, ctx);
    let args: Vec<TokenStream2> = mc.args.iter().map(|a| compile_expr(a, ctx)).collect();

    let mut result = quote! {
        ctx.resolve_call(#method_name, rustus_core::sir_type::SIRType::Unresolved)
    };

    result = quote! {
        rustus_core::sir::SIR::Apply {
            f: Box::new(#result),
            arg: Box::new(#receiver),
            tp: rustus_core::sir_type::SIRType::Unresolved,
            anns: #mc_anns,
        }
    };

    for arg in args {
        result = quote! {
            rustus_core::sir::SIR::Apply {
                f: Box::new(#result),
                arg: Box::new(#arg),
                tp: rustus_core::sir_type::SIRType::Unresolved,
                anns: #mc_anns,
            }
        };
    }
    result
}

/// Check if expr is `T::from_data(&x).unwrap()` with a known type hint from let annotation.
fn try_compile_from_data_with_type(
    expr: &Expr,
    type_hint: &TokenStream2,
    ctx: &CompileCtx,
) -> Option<TokenStream2> {
    // Expect: Expr::MethodCall { receiver: Expr::Call { T::from_data(arg) }, method: "unwrap" }
    if let Expr::MethodCall(mc) = expr {
        if mc.method.to_string() == "unwrap" {
            if let Expr::Call(call) = mc.receiver.as_ref() {
                if let Expr::Path(path) = call.func.as_ref() {
                    let segments: Vec<String> = path.path.segments.iter()
                        .map(|s| s.ident.to_string()).collect();
                    if segments.len() == 2 && segments[1] == "from_data" && call.args.len() == 1 {
                        let arg_expr = call.args.first().unwrap();
                        let inner_arg = if let Expr::Reference(r) = arg_expr {
                            compile_expr(&r.expr, ctx)
                        } else {
                            compile_expr(arg_expr, ctx)
                        };
                        return Some(quote! {
                            rustus_core::sir::SIR::Apply {
                                f: Box::new(rustus_core::sir::SIR::ExternalVar {
                                    module_name: "scalus.uplc.builtin.internal.UniversalDataConversion$".to_string(),
                                    name: "scalus.uplc.builtin.internal.UniversalDataConversion$.fromData".to_string(),
                                    tp: rustus_core::sir_type::SIRType::Fun {
                                        from: Box::new(rustus_core::sir_type::SIRType::Data),
                                        to: Box::new(#type_hint),
                                    },
                                    anns: rustus_core::module::AnnotationsDecl::empty(),
                                }),
                                arg: Box::new(#inner_arg),
                                tp: #type_hint,
                                anns: rustus_core::module::AnnotationsDecl::with_from_data(),
                            }
                        });
                    }
                }
            }
        }
    }
    None
}

/// Check if `receiver` is `T::from_data(&expr)` — if so, emit fromData SIR Apply.
fn try_compile_from_data_unwrap(receiver: &Expr, ctx: &CompileCtx) -> Option<TokenStream2> {
    // receiver should be Expr::Call with func = T::from_data
    if let Expr::Call(call) = receiver {
        if let Expr::Path(path) = call.func.as_ref() {
            let segments: Vec<String> = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            if segments.len() == 2 && segments[1] == "from_data" && call.args.len() == 1 {
                let type_name = &segments[0];
                // Strip the & from &expr if present
                let arg_expr = call.args.first().unwrap();
                let inner_arg = if let Expr::Reference(r) = arg_expr {
                    compile_expr(&r.expr, ctx)
                } else {
                    compile_expr(arg_expr, ctx)
                };
                // Target type: if called as T::from_data, use T's SIRType.
                // If called as FromData::from_data, use Unresolved (resolved by typing pass from let binding).
                let target_type = if *type_name == "FromData" {
                    quote! { rustus_core::sir_type::SIRType::Unresolved }
                } else {
                    let type_path = &path.path.segments.first().unwrap().ident;
                    rust_type_to_sir_type_expr(&syn::parse_quote! { #type_path })
                };
                return Some(quote! {
                    rustus_core::sir::SIR::Apply {
                        f: Box::new(rustus_core::sir::SIR::ExternalVar {
                            module_name: "scalus.uplc.builtin.internal.UniversalDataConversion$".to_string(),
                            name: "scalus.uplc.builtin.internal.UniversalDataConversion$.fromData".to_string(),
                            tp: rustus_core::sir_type::SIRType::Fun {
                                from: Box::new(rustus_core::sir_type::SIRType::Data),
                                to: Box::new(#target_type),
                            },
                            anns: rustus_core::module::AnnotationsDecl::empty(),
                        }),
                        arg: Box::new(#inner_arg),
                        tp: #target_type,
                        anns: rustus_core::module::AnnotationsDecl::with_from_data(),
                    }
                });
            }
        }
    }
    None
}

fn compile_unsupported(what: &str) -> TokenStream2 {
    let msg = format!("unsupported {}", what);
    quote! {
        rustus_core::sir::SIR::Error {
            msg: Box::new(rustus_core::sir::SIR::Const {
                uplc_const: rustus_core::constant::UplcConstant::String {
                    value: #msg.to_string(),
                },
                tp: rustus_core::sir_type::SIRType::String,
                anns: rustus_core::module::AnnotationsDecl::empty(),
            }),
            anns: rustus_core::module::AnnotationsDecl::empty(),
        }
    }
}

fn extract_enum_path(path: &syn::Path) -> (String, String) {
    let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    if segments.len() >= 2 {
        let decl = segments[segments.len() - 2].clone();
        let full = format!("{}::{}", decl, segments[segments.len() - 1]);
        (decl, full)
    } else if segments.len() == 1 {
        (segments[0].clone(), segments[0].clone())
    } else {
        ("unknown".to_string(), "unknown".to_string())
    }
}

fn gen_decl_wraps(params: &[(&syn::Ident, &syn::Type)]) -> Vec<TokenStream2> {
    let mut seen = std::collections::HashSet::new();
    let mut wraps = vec![];

    for (_, ty) in params {
        let type_str = quote!(#ty).to_string().replace(' ', "");
        match type_str.as_str() {
            "bool" | "i64" | "i128" | "Vec<u8>" | "String" | "()" | "Data"
            | "rustus_core::data::Data" => continue,
            _ => {}
        }
        if seen.insert(type_str) {
            wraps.push(quote! {
                <#ty as rustus_core::sir_type::HasSIRType>::sir_data_decl()
            });
        }
    }
    wraps
}

fn rust_type_to_sir_type_expr(ty: &syn::Type) -> TokenStream2 {
    let type_str = quote!(#ty).to_string().replace(' ', "");
    match type_str.as_str() {
        "bool" => quote! { rustus_core::sir_type::SIRType::Boolean },
        "i64" | "i128" | "BigInt" | "num_bigint::BigInt" | "rustus_core::num_bigint::BigInt" => {
            quote! { rustus_core::sir_type::SIRType::Integer }
        }
        "Vec<u8>" => quote! { rustus_core::sir_type::SIRType::ByteString },
        "String" => quote! { rustus_core::sir_type::SIRType::String },
        "()" => quote! { rustus_core::sir_type::SIRType::Unit },
        "Data" | "rustus_core::data::Data" => quote! { rustus_core::sir_type::SIRType::Data },
        _ => quote! { <#ty as rustus_core::sir_type::HasSIRType>::sir_type() },
    }
}

/// Parse #[compile(module = "...", name = "...")] attributes.
/// Returns (Option<module>, Option<name>).
fn parse_compile_attrs(attr: TokenStream) -> (Option<String>, Option<String>) {
    let mut module = None;
    let mut name = None;

    if attr.is_empty() {
        return (module, name);
    }

    // Parse as: module = "...", name = "..."
    let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
    let parsed = match parser.parse(attr) {
        Ok(p) => p,
        Err(_) => return (module, name),
    };

    for meta in parsed {
        if let syn::Meta::NameValue(nv) = meta {
            let key = nv.path.get_ident().map(|i| i.to_string());
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(lit_str),
                ..
            }) = &nv.value
            {
                match key.as_deref() {
                    Some("module") => module = Some(lit_str.value()),
                    Some("name") => name = Some(lit_str.value()),
                    _ => {}
                }
            }
        }
    }

    (module, name)
}
