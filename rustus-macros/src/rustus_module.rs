use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Item, ItemMod, LitStr};

/// Rewrites `#[compile]` attributes inside a module to include `module = "..."`.
///
/// ```ignore
/// #[rustus_module("scalus.prelude.List$")]
/// mod list {
///     #[compile]
///     pub fn head(list: Data) -> Data { ... }
/// }
/// ```
/// becomes:
/// ```ignore
/// mod list {
///     #[compile(module = "scalus.prelude.List$")]
///     pub fn head(list: Data) -> Data { ... }
/// }
/// ```
pub fn rustus_module_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let module_name = parse_macro_input!(attr as LitStr);
    let module_name_str = module_name.value();
    let mut input_mod = parse_macro_input!(item as ItemMod);

    if let Some((brace, ref mut items)) = input_mod.content {
        let new_items: Vec<Item> = items
            .drain(..)
            .map(|item| inject_module_attr(item, &module_name_str))
            .collect();
        input_mod.content = Some((brace, new_items));
    }

    quote! { #input_mod }.into()
}

/// If item is a function with `#[compile]` or `#[compile(...)]`,
/// add/merge `module = "<module_name>"` into the attribute.
fn inject_module_attr(item: Item, module_name: &str) -> Item {
    match item {
        Item::Fn(mut func) => {
            for attr in &mut func.attrs {
                if is_compile_attr(attr) {
                    // Replace the attribute with module injected
                    let existing_tokens = &attr.meta;
                    let new_meta: syn::Meta = match existing_tokens {
                        // #[compile] → #[compile(module = "...")]
                        syn::Meta::Path(path) => {
                            syn::parse_quote! { #path(module = #module_name) }
                        }
                        // #[compile(name = "...")] → #[compile(module = "...", name = "...")]
                        syn::Meta::List(list) => {
                            let path = &list.path;
                            let existing = &list.tokens;
                            syn::parse_quote! { #path(module = #module_name, #existing) }
                        }
                        other => other.clone(),
                    };
                    attr.meta = new_meta;
                }
            }
            Item::Fn(func)
        }
        other => other,
    }
}

fn is_compile_attr(attr: &syn::Attribute) -> bool {
    let path = attr.path();
    let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    // Match both `#[compile]` and `#[rustus_macros::compile]`
    segments.last().map(|s| s == "compile").unwrap_or(false)
}
