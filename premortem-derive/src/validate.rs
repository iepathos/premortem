//! Validate derive implementation.
//!
//! This module contains the main derive macro implementation for `Validate`.

use proc_macro2::TokenStream;
use syn::{Data, DeriveInput, Error, Fields, Result};

use crate::codegen::generate_validate_impl;
use crate::parse::{
    is_sensitive_attr, is_validate_attr, parse_struct_validate_attr, parse_validate_attr,
    FieldValidation, StructValidation,
};
use crate::validators::validate_validator_combination;

/// Derive the `Validate` trait for a struct.
pub fn derive_validate(input: DeriveInput) -> Result<TokenStream> {
    // Only support structs
    let data = match &input.data {
        Data::Struct(data) => data,
        Data::Enum(_) => {
            return Err(Error::new_spanned(
                &input,
                "Validate can only be derived for structs, not enums",
            ));
        }
        Data::Union(_) => {
            return Err(Error::new_spanned(
                &input,
                "Validate can only be derived for structs, not unions",
            ));
        }
    };

    // Parse struct-level attributes
    let struct_validation = parse_struct_attrs(&input.attrs)?;

    // Parse field attributes
    let fields = match &data.fields {
        Fields::Named(fields) => parse_named_fields(fields)?,
        Fields::Unnamed(_) => {
            return Err(Error::new_spanned(
                &input,
                "Validate does not support tuple structs; use named fields",
            ));
        }
        Fields::Unit => Vec::new(),
    };

    // Generate the impl
    Ok(generate_validate_impl(
        &input.ident,
        &fields,
        &struct_validation,
    ))
}

/// Parse struct-level `#[validate(...)]` attributes.
fn parse_struct_attrs(attrs: &[syn::Attribute]) -> Result<StructValidation> {
    let mut validation = StructValidation::default();

    for attr in attrs {
        if is_validate_attr(attr) {
            let parsed = parse_struct_validate_attr(attr)?;
            if parsed.custom_fn.is_some() {
                validation.custom_fn = parsed.custom_fn;
            }
        }
    }

    Ok(validation)
}

/// Parse named struct fields and their validation attributes.
fn parse_named_fields(
    fields: &syn::FieldsNamed,
) -> Result<Vec<(syn::Ident, syn::Type, FieldValidation)>> {
    let mut result = Vec::new();

    for field in &fields.named {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| Error::new_spanned(field, "field must have a name"))?;

        let mut validation = FieldValidation::default();

        // Check for #[sensitive]
        for attr in &field.attrs {
            if is_sensitive_attr(attr) {
                validation.sensitive = true;
            }
        }

        // Parse #[validate(...)] attributes
        for attr in &field.attrs {
            if is_validate_attr(attr) {
                let (validators, message) = parse_validate_attr(attr)?;

                // Validate validator combinations
                if let Err(e) = validate_validator_combination(&validators) {
                    return Err(Error::new_spanned(attr, e));
                }

                validation.validators.extend(validators);
                if message.0.is_some() {
                    validation.message = message;
                }
            }
        }

        result.push((ident, field.ty.clone(), validation));
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_validate_basic() {
        let input: DeriveInput = syn::parse_quote! {
            struct Config {
                host: String,
                port: u16,
            }
        };

        let result = derive_validate(input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_derive_validate_with_validators() {
        let input: DeriveInput = syn::parse_quote! {
            struct Config {
                #[validate(non_empty)]
                host: String,
                #[validate(range(1..=65535))]
                port: u16,
            }
        };

        let result = derive_validate(input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_derive_validate_enum_fails() {
        let input: DeriveInput = syn::parse_quote! {
            enum Config {
                A,
                B,
            }
        };

        let result = derive_validate(input);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("only be derived for structs"));
    }
}
