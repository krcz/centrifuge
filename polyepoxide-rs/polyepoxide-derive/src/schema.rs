use proc_macro2::TokenStream;
use quote::quote;
use std::collections::{BTreeSet, HashMap};
use syn::{DeriveInput, Type};

use crate::{FieldAttrs, parse_field_attrs};

#[derive(Default, Clone)]
struct SlotUsage {
    self_slot: bool,
    generic_slots: BTreeSet<u16>,
}

impl SlotUsage {
    fn merge(&mut self, other: Self) {
        self.self_slot |= other.self_slot;
        self.generic_slots.extend(other.generic_slots);
    }
}

/// Generates the `schema()` method implementation.
pub fn generate_schema(input: &DeriveInput, crate_path: &TokenStream) -> syn::Result<TokenStream> {
    let self_type = &input.ident;
    let generic_params: Vec<_> = input
        .generics
        .type_params()
        .map(|p| p.ident.clone())
        .collect();
    let generic_slot_map: HashMap<String, u16> = generic_params
        .iter()
        .enumerate()
        .map(|(i, ident)| (ident.to_string(), (i + 1) as u16))
        .collect();

    let (root_bond_expr, usage) = match &input.data {
        syn::Data::Struct(data) => {
            generate_schema_struct(self_type, data, crate_path, &generic_slot_map)?
        }
        syn::Data::Enum(data) => {
            generate_schema_enum(self_type, data, crate_path, &generic_slot_map)?
        }
        syn::Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                input,
                "Oxide cannot be derived for unions",
            ));
        }
    };

    let template_body = root_bond_expr.clone();
    let body = instantiate_schema(root_bond_expr, usage, &generic_params, crate_path);

    Ok(quote! {
        fn schema() -> #crate_path::Bond<#crate_path::Structure> {
            #body
        }

        fn schema_template() -> #crate_path::Bond<#crate_path::Structure> {
            #template_body
        }
    })
}

