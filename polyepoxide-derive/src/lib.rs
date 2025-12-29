use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

mod schema;

/// Attribute macro that derives all required traits for Oxide types.
///
/// This is syntax sugar that expands to:
/// ```ignore
/// #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Oxide)]
/// ```
///
/// Additionally, it adds serde attributes to ensure correct CBOR encoding:
/// - `Option<T>` fields get `#[serde(with = "polyepoxide_core::serde_helpers::option_as_array")]`
/// - `Result<T, E>` fields get `#[serde(with = "polyepoxide_core::serde_helpers::result_lowercase")]`
///
/// # Example
///
/// ```ignore
/// use polyepoxide_derive::oxide;
///
/// #[oxide]
/// struct MyStruct {
///     name: String,
///     count: u32,
/// }
/// ```
#[proc_macro_attribute]
pub fn oxide(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    let modified = add_serde_attributes(input);

    let output = quote! {
        #[derive(
            ::std::fmt::Debug,
            ::std::clone::Clone,
            ::serde::Serialize,
            ::serde::Deserialize,
            ::polyepoxide_core::Oxide
        )]
        #modified
    };

    output.into()
}

/// Add serde attributes to Option and Result fields for correct encoding.
fn add_serde_attributes(mut input: DeriveInput) -> DeriveInput {
    match &mut input.data {
        syn::Data::Struct(data) => {
            add_serde_attrs_to_fields(&mut data.fields);
        }
        syn::Data::Enum(data) => {
            for variant in &mut data.variants {
                add_serde_attrs_to_fields(&mut variant.fields);
            }
        }
        syn::Data::Union(_) => {}
    }
    input
}

fn add_serde_attrs_to_fields(fields: &mut syn::Fields) {
    match fields {
        syn::Fields::Named(named) => {
            for field in &mut named.named {
                add_serde_attr_to_field(field);
            }
        }
        syn::Fields::Unnamed(unnamed) => {
            for field in &mut unnamed.unnamed {
                add_serde_attr_to_field(field);
            }
        }
        syn::Fields::Unit => {}
    }
}

fn add_serde_attr_to_field(field: &mut syn::Field) {
    if let Some(serde_with) = get_serde_with_for_type(&field.ty) {
        // Check if field already has a serde(with) attribute
        let has_serde_with = field.attrs.iter().any(|attr| {
            if !attr.path().is_ident("serde") {
                return false;
            }
            let mut has_with = false;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("with") {
                    has_with = true;
                }
                Ok(())
            });
            has_with
        });

        if !has_serde_with {
            field.attrs.push(syn::parse_quote! {
                #[serde(with = #serde_with)]
            });
        }
    }
}

/// Returns the serde "with" module path for types needing special encoding.
fn get_serde_with_for_type(ty: &syn::Type) -> Option<&'static str> {
    if let syn::Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            match segment.ident.to_string().as_str() {
                "Option" => return Some("::polyepoxide_core::serde_helpers::option_as_array"),
                "Result" => return Some("::polyepoxide_core::serde_helpers::result_lowercase"),
                _ => {}
            }
        }
    }
    None
}

