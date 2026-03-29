extern crate proc_macro;

mod compile;
mod derive_data;
mod functional_typeclass;
mod rustus_module;

use proc_macro::TokenStream;

#[proc_macro_derive(ToData, attributes(rustus))]
pub fn derive_to_data(input: TokenStream) -> TokenStream {
    derive_data::derive_to_data_impl(input)
}

#[proc_macro_derive(FromData, attributes(rustus))]
pub fn derive_from_data(input: TokenStream) -> TokenStream {
    derive_data::derive_from_data_impl(input)
}

#[proc_macro_attribute]
pub fn compile(attr: TokenStream, item: TokenStream) -> TokenStream {
    compile::compile_impl(attr, item)
}

/// Apply a module name to all `#[compile]` functions inside a module.
/// Usage: `#[rustus_macros::rustus_module("scalus.prelude.List$")]`
#[proc_macro_attribute]
pub fn rustus_module(attr: TokenStream, item: TokenStream) -> TokenStream {
    rustus_module::rustus_module_impl(attr, item)
}

/// Register a trait as a functional typeclass for on-chain compilation.
/// Usage: `#[rustus::functional_typeclass(name = "scalus...Eq", method = "binary_predicate")]`
///
/// method types: "binary_predicate" → (A, A) -> Boolean, "binary_order" → (A, A) -> Order
#[proc_macro_attribute]
pub fn functional_typeclass(attr: TokenStream, item: TokenStream) -> TokenStream {
    functional_typeclass::functional_typeclass_impl(attr, item)
}