fn generate_schema_struct(
    self_type: &syn::Ident,
    data: &syn::DataStruct,
    crate_path: &TokenStream,
    generic_slot_map: &HashMap<String, u16>,
) -> syn::Result<(TokenStream, SlotUsage)> {
    match &data.fields {
        syn::Fields::Named(fields) => {
            let mut usage = SlotUsage::default();
            let field_schemas: Vec<_> = fields
                .named
                .iter()
                .filter_map(|f| {
                    let attrs = parse_field_attrs(&f.attrs);
                    if attrs.skip {
                        return None;
                    }
                    let name = get_field_name(f, &attrs);
                    let (schema, field_usage) =
                        type_to_schema(&f.ty, self_type, crate_path, generic_slot_map);
                    usage.merge(field_usage);
                    Some(quote! { (#name.to_string(), #schema) })
                })
                .collect();
            Ok((
                quote! {
                    #crate_path::Bond::new(#crate_path::Structure::Record(
                        [#(#field_schemas),*].into_iter().collect()
                    ))
                },
                usage,
            ))
        }
        syn::Fields::Unnamed(fields) => {
            let mut usage = SlotUsage::default();
            let elem_schemas: Vec<_> = fields
                .unnamed
                .iter()
                .filter_map(|f| {
                    let attrs = parse_field_attrs(&f.attrs);
                    if attrs.skip {
                        return None;
                    }
                    let (schema, elem_usage) =
                        type_to_schema(&f.ty, self_type, crate_path, generic_slot_map);
                    usage.merge(elem_usage);
                    Some(schema)
                })
                .collect();
            Ok((
                quote! {
                    #crate_path::Bond::new(#crate_path::Structure::Tuple(vec![#(#elem_schemas),*]))
                },
                usage,
            ))
        }
        syn::Fields::Unit => Ok((
            quote! { #crate_path::Bond::new(#crate_path::Structure::Unit) },
            SlotUsage::default(),
        )),
    }
}

fn generate_schema_enum(
    self_type: &syn::Ident,
    data: &syn::DataEnum,
    crate_path: &TokenStream,
    generic_slot_map: &HashMap<String, u16>,
) -> syn::Result<(TokenStream, SlotUsage)> {
    let all_unit = data
        .variants
        .iter()
        .all(|v| matches!(v.fields, syn::Fields::Unit));

    if all_unit {
        let variant_names: Vec<_> = data
            .variants
            .iter()
            .map(|v| {
                let attrs = parse_variant_attrs(&v.attrs);
                let name = attrs.rename.unwrap_or_else(|| v.ident.to_string());
                quote! { #name.to_string() }
            })
            .collect();
        return Ok((
            quote! { #crate_path::Bond::new(#crate_path::Structure::Enum(vec![#(#variant_names),*])) },
            SlotUsage::default(),
        ));
    }

    let mut usage = SlotUsage::default();
    let variant_schemas: Vec<_> = data
        .variants
        .iter()
        .map(|v| {
            let attrs = parse_variant_attrs(&v.attrs);
            let name = attrs.rename.unwrap_or_else(|| v.ident.to_string());
            let (payload, payload_usage) =
                variant_payload_schema(&v.fields, self_type, crate_path, generic_slot_map);
            usage.merge(payload_usage);
            quote! { (#name.to_string(), #payload) }
        })
        .collect();

    Ok((
        quote! {
            #crate_path::Bond::new(#crate_path::Structure::Tagged(
                [#(#variant_schemas),*].into_iter().collect()
            ))
        },
        usage,
    ))
}

fn variant_payload_schema(
    fields: &syn::Fields,
    self_type: &syn::Ident,
    crate_path: &TokenStream,
    generic_slot_map: &HashMap<String, u16>,
) -> (TokenStream, SlotUsage) {
    match fields {
        syn::Fields::Unit => (
            quote! { #crate_path::Bond::new(#crate_path::Structure::Unit) },
            SlotUsage::default(),
        ),
        syn::Fields::Named(named) => {
            let mut usage = SlotUsage::default();
            let field_schemas: Vec<_> = named
                .named
                .iter()
                .filter_map(|f| {
                    let attrs = parse_field_attrs(&f.attrs);
                    if attrs.skip {
                        return None;
                    }
                    let name = get_field_name(f, &attrs);
                    let (schema, field_usage) =
                        type_to_schema(&f.ty, self_type, crate_path, generic_slot_map);
                    usage.merge(field_usage);
                    Some(quote! { (#name.to_string(), #schema) })
                })
                .collect();
            (
                quote! {
                    #crate_path::Bond::new(#crate_path::Structure::Record(
                        [#(#field_schemas),*].into_iter().collect()
                    ))
                },
                usage,
            )
        }
        syn::Fields::Unnamed(unnamed) => {
            if unnamed.unnamed.len() == 1 {
                let f = &unnamed.unnamed[0];
                let attrs = parse_field_attrs(&f.attrs);
                if attrs.skip {
                    return (
                        quote! { #crate_path::Bond::new(#crate_path::Structure::Unit) },
                        SlotUsage::default(),
                    );
                }
                return type_to_schema(&f.ty, self_type, crate_path, generic_slot_map);
            }

            let mut usage = SlotUsage::default();
            let elem_schemas: Vec<_> = unnamed
                .unnamed
                .iter()
                .filter_map(|f| {
                    let attrs = parse_field_attrs(&f.attrs);
                    if attrs.skip {
                        return None;
                    }
                    let (schema, elem_usage) =
                        type_to_schema(&f.ty, self_type, crate_path, generic_slot_map);
                    usage.merge(elem_usage);
                    Some(schema)
                })
                .collect();
            (
                quote! {
                    #crate_path::Bond::new(#crate_path::Structure::Tuple(vec![#(#elem_schemas),*]))
                },
                usage,
            )
        }
    }
}

fn get_field_name(field: &syn::Field, attrs: &FieldAttrs) -> String {
    attrs
        .rename
        .clone()
        .unwrap_or_else(|| field.ident.as_ref().unwrap().to_string())
}

/// Convert a Rust type to schema bond expression.
fn type_to_schema(
    ty: &Type,
    self_type: &syn::Ident,
    crate_path: &TokenStream,
    generic_slot_map: &HashMap<String, u16>,
) -> (TokenStream, SlotUsage) {
    match ty {
        Type::Path(type_path) => {
            if is_self_reference(type_path, self_type) {
                return (
                    quote! { #crate_path::Bond::from_ligation(#crate_path::Ligation::Slot(0)) },
                    SlotUsage {
                        self_slot: true,
                        generic_slots: BTreeSet::new(),
                    },
                );
            }

            if let Some(slot) = generic_slot_for(type_path, generic_slot_map) {
                let mut generic_slots = BTreeSet::new();
                let _ = generic_slots.insert(slot);
                return (
                    quote! { #crate_path::Bond::from_ligation(#crate_path::Ligation::Slot(#slot)) },
                    SlotUsage {
                        self_slot: false,
                        generic_slots,
                    },
                );
            }

            let last_segment = type_path.path.segments.last();
            if let Some(segment) = last_segment {
                let ident_str = segment.ident.to_string();
                match ident_str.as_str() {
                    "Vec" => {
                        if let Some(inner) = extract_single_generic_arg(&segment.arguments) {
                            let (inner_schema, usage) =
                                type_to_schema(&inner, self_type, crate_path, generic_slot_map);
                            return (
                                quote! {
                                    #crate_path::Bond::new(#crate_path::Structure::Sequence(#inner_schema))
                                },
                                usage,
                            );
                        }
                    }
                    "Option" => {
                        if let Some(inner) = extract_single_generic_arg(&segment.arguments) {
                            let (inner_schema, usage) =
                                type_to_schema(&inner, self_type, crate_path, generic_slot_map);
                            return (
                                quote! {
                                    #crate_path::Bond::new(#crate_path::Structure::Sequence(#inner_schema))
                                },
                                usage,
                            );
                        }
                    }
                    "Bond" => {
                        if let Some(inner) = extract_single_generic_arg(&segment.arguments) {
                            let (inner_schema, usage) =
                                type_to_schema(&inner, self_type, crate_path, generic_slot_map);
                            return (
                                quote! {
                                    #crate_path::Bond::new(#crate_path::Structure::Bond(#inner_schema))
                                },
                                usage,
                            );
                        }
                    }
                    "Box" => {
                        if let Some(inner) = extract_single_generic_arg(&segment.arguments) {
                            return type_to_schema(&inner, self_type, crate_path, generic_slot_map);
                        }
                    }
                    "Result" => {
                        if let Some(args) = extract_type_generic_args(&segment.arguments) {
                            if args.len() == 2 {
                                let mut usage = SlotUsage::default();
                                let (ok_schema, ok_usage) = type_to_schema(
                                    &args[0],
                                    self_type,
                                    crate_path,
                                    generic_slot_map,
                                );
                                usage.merge(ok_usage);
                                let (err_schema, err_usage) = type_to_schema(
                                    &args[1],
                                    self_type,
                                    crate_path,
                                    generic_slot_map,
                                );
                                usage.merge(err_usage);
                                return (
                                    quote! {
                                        #crate_path::Structure::result(#ok_schema, #err_schema)
                                    },
                                    usage,
                                );
                            }
                        }
                    }
                    "IndexMap" => {
                        if let Some(args) = extract_type_generic_args(&segment.arguments) {
                            if args.len() == 2 {
                                let mut usage = SlotUsage::default();
                                let (key_schema, key_usage) = type_to_schema(
                                    &args[0],
                                    self_type,
                                    crate_path,
                                    generic_slot_map,
                                );
                                usage.merge(key_usage);
                                let (value_schema, value_usage) = type_to_schema(
                                    &args[1],
                                    self_type,
                                    crate_path,
                                    generic_slot_map,
                                );
                                usage.merge(value_usage);
                                return (
                                    quote! {
                                        #crate_path::Structure::ordered_map(#key_schema, #value_schema)
                                    },
                                    usage,
                                );
                            }
                        }
                    }
                    "HashMap" => {
                        if let Some(args) = extract_type_generic_args(&segment.arguments) {
                            if args.len() == 2 {
                                let mut usage = SlotUsage::default();
                                let (key_schema, key_usage) = type_to_schema(
                                    &args[0],
                                    self_type,
                                    crate_path,
                                    generic_slot_map,
                                );
                                usage.merge(key_usage);
                                let (value_schema, value_usage) = type_to_schema(
                                    &args[1],
                                    self_type,
                                    crate_path,
                                    generic_slot_map,
                                );
                                usage.merge(value_usage);
                                return (
                                    quote! {
                                        #crate_path::Structure::map(#key_schema, #value_schema)
                                    },
                                    usage,
                                );
                            }
                        }
                    }
                    _ => {}
                }

                if let Some(args) = extract_type_generic_args(&segment.arguments) {
                    if !args.is_empty() {
                        return instantiate_generic_type(
                            ty,
                            &args,
                            self_type,
                            crate_path,
                            generic_slot_map,
                        );
                    }
                }
            }

            (
                quote! { <#type_path as #crate_path::Oxide>::schema() },
                SlotUsage::default(),
            )
        }
        Type::Tuple(tuple) if tuple.elems.is_empty() => (
            quote! { #crate_path::Bond::new(#crate_path::Structure::Unit) },
            SlotUsage::default(),
        ),
        Type::Tuple(tuple) => {
            let mut usage = SlotUsage::default();
            let elem_schemas: Vec<_> = tuple
                .elems
                .iter()
                .map(|t| {
                    let (schema, elem_usage) =
                        type_to_schema(t, self_type, crate_path, generic_slot_map);
                    usage.merge(elem_usage);
                    schema
                })
                .collect();
            (
                quote! { #crate_path::Bond::new(#crate_path::Structure::Tuple(vec![#(#elem_schemas),*])) },
                usage,
            )
        }
        Type::Reference(reference) => {
            type_to_schema(&reference.elem, self_type, crate_path, generic_slot_map)
        }
        _ => (
            quote! { <#ty as #crate_path::Oxide>::schema() },
            SlotUsage::default(),
        ),
    }
}

fn instantiate_schema(
    root_bond_expr: TokenStream,
    usage: SlotUsage,
    generic_params: &[syn::Ident],
    crate_path: &TokenStream,
) -> TokenStream {
    let instantiate_generics = !generic_params.is_empty();
    let generic_arg_exprs: Vec<_> = if instantiate_generics {
        generic_params
            .iter()
            .map(|ident| {
                quote! {
                    <#ident as #crate_path::Oxide>::schema()
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let instantiated = if instantiate_generics {
        quote! {
            #crate_path::instantiate_schema_template(#root_bond_expr, &[#(#generic_arg_exprs),*])
        }
    } else {
        root_bond_expr
    };

    if !usage.self_slot {
        return instantiated;
    }

    quote! {{
        let root = #instantiated;
        #crate_path::Bond::from_ligation(#crate_path::Ligation::Ligase(vec![
            #crate_path::ErasedBond::from(&root),
        ]))
    }}
}

fn instantiate_generic_type(
    type_path: &Type,
    generic_args: &[Type],
    self_type: &syn::Ident,
    crate_path: &TokenStream,
    generic_slot_map: &HashMap<String, u16>,
) -> (TokenStream, SlotUsage) {
    let mut usage = SlotUsage::default();
    let arg_schemas: Vec<_> = generic_args
        .iter()
        .map(|arg| {
            let (schema, arg_usage) = type_to_schema(arg, self_type, crate_path, generic_slot_map);
            usage.merge(arg_usage);
            schema
        })
        .collect();

    (
        quote! {
            #crate_path::instantiate_schema_template(
                <#type_path as #crate_path::Oxide>::schema_template(),
                &[#(#arg_schemas),*],
            )
        },
        usage,
    )
}

/// Check if a type path refers to the type being derived (self-reference).
fn is_self_reference(type_path: &syn::TypePath, self_type: &syn::Ident) -> bool {
    if let Some(segment) = type_path.path.segments.last() {
        return segment.ident == *self_type;
    }
    false
}

fn generic_slot_for(
    type_path: &syn::TypePath,
    generic_slot_map: &HashMap<String, u16>,
) -> Option<u16> {
    if type_path.qself.is_some() {
        return None;
    }

    if type_path.path.segments.len() != 1 {
        return None;
    }

    let segment = type_path.path.segments.last()?;
    if !matches!(segment.arguments, syn::PathArguments::None) {
        return None;
    }

    generic_slot_map.get(&segment.ident.to_string()).copied()
}

/// Extract the single generic argument from angle brackets, e.g., T from Vec<T>.
fn extract_single_generic_arg(args: &syn::PathArguments) -> Option<Type> {
    match args {
        syn::PathArguments::AngleBracketed(angle) => {
            if angle.args.len() == 1 {
                if let syn::GenericArgument::Type(ty) = &angle.args[0] {
                    return Some(ty.clone());
                }
            }
            None
        }
        _ => None,
    }
}

fn extract_type_generic_args(args: &syn::PathArguments) -> Option<Vec<Type>> {
    match args {
        syn::PathArguments::AngleBracketed(angle) => {
            let out: Vec<_> = angle
                .args
                .iter()
                .filter_map(|arg| match arg {
                    syn::GenericArgument::Type(ty) => Some(ty.clone()),
                    _ => None,
                })
                .collect();
            Some(out)
        }
        _ => None,
    }
}

#[derive(Default)]
struct VariantAttrs {
    rename: Option<String>,
}

fn parse_variant_attrs(attrs: &[syn::Attribute]) -> VariantAttrs {
    let mut result = VariantAttrs::default();

    for attr in attrs {
        if !attr.path().is_ident("oxide") {
            continue;
        }

        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let value: syn::LitStr = meta.value()?.parse()?;
                result.rename = Some(value.value());
            }
            Ok(())
        });
    }

    result
}
