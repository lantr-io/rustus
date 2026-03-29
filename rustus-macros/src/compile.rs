use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use std::collections::{HashMap, HashSet};
use syn::parse::Parser;
use syn::spanned::Spanned;
use syn::{parse_macro_input, Expr, FnArg, ItemFn, Pat, ReturnType, Stmt};

/// Emit `__anns(line, col)` call from a proc_macro2 Span.
fn anns(span: Span) -> TokenStream2 {
    let start = span.start();
    let line = start.line as i32 - 1; // 0-based
    let col = start.column as i32;
    quote! { __anns(#line, #col) }
}

// ---------------------------------------------------------------------------
// CompileCtx: tracks type registrations and variable renaming
// ---------------------------------------------------------------------------

struct CompileCtx {
    /// TypeDict registration calls to emit at builder time.
    type_registrations: Vec<TokenStream2>,
    /// Track which types have been registered (avoid duplicates).
    registered_types: HashSet<String>,
    /// Generic type parameter names (e.g., ["T"]).
    generic_type_params: Vec<String>,
    /// Current function's Rust name (for detecting recursive calls).
    current_fn_name: Option<String>,
    /// Map from Rust function name to SIR name (for recursive calls).
    name_remaps: HashMap<String, String>,
    /// Per-name counter for variable renumbering (eliminates shadowing).
    name_counts: HashMap<String, u32>,
    /// Scoped stack: original name → current renamed name.
    name_scopes: Vec<HashMap<String, String>>,
}

impl CompileCtx {
    fn new() -> Self {
        CompileCtx {
            type_registrations: Vec::new(),
            registered_types: HashSet::new(),
            generic_type_params: Vec::new(),
            current_fn_name: None,
            name_remaps: HashMap::new(),
            name_counts: HashMap::new(),
            name_scopes: vec![HashMap::new()],
        }
    }

    // --- Variable renaming ---

    fn fresh_var(&mut self, original: &str) -> String {
        let count = self.name_counts.entry(original.to_string()).or_insert(0);
        let renamed = if *count == 0 {
            original.to_string()
        } else {
            format!("{}_{}", original, count)
        };
        *count += 1;
        if let Some(scope) = self.name_scopes.last_mut() {
            scope.insert(original.to_string(), renamed.clone());
        }
        renamed
    }

    fn resolve_var(&self, original: &str) -> String {
        for scope in self.name_scopes.iter().rev() {
            if let Some(renamed) = scope.get(original) {
                return renamed.clone();
            }
        }
        // Not found in any scope — could be a function param (registered with original name)
        original.to_string()
    }

    fn push_scope(&mut self) {
        self.name_scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.name_scopes.pop();
    }

    // --- Type registration ---

    /// Register a variable's type in the TypeDict (emits code that runs at builder time).
    fn register_var_type(&mut self, renamed: &str, rust_type: &syn::Type) {
        let type_call = self.make_sir_type_call(rust_type);
        let name = renamed.to_string();
        self.type_registrations.push(quote! {
            __td.register_var(#name, #type_call);
        });
    }

    /// Register a Rust type's SIRType + DataDecl in the TypeDict.
    fn register_rust_type(&mut self, rust_type: &syn::Type) {
        let type_str = quote!(#rust_type).to_string().replace(' ', "");
        // Skip primitives and generic params
        match type_str.as_str() {
            "bool" | "i64" | "i128" | "Vec<u8>" | "ByteString" | "String" | "()" | "Data"
            | "rustus_core::data::Data" | "BigInt" | "num_bigint::BigInt" => return,
            _ => {}
        }
        if self.generic_type_params.contains(&type_str) {
            return;
        }
        if !self.registered_types.insert(type_str.clone()) {
            return; // already registered
        }
        let replaced = self.replace_generics(rust_type);
        self.type_registrations.push(quote! {
            __td.register_type_info(
                #type_str,
                <#replaced as rustus_core::sir_type::HasSIRType>::sir_type(),
                <#replaced as rustus_core::sir_type::HasSIRType>::sir_data_decl(),
            );
        });
    }

    /// Generate `<T as HasSIRType>::sir_type()` call, replacing generics with TypeParam.
    fn make_sir_type_call(&self, ty: &syn::Type) -> TokenStream2 {
        let type_str = quote!(#ty).to_string().replace(' ', "");
        match type_str.as_str() {
            "bool" => quote! { rustus_core::sir_type::SIRType::Boolean },
            "i64" | "i128" | "BigInt" | "num_bigint::BigInt" | "rustus_core::num_bigint::BigInt" => {
                quote! { rustus_core::sir_type::SIRType::Integer }
            }
            "Vec<u8>" | "ByteString" | "rustus_core::bytestring::ByteString" => quote! { rustus_core::sir_type::SIRType::ByteString },
            "String" => quote! { rustus_core::sir_type::SIRType::String },
            "()" => quote! { rustus_core::sir_type::SIRType::Unit },
            "Data" | "rustus_core::data::Data" => quote! { rustus_core::sir_type::SIRType::Data },
            _ => {
                if let Some(idx) = self.generic_type_params.iter().position(|p| p == &type_str) {
                    let id = (idx + 1) as i64;
                    quote! {
                        rustus_core::sir_type::SIRType::TypeVar {
                            name: #type_str.to_string(),
                            opt_id: Some(#id),
                            is_builtin: false,
                        }
                    }
                } else {
                    let replaced = self.replace_generics(ty);
                    quote! { <#replaced as rustus_core::sir_type::HasSIRType>::sir_type() }
                }
            }
        }
    }

    /// Replace generic type params with TypeParam<N> in a type.
    fn replace_generics(&self, ty: &syn::Type) -> syn::Type {
        if self.generic_type_params.is_empty() {
            return ty.clone();
        }
        match ty {
            syn::Type::Path(type_path) => {
                let mut new_path = type_path.clone();
                for seg in &mut new_path.path.segments {
                    if let syn::PathArguments::AngleBracketed(args) = &mut seg.arguments {
                        for arg in &mut args.args {
                            if let syn::GenericArgument::Type(inner_ty) = arg {
                                let inner_str = quote!(#inner_ty).to_string().replace(' ', "");
                                if let Some(idx) = self.generic_type_params.iter().position(|p| p == &inner_str) {
                                    let id = (idx + 1) as i64;
                                    *inner_ty = syn::parse_quote! { rustus_core::sir_type::TypeParam<#id> };
                                } else {
                                    *inner_ty = self.replace_generics(inner_ty);
                                }
                            }
                        }
                    }
                }
                syn::Type::Path(new_path)
            }
            _ => ty.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// TypeHint construction helpers
// ---------------------------------------------------------------------------

fn rust_type_to_type_hint(ty: &syn::Type, generic_params: &[String]) -> TokenStream2 {
    let type_str = quote!(#ty).to_string().replace(' ', "");

    // Check generic type params first
    if let Some(idx) = generic_params.iter().position(|p| p == &type_str) {
        let id = (idx + 1) as i64;
        return quote! {
            rustus_core::pre_sir::TypeHint::TypeParam {
                name: #type_str.to_string(),
                index: #id,
            }
        };
    }

    match type_str.as_str() {
        "bool" => quote! { rustus_core::pre_sir::TypeHint::Bool },
        "i64" | "i128" | "BigInt" | "num_bigint::BigInt" | "rustus_core::num_bigint::BigInt" => {
            quote! { rustus_core::pre_sir::TypeHint::Integer }
        }
        "Vec<u8>" | "ByteString" | "rustus_core::bytestring::ByteString" => quote! { rustus_core::pre_sir::TypeHint::ByteString },
        "String" => quote! { rustus_core::pre_sir::TypeHint::String },
        "()" => quote! { rustus_core::pre_sir::TypeHint::Unit },
        "Data" | "rustus_core::data::Data" => quote! { rustus_core::pre_sir::TypeHint::Data },
        _ => {
            // Named type with possible type args
            let base_name = extract_base_type_name(ty);
            let type_args = extract_type_args(ty, generic_params);
            quote! {
                rustus_core::pre_sir::TypeHint::Named {
                    rust_path: #base_name.to_string(),
                    type_args: vec![#(#type_args),*],
                }
            }
        }
    }
}

fn extract_base_type_name(ty: &syn::Type) -> String {
    if let syn::Type::Path(type_path) = ty {
        // Use full path (e.g., "v3::ScriptContext") not just last segment
        type_path
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::")
    } else {
        "unknown".to_string()
    }
}

fn extract_type_args(ty: &syn::Type, generic_params: &[String]) -> Vec<TokenStream2> {
    if let syn::Type::Path(type_path) = ty {
        if let Some(seg) = type_path.path.segments.last() {
            if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                return args
                    .args
                    .iter()
                    .filter_map(|arg| {
                        if let syn::GenericArgument::Type(inner_ty) = arg {
                            Some(rust_type_to_type_hint(inner_ty, generic_params))
                        } else {
                            None
                        }
                    })
                    .collect();
            }
        }
    }
    vec![]
}

// ---------------------------------------------------------------------------
// compile_impl: the main entry point
// ---------------------------------------------------------------------------

pub fn compile_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();
    let register_fn_name = format_ident!("__rustus_compile_{}", fn_name_str);

    // Parse optional attributes: #[compile(module = "...", name = "...")]
    let (module_attr, name_attr) = parse_compile_attrs(attr);
    let sir_name = name_attr.unwrap_or_else(|| fn_name_str.clone());

    // Extract parameter info
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

    // Parse generic type params and trait bounds
    let mut generic_type_params: Vec<String> = Vec::new();
    let mut typeclass_bounds: Vec<TokenStream2> = Vec::new();

    for gp in input_fn.sig.generics.type_params() {
        let param_name = gp.ident.to_string();
        generic_type_params.push(param_name.clone());

        for bound in &gp.bounds {
            if let syn::TypeParamBound::Trait(trait_bound) = bound {
                let trait_name = trait_bound
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_default();
                if trait_name == "PartialEq" || trait_name == "PartialOrd" {
                    let idx = generic_type_params.len() as i64; // 1-based since we just pushed
                    let tn = trait_name.clone();
                    typeclass_bounds.push(quote! {
                        rustus_core::pre_sir::TypeclassBound {
                            trait_name: #tn.to_string(),
                            type_param_index: #idx,
                        }
                    });
                }
            }
        }
    }

    // Build CompileCtx
    let mut ctx = CompileCtx::new();
    ctx.generic_type_params = generic_type_params.clone();
    ctx.current_fn_name = Some(fn_name_str.clone());
    // Typeclass var names are set by lowering, not the macro.
    // The macro just passes the bounds through to PreFnDef.
    if sir_name != fn_name_str {
        ctx.name_remaps
            .insert(fn_name_str.clone(), sir_name.clone());
    }

    // Register function param types in TypeDict
    for (name, ty) in &params {
        let name_str = name.to_string();
        ctx.register_var_type(&name_str, ty);
        ctx.register_rust_type(ty);
    }

    // Build PreParam expressions
    let param_exprs: Vec<TokenStream2> = params
        .iter()
        .map(|(name, ty)| {
            let name_str = name.to_string();
            let type_hint = rust_type_to_type_hint(ty, &generic_type_params);
            let rust_type_path = extract_base_type_name(ty);
            quote! {
                rustus_core::pre_sir::PreParam {
                    name: #name_str.to_string(),
                    type_hint: #type_hint,
                    rust_type_path: #rust_type_path.to_string(),
                }
            }
        })
        .collect();

    // Return type hint
    let ret_type_hint = match &input_fn.sig.output {
        ReturnType::Default => quote! { rustus_core::pre_sir::TypeHint::Unit },
        ReturnType::Type(_, ty) => rust_type_to_type_hint(ty, &generic_type_params),
    };

    // Generic param string literals
    let generic_param_lits: Vec<TokenStream2> = generic_type_params
        .iter()
        .map(|s| quote! { #s.to_string() })
        .collect();

    // Compile body to PreSIR
    let body_pre_sir = compile_fn_body(&input_fn, &mut ctx);

    // Collect type registrations
    let type_regs = &ctx.type_registrations;

    let module_literal = match &module_attr {
        Some(m) => quote! { Some(#m.to_string()) },
        None => quote! { None },
    };

    let module_static = match &module_attr {
        Some(m) => quote! { Some(#m) },
        None => quote! { None },
    };

    let expanded = quote! {
        #[allow(dead_code)]
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

            // TypeDict: type registrations evaluated at builder time
            let mut __td = rustus_core::pre_sir::TypeDict::new();
            #(#type_regs)*

            let pre_fn = rustus_core::pre_sir::PreFnDef {
                rust_name: #fn_name_str.to_string(),
                sir_name: #sir_name.to_string(),
                module: #module_literal,
                params: vec![#(#param_exprs),*],
                ret_type: #ret_type_hint,
                generic_params: vec![#(#generic_param_lits),*],
                typeclass_bounds: vec![#(#typeclass_bounds),*],
                body: #body_pre_sir,
                type_dict: __td,
            };
            ctx.lower_fn_def(pre_fn);
        }

        rustus_core::inventory::submit! {
            rustus_core::registry::PreSirEntry {
                name: #fn_name_str,
                module: #module_static,
                kind: rustus_core::registry::EntryKind::Function,
                builder: #register_fn_name,
            }
        }
    };

    expanded.into()
}

// ---------------------------------------------------------------------------
// Expression compilation: Rust → PreSIR
// ---------------------------------------------------------------------------

fn compile_fn_body(func: &ItemFn, ctx: &mut CompileCtx) -> TokenStream2 {
    compile_stmts(&func.block.stmts, ctx)
}

fn compile_stmts(stmts: &[Stmt], ctx: &mut CompileCtx) -> TokenStream2 {
    if stmts.is_empty() {
        return quote! {
            rustus_core::pre_sir::PreSIR::Const {
                value: rustus_core::constant::UplcConstant::Unit,
                anns: rustus_core::module::AnnotationsDecl::empty(),
            }
        };
    }

    if stmts.len() == 1 {
        return match &stmts[0] {
            Stmt::Expr(expr, _) => compile_expr(expr, ctx),
            Stmt::Local(local) => compile_let_stmt(local, &[], ctx),
            Stmt::Macro(stmt_mac) => compile_expr(&Expr::Macro(syn::ExprMacro {
                attrs: stmt_mac.attrs.clone(),
                mac: stmt_mac.mac.clone(),
            }), ctx),
            _ => compile_unsupported("statement"),
        };
    }

    match &stmts[0] {
        Stmt::Local(local) => compile_let_stmt(local, &stmts[1..], ctx),
        Stmt::Expr(expr, _semi) => {
            let value = compile_expr(expr, ctx);
            let body = compile_stmts(&stmts[1..], ctx);
            quote! {
                rustus_core::pre_sir::PreSIR::Let {
                    name: "_".to_string(),
                    type_hint: rustus_core::pre_sir::TypeHint::Infer,
                    value: Box::new(#value),
                    body: Box::new(#body),
                    is_rec: false,
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }
        Stmt::Macro(stmt_mac) => {
            let value = compile_expr(&Expr::Macro(syn::ExprMacro {
                attrs: stmt_mac.attrs.clone(),
                mac: stmt_mac.mac.clone(),
            }), ctx);
            let body = compile_stmts(&stmts[1..], ctx);
            quote! {
                rustus_core::pre_sir::PreSIR::Let {
                    name: "_".to_string(),
                    type_hint: rustus_core::pre_sir::TypeHint::Infer,
                    value: Box::new(#value),
                    body: Box::new(#body),
                    is_rec: false,
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }
        _ => compile_unsupported("statement"),
    }
}

fn compile_let_stmt(local: &syn::Local, rest: &[Stmt], ctx: &mut CompileCtx) -> TokenStream2 {
    let (original_name, type_hint, rust_type) = match &local.pat {
        Pat::Ident(ident) => (
            ident.ident.to_string(),
            quote! { rustus_core::pre_sir::TypeHint::Infer },
            None,
        ),
        Pat::Type(pat_type) => {
            let name = if let Pat::Ident(ident) = pat_type.pat.as_ref() {
                ident.ident.to_string()
            } else {
                "_".to_string()
            };
            let ty = &pat_type.ty;
            let hint = rust_type_to_type_hint(ty, &ctx.generic_type_params);
            (name, hint, Some(ty.as_ref().clone()))
        }
        _ => (
            "_".to_string(),
            quote! { rustus_core::pre_sir::TypeHint::Infer },
            None,
        ),
    };

    // Rename variable (eliminates shadowing)
    let renamed = ctx.fresh_var(&original_name);

    // Register type in TypeDict if annotation present
    if let Some(ref rty) = rust_type {
        ctx.register_var_type(&renamed, rty);
        ctx.register_rust_type(rty);
    }

    let value = if let Some(init) = &local.init {
        // Check for fromData pattern: T::from_data(&x).unwrap()
        if let Some(pre) = try_compile_from_data(&init.expr, &type_hint, ctx) {
            pre
        } else {
            compile_expr(&init.expr, ctx)
        }
    } else {
        compile_unsupported("let without initializer")
    };

    let body = compile_stmts(rest, ctx);

    quote! {
        rustus_core::pre_sir::PreSIR::Let {
            name: #renamed.to_string(),
            type_hint: #type_hint,
            value: Box::new(#value),
            body: Box::new(#body),
            is_rec: false,
            anns: rustus_core::module::AnnotationsDecl::empty(),
        }
    }
}

fn compile_expr(expr: &Expr, ctx: &mut CompileCtx) -> TokenStream2 {
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
                rustus_core::pre_sir::PreSIR::Const {
                    value: rustus_core::constant::UplcConstant::Unit,
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }

        // *expr — deref is transparent
        Expr::Unary(unary) => match &unary.op {
            syn::UnOp::Deref(_) => compile_expr(&unary.expr, ctx),
            syn::UnOp::Neg(_) => {
                let inner = compile_expr(&unary.expr, ctx);
                let neg_anns = anns(unary.span());
                quote! {
                    rustus_core::pre_sir::PreSIR::Negate {
                        expr: Box::new(#inner),
                        anns: #neg_anns,
                    }
                }
            }
            _ => compile_unsupported("unary operator"),
        },

        // (expr) — unwrap parens
        Expr::Paren(paren) => compile_expr(&paren.expr, ctx),

        // &expr — transparent
        Expr::Reference(reference) => compile_expr(&reference.expr, ctx),

        // panic!("msg"), require!(...)
        Expr::Macro(mac) => compile_macro(mac, ctx),

        // { stmts }
        Expr::Block(block) => compile_stmts(&block.block.stmts, ctx),

        // x.method(args)
        Expr::MethodCall(mc) => compile_method_call(mc, ctx),

        _ => compile_unsupported("expression"),
    }
}

fn compile_lit(lit: &syn::ExprLit) -> TokenStream2 {
    match &lit.lit {
        syn::Lit::Bool(b) => {
            let val = b.value();
            quote! {
                rustus_core::pre_sir::PreSIR::Const {
                    value: rustus_core::constant::UplcConstant::Bool { value: #val },
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }
        syn::Lit::Int(i) => {
            let val: i64 = i.base10_parse().unwrap_or(0);
            quote! {
                rustus_core::pre_sir::PreSIR::Const {
                    value: rustus_core::constant::UplcConstant::Integer {
                        value: rustus_core::num_bigint::BigInt::from(#val),
                    },
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }
        syn::Lit::Str(s) => {
            let val = s.value();
            quote! {
                rustus_core::pre_sir::PreSIR::Const {
                    value: rustus_core::constant::UplcConstant::String { value: #val.to_string() },
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }
        _ => compile_unsupported("literal"),
    }
}

fn compile_binop(binop: &syn::ExprBinary, ctx: &mut CompileCtx) -> TokenStream2 {
    let left = compile_expr(&binop.left, ctx);
    let right = compile_expr(&binop.right, ctx);
    let op_anns = anns(binop.span());

    let op = match &binop.op {
        syn::BinOp::Add(_) | syn::BinOp::AddAssign(_) => quote! { rustus_core::pre_sir::BinOp::Add },
        syn::BinOp::Sub(_) | syn::BinOp::SubAssign(_) => quote! { rustus_core::pre_sir::BinOp::Sub },
        syn::BinOp::Mul(_) | syn::BinOp::MulAssign(_) => quote! { rustus_core::pre_sir::BinOp::Mul },
        syn::BinOp::Eq(_) => quote! { rustus_core::pre_sir::BinOp::Eq },
        syn::BinOp::Lt(_) => quote! { rustus_core::pre_sir::BinOp::Lt },
        syn::BinOp::Le(_) => quote! { rustus_core::pre_sir::BinOp::Le },
        _ => return compile_unsupported("binary operator"),
    };

    quote! {
        rustus_core::pre_sir::PreSIR::BinOp {
            op: #op,
            left: Box::new(#left),
            right: Box::new(#right),
            anns: #op_anns,
        }
    }
}

fn compile_match(m: &syn::ExprMatch, ctx: &mut CompileCtx) -> TokenStream2 {
    let match_anns = anns(m.match_token.span);
    let scrutinee = compile_expr(&m.expr, ctx);

    let arms: Vec<TokenStream2> = m
        .arms
        .iter()
        .map(|arm| {
            let arm_anns = anns(arm.pat.span());
            ctx.push_scope();
            let pattern = compile_pattern(&arm.pat, ctx);
            let body = compile_expr(&arm.body, ctx);
            ctx.pop_scope();
            quote! {
                rustus_core::pre_sir::PreMatchArm {
                    pattern: #pattern,
                    body: #body,
                    anns: #arm_anns,
                }
            }
        })
        .collect();

    quote! {
        rustus_core::pre_sir::PreSIR::Match {
            scrutinee: Box::new(#scrutinee),
            arms: vec![#(#arms),*],
            anns: #match_anns,
        }
    }
}

fn compile_pattern(pat: &Pat, ctx: &mut CompileCtx) -> TokenStream2 {
    match pat {
        Pat::Wild(_) => {
            quote! { rustus_core::pre_sir::PrePattern::Wildcard }
        }
        Pat::Path(pat_path) => {
            let (type_name, constr_name) = extract_enum_path(&pat_path.path);
            quote! {
                rustus_core::pre_sir::PrePattern::Constr {
                    type_name: #type_name.to_string(),
                    constr_name: #constr_name.to_string(),
                    bindings: vec![],
                }
            }
        }
        Pat::TupleStruct(pat_ts) => {
            let (type_name, constr_name) = extract_enum_path(&pat_ts.path);
            let bindings: Vec<String> = pat_ts
                .elems
                .iter()
                .map(|p| {
                    if let Pat::Ident(ident) = p {
                        ctx.fresh_var(&ident.ident.to_string())
                    } else {
                        "_".to_string()
                    }
                })
                .collect();
            quote! {
                rustus_core::pre_sir::PrePattern::Constr {
                    type_name: #type_name.to_string(),
                    constr_name: #constr_name.to_string(),
                    bindings: vec![#(#bindings.to_string()),*],
                }
            }
        }
        Pat::Struct(pat_struct) => {
            let (type_name, constr_name) = extract_enum_path(&pat_struct.path);
            let bindings: Vec<String> = pat_struct
                .fields
                .iter()
                .filter_map(|field| {
                    if let syn::Member::Named(ident) = &field.member {
                        Some(ctx.fresh_var(&ident.to_string()))
                    } else {
                        None
                    }
                })
                .collect();
            quote! {
                rustus_core::pre_sir::PrePattern::Constr {
                    type_name: #type_name.to_string(),
                    constr_name: #constr_name.to_string(),
                    bindings: vec![#(#bindings.to_string()),*],
                }
            }
        }
        Pat::Ident(ident) => {
            let renamed = ctx.fresh_var(&ident.ident.to_string());
            quote! {
                rustus_core::pre_sir::PrePattern::Constr {
                    type_name: "".to_string(),
                    constr_name: #renamed.to_string(),
                    bindings: vec![#renamed.to_string()],
                }
            }
        }
        _ => {
            quote! { rustus_core::pre_sir::PrePattern::Wildcard }
        }
    }
}

fn compile_field_access(field: &syn::ExprField, ctx: &mut CompileCtx) -> TokenStream2 {
    let field_anns = anns(field.span());
    let base = compile_expr(&field.base, ctx);
    let field_name = match &field.member {
        syn::Member::Named(ident) => ident.to_string(),
        syn::Member::Unnamed(index) => format!("_{}", index.index),
    };

    quote! {
        rustus_core::pre_sir::PreSIR::FieldAccess {
            base: Box::new(#base),
            field: #field_name.to_string(),
            anns: #field_anns,
        }
    }
}

fn compile_path(path: &syn::ExprPath, ctx: &mut CompileCtx) -> TokenStream2 {
    let span_anns = anns(path.span());
    let segments: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();

    if segments.len() == 1 {
        let name = &segments[0];
        let resolved = ctx.resolve_var(name);
        // Remap for recursive calls
        let sir_name = ctx
            .name_remaps
            .get(name)
            .cloned()
            .unwrap_or(resolved.clone());
        quote! {
            rustus_core::pre_sir::PreSIR::Var {
                name: #sir_name.to_string(),
                anns: #span_anns,
            }
        }
    } else if segments.len() == 2 {
        let type_name = &segments[0];
        let constr_name = format!("{}::{}", segments[0], segments[1]);
        quote! {
            rustus_core::pre_sir::PreSIR::Construct {
                type_name: #type_name.to_string(),
                constr_name: #constr_name.to_string(),
                args: vec![],
                anns: #span_anns,
            }
        }
    } else {
        compile_unsupported("multi-segment path")
    }
}

fn compile_call(call: &syn::ExprCall, ctx: &mut CompileCtx) -> TokenStream2 {
    let call_anns = anns(call.span());
    if let Expr::Path(path) = call.func.as_ref() {
        let segments: Vec<String> = path
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();

        // BigInt::from(N) → integer constant
        if segments.len() == 2
            && (segments[0] == "BigInt" || segments[0] == "num_bigint")
            && segments[1] == "from"
            && call.args.len() == 1
        {
            if let Some(Expr::Lit(lit)) = call.args.first() {
                if let syn::Lit::Int(int_lit) = &lit.lit {
                    let val: i64 = int_lit.base10_parse().unwrap_or(0);
                    return quote! {
                        rustus_core::pre_sir::PreSIR::Const {
                            value: rustus_core::constant::UplcConstant::Integer {
                                value: rustus_core::num_bigint::BigInt::from(#val),
                            },
                            anns: rustus_core::module::AnnotationsDecl::empty(),
                        }
                    };
                }
            }
            if let Some(Expr::Unary(unary)) = call.args.first() {
                if let syn::UnOp::Neg(_) = &unary.op {
                    if let Expr::Lit(lit) = unary.expr.as_ref() {
                        if let syn::Lit::Int(int_lit) = &lit.lit {
                            let val: i64 = -(int_lit.base10_parse::<i64>().unwrap_or(0));
                            return quote! {
                                rustus_core::pre_sir::PreSIR::Const {
                                    value: rustus_core::constant::UplcConstant::Integer {
                                        value: rustus_core::num_bigint::BigInt::from(#val),
                                    },
                                    anns: rustus_core::module::AnnotationsDecl::empty(),
                                }
                            };
                        }
                    }
                }
            }
        }

        // Error on bare from_data() without .unwrap()
        if segments.len() == 2 && segments[1] == "from_data" {
            return syn::Error::new_spanned(
                &call.func,
                "T::from_data() must be followed by .unwrap() in #[compile] functions",
            )
            .to_compile_error()
            .into();
        }

        // Function call → PreSIR::Call
        if segments.len() == 1 || segments.len() == 2 {
            let rust_path = if segments.len() == 2 {
                format!("{}::{}", segments[0], segments[1])
            } else {
                ctx.name_remaps
                    .get(&segments[0])
                    .cloned()
                    .unwrap_or(segments[0].clone())
            };
            let args: Vec<TokenStream2> =
                call.args.iter().map(|a| compile_expr(a, ctx)).collect();

            return quote! {
                rustus_core::pre_sir::PreSIR::Call {
                    func_path: #rust_path.to_string(),
                    args: vec![#(#args),*],
                    anns: #call_anns,
                }
            };
        }
    }

    // Fallback: generic call
    let func_path = quote!(#(call.func)).to_string();
    let args: Vec<TokenStream2> = call.args.iter().map(|a| compile_expr(a, ctx)).collect();
    quote! {
        rustus_core::pre_sir::PreSIR::Call {
            func_path: #func_path.to_string(),
            args: vec![#(#args),*],
            anns: #call_anns,
        }
    }
}

fn compile_if(if_expr: &syn::ExprIf, ctx: &mut CompileCtx) -> TokenStream2 {
    let cond = compile_expr(&if_expr.cond, ctx);
    let then_branch = compile_stmts(&if_expr.then_branch.stmts, ctx);
    let else_branch = if let Some((_, else_expr)) = &if_expr.else_branch {
        let e = compile_expr(else_expr, ctx);
        quote! { Some(Box::new(#e)) }
    } else {
        quote! { None }
    };

    quote! {
        rustus_core::pre_sir::PreSIR::IfThenElse {
            cond: Box::new(#cond),
            then_branch: Box::new(#then_branch),
            else_branch: #else_branch,
            anns: rustus_core::module::AnnotationsDecl::empty(),
        }
    }
}

fn compile_macro(mac: &syn::ExprMacro, ctx: &mut CompileCtx) -> TokenStream2 {
    let macro_name = mac
        .mac
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default();

    match macro_name.as_str() {
        "require" => {
            let tokens = mac.mac.tokens.to_string();
            let parts: Vec<&str> = tokens.splitn(2, ',').collect();
            let cond_str = parts[0].trim();
            let msg = parts
                .get(1)
                .map(|s| s.trim().trim_matches('"'))
                .unwrap_or("require failed");
            let cond_tokens: proc_macro2::TokenStream = cond_str.parse().unwrap();
            let cond_expr: Expr = syn::parse2(cond_tokens).unwrap();
            let cond_sir = compile_expr(&cond_expr, ctx);
            quote! {
                rustus_core::pre_sir::PreSIR::Require {
                    cond: Box::new(#cond_sir),
                    message: #msg.to_string(),
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }
        "panic" | "todo" | "unimplemented" => {
            let msg = mac.mac.tokens.to_string().trim_matches('"').to_string();
            let msg = if msg.is_empty() {
                macro_name.clone()
            } else {
                msg
            };
            quote! {
                rustus_core::pre_sir::PreSIR::Error {
                    message: #msg.to_string(),
                    anns: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        }
        _ => compile_unsupported(&format!("macro {}", macro_name)),
    }
}

fn compile_method_call(mc: &syn::ExprMethodCall, ctx: &mut CompileCtx) -> TokenStream2 {
    let mc_anns = anns(mc.span());
    let method_name = mc.method.to_string();

    // .unwrap() — check for fromData pattern
    if method_name == "unwrap" {
        if let Some(from_data) = try_compile_from_data_unwrap(&mc.receiver, ctx) {
            return from_data;
        }
        return compile_expr(&mc.receiver, ctx);
    }

    // .to_data()
    if method_name == "to_data" {
        let arg = compile_expr(&mc.receiver, ctx);
        return quote! {
            rustus_core::pre_sir::PreSIR::ToData {
                arg: Box::new(#arg),
                source_type: rustus_core::pre_sir::TypeHint::Infer,
                anns: #mc_anns,
            }
        };
    }

    // General method call → Call with receiver as first arg
    let receiver = compile_expr(&mc.receiver, ctx);
    let mut all_args = vec![receiver];
    for arg in &mc.args {
        all_args.push(compile_expr(arg, ctx));
    }

    quote! {
        rustus_core::pre_sir::PreSIR::Call {
            func_path: #method_name.to_string(),
            args: vec![#(#all_args),*],
            anns: #mc_anns,
        }
    }
}

// ---------------------------------------------------------------------------
// fromData pattern detection
// ---------------------------------------------------------------------------

/// Check if expr is `T::from_data(&x).unwrap()` — with type hint from let annotation.
fn try_compile_from_data(
    expr: &Expr,
    type_hint: &TokenStream2,
    ctx: &mut CompileCtx,
) -> Option<TokenStream2> {
    if let Expr::MethodCall(mc) = expr {
        if mc.method.to_string() == "unwrap" {
            return try_compile_from_data_call(&mc.receiver, Some(type_hint), ctx);
        }
    }
    None
}

/// Check if receiver is `T::from_data(&expr)`.
fn try_compile_from_data_unwrap(
    receiver: &Expr,
    ctx: &mut CompileCtx,
) -> Option<TokenStream2> {
    try_compile_from_data_call(receiver, None, ctx)
}

fn try_compile_from_data_call(
    expr: &Expr,
    type_hint_override: Option<&TokenStream2>,
    ctx: &mut CompileCtx,
) -> Option<TokenStream2> {
    if let Expr::Call(call) = expr {
        if let Expr::Path(path) = call.func.as_ref() {
            let segments: Vec<String> = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            if segments.len() == 2 && segments[1] == "from_data" && call.args.len() == 1 {
                let arg_expr = call.args.first().unwrap();
                let inner = if let Expr::Reference(r) = arg_expr {
                    compile_expr(&r.expr, ctx)
                } else {
                    compile_expr(arg_expr, ctx)
                };

                let target_type = if let Some(hint) = type_hint_override {
                    hint.clone()
                } else if segments[0] == "FromData" {
                    quote! { rustus_core::pre_sir::TypeHint::Infer }
                } else {
                    let type_ident = &path.path.segments.first().unwrap().ident;
                    let ty: syn::Type = syn::parse_quote! { #type_ident };
                    rust_type_to_type_hint(&ty, &ctx.generic_type_params)
                };

                return Some(quote! {
                    rustus_core::pre_sir::PreSIR::FromData {
                        arg: Box::new(#inner),
                        target_type: #target_type,
                        anns: rustus_core::module::AnnotationsDecl::empty(),
                    }
                });
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn compile_unsupported(what: &str) -> TokenStream2 {
    let msg = format!("#[compile] unsupported {}", what);
    quote! {
        compile_error!(#msg)
    }
}

fn extract_enum_path(path: &syn::Path) -> (String, String) {
    let segments: Vec<String> = path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
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

/// Parse #[compile(module = "...", name = "...")] attributes.
fn parse_compile_attrs(attr: TokenStream) -> (Option<String>, Option<String>) {
    let mut module = None;
    let mut name = None;

    if attr.is_empty() {
        return (module, name);
    }

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
