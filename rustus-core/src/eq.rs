//! `Eq` instances, shaped as the Scalus compiler emits them.
//!
//! On-chain, Scalus treats `Eq` as a marker. `a === b` is the instance of the operands'
//! type applied to both, with each `Apply` annotated `functionalInterfaceType = Eq`;
//! lowering (`LoweringEq`) replaces that application with a comparison chosen from how
//! the operands are represented. The instance body is never called, but the instance
//! has to exist as a binding for the linker to resolve.
//!
//! This module builds both halves for `==`: the annotated application, and the instance
//! bindings, with the bodies Scalus's `Eq.derived` gives them.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::constant::UplcConstant;
use crate::default_fun::DefaultFun;
use crate::lower::make_builtin_apply2;
use crate::module::{AnnotationsDecl, Binding};
use crate::sir::{Case, Pattern, SIR};
use crate::sir_type::{ConstrDecl, DataDecl, SIRType, TypeVar};

const EQ_TYPE: &str = "scalus.cardano.onchain.plutus.prelude.Eq";
const EQ_MODULE: &str = "scalus.cardano.onchain.plutus.prelude.Eq$";
const BYTESTRING_MODULE: &str = "scalus.uplc.builtin.ByteString$";

/// Scalus keeps `AssocMap` out of `Eq`: its equality ignores entry order, so it is not
/// the structural equality that `Eq` stands for.
const ASSOC_MAP: &str = "scalus.cardano.onchain.plutus.prelude.AssocMap";

const PRIMITIVES: [SIRType; 6] = [
    SIRType::Integer,
    SIRType::ByteString,
    SIRType::String,
    SIRType::Boolean,
    SIRType::Data,
    SIRType::Unit,
];

/// Instances of the type parameters in scope, by parameter name.
type Evidence = HashMap<String, SIR>;

// ---------------------------------------------------------------------------
// Use site
// ---------------------------------------------------------------------------

/// The `Eq` instance of `tp`, as an expression of type `tp -> tp -> Boolean`.
/// `None` when `tp` has none: a function, a type that is not resolved yet, `AssocMap`.
pub fn instance(tp: &SIRType, data_decls: &BTreeMap<String, DataDecl>) -> Option<SIR> {
    instance_in(tp, data_decls, &Evidence::new())
}

/// `instance(left, right)`, marked as an application of `Eq`.
pub fn apply(
    instance: SIR,
    operand_tp: &SIRType,
    left: SIR,
    right: SIR,
    anns: &AnnotationsDecl,
) -> SIR {
    let mut marked = anns.clone();
    marked.data.insert(
        "functionalInterfaceType".to_string(),
        SIR::Const {
            uplc_const: UplcConstant::String { value: EQ_TYPE.to_string() },
            tp: SIRType::String,
            anns: AnnotationsDecl::empty(),
        },
    );
    SIR::Apply {
        f: Box::new(SIR::Apply {
            f: Box::new(instance),
            arg: Box::new(left),
            tp: fun(operand_tp.clone(), SIRType::Boolean),
            anns: marked.clone(),
        }),
        arg: Box::new(right),
        tp: SIRType::Boolean,
        anns: marked,
    }
}

/// `left == right` on booleans, as the Scalus compiler writes it:
/// `if left then right else (if right then false else true)`.
pub(crate) fn bool_equals(left: SIR, right: SIR, anns: &AnnotationsDecl) -> SIR {
    SIR::IfThenElse {
        cond: Box::new(left),
        t: Box::new(right.clone()),
        f: Box::new(SIR::IfThenElse {
            cond: Box::new(right),
            t: Box::new(bool_const(false)),
            f: Box::new(bool_const(true)),
            tp: SIRType::Boolean,
            anns: anns.clone(),
        }),
        tp: SIRType::Boolean,
        anns: anns.clone(),
    }
}