/// Derive macro for the Oxide trait.
///
/// Generates implementations for `schema()`, `visit_bonds()`, and `map_bonds()`.
///
/// # Example
///
/// ```ignore
/// use polyepoxide_core::Oxide;
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Debug, Clone, Serialize, Deserialize, Oxide)]
/// struct MyStruct {
///     name: String,
///     count: u32,
/// }
/// ```
///
/// # Attributes
///
/// - `#[oxide(skip)]` - Skip this field in schema/visit/map (field must impl Default)
/// - `#[oxide(rename = "name")]` - Use custom name in schema
#[proc_macro_derive(Oxide, attributes(oxide))]
pub fn derive_oxide(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match derive_oxide_impl(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn derive_oxide_impl(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let generics = &input.generics;

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Build where clause with Oxide bounds for type parameters
    let where_clause = build_where_clause(generics, where_clause);

    let schema_impl = schema::generate_schema(input)?;
    let visit_bonds_impl = generate_visit_bonds(input)?;
    let map_bonds_impl = generate_map_bonds(input)?;

    Ok(quote! {
        impl #impl_generics ::polyepoxide_core::Oxide for #name #ty_generics #where_clause {
            #schema_impl
            #visit_bonds_impl
            #map_bonds_impl
        }
    })
}

fn build_where_clause(
    generics: &syn::Generics,
    existing: Option<&syn::WhereClause>,
) -> proc_macro2::TokenStream {
    let type_params: Vec<_> = generics.type_params().map(|p| &p.ident).collect();

    if type_params.is_empty() && existing.is_none() {
        return quote! {};
    }

    let oxide_bounds = type_params.iter().map(|p| {
        quote! { #p: ::polyepoxide_core::Oxide }
    });

    let existing_predicates = existing.map(|w| {
        let predicates = &w.predicates;
        quote! { #predicates, }
    }).unwrap_or_default();

    quote! {
        where
            Self: ::serde::Serialize + ::serde::de::DeserializeOwned,
            #existing_predicates
            #(#oxide_bounds),*
    }
}

fn generate_visit_bonds(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    match &input.data {
        syn::Data::Struct(data) => generate_visit_bonds_struct(data),
        syn::Data::Enum(data) => generate_visit_bonds_enum(data),
        syn::Data::Union(_) => Err(syn::Error::new_spanned(input, "Oxide cannot be derived for unions")),
    }
}

fn generate_visit_bonds_struct(data: &syn::DataStruct) -> syn::Result<proc_macro2::TokenStream> {
    let visits = generate_field_visits(&data.fields, quote! { self })?;

    Ok(quote! {
        fn visit_bonds(&self, visitor: &mut dyn ::polyepoxide_core::BondVisitor) {
            #visits
        }
    })
}

fn generate_visit_bonds_enum(data: &syn::DataEnum) -> syn::Result<proc_macro2::TokenStream> {
    let arms: Vec<_> = data.variants.iter().map(|variant| {
        let variant_ident = &variant.ident;

        match &variant.fields {
            syn::Fields::Unit => {
                quote! { Self::#variant_ident => {} }
            }
            syn::Fields::Named(fields) => {
                let field_names: Vec<_> = fields.named.iter()
                    .filter_map(|f| f.ident.as_ref())
                    .collect();
                let visits: Vec<_> = fields.named.iter()
                    .filter_map(|f| {
                        let attrs = parse_field_attrs(&f.attrs);
                        if attrs.skip { return None; }
                        let ident = f.ident.as_ref()?;
                        Some(quote! { #ident.visit_bonds(visitor); })
                    })
                    .collect();
                quote! {
                    Self::#variant_ident { #(#field_names),* } => {
                        #(#visits)*
                    }
                }
            }
            syn::Fields::Unnamed(fields) => {
                let bindings: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| quote::format_ident!("f{}", i))
                    .collect();
                let visits: Vec<_> = fields.unnamed.iter().enumerate()
                    .filter_map(|(i, f)| {
                        let attrs = parse_field_attrs(&f.attrs);
                        if attrs.skip { return None; }
                        let binding = quote::format_ident!("f{}", i);
                        Some(quote! { #binding.visit_bonds(visitor); })
                    })
                    .collect();
                quote! {
                    Self::#variant_ident(#(#bindings),*) => {
                        #(#visits)*
                    }
                }
            }
        }
    }).collect();

    Ok(quote! {
        fn visit_bonds(&self, visitor: &mut dyn ::polyepoxide_core::BondVisitor) {
            match self {
                #(#arms)*
            }
        }
    })
}

fn generate_field_visits(
    fields: &syn::Fields,
    prefix: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    match fields {
        syn::Fields::Named(named) => {
            let visits: Vec<_> = named.named.iter()
                .filter_map(|f| {
                    let attrs = parse_field_attrs(&f.attrs);
                    if attrs.skip { return None; }
                    let ident = f.ident.as_ref()?;
                    Some(quote! { #prefix.#ident.visit_bonds(visitor); })
                })
                .collect();
            Ok(quote! { #(#visits)* })
        }
        syn::Fields::Unnamed(unnamed) => {
            let visits: Vec<_> = unnamed.unnamed.iter().enumerate()
                .filter_map(|(i, f)| {
                    let attrs = parse_field_attrs(&f.attrs);
                    if attrs.skip { return None; }
                    let idx = syn::Index::from(i);
                    Some(quote! { #prefix.#idx.visit_bonds(visitor); })
                })
                .collect();
            Ok(quote! { #(#visits)* })
        }
        syn::Fields::Unit => Ok(quote! {}),
    }
}

fn generate_map_bonds(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    match &input.data {
        syn::Data::Struct(data) => generate_map_bonds_struct(&input.ident, data),
        syn::Data::Enum(data) => generate_map_bonds_enum(data),
        syn::Data::Union(_) => Err(syn::Error::new_spanned(input, "Oxide cannot be derived for unions")),
    }
}

fn generate_map_bonds_struct(
    name: &syn::Ident,
    data: &syn::DataStruct,
) -> syn::Result<proc_macro2::TokenStream> {
    let construction = generate_field_mappings(name, &data.fields)?;

    Ok(quote! {
        fn map_bonds(&self, mapper: &mut impl ::polyepoxide_core::BondMapper) -> Self {
            #construction
        }
    })
}

fn generate_map_bonds_enum(data: &syn::DataEnum) -> syn::Result<proc_macro2::TokenStream> {
    let arms: Vec<_> = data.variants.iter().map(|variant| {
        let variant_ident = &variant.ident;

        match &variant.fields {
            syn::Fields::Unit => {
                quote! { Self::#variant_ident => Self::#variant_ident }
            }
            syn::Fields::Named(fields) => {
                let field_names: Vec<_> = fields.named.iter()
                    .filter_map(|f| f.ident.as_ref())
                    .collect();
                let mappings: Vec<_> = fields.named.iter()
                    .filter_map(|f| {
                        let ident = f.ident.as_ref()?;
                        let attrs = parse_field_attrs(&f.attrs);
                        let mapping = if attrs.skip {
                            quote! { ::std::default::Default::default() }
                        } else {
                            quote! { #ident.map_bonds(mapper) }
                        };
                        Some(quote! { #ident: #mapping })
                    })
                    .collect();
                quote! {
                    Self::#variant_ident { #(#field_names),* } => Self::#variant_ident { #(#mappings),* }
                }
            }
            syn::Fields::Unnamed(fields) => {
                let bindings: Vec<_> = (0..fields.unnamed.len())
                    .map(|i| quote::format_ident!("f{}", i))
                    .collect();
                let mappings: Vec<_> = fields.unnamed.iter().enumerate()
                    .map(|(i, f)| {
                        let binding = quote::format_ident!("f{}", i);
                        let attrs = parse_field_attrs(&f.attrs);
                        if attrs.skip {
                            quote! { ::std::default::Default::default() }
                        } else {
                            quote! { #binding.map_bonds(mapper) }
                        }
                    })
                    .collect();
                quote! {
                    Self::#variant_ident(#(#bindings),*) => Self::#variant_ident(#(#mappings),*)
                }
            }
        }
    }).collect();

    Ok(quote! {
        fn map_bonds(&self, mapper: &mut impl ::polyepoxide_core::BondMapper) -> Self {
            match self {
                #(#arms),*
            }
        }
    })
}

fn generate_field_mappings(
    name: &syn::Ident,
    fields: &syn::Fields,
) -> syn::Result<proc_macro2::TokenStream> {
    match fields {
        syn::Fields::Named(named) => {
            let mappings: Vec<_> = named.named.iter()
                .filter_map(|f| {
                    let ident = f.ident.as_ref()?;
                    let attrs = parse_field_attrs(&f.attrs);
                    let mapping = if attrs.skip {
                        quote! { ::std::default::Default::default() }
                    } else {
                        quote! { self.#ident.map_bonds(mapper) }
                    };
                    Some(quote! { #ident: #mapping })
                })
                .collect();
            Ok(quote! { #name { #(#mappings),* } })
        }
        syn::Fields::Unnamed(unnamed) => {
            let mappings: Vec<_> = unnamed.unnamed.iter().enumerate()
                .map(|(i, f)| {
                    let idx = syn::Index::from(i);
                    let attrs = parse_field_attrs(&f.attrs);
                    if attrs.skip {
                        quote! { ::std::default::Default::default() }
                    } else {
                        quote! { self.#idx.map_bonds(mapper) }
                    }
                })
                .collect();
            Ok(quote! { #name(#(#mappings),*) })
        }
        syn::Fields::Unit => Ok(quote! { #name }),
    }
}

#[derive(Default)]
pub(crate) struct FieldAttrs {
    pub skip: bool,
    pub rename: Option<String>,
}

pub(crate) fn parse_field_attrs(attrs: &[syn::Attribute]) -> FieldAttrs {
    let mut result = FieldAttrs::default();

    for attr in attrs {
        if !attr.path().is_ident("oxide") {
            continue;
        }

        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                result.skip = true;
            } else if meta.path.is_ident("rename") {
                let value: syn::LitStr = meta.value()?.parse()?;
                result.rename = Some(value.value());
            }
            Ok(())
        });
    }

    result
}
