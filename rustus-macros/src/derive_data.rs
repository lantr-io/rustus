use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::Parser;
use syn::{parse_macro_input, Data, DeriveInput, Fields, GenericParam, TypeParam};

/// Collected info about type parameters for generic types.
struct GenericsInfo {
    /// Rust type param idents: [T, U, ...]
    param_idents: Vec<syn::Ident>,
    /// SIR TypeVar names: ["A", "B", ...]
    sir_names: Vec<String>,
    /// SIR TypeVar opt_ids: [1, 2, ...]
    sir_ids: Vec<i64>,
}

impl GenericsInfo {
    fn from_generics(generics: &syn::Generics) -> Self {
        let type_params: Vec<&TypeParam> = generics
            .params
            .iter()
            .filter_map(|p| {
                if let GenericParam::Type(tp) = p {
                    Some(tp)
                } else {
                    None
                }
            })
            .collect();

        let param_idents: Vec<syn::Ident> = type_params.iter().map(|tp| tp.ident.clone()).collect();
        let sir_names: Vec<String> = (0..param_idents.len())
            .map(|i| ((b'A' + i as u8) as char).to_string())
            .collect();
        let sir_ids: Vec<i64> = (1..=param_idents.len() as i64).collect();

        GenericsInfo {
            param_idents,
            sir_names,
            sir_ids,
        }
    }

    fn is_empty(&self) -> bool {
        self.param_idents.is_empty()
    }