fn instance_in(
    tp: &SIRType,
    data_decls: &BTreeMap<String, DataDecl>,
    evidence: &Evidence,
) -> Option<SIR> {
    if let Some((module, name)) = primitive_instance_name(tp) {
        return Some(SIR::ExternalVar {
            module_name: module.to_string(),
            name,
            tp: equality_type(tp),
            anns: AnnotationsDecl::empty(),
        });
    }
    match tp {
        SIRType::TypeVar { name, .. } => evidence.get(name).cloned(),
        SIRType::CaseClass { decl_name, type_args, .. }
        | SIRType::SumCaseClass { decl_name, type_args } => {
            let decl = data_decls.get(decl_name)?;
            if decl.name == ASSOC_MAP || type_args.len() != decl.type_params.len() {
                return None;
            }
            let (module, name) = data_instance_name(&decl.name);
            let declared_tp = instance_type(decl);
            let instantiation: HashMap<i64, SIRType> = decl
                .type_params
                .iter()
                .zip(type_args)
                .filter_map(|(param, arg)| param.opt_id.map(|id| (id, arg.clone())))
                .collect();
            let mut applied_tp = declared_tp.substitute(&instantiation);
            let mut expr = SIR::ExternalVar {
                module_name: module,
                name,
                tp: declared_tp,
                anns: AnnotationsDecl::empty(),
            };
            // A generic instance takes the instances of its type arguments first.
            for arg in type_args {
                let SIRType::Fun { to, .. } = applied_tp else {
                    return None;
                };
                applied_tp = *to;
                expr = SIR::Apply {
                    f: Box::new(expr),
                    arg: Box::new(instance_in(arg, data_decls, evidence)?),
                    tp: applied_tp.clone(),
                    anns: AnnotationsDecl::empty(),
                };
            }
            Some(expr)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Names and types
// ---------------------------------------------------------------------------

/// Module and binding name of the instance Scalus declares for a primitive type.
fn primitive_instance_name(tp: &SIRType) -> Option<(&'static str, String)> {
    let (module, short) = match tp {
        SIRType::Integer => (EQ_MODULE, "given_Eq_BigInt"),
        SIRType::String => (EQ_MODULE, "given_Eq_String"),
        SIRType::Boolean => (EQ_MODULE, "given_Eq_Boolean"),
        SIRType::Data => (EQ_MODULE, "given_Eq_Data"),
        SIRType::Unit => (EQ_MODULE, "given_Eq_Unit"),
        SIRType::ByteString => (BYTESTRING_MODULE, "given_Eq_ByteString"),
        _ => return None,
    };
    Some((module, format!("{module}.{short}")))
}

/// Module and binding name of the instance for a data type. Scalus puts an instance in
/// the type's companion as `given_Eq_<Type>`; `List` and `Option` name theirs.
fn data_instance_name(decl_name: &str) -> (String, String) {
    let module = format!("{decl_name}$");
    let short = match decl_name {
        "scalus.cardano.onchain.plutus.prelude.List" => "listEq".to_string(),
        "scalus.cardano.onchain.plutus.prelude.Option" => "optionEq".to_string(),
        _ => format!("given_Eq_{}", decl_name.rsplit('.').next().unwrap_or(decl_name)),
    };
    let name = format!("{module}.{short}");
    (module, name)
}

fn fun(from: SIRType, to: SIRType) -> SIRType {
    SIRType::Fun {
        from: Box::new(from),
        to: Box::new(to),
    }
}

/// `tp -> tp -> Boolean`
fn equality_type(tp: &SIRType) -> SIRType {
    fun(tp.clone(), fun(tp.clone(), SIRType::Boolean))
}

fn type_var(param: &TypeVar) -> SIRType {
    SIRType::TypeVar {
        name: param.name.clone(),
        opt_id: param.opt_id,
        is_builtin: param.is_builtin,
    }
}

/// A product is a declaration whose only constructor carries its name.
fn is_product(decl: &DataDecl) -> bool {
    decl.constructors.len() == 1 && decl.constructors[0].name == decl.name
}

/// The declared type itself, applied to its own type parameters.
fn self_type(decl: &DataDecl) -> SIRType {
    let type_args = decl.type_params.iter().map(type_var).collect();
    if is_product(decl) {
        SIRType::CaseClass {
            constr_name: decl.name.clone(),
            decl_name: decl.name.clone(),
            type_args,
        }
    } else {
        SIRType::SumCaseClass {
            decl_name: decl.name.clone(),
            type_args,
        }
    }
}

/// `Eq[A] -> .. -> T[A, ..] -> T[A, ..] -> Boolean`
fn instance_type(decl: &DataDecl) -> SIRType {
    decl.type_params
        .iter()
        .rev()
        .fold(equality_type(&self_type(decl)), |tp, param| {
            fun(equality_type(&type_var(param)), tp)
        })
}

// ---------------------------------------------------------------------------
// Instance bindings
// ---------------------------------------------------------------------------

/// Adds the instance bindings that `bindings` refer to and do not define, and the ones
/// those refer to in turn.
pub fn add_referenced_instances(
    bindings: &mut Vec<Binding>,
    data_decls: &BTreeMap<String, DataDecl>,
) {
    let mut defined: HashSet<String> = bindings.iter().map(|b| b.name.clone()).collect();
    let mut next = 0;
    while next < bindings.len() {
        let mut referenced = Vec::new();
        external_vars(&bindings[next].value, &mut referenced);
        next += 1;
        for (module, name) in referenced {
            if defined.contains(&name) {
                continue;
            }
            if let Some(binding) = instance_binding(&module, &name, data_decls) {
                defined.insert(name);
                bindings.push(binding);
            }
        }
    }
}

fn instance_binding(
    module: &str,
    name: &str,
    data_decls: &BTreeMap<String, DataDecl>,
) -> Option<Binding> {
    let (tp, value) = primitive_instance(name).or_else(|| {
        let decl = data_decls.get(module.strip_suffix('$')?)?;
        if data_instance_name(&decl.name).1 != name {
            return None;
        }
        derived_instance(decl, data_decls)
    })?;
    Some(Binding {
        name: name.to_string(),
        module_name: Some(module.to_string()),
        tp,
        value,
        redirect_to_scalus: false,
    })
}

/// The instances Scalus declares for the primitive types: `λx y -> builtin(x, y)`.
fn primitive_instance(name: &str) -> Option<(SIRType, SIR)> {
    let tp = PRIMITIVES
        .iter()
        .find(|tp| primitive_instance_name(tp).is_some_and(|(_, n)| n == name))?;
    let x = var("x", tp);
    let y = var("y", tp);
    let no_anns = AnnotationsDecl::empty();
    let builtin = |fun| make_builtin_apply2(fun, tp.clone(), SIRType::Boolean, x.clone(), y.clone(), &no_anns);
    let body = match tp {
        SIRType::Integer => builtin(DefaultFun::EqualsInteger),
        SIRType::ByteString => builtin(DefaultFun::EqualsByteString),
        SIRType::String => builtin(DefaultFun::EqualsString),
        SIRType::Data => builtin(DefaultFun::EqualsData),
        SIRType::Boolean => bool_equals(x.clone(), y.clone(), &no_anns),
        _ => bool_const(true),
    };
    Some((equality_type(tp), lambda(x, vec![], lambda(y, vec![], body))))
}

/// What `Eq.derived` generates: fields compared pairwise and joined with `&&` for a
/// product; for a sum, a match on the left operand with a match on the right in each
/// branch. `None` if some field's type has no instance.
fn derived_instance(
    decl: &DataDecl,
    data_decls: &BTreeMap<String, DataDecl>,
) -> Option<(SIRType, SIR)> {
    let evidence: Evidence = decl
        .type_params
        .iter()
        .map(|param| {
            let tp = type_var(param);
            (param.name.clone(), var(&evidence_name(param), &equality_type(&tp)))
        })
        .collect();
    let compare = |tp: &SIRType, left: SIR, right: SIR| {
        let instance = instance_in(tp, data_decls, &evidence)?;
        Some(apply(instance, tp, left, right, &AnnotationsDecl::empty()))
    };

    let self_tp = self_type(decl);
    let lhs = var("lhs", &self_tp);
    let rhs = var("rhs", &self_tp);

    let body = if is_product(decl) {
        let select = |operand: &SIR, field: &crate::sir_type::TypeBinding| SIR::Select {
            scrutinee: Box::new(operand.clone()),
            field: field.name.clone(),
            tp: field.tp.clone(),
            anns: AnnotationsDecl::empty(),
        };
        let comparisons = decl.constructors[0]
            .params
            .iter()
            .map(|field| compare(&field.tp, select(&lhs, field), select(&rhs, field)))
            .collect::<Option<Vec<_>>>()?;
        all(comparisons)
    } else {
        let cases = decl
            .constructors
            .iter()
            .map(|constr| {
                let bound = |side: &str| -> Vec<String> {
                    constr.params.iter().map(|p| format!("{side}_{}", p.name)).collect()
                };
                let (left_names, right_names) = (bound("lhs"), bound("rhs"));
                let comparisons = constr
                    .params
                    .iter()
                    .zip(left_names.iter().zip(&right_names))
                    .map(|(field, (l, r))| compare(&field.tp, var(l, &field.tp), var(r, &field.tp)))
                    .collect::<Option<Vec<_>>>()?;
                let same_constr = SIR::Match {
                    scrutinee: Box::new(rhs.clone()),
                    cases: vec![
                        case(constr_pattern(decl, constr, right_names), all(comparisons)),
                        case(Pattern::Wildcard, bool_const(false)),
                    ],
                    tp: SIRType::Boolean,
                    anns: AnnotationsDecl::empty(),
                };
                Some(case(constr_pattern(decl, constr, left_names), same_constr))
            })
            .collect::<Option<Vec<_>>>()?;
        // Scalus marks the outer match @unchecked: every constructor has a branch.
        let mut unchecked = AnnotationsDecl::empty();
        unchecked.data.insert("unchecked".to_string(), bool_const(true));
        SIR::Match {
            scrutinee: Box::new(lhs.clone()),
            cases,
            tp: SIRType::Boolean,
            anns: unchecked,
        }
    };

    // λevidence.. lhs rhs: the outermost lambda carries the type parameters
    let mut value = lambda(rhs, vec![], body);
    let mut params: Vec<SIR> = decl
        .type_params
        .iter()
        .map(|param| var(&evidence_name(param), &equality_type(&type_var(param))))
        .collect();
    params.push(lhs);
    let outermost = params.len() - 1;
    for (i, param) in params.into_iter().rev().enumerate() {
        let type_params = if i == outermost { decl.type_params.clone() } else { vec![] };
        value = lambda(param, type_params, value);
    }
    Some((instance_type(decl), value))
}

fn evidence_name(param: &TypeVar) -> String {
    format!("eq_{}", param.name)
}

// ---------------------------------------------------------------------------
// SIR construction
// ---------------------------------------------------------------------------

fn var(name: &str, tp: &SIRType) -> SIR {
    SIR::Var {
        name: name.to_string(),
        tp: tp.clone(),
        anns: AnnotationsDecl::empty(),
    }
}

fn bool_const(value: bool) -> SIR {
    SIR::Const {
        uplc_const: UplcConstant::Bool { value },
        tp: SIRType::Boolean,
        anns: AnnotationsDecl::empty(),
    }
}

fn lambda(param: SIR, type_params: Vec<TypeVar>, term: SIR) -> SIR {
    SIR::LamAbs {
        param: Box::new(param),
        term: Box::new(term),
        type_params,
        anns: AnnotationsDecl::empty(),
    }
}

fn case(pattern: Pattern, body: SIR) -> Case {
    Case {
        pattern,
        body,
        anns: AnnotationsDecl::empty(),
    }
}

fn constr_pattern(decl: &DataDecl, constr: &ConstrDecl, bindings: Vec<String>) -> Pattern {
    Pattern::Constr {
        constr_name: constr.name.clone(),
        decl_name: decl.name.clone(),
        bindings,
        type_params_bindings: vec![],
    }
}

/// `a && b && ..`, nested to the left as Scala parses it; `true` for no operands.
fn all(comparisons: Vec<SIR>) -> SIR {
    comparisons
        .into_iter()
        .reduce(|a, b| SIR::And {
            a: Box::new(a),
            b: Box::new(b),
            anns: AnnotationsDecl::empty(),
        })
        .unwrap_or_else(|| bool_const(true))
}

/// Every `ExternalVar` in `sir`, as (module, name).
fn external_vars(sir: &SIR, out: &mut Vec<(String, String)>) {
    match sir {
        SIR::ExternalVar { module_name, name, .. } => out.push((module_name.clone(), name.clone())),
        SIR::Var { .. } | SIR::Const { .. } | SIR::Builtin { .. } => {}
        SIR::LamAbs { term, .. } => external_vars(term, out),
        SIR::Apply { f, arg, .. } => {
            external_vars(f, out);
            external_vars(arg, out);
        }
        SIR::Let { bindings, body, .. } => {
            for binding in bindings {
                external_vars(&binding.value, out);
            }
            external_vars(body, out);
        }
        SIR::Constr { args, .. } => args.iter().for_each(|arg| external_vars(arg, out)),
        SIR::Match { scrutinee, cases, .. } => {
            external_vars(scrutinee, out);
            cases.iter().for_each(|c| external_vars(&c.body, out));
        }
        SIR::IfThenElse { cond, t, f, .. } => {
            external_vars(cond, out);
            external_vars(t, out);
            external_vars(f, out);
        }
        SIR::And { a, b, .. } => {
            external_vars(a, out);
            external_vars(b, out);
        }
        SIR::Error { msg, .. } => external_vars(msg, out),
        SIR::Decl { term, .. } => external_vars(term, out),
        SIR::Select { scrutinee, .. } => external_vars(scrutinee, out),
    }
}
