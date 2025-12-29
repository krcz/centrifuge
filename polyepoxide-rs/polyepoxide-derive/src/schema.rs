use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Type};

use crate::{parse_field_attrs, FieldAttrs};

/// Generates the `schema()` method implementation.
pub fn generate_schema(input: &DeriveInput, crate_path: &TokenStream) -> syn::Result<TokenStream> {
    let self_type = &input.ident;

    match &input.data {
        syn::Data::Struct(data) => generate_schema_struct(self_type, data, crate_path),
        syn::Data::Enum(data) => generate_schema_enum(self_type, data, crate_path),
        syn::Data::Union(_) => Err(syn::Error::new_spanned(
            input,
            "Oxide cannot be derived for unions",
        )),
    }
}

fn generate_schema_struct(
    self_type: &syn::Ident,
    data: &syn::DataStruct,
    crate_path: &TokenStream,
) -> syn::Result<TokenStream> {
    let schema_expr = match &data.fields {
        syn::Fields::Named(fields) => {
            let field_schemas: Vec<_> = fields
                .named
                .iter()
                .filter_map(|f| {
                    let attrs = parse_field_attrs(&f.attrs);
                    if attrs.skip {
                        return None;
                    }
                    let name = get_field_name(f, &attrs);
                    let ty = &f.ty;
                    let schema = type_to_schema(ty, self_type, crate_path);
                    Some(quote! { (#name, #schema) })
                })
                .collect();
            quote! { #crate_path::Structure::record([#(#field_schemas),*]) }
        }
        syn::Fields::Unnamed(fields) => {
            let elem_schemas: Vec<_> = fields
                .unnamed
                .iter()
                .filter_map(|f| {
                    let attrs = parse_field_attrs(&f.attrs);
                    if attrs.skip {
                        return None;
                    }
                    let schema = type_to_schema(&f.ty, self_type, crate_path);
                    Some(schema)
                })
                .collect();
            quote! { #crate_path::Structure::tuple([#(#elem_schemas),*]) }
        }
        syn::Fields::Unit => {
            quote! { #crate_path::Structure::Unit }
        }
    };

    Ok(quote! {
        fn schema() -> #crate_path::Structure {
            #schema_expr
        }
    })
}

fn generate_schema_enum(self_type: &syn::Ident, data: &syn::DataEnum, crate_path: &TokenStream) -> syn::Result<TokenStream> {
    // Check if all variants are unit variants (C-style enum)
    let all_unit = data
        .variants
        .iter()
        .all(|v| matches!(v.fields, syn::Fields::Unit));

    let schema_expr = if all_unit {
        // C-style enum: Structure::Enum
        let variant_names: Vec<_> = data
            .variants
            .iter()
            .map(|v| {
                let attrs = parse_variant_attrs(&v.attrs);
                let name = attrs
                    .rename
                    .unwrap_or_else(|| v.ident.to_string());
                quote! { #name.to_string() }
            })
            .collect();
        quote! { #crate_path::Structure::Enum(vec![#(#variant_names),*]) }
    } else {
        // Tagged union: Structure::tagged
        let variant_schemas: Vec<_> = data
            .variants
            .iter()
            .map(|v| {
                let attrs = parse_variant_attrs(&v.attrs);
                let name = attrs
                    .rename
                    .unwrap_or_else(|| v.ident.to_string());
                let payload = variant_payload_schema(&v.fields, self_type, crate_path);
                quote! { (#name, #payload) }
            })
            .collect();
        quote! { #crate_path::Structure::tagged([#(#variant_schemas),*]) }
    };

    Ok(quote! {
        fn schema() -> #crate_path::Structure {
            #schema_expr
        }
    })
}

fn variant_payload_schema(fields: &syn::Fields, self_type: &syn::Ident, crate_path: &TokenStream) -> TokenStream {
    match fields {
        syn::Fields::Unit => {
            quote! { #crate_path::Structure::Unit }
        }
        syn::Fields::Named(named) => {
            let field_schemas: Vec<_> = named
                .named
                .iter()
                .filter_map(|f| {
                    let attrs = parse_field_attrs(&f.attrs);
                    if attrs.skip {
                        return None;
                    }
                    let name = get_field_name(f, &attrs);
                    let schema = type_to_schema(&f.ty, self_type, crate_path);
                    Some(quote! { (#name, #schema) })
                })
                .collect();
            quote! { #crate_path::Structure::record([#(#field_schemas),*]) }
        }
        syn::Fields::Unnamed(unnamed) => {
            if unnamed.unnamed.len() == 1 {
                // Single-element tuple: unwrap to just the inner type
                let f = &unnamed.unnamed[0];
                let attrs = parse_field_attrs(&f.attrs);
                if attrs.skip {
                    quote! { #crate_path::Structure::Unit }
                } else {
                    type_to_schema(&f.ty, self_type, crate_path)
                }
            } else {
                let elem_schemas: Vec<_> = unnamed
                    .unnamed
                    .iter()
                    .filter_map(|f| {
                        let attrs = parse_field_attrs(&f.attrs);
                        if attrs.skip {
                            return None;
                        }
                        Some(type_to_schema(&f.ty, self_type, crate_path))
                    })
                    .collect();
                quote! { #crate_path::Structure::tuple([#(#elem_schemas),*]) }
            }
        }
    }
}

fn get_field_name(field: &syn::Field, attrs: &FieldAttrs) -> String {
    attrs
        .rename
        .clone()
        .unwrap_or_else(|| field.ident.as_ref().unwrap().to_string())
}

/// Convert a Rust type to its Schema representation.
/// Detects self-references and replaces them with SelfRef(0).
fn type_to_schema(ty: &Type, self_type: &syn::Ident, crate_path: &TokenStream) -> TokenStream {
    match ty {
        Type::Path(type_path) => {
            // Check if this is a self-reference
            if is_self_reference(type_path, self_type) {
                return quote! { #crate_path::Structure::SelfRef(0) };
            }

            let last_segment = type_path.path.segments.last();
            if let Some(segment) = last_segment {
                let ident_str = segment.ident.to_string();

                match ident_str.as_str() {
                    // Wrapper types that need special handling for SelfRef detection
                    "Vec" => {
                        if let Some(inner) = extract_single_generic_arg(&segment.arguments) {
                            let inner_schema = type_to_schema(&inner, self_type, crate_path);
                            return quote! { #crate_path::Structure::sequence(#inner_schema) };
                        }
                    }
                    "Option" => {
                        if let Some(inner) = extract_single_generic_arg(&segment.arguments) {
                            let inner_schema = type_to_schema(&inner, self_type, crate_path);
                            return quote! { #crate_path::Structure::option(#inner_schema) };
                        }
                    }
                    "Bond" => {
                        if let Some(inner) = extract_single_generic_arg(&segment.arguments) {
                            let inner_schema = type_to_schema(&inner, self_type, crate_path);
                            return quote! { #crate_path::Structure::bond(#inner_schema) };
                        }
                    }
                    "Box" => {
                        if let Some(inner) = extract_single_generic_arg(&segment.arguments) {
                            // Box<T> has same schema as T
                            return type_to_schema(&inner, self_type, crate_path);
                        }
                    }
                    _ => {}
                }
            }

            // Delegate to the type's Oxide implementation
            quote! { <#type_path as #crate_path::Oxide>::schema() }
        }
        Type::Tuple(tuple) if tuple.elems.is_empty() => {
            quote! { #crate_path::Structure::Unit }
        }
        Type::Tuple(tuple) => {
            let elem_schemas: Vec<_> = tuple
                .elems
                .iter()
                .map(|t| type_to_schema(t, self_type, crate_path))
                .collect();
            quote! { #crate_path::Structure::tuple([#(#elem_schemas),*]) }
        }
        Type::Reference(reference) => {
            // References have same schema as the referenced type
            type_to_schema(&reference.elem, self_type, crate_path)
        }
        _ => {
            // Fallback: delegate to Oxide implementation
            quote! { <#ty as #crate_path::Oxide>::schema() }
        }
    }
}

/// Check if a type path refers to the type being derived (self-reference).
fn is_self_reference(type_path: &syn::TypePath, self_type: &syn::Ident) -> bool {
    // Simple check: last segment matches self_type (ignoring generics)
    if let Some(segment) = type_path.path.segments.last() {
        return segment.ident == *self_type;
    }
    false
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