    /// Generate TypeVar tokens for DataDecl type_params field
    fn type_var_decls(&self) -> Vec<TokenStream2> {
        self.sir_names
            .iter()
            .zip(&self.sir_ids)
            .map(|(name, id)| {
                quote! {
                    rustus_core::sir_type::TypeVar {
                        name: #name.to_string(),
                        opt_id: Some(#id),
                        is_builtin: false,
                    }
                }
            })
            .collect()
    }

    /// Generate type_args for sir_type() — type-application using T::sir_type()
    fn type_application_args(&self) -> Vec<TokenStream2> {
        self.param_idents
            .iter()
            .map(|p| {
                quote! { <#p as rustus_core::sir_type::HasSIRType>::sir_type() }
            })
            .collect()
    }

    /// Generate TypeVar SIRType expressions (for use in DataDecl constructor params)
    fn type_var_sir_types(&self) -> Vec<(String, TokenStream2)> {
        self.param_idents
            .iter()
            .zip(self.sir_names.iter().zip(&self.sir_ids))
            .map(|(ident, (name, id))| {
                let ident_str = ident.to_string();
                let expr = quote! {
                    rustus_core::sir_type::SIRType::TypeVar {
                        name: #name.to_string(),
                        opt_id: Some(#id),
                        is_builtin: false,
                    }
                };
                (ident_str, expr)
            })
            .collect()
    }
}

pub fn derive_to_data_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let name_str = name.to_string();
    let register_fn_name = format_ident!("__rustus_register_{}", name_str.to_lowercase());

    let rattrs = parse_rustus_attrs(&input.attrs);
    let sir_name = rattrs.name.unwrap_or_else(|| name_str.clone());
    let is_one_element = rattrs.repr.as_deref() == Some("one_element");
    let is_list = rattrs.repr.as_deref() == Some("list");
    let ginfo = GenericsInfo::from_generics(&input.generics);

    let (to_data_impl, has_sir_type_impl) = match &input.data {
        Data::Enum(data_enum) => {
            let to_data_arms = if is_list {
                gen_list_to_data_body(name, data_enum)
            } else {
                gen_enum_to_data_arms(name, data_enum)
            };
            let sir_type_impl = gen_enum_has_sir_type(name, &sir_name, data_enum, &ginfo);
            (to_data_arms, sir_type_impl)
        }
        Data::Struct(data_struct) => {
            let is_map = rattrs.repr.as_deref() == Some("map");
            let to_data_body = if is_map {
                gen_map_to_data_body(name, data_struct)
            } else {
                gen_struct_to_data_body(data_struct, is_one_element)
            };
            let sir_type_impl = gen_struct_has_sir_type(name, &sir_name, data_struct, &ginfo, is_one_element, is_map);
            (to_data_body, sir_type_impl)
        }
        Data::Union(_) => {
            return syn::Error::new_spanned(name, "ToData cannot be derived for unions")
                .to_compile_error()
                .into();
        }
    };

    // For ToData impl: add generic bounds
    let (to_data_impl_generics, to_data_ty_generics) = if ginfo.is_empty() {
        (quote! {}, quote! {})
    } else {
        let params = &ginfo.param_idents;
        (
            quote! { <#(#params: rustus_core::data::ToData),*> },
            quote! { <#(#params),*> },
        )
    };

    // For registration: use concrete dummy type if generic
    let register_block = if ginfo.is_empty() {
        quote! {
            fn #register_fn_name(ctx: &mut rustus_core::registry::ResolutionContext) {
                if let ::core::option::Option::Some(decl) = <#name as rustus_core::sir_type::HasSIRType>::sir_data_decl() {
                    ctx.register_data_decl(#sir_name, decl);
                }
            }

            rustus_core::inventory::submit! {
                rustus_core::registry::PreSirEntry {
                    name: #sir_name,
                    module: None,
                    kind: rustus_core::registry::EntryKind::TypeDecl,
                    builder: #register_fn_name,
                }
            }
        }
    } else {
        // Use TypeParam<N> as dummy — sir_type() returns TypeVar, so
        // self-referential fields like List<T> naturally get TypeVars.
        let dummy_args: Vec<TokenStream2> = ginfo
            .sir_ids
            .iter()
            .map(|id| quote! { rustus_core::sir_type::TypeParam<#id> })
            .collect();
        quote! {
            fn #register_fn_name(ctx: &mut rustus_core::registry::ResolutionContext) {
                if let ::core::option::Option::Some(decl) = <#name<#(#dummy_args),*> as rustus_core::sir_type::HasSIRType>::sir_data_decl() {
                    ctx.register_data_decl(#sir_name, decl);
                }
            }

            rustus_core::inventory::submit! {
                rustus_core::registry::PreSirEntry {
                    name: #sir_name,
                    module: None,
                    kind: rustus_core::registry::EntryKind::TypeDecl,
                    builder: #register_fn_name,
                }
            }
        }
    };

    // Generate OnchainPartialEq impl
    let onchain_eq_impl = gen_onchain_partial_eq(name, &input.data, is_one_element, &ginfo);

    let expanded = quote! {
        impl #to_data_impl_generics rustus_core::data::ToData for #name #to_data_ty_generics {
            fn to_data(&self) -> rustus_core::data::Data {
                #to_data_impl
            }
        }

        #has_sir_type_impl

        #onchain_eq_impl

        #register_block
    };

    expanded.into()
}

pub fn derive_from_data_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let ginfo = GenericsInfo::from_generics(&input.generics);
    let rattrs = parse_rustus_attrs(&input.attrs);
    let is_one_element = rattrs.repr.as_deref() == Some("one_element");
    let is_list = rattrs.repr.as_deref() == Some("list");

    let from_data_body = match &input.data {
        Data::Enum(data_enum) => if is_list {
            gen_list_from_data_body(name)
        } else {
            gen_enum_from_data_body(name, data_enum)
        },
        Data::Struct(data_struct) => {
            let is_map = rattrs.repr.as_deref() == Some("map");
            if is_map {
                gen_map_from_data_body(name, data_struct)
            } else {
                gen_struct_from_data_body(name, data_struct, is_one_element)
            }
        },
        Data::Union(_) => {
            return syn::Error::new_spanned(name, "FromData cannot be derived for unions")
                .to_compile_error()
                .into();
        }
    };

    let (impl_generics, ty_generics) = if ginfo.is_empty() {
        (quote! {}, quote! {})
    } else {
        let params = &ginfo.param_idents;
        (
            quote! { <#(#params: rustus_core::data::FromData),*> },
            quote! { <#(#params),*> },
        )
    };

    let expanded = quote! {
        impl #impl_generics rustus_core::data::FromData for #name #ty_generics {
            fn from_data(data: &rustus_core::data::Data) -> Result<Self, rustus_core::data::DataError> {
                #from_data_body
            }
        }
    };

    expanded.into()
}

// --- Map repr: struct with single Vec<(K, V)> field → Data::Map ---

fn gen_map_to_data_body(_name: &syn::Ident, data_struct: &syn::DataStruct) -> TokenStream2 {
    // Walk the List<Pair<K, V>> field directly, encoding each pair's fst/snd.
    let field = data_struct.fields.iter().next().expect("map repr requires at least one field");
    let fname = &field.ident;
    quote! {
        {
            let mut pairs = vec![];
            let mut current = &self.#fname;
            loop {
                match current {
                    // Nil variant (tag 0, no fields)
                    list @ _ if {
                        let d = rustus_core::data::ToData::to_data(list);
                        matches!(d, rustus_core::data::Data::List { ref values } if values.is_empty())
                    } => break,
                    _ => {
                        // Cons variant — encode the whole list and extract pairs from Data::List
                        let list_data = rustus_core::data::ToData::to_data(&self.#fname);
                        if let rustus_core::data::Data::List { values } = list_data {
                            for item in values {
                                if let rustus_core::data::Data::Constr { tag: 0, args } = item {
                                    let mut it = args.into_iter();
                                    let k = it.next().unwrap_or(rustus_core::data::Data::unit());
                                    let v = it.next().unwrap_or(rustus_core::data::Data::unit());
                                    pairs.push((k, v));
                                }
                            }
                        }
                        break;
                    }
                }
            }
            rustus_core::data::Data::Map { values: pairs }
        }
    }
}

fn gen_map_from_data_body(name: &syn::Ident, data_struct: &syn::DataStruct) -> TokenStream2 {
    let name_str = name.to_string();
    let field = data_struct.fields.iter().next().expect("map repr requires at least one field");
    let fname = &field.ident;
    quote! {
        match data {
            rustus_core::data::Data::Map { values } => {
                // Convert Data::Map pairs back through Data::List of Constr(0, [k, v])
                // to decode via the inner field's FromData
                let list_items: Vec<rustus_core::data::Data> = values.iter().map(|(k, v)| {
                    rustus_core::data::Data::Constr {
                        tag: 0,
                        args: vec![k.clone(), v.clone()],
                    }
                }).collect();
                let list_data = rustus_core::data::Data::List { values: list_items };
                let #fname = rustus_core::data::FromData::from_data(&list_data)?;
                Ok(#name { #fname })
            }
            _ => Err(rustus_core::data::DataError::UnexpectedVariant {
                expected: concat!(#name_str, " (Data::Map)"),
            }),
        }
    }
}

// --- List repr: Nil/Cons enum ↔ Data::List ---

fn gen_list_from_data_body(name: &syn::Ident) -> TokenStream2 {
    // Data::List { values } → build Cons chain in reverse
    quote! {
        match data {
            rustus_core::data::Data::List { values } => {
                let mut result = #name::Nil;
                for item in values.iter().rev() {
                    result = #name::Cons {
                        head: rustus_core::data::FromData::from_data(item)?,
                        tail: Box::new(result),
                    };
                }
                Ok(result)
            }
            _ => Err(rustus_core::data::DataError::UnexpectedVariant {
                expected: "List (Data::List)",
            }),
        }
    }
}

fn gen_list_to_data_body(name: &syn::Ident, _data_enum: &syn::DataEnum) -> TokenStream2 {
    // Walk the Cons chain and collect elements into Data::List
    quote! {
        {
            let mut items = vec![];
            let mut current = self;
            loop {
                match current {
                    #name::Nil => break,
                    #name::Cons { head, tail } => {
                        items.push(rustus_core::data::ToData::to_data(head));
                        current = tail;
                    }
                }
            }
            rustus_core::data::Data::List { values: items }
        }
    }
}

// --- Enum ToData ---

fn gen_enum_to_data_arms(name: &syn::Ident, data_enum: &syn::DataEnum) -> TokenStream2 {
    let arms: Vec<TokenStream2> = data_enum
        .variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let vname = &variant.ident;
            let tag = i as i64;
            match &variant.fields {
                Fields::Unit => {
                    quote! {
                        #name::#vname => rustus_core::data::Data::Constr { tag: #tag, args: vec![] }
                    }
                }
                Fields::Named(fields) => {
                    let field_names: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();
                    let to_data_calls: Vec<TokenStream2> = field_names
                        .iter()
                        .map(|f| quote! { rustus_core::data::ToData::to_data(#f) })
                        .collect();
                    quote! {
                        #name::#vname { #(#field_names),* } => rustus_core::data::Data::Constr {
                            tag: #tag,
                            args: vec![#(#to_data_calls),*],
                        }
                    }
                }
                Fields::Unnamed(fields) => {
                    let bindings: Vec<syn::Ident> = (0..fields.unnamed.len())
                        .map(|i| format_ident!("__field{}", i))
                        .collect();
                    let to_data_calls: Vec<TokenStream2> = bindings
                        .iter()
                        .map(|b| quote! { rustus_core::data::ToData::to_data(#b) })
                        .collect();
                    quote! {
                        #name::#vname(#(#bindings),*) => rustus_core::data::Data::Constr {
                            tag: #tag,
                            args: vec![#(#to_data_calls),*],
                        }
                    }
                }
            }
        })
        .collect();

    quote! {
        match self {
            #(#arms),*
        }
    }
}

// --- Enum FromData ---

fn gen_enum_from_data_body(name: &syn::Ident, data_enum: &syn::DataEnum) -> TokenStream2 {
    let name_str = name.to_string();
    let arms: Vec<TokenStream2> = data_enum
        .variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let vname = &variant.ident;
            let tag = i as i64;
            match &variant.fields {
                Fields::Unit => quote! { #tag => Ok(#name::#vname) },
                Fields::Named(fields) => {
                    let decoders: Vec<TokenStream2> = fields.named.iter().enumerate().map(|(fi, f)| {
                        let fname = &f.ident;
                        quote! {
                            #fname: rustus_core::data::FromData::from_data(
                                args.get(#fi).ok_or(rustus_core::data::DataError::MissingField { index: #fi })?
                            )?
                        }
                    }).collect();
                    quote! { #tag => Ok(#name::#vname { #(#decoders),* }) }
                }
                Fields::Unnamed(fields) => {
                    let decoders: Vec<TokenStream2> = (0..fields.unnamed.len()).map(|fi| {
                        quote! {
                            rustus_core::data::FromData::from_data(
                                args.get(#fi).ok_or(rustus_core::data::DataError::MissingField { index: #fi })?
                            )?
                        }
                    }).collect();
                    quote! { #tag => Ok(#name::#vname(#(#decoders),*)) }
                }
            }
        })
        .collect();

    quote! {
        match data {
            rustus_core::data::Data::Constr { tag, args } => {
                match *tag {
                    #(#arms,)*
                    other => Err(rustus_core::data::DataError::UnexpectedTag {
                        expected: #name_str, got: other,
                    }),
                }
            }
            _ => Err(rustus_core::data::DataError::UnexpectedVariant { expected: "Constr" }),
        }
    }
}

// --- Struct ToData / FromData ---

fn gen_struct_to_data_body(data_struct: &syn::DataStruct, one_element: bool) -> TokenStream2 {
    if one_element {
        // ProductCaseOneElement: just the single field's toData, no Constr wrapper
        return match &data_struct.fields {
            Fields::Named(fields) => {
                let fname = &fields.named.first().unwrap().ident;
                quote! { rustus_core::data::ToData::to_data(&self.#fname) }
            }
            Fields::Unnamed(_) => {
                quote! { rustus_core::data::ToData::to_data(&self.0) }
            }
            Fields::Unit => quote! { rustus_core::data::Data::unit() },
        };
    }
    match &data_struct.fields {
        Fields::Named(fields) => {
            let calls: Vec<TokenStream2> = fields.named.iter().map(|f| {
                let fname = &f.ident;
                quote! { rustus_core::data::ToData::to_data(&self.#fname) }
            }).collect();
            quote! { rustus_core::data::Data::Constr { tag: 0, args: vec![#(#calls),*] } }
        }
        Fields::Unit => quote! { rustus_core::data::Data::Constr { tag: 0, args: vec![] } },
        Fields::Unnamed(fields) => {
            let calls: Vec<TokenStream2> = (0..fields.unnamed.len()).map(|i| {
                let idx = syn::Index::from(i);
                quote! { rustus_core::data::ToData::to_data(&self.#idx) }
            }).collect();
            quote! { rustus_core::data::Data::Constr { tag: 0, args: vec![#(#calls),*] } }
        }
    }
}

fn gen_struct_from_data_body(name: &syn::Ident, data_struct: &syn::DataStruct, one_element: bool) -> TokenStream2 {
    let name_str = name.to_string();
    if one_element {
        // ProductCaseOneElement: the Data IS the single field's data (no Constr wrapper)
        return match &data_struct.fields {
            Fields::Named(fields) => {
                let fname = &fields.named.first().unwrap().ident;
                quote! {
                    Ok(#name { #fname: rustus_core::data::FromData::from_data(data)? })
                }
            }
            Fields::Unnamed(_) => {
                quote! {
                    Ok(#name(rustus_core::data::FromData::from_data(data)?))
                }
            }
            Fields::Unit => quote! { Ok(#name) },
        };
    }
    match &data_struct.fields {
        Fields::Named(fields) => {
            let decoders: Vec<TokenStream2> = fields.named.iter().enumerate().map(|(i, f)| {
                let fname = &f.ident;
                quote! {
                    #fname: rustus_core::data::FromData::from_data(
                        args.get(#i).ok_or(rustus_core::data::DataError::MissingField { index: #i })?
                    )?
                }
            }).collect();
            quote! {
                match data {
                    rustus_core::data::Data::Constr { tag: 0, args } => Ok(#name { #(#decoders),* }),
                    rustus_core::data::Data::Constr { tag, .. } =>
                        Err(rustus_core::data::DataError::UnexpectedTag { expected: #name_str, got: *tag }),
                    _ => Err(rustus_core::data::DataError::UnexpectedVariant { expected: "Constr" }),
                }
            }
        }
        Fields::Unit => quote! {
            match data {
                rustus_core::data::Data::Constr { tag: 0, .. } => Ok(#name),
                rustus_core::data::Data::Constr { tag, .. } =>
                    Err(rustus_core::data::DataError::UnexpectedTag { expected: #name_str, got: *tag }),
                _ => Err(rustus_core::data::DataError::UnexpectedVariant { expected: "Constr" }),
            }
        },
        Fields::Unnamed(fields) => {
            let decoders: Vec<TokenStream2> = (0..fields.unnamed.len()).map(|i| {
                quote! {
                    rustus_core::data::FromData::from_data(
                        args.get(#i).ok_or(rustus_core::data::DataError::MissingField { index: #i })?
                    )?
                }
            }).collect();
            quote! {
                match data {
                    rustus_core::data::Data::Constr { tag: 0, args } => Ok(#name(#(#decoders),*)),
                    rustus_core::data::Data::Constr { tag, .. } =>
                        Err(rustus_core::data::DataError::UnexpectedTag { expected: #name_str, got: *tag }),
                    _ => Err(rustus_core::data::DataError::UnexpectedVariant { expected: "Constr" }),
                }
            }
        }
    }
}

// --- Enum HasSIRType ---

fn gen_enum_has_sir_type(
    name: &syn::Ident,
    sir_name: &str,
    data_enum: &syn::DataEnum,
    ginfo: &GenericsInfo,
) -> TokenStream2 {
    let is_scalus_style = sir_name.contains('.');
    let tv_map = ginfo.type_var_sir_types();
    let type_var_decls = ginfo.type_var_decls();
    let type_app_args = ginfo.type_application_args();

    let constr_decls: Vec<TokenStream2> = data_enum
        .variants
        .iter()
        .map(|variant| {
            let vname_str = if is_scalus_style {
                format!("{}$.{}", sir_name, variant.ident)
            } else {
                format!("{}::{}", sir_name, variant.ident)
            };
            let params = gen_type_bindings_for_fields_generic(&variant.fields, &tv_map);
            let constr_tv = &type_var_decls;
            let parent_ta: Vec<TokenStream2> = ginfo
                .sir_names
                .iter()
                .zip(&ginfo.sir_ids)
                .map(|(n, id)| {
                    quote! {
                        rustus_core::sir_type::SIRType::TypeVar {
                            name: #n.to_string(), opt_id: Some(#id), is_builtin: false,
                        }
                    }
                })
                .collect();
            quote! {
                rustus_core::sir_type::ConstrDecl {
                    name: #vname_str.to_string(),
                    params: vec![#(#params),*],
                    type_params: vec![#(#constr_tv.clone()),*],
                    parent_type_args: vec![#(#parent_ta),*],
                    annotations: rustus_core::module::AnnotationsDecl::empty(),
                }
            }
        })
        .collect();

    // Generate impl with or without generics
    if ginfo.is_empty() {
        quote! {
            impl rustus_core::sir_type::HasSIRType for #name {
                fn sir_type() -> rustus_core::sir_type::SIRType {
                    rustus_core::sir_type::SIRType::SumCaseClass {
                        decl_name: #sir_name.to_string(),
                        type_args: vec![],
                    }
                }
                fn sir_data_decl() -> ::core::option::Option<rustus_core::sir_type::DataDecl> {
                    ::core::option::Option::Some(rustus_core::sir_type::DataDecl {
                        name: #sir_name.to_string(),
                        constructors: vec![#(#constr_decls),*],
                        type_params: vec![],
                        annotations: rustus_core::module::AnnotationsDecl::empty(),
                    })
                }
            }
        }
    } else {
        let params = &ginfo.param_idents;
        quote! {
            impl<#(#params: rustus_core::sir_type::HasSIRType),*> rustus_core::sir_type::HasSIRType for #name<#(#params),*> {
                fn sir_type() -> rustus_core::sir_type::SIRType {
                    rustus_core::sir_type::SIRType::SumCaseClass {
                        decl_name: #sir_name.to_string(),
                        type_args: vec![#(#type_app_args),*],
                    }
                }
                fn sir_data_decl() -> ::core::option::Option<rustus_core::sir_type::DataDecl> {
                    ::core::option::Option::Some(rustus_core::sir_type::DataDecl {
                        name: #sir_name.to_string(),
                        constructors: vec![#(#constr_decls),*],
                        type_params: vec![#(#type_var_decls),*],
                        annotations: rustus_core::module::AnnotationsDecl::empty(),
                    })
                }
            }
        }
    }
}

// --- Struct HasSIRType ---

fn gen_struct_has_sir_type(
    name: &syn::Ident,
    sir_name: &str,
    data_struct: &syn::DataStruct,
    ginfo: &GenericsInfo,
    is_one_element: bool,
    is_map: bool,
) -> TokenStream2 {
    let tv_map = ginfo.type_var_sir_types();
    let type_var_decls = ginfo.type_var_decls();
    let type_app_args = ginfo.type_application_args();
    let params = gen_type_bindings_for_fields_generic(&data_struct.fields, &tv_map);

    // UplcRepr annotation: maps repr attribute to Scalus representation name
    let uplc_repr_name = if is_one_element {
        Some("ProductCaseOneElement")
    } else if is_map {
        Some("Map")
    } else {
        None
    };
    let decl_annotations = if let Some(repr_name) = uplc_repr_name {
        quote! {
            {
                let mut __anns = rustus_core::module::AnnotationsDecl::empty();
                __anns.data.insert(
                    "uplcRepr".to_string(),
                    rustus_core::sir::SIR::Const {
                        uplc_const: rustus_core::constant::UplcConstant::String {
                            value: #repr_name.to_string(),
                        },
                        tp: rustus_core::sir_type::SIRType::String,
                        anns: rustus_core::module::AnnotationsDecl::empty(),
                    },
                );
                __anns
            }
        }
    } else {
        quote! { rustus_core::module::AnnotationsDecl::empty() }
    };

    if ginfo.is_empty() {
        quote! {
            impl rustus_core::sir_type::HasSIRType for #name {
                fn sir_type() -> rustus_core::sir_type::SIRType {
                    rustus_core::sir_type::SIRType::CaseClass {
                        constr_name: #sir_name.to_string(),
                        decl_name: #sir_name.to_string(),
                        type_args: vec![],
                    }
                }
                fn sir_data_decl() -> ::core::option::Option<rustus_core::sir_type::DataDecl> {
                    ::core::option::Option::Some(rustus_core::sir_type::DataDecl {
                        name: #sir_name.to_string(),
                        constructors: vec![
                            rustus_core::sir_type::ConstrDecl {
                                name: #sir_name.to_string(),
                                params: vec![#(#params),*],
                                type_params: vec![],
                                parent_type_args: vec![],
                                annotations: rustus_core::module::AnnotationsDecl::empty(),
                            }
                        ],
                        type_params: vec![],
                        annotations: #decl_annotations,
                    })
                }
            }
        }
    } else {
        let gparams = &ginfo.param_idents;
        quote! {
            impl<#(#gparams: rustus_core::sir_type::HasSIRType),*> rustus_core::sir_type::HasSIRType for #name<#(#gparams),*> {
                fn sir_type() -> rustus_core::sir_type::SIRType {
                    rustus_core::sir_type::SIRType::CaseClass {
                        constr_name: #sir_name.to_string(),
                        decl_name: #sir_name.to_string(),
                        type_args: vec![#(#type_app_args),*],
                    }
                }
                fn sir_data_decl() -> ::core::option::Option<rustus_core::sir_type::DataDecl> {
                    ::core::option::Option::Some(rustus_core::sir_type::DataDecl {
                        name: #sir_name.to_string(),
                        constructors: vec![
                            rustus_core::sir_type::ConstrDecl {
                                name: #sir_name.to_string(),
                                params: vec![#(#params),*],
                                type_params: vec![#(#type_var_decls),*],
                                parent_type_args: vec![],
                                annotations: rustus_core::module::AnnotationsDecl::empty(),
                            }
                        ],
                        type_params: vec![#(#type_var_decls),*],
                        annotations: #decl_annotations,
                    })
                }
            }
        }
    }
}

// --- Helper: field types → TypeBinding expressions ---
// "Generic mode": type params become TypeVars, other types use HasSIRType

fn gen_type_bindings_for_fields_generic(
    fields: &Fields,
    tv_map: &[(String, TokenStream2)],
) -> Vec<TokenStream2> {
    match fields {
        Fields::Named(named) => named
            .named
            .iter()
            .map(|f| {
                let fname_str = f.ident.as_ref().unwrap().to_string();
                let sir_type_expr = rust_type_to_sir_type_generic(&f.ty, tv_map);
                quote! {
                    rustus_core::sir_type::TypeBinding {
                        name: #fname_str.to_string(),
                        tp: #sir_type_expr,
                    }
                }
            })
            .collect(),
        Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let fname_str = format!("_{}", i);
                let sir_type_expr = rust_type_to_sir_type_generic(&f.ty, tv_map);
                quote! {
                    rustus_core::sir_type::TypeBinding {
                        name: #fname_str.to_string(),
                        tp: #sir_type_expr,
                    }
                }
            })
            .collect(),
        Fields::Unit => vec![],
    }
}

/// Map a Rust type to a SIRType expression, with type params → TypeVars.
fn rust_type_to_sir_type_generic(
    ty: &syn::Type,
    tv_map: &[(String, TokenStream2)],
) -> TokenStream2 {
    let type_str = quote!(#ty).to_string().replace(' ', "");

    // Check if it's a type parameter
    for (param_name, tv_expr) in tv_map {
        if type_str == *param_name {
            return tv_expr.clone();
        }
    }

    // Primitives
    match type_str.as_str() {
        "bool" => return quote! { rustus_core::sir_type::SIRType::Boolean },
        "i64" | "i128" => return quote! { rustus_core::sir_type::SIRType::Integer },
        "Vec<u8>" => return quote! { rustus_core::sir_type::SIRType::ByteString },
        "String" => return quote! { rustus_core::sir_type::SIRType::String },
        "()" => return quote! { rustus_core::sir_type::SIRType::Unit },
        "Data" | "rustus_core::data::Data" => {
            return quote! { rustus_core::sir_type::SIRType::Data }
        }
        _ => {}
    }

    // Box<T> — transparent
    if let Some(inner) = extract_box_inner(ty) {
        return rust_type_to_sir_type_generic(inner, tv_map);
    }

    // User type — use HasSIRType. For generic types like List<T> where T is a type param,
    // TypeParam<N> handles this: sir_type() returns TypeVar, so List<TypeParam<1>> naturally
    // produces SumCaseClass("List", [TypeVar("A",1)]).
    quote! { <#ty as rustus_core::sir_type::HasSIRType>::sir_type() }
}

// --- OnchainPartialEq generation ---

fn gen_onchain_partial_eq(
    name: &syn::Ident,
    data: &Data,
    one_element: bool,
    ginfo: &GenericsInfo,
) -> TokenStream2 {
    let eq_body = if one_element {
        // one_element struct: delegate to inner field's OnchainPartialEq
        if let Data::Struct(ds) = data {
            let inner_ty = match &ds.fields {
                Fields::Named(f) => &f.named.first().unwrap().ty,
                Fields::Unnamed(f) => &f.unnamed.first().unwrap().ty,
                Fields::Unit => {
                    return quote! {
                        impl rustus_core::typeclasses::OnchainPartialEq for #name {
                            fn sir_eq() -> rustus_core::sir::SIR {
                                <rustus_core::data::Data as rustus_core::typeclasses::OnchainPartialEq>::sir_eq()
                            }
                        }
                    };
                }
            };
            // Unwrap Box if present
            let actual_ty = if let Some(inner) = extract_box_inner(inner_ty) {
                inner
            } else {
                inner_ty
            };
            quote! {
                <#actual_ty as rustus_core::typeclasses::OnchainPartialEq>::sir_eq()
            }
        } else {
            quote! { <rustus_core::data::Data as rustus_core::typeclasses::OnchainPartialEq>::sir_eq() }
        }
    } else {
        // Default: use equalsData (compare full Data encoding)
        quote! { <rustus_core::data::Data as rustus_core::typeclasses::OnchainPartialEq>::sir_eq() }
    };

    if ginfo.is_empty() {
        quote! {
            impl rustus_core::typeclasses::OnchainPartialEq for #name {
                fn sir_eq() -> rustus_core::sir::SIR {
                    #eq_body
                }
            }
        }
    } else {
        let params = &ginfo.param_idents;
        quote! {
            impl<#(#params: rustus_core::typeclasses::OnchainPartialEq),*> rustus_core::typeclasses::OnchainPartialEq for #name<#(#params),*> {
                fn sir_eq() -> rustus_core::sir::SIR {
                    #eq_body
                }
            }
        }
    }
}

// --- Attribute parsing ---

/// Parsed rustus attributes from `#[rustus(name = "...", repr = "...")]`
struct RustusAttrs {
    name: Option<String>,
    repr: Option<String>, // "one_element", "product", "sum"
}

fn parse_rustus_attrs(attrs: &[syn::Attribute]) -> RustusAttrs {
    let mut result = RustusAttrs {
        name: None,
        repr: None,
    };
    for attr in attrs {
        if !attr.path().is_ident("rustus") {
            continue;
        }
        if let syn::Meta::List(list) = &attr.meta {
            // Try parsing as comma-separated name-value pairs
            let parser =
                syn::punctuated::Punctuated::<syn::MetaNameValue, syn::Token![,]>::parse_terminated;
            if let Ok(pairs) = parser.parse2(list.tokens.clone()) {
                for nv in pairs {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(lit_str),
                        ..
                    }) = &nv.value
                    {
                        if nv.path.is_ident("name") {
                            result.name = Some(lit_str.value());
                        } else if nv.path.is_ident("repr") {
                            result.repr = Some(lit_str.value());
                        }
                    }
                }
            } else {
                // Try single name-value
                let nested: Result<syn::MetaNameValue, _> = syn::parse2(list.tokens.clone());
                if let Ok(nv) = nested {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(lit_str),
                        ..
                    }) = &nv.value
                    {
                        if nv.path.is_ident("name") {
                            result.name = Some(lit_str.value());
                        } else if nv.path.is_ident("repr") {
                            result.repr = Some(lit_str.value());
                        }
                    }
                }
            }
        }
    }
    result
}

/// If ty is `Box<T>`, return the inner T.
fn extract_box_inner(ty: &syn::Type) -> Option<&syn::Type> {
    if let syn::Type::Path(type_path) = ty {
        let seg = type_path.path.segments.last()?;
        if seg.ident == "Box" {
            if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                    return Some(inner);
                }
            }
        }
    }
    None
}
