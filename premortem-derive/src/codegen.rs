//! Code generation for the Validate derive macro.
//!
//! This module generates the `Validate` trait implementation for structs
//! using stillwater's `Validation::all()` pattern for error accumulation.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::parse::{FieldValidation, MessageOverride, StructValidation, ValidatorAttr};

/// Generate the complete `Validate` impl for a struct.
pub fn generate_validate_impl(
    struct_name: &Ident,
    fields: &[(Ident, Type, FieldValidation)],
    struct_validation: &StructValidation,
) -> TokenStream {
    let field_validations = generate_field_validations(fields);
    let struct_custom = generate_struct_custom(struct_validation);

    // If no field validations, just return struct-level or success
    if field_validations.is_empty() {
        if let Some(custom_call) = struct_custom {
            return quote! {
                impl ::premortem::Validate for #struct_name {
                    fn validate(&self) -> ::premortem::ConfigValidation<()> {
                        #custom_call
                    }
                }
            };
        } else {
            return quote! {
                impl ::premortem::Validate for #struct_name {
                    fn validate(&self) -> ::premortem::ConfigValidation<()> {
                        ::stillwater::Validation::Success(())
                    }
                }
            };
        }
    }

    // Build body based on number of validations
    let body = match field_validations.len() {
        1 => {
            // Single validation - no need for all()
            let v = &field_validations[0];
            if let Some(custom_call) = struct_custom {
                quote! {
                    #v.and_then(|_| #custom_call)
                }
            } else {
                quote! { #v }
            }
        }
        _ => {
            // Multiple validations - use all_vec with explicit type annotation
            if let Some(custom_call) = struct_custom {
                quote! {
                    {
                        let results: ::std::vec::Vec<::premortem::ConfigValidation<()>> = ::std::vec![
                            #(#field_validations),*
                        ];
                        ::stillwater::Validation::all_vec(results)
                            .map(|_| ())
                            .and_then(|_| #custom_call)
                    }
                }
            } else {
                quote! {
                    {
                        let results: ::std::vec::Vec<::premortem::ConfigValidation<()>> = ::std::vec![
                            #(#field_validations),*
                        ];
                        ::stillwater::Validation::all_vec(results).map(|_| ())
                    }
                }
            }
        }
    };

    quote! {
        impl ::premortem::Validate for #struct_name {
            fn validate(&self) -> ::premortem::ConfigValidation<()> {
                #body
            }
        }
    }
}

/// Generate validation expressions for all fields.
fn generate_field_validations(fields: &[(Ident, Type, FieldValidation)]) -> Vec<TokenStream> {
    fields
        .iter()
        .filter_map(|(name, ty, validation)| {
            // Skip fields with #[validate(skip)]
            if validation.validators.iter().any(|v| v.is_skip()) {
                return None;
            }

            // Handle nested validation
            if validation.validators.iter().any(|v| v.is_nested()) {
                return Some(generate_nested_validation(name, ty));
            }

            // Generate validations for this field
            if validation.validators.is_empty() {
                return None;
            }

            Some(generate_validators_for_field(
                name,
                &validation.validators,
                &validation.message,
                validation.sensitive,
            ))
        })
        .collect()
}

