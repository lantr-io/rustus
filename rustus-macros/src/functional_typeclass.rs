use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemTrait, Meta, Expr, Lit, TraitItem};
use syn::parse::Parser;

/// `#[functional_typeclass]` — registers a trait as a single-method typeclass.
///
/// Derives everything from the trait definition:
/// - `scalus_name`: from the trait name (override with `name = "..."`)
/// - `method_name`: from the trait's single method name
///
/// Usage:
///   #[rustus::functional_typeclass]
///   pub trait MyEq { fn sir_eq() -> SIR; }
///
///   #[rustus::functional_typeclass(name = "scalus.custom.MyEq")]
///   pub trait MyEq { fn sir_eq() -> SIR; }
pub fn functional_typeclass_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemTrait);
    let trait_name = &input.ident;
    let trait_name_str = trait_name.to_string();

    // Parse optional attributes
    let parser = syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated;
    let meta = parser.parse(attr).expect("failed to parse functional_typeclass attributes");

    let mut scalus_name_override: Option<String> = None;
    let mut method_name_override: Option<String> = None;

    for m in &meta {
        if let Meta::NameValue(nv) = m {
            let key = nv.path.get_ident().map(|i| i.to_string()).unwrap_or_default();
            if let Expr::Lit(expr_lit) = &nv.value {
                if let Lit::Str(lit) = &expr_lit.lit {
                    match key.as_str() {
                        "name" => scalus_name_override = Some(lit.value()),
                        "method" => method_name_override = Some(lit.value()),
                        _ => {}
                    }
                }
            }
        }
    }

    let scalus_name = scalus_name_override.unwrap_or_else(|| trait_name_str.clone());
    let method_name = method_name_override.unwrap_or_else(|| find_single_method(&input));

    let expanded = quote! {
        #input

        rustus_core::inventory::submit! {
            rustus_core::typeclasses::TypeclassEntry {
                info: rustus_core::typeclasses::FunctionalTypeclassInfo {
                    rust_trait_name: #trait_name_str,
                    scalus_name: #scalus_name,
                    method_name: #method_name,
                },
            }
        }
    };

    expanded.into()
}

/// Extract the single method name from a functional typeclass trait.
fn find_single_method(trait_def: &ItemTrait) -> String {
    let methods: Vec<_> = trait_def
        .items
        .iter()
        .filter_map(|item| {
            if let TraitItem::Fn(method) = item {
                Some(method.sig.ident.to_string())
            } else {
                None
            }
        })
        .collect();

    match methods.len() {
        0 => panic!(
            "functional_typeclass trait `{}` must have exactly one method",
            trait_def.ident
        ),
        1 => methods.into_iter().next().unwrap(),
        _ => panic!(
            "functional_typeclass trait `{}` must have exactly one method, found {}",
            trait_def.ident,
            methods.len()
        ),
    }
}