/// Generate nested validation for a field.
fn generate_nested_validation(name: &Ident, ty: &Type) -> TokenStream {
    let field_name = name.to_string();

    // Check if type is Option<T>
    if is_option_type(ty) {
        quote! {
            ::premortem::validate_optional_nested(&self.#name, #field_name)
        }
    } else {
        quote! {
            ::premortem::validate_nested(&self.#name, #field_name)
        }
    }
}

/// Generate validators for a field with multiple validators.
fn generate_validators_for_field(
    name: &Ident,
    validators: &[ValidatorAttr],
    message: &MessageOverride,
    sensitive: bool,
) -> TokenStream {
    let field_name = name.to_string();

    // Separate conditional validators from regular ones
    let mut regular_validators = Vec::new();
    let mut conditional_validators = Vec::new();

    for v in validators {
        match v {
            ValidatorAttr::When(condition, inner) => {
                conditional_validators.push((condition.clone(), inner.as_ref().clone()));
            }
            _ => {
                regular_validators.push(v.clone());
            }
        }
    }

    let mut validation_exprs = Vec::new();

    // Generate regular validators
    for v in &regular_validators {
        validation_exprs.push(generate_validator_expr(v, &field_name, message, sensitive));
    }

    // Generate conditional validators
    for (condition, inner) in conditional_validators {
        let inner_expr = generate_validator_expr(&inner, &field_name, message, sensitive);
        let cond_tokens: TokenStream = condition.parse().unwrap_or_else(|_| quote! { false });

        validation_exprs.push(quote! {
            if #cond_tokens {
                #inner_expr
            } else {
                ::stillwater::Validation::Success(())
            }
        });
    }

    // Combine all validation expressions
    if validation_exprs.is_empty() {
        quote! { ::stillwater::Validation::Success(()) }
    } else if validation_exprs.len() == 1 {
        validation_exprs.pop().unwrap()
    } else {
        // Use all_vec with explicit type annotation
        quote! {
            {
                let field_results: ::std::vec::Vec<::premortem::ConfigValidation<()>> = ::std::vec![
                    #(#validation_exprs),*
                ];
                ::stillwater::Validation::all_vec(field_results).map(|_| ())
            }
        }
    }
}

/// Generate the expression for a single validator.
fn generate_validator_expr(
    validator: &ValidatorAttr,
    field_name: &str,
    message: &MessageOverride,
    sensitive: bool,
) -> TokenStream {
    let custom_msg = message.0.as_ref();

    match validator {
        ValidatorAttr::NonEmpty => {
            generate_simple_validator(field_name, quote! { NonEmpty }, custom_msg, sensitive)
        }
        ValidatorAttr::MinLength(n) => {
            generate_simple_validator(field_name, quote! { MinLength(#n) }, custom_msg, sensitive)
        }
        ValidatorAttr::MaxLength(n) => {
            generate_simple_validator(field_name, quote! { MaxLength(#n) }, custom_msg, sensitive)
        }
        ValidatorAttr::Length(min, max) => generate_simple_validator(
            field_name,
            quote! { Length(#min..=#max) },
            custom_msg,
            sensitive,
        ),
        ValidatorAttr::Pattern(pat) => generate_simple_validator(
            field_name,
            quote! { Pattern::new(#pat) },
            custom_msg,
            sensitive,
        ),
        ValidatorAttr::Email => {
            generate_simple_validator(field_name, quote! { Email }, custom_msg, sensitive)
        }
        ValidatorAttr::Url => {
            generate_simple_validator(field_name, quote! { Url }, custom_msg, sensitive)
        }
        ValidatorAttr::Ip => generate_ip_validator(field_name, custom_msg, sensitive),
        ValidatorAttr::Uuid => generate_uuid_validator(field_name, custom_msg, sensitive),

        ValidatorAttr::Range(min, max) => {
            let min_tokens: TokenStream = min.parse().unwrap_or_else(|_| quote! { 0 });
            let max_tokens: TokenStream = max.parse().unwrap_or_else(|_| quote! { 0 });
            generate_simple_validator(
                field_name,
                quote! { Range(#min_tokens..=#max_tokens) },
                custom_msg,
                sensitive,
            )
        }
        ValidatorAttr::Positive => {
            generate_simple_validator(field_name, quote! { Positive }, custom_msg, sensitive)
        }
        ValidatorAttr::Negative => {
            generate_simple_validator(field_name, quote! { Negative }, custom_msg, sensitive)
        }
        ValidatorAttr::NonZero => {
            generate_simple_validator(field_name, quote! { NonZero }, custom_msg, sensitive)
        }

        ValidatorAttr::FileExists => {
            generate_simple_validator(field_name, quote! { FileExists }, custom_msg, sensitive)
        }
        ValidatorAttr::DirExists => {
            generate_simple_validator(field_name, quote! { DirExists }, custom_msg, sensitive)
        }
        ValidatorAttr::ParentExists => {
            generate_simple_validator(field_name, quote! { ParentExists }, custom_msg, sensitive)
        }
        ValidatorAttr::Extension(ext) => generate_simple_validator(
            field_name,
            quote! { Extension::new(#ext) },
            custom_msg,
            sensitive,
        ),

        ValidatorAttr::Each(inner) => {
            let inner_validator = match inner.as_ref() {
                ValidatorAttr::NonEmpty => quote! { NonEmpty },
                ValidatorAttr::MinLength(n) => quote! { MinLength(#n) },
                ValidatorAttr::MaxLength(n) => quote! { MaxLength(#n) },
                ValidatorAttr::Positive => quote! { Positive },
                ValidatorAttr::NonZero => quote! { NonZero },
                _ => quote! { NonEmpty }, // fallback
            };
            generate_simple_validator(
                field_name,
                quote! { Each(#inner_validator) },
                custom_msg,
                sensitive,
            )
        }

        ValidatorAttr::Custom(fn_name) => {
            let fn_ident: TokenStream = fn_name.parse().unwrap_or_else(|_| quote! { validate });
            quote! {
                #fn_ident(&self)
            }
        }

        // These are handled elsewhere
        ValidatorAttr::Nested | ValidatorAttr::Skip | ValidatorAttr::When(_, _) => {
            quote! { ::stillwater::Validation::Success(()) }
        }
    }
}

/// Generate a simple validator call using validate_field.
fn generate_simple_validator(
    field_name: &str,
    validator: TokenStream,
    custom_msg: Option<&String>,
    sensitive: bool,
) -> TokenStream {
    let field_ident: TokenStream = format!("self.{}", field_name)
        .parse()
        .unwrap_or_else(|_| quote! { self.field });

    if let Some(msg) = custom_msg {
        // With custom message
        if sensitive {
            quote! {
                {
                    use ::premortem::validators::*;
                    use ::premortem::Validator;
                    let result = (#validator).validate(&#field_ident, #field_name);
                    result.map_err(|errors| {
                        ::premortem::ConfigErrors::from_nonempty(
                            errors.0.map(|e| {
                                match e {
                                    ::premortem::ConfigError::ValidationError { path, source_location, .. } => {
                                        ::premortem::ConfigError::ValidationError {
                                            path,
                                            source_location,
                                            value: None, // redacted
                                            message: #msg.to_string(),
                                        }
                                    }
                                    other => other,
                                }
                            })
                        )
                    })
                }
            }
        } else {
            quote! {
                {
                    use ::premortem::validators::*;
                    use ::premortem::Validator;
                    let result = (#validator).validate(&#field_ident, #field_name);
                    result.map_err(|errors| {
                        ::premortem::ConfigErrors::from_nonempty(
                            errors.0.map(|e| {
                                match e {
                                    ::premortem::ConfigError::ValidationError { path, source_location, value, .. } => {
                                        ::premortem::ConfigError::ValidationError {
                                            path,
                                            source_location,
                                            value,
                                            message: #msg.to_string(),
                                        }
                                    }
                                    other => other,
                                }
                            })
                        )
                    })
                }
            }
        }
    } else if sensitive {
        // Sensitive without custom message - redact value
        quote! {
            {
                use ::premortem::validators::*;
                use ::premortem::Validator;
                let result = (#validator).validate(&#field_ident, #field_name);
                result.map_err(|errors| {
                    ::premortem::ConfigErrors::from_nonempty(
                        errors.0.map(|e| {
                            match e {
                                ::premortem::ConfigError::ValidationError { path, source_location, message, .. } => {
                                    ::premortem::ConfigError::ValidationError {
                                        path,
                                        source_location,
                                        value: None, // redacted
                                        message,
                                    }
                                }
                                other => other,
                            }
                        })
                    )
                })
            }
        }
    } else {
        // Standard case
        quote! {
            {
                use ::premortem::validators::*;
                use ::premortem::Validator;
                (#validator).validate(&#field_ident, #field_name)
            }
        }
    }
}

/// Generate IP address validator (pattern-based).
fn generate_ip_validator(
    field_name: &str,
    custom_msg: Option<&String>,
    sensitive: bool,
) -> TokenStream {
    // Simple IP pattern - for strict validation, user should use custom validator
    let pattern = r"^(\d{1,3}\.){3}\d{1,3}$|^([0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}$";
    let msg = custom_msg
        .map(|s| s.to_string())
        .unwrap_or_else(|| "value is not a valid IP address".to_string());

    generate_simple_validator(
        field_name,
        quote! { Pattern::new(#pattern) },
        Some(&msg),
        sensitive,
    )
}

/// Generate UUID validator (pattern-based).
fn generate_uuid_validator(
    field_name: &str,
    custom_msg: Option<&String>,
    sensitive: bool,
) -> TokenStream {
    let pattern = r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$";
    let msg = custom_msg
        .map(|s| s.to_string())
        .unwrap_or_else(|| "value is not a valid UUID".to_string());

    generate_simple_validator(
        field_name,
        quote! { Pattern::new(#pattern) },
        Some(&msg),
        sensitive,
    )
}

/// Generate struct-level custom validation call.
fn generate_struct_custom(validation: &StructValidation) -> Option<TokenStream> {
    validation.custom_fn.as_ref().map(|fn_name| {
        let fn_ident: TokenStream = fn_name.parse().unwrap_or_else(|_| quote! { validate });
        quote! { #fn_ident(self) }
    })
}

/// Check if a type is Option<T>.
fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}
