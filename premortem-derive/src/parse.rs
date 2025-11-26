//! Attribute parsing for the Validate derive macro.
//!
//! This module handles parsing of `#[validate(...)]` and `#[sensitive]` attributes
//! on struct fields and structs themselves.

use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    Attribute, Error, Expr, ExprLit, ExprRange, Ident, Lit, LitStr, Result, Token,
};

/// A parsed field attribute representing a single validation rule.
#[derive(Debug, Clone)]
pub enum ValidatorAttr {
    // String validators
    NonEmpty,
    MinLength(usize),
    MaxLength(usize),
    Length(usize, usize), // (min, max)
    Pattern(String),
    Email,
    Url,
    Ip,
    Uuid,

    // Numeric validators
    Range(String, String), // Store as strings, let codegen handle the types
    Positive,
    Negative,
    NonZero,

    // Path validators
    FileExists,
    DirExists,
    ParentExists,
    Extension(String),

    // Structural validators
    Nested,
    Each(Box<ValidatorAttr>),

    // Control flow
    Skip,
    Custom(String),                   // function name
    When(String, Box<ValidatorAttr>), // condition, validator
}

/// Custom error message override.
#[derive(Debug, Clone, Default)]
pub struct MessageOverride(pub Option<String>);

/// A complete field validation configuration.
#[derive(Debug, Clone, Default)]
pub struct FieldValidation {
    pub validators: Vec<ValidatorAttr>,
    pub message: MessageOverride,
    pub sensitive: bool,
}

/// Struct-level validation configuration.
#[derive(Debug, Clone, Default)]
pub struct StructValidation {
    pub custom_fn: Option<String>,
}

/// Helper for validators that take no arguments.
fn parse_no_arg_validator(
    ident: &Ident,
    content: &Option<ValidatorContent>,
    validator: ValidatorAttr,
) -> Result<ValidatorAttr> {
    if content.is_some() {
        return Err(Error::new(
            ident.span(),
            format!("{} takes no arguments", ident),
        ));
    }
    Ok(validator)
}

/// Helper for validators that take a single usize argument.
fn parse_usize_validator(
    ident: &Ident,
    content: &Option<ValidatorContent>,
    constructor: impl FnOnce(usize) -> ValidatorAttr,
    error_msg: &str,
) -> Result<ValidatorAttr> {
    match content {
        Some(ValidatorContent::SingleArg(expr)) => {
            let n = parse_usize_expr(expr)?;
            Ok(constructor(n))
        }
        _ => Err(Error::new(ident.span(), error_msg)),
    }
}

/// Helper for validators that take a single string argument.
fn parse_string_validator(
    ident: &Ident,
    content: &Option<ValidatorContent>,
    constructor: impl FnOnce(String) -> ValidatorAttr,
    error_msg: &str,
) -> Result<ValidatorAttr> {
    match content {
        Some(ValidatorContent::SingleArg(expr)) => {
            let s = parse_string_expr(expr)?;
            Ok(constructor(s))
        }
        _ => Err(Error::new(ident.span(), error_msg)),
    }
}

/// Helper for validators that take a usize range argument.
fn parse_usize_range_validator(
    ident: &Ident,
    content: &Option<ValidatorContent>,
    constructor: impl FnOnce(usize, usize) -> ValidatorAttr,
    error_msg: &str,
) -> Result<ValidatorAttr> {
    match content {
        Some(ValidatorContent::SingleArg(expr)) => {
            let (min, max) = parse_range_expr(expr)?;
            Ok(constructor(min, max))
        }
        _ => Err(Error::new(ident.span(), error_msg)),
    }
}

/// Helper for validators that take a string range argument (for numeric ranges).
fn parse_string_range_validator(
    ident: &Ident,
    content: &Option<ValidatorContent>,
    constructor: impl FnOnce(String, String) -> ValidatorAttr,
    error_msg: &str,
) -> Result<ValidatorAttr> {
    match content {
        Some(ValidatorContent::SingleArg(expr)) => {
            let (min, max) = parse_range_strings(expr)?;
            Ok(constructor(min, max))
        }
        _ => Err(Error::new(ident.span(), error_msg)),
    }
}

/// Check for common typos and return a helpful error message.
fn check_typo_suggestion(name: &str, ident: &Ident) -> Option<Error> {
    let suggestion = match name {
        "rang" => Some("range"),
        "nonempty" | "not_empty" => Some("non_empty"),
        "nonzero" | "not_zero" => Some("non_zero"),
        "minlength" | "min_len" => Some("min_length"),
        "maxlength" | "max_len" => Some("max_length"),
        _ => None,
    };
    suggestion.map(|s| {
        Error::new(
            ident.span(),
            format!("unknown validator '{}'; did you mean '{}'?", name, s),
        )
    })
}

/// Parse a nested validator (for `each(validator)`).
fn parse_nested_validator(
    ident: &Ident,
    content: Option<ValidatorContent>,
) -> Result<ValidatorAttr> {
    match content {
        Some(ValidatorContent::NestedValidator(inner_ident, inner_content)) => {
            let inner = parse_validator(&inner_ident, inner_content.map(|b| *b))?;
            Ok(ValidatorAttr::Each(Box::new(inner)))
        }
        _ => Err(Error::new(
            ident.span(),
            "each requires a validator argument like each(non_empty)",
        )),
    }
}

/// Parse a custom validator with name = "value" syntax.
fn parse_custom_validator(
    ident: &Ident,
    content: &Option<ValidatorContent>,
) -> Result<ValidatorAttr> {
    match content {
        Some(ValidatorContent::NameValue(s)) => Ok(ValidatorAttr::Custom(s.clone())),
        _ => Err(Error::new(
            ident.span(),
            "custom requires a function name like custom = \"validate_fn\"",
        )),
    }
}

/// Parse a single validator from the attribute content.
fn parse_validator(ident: &Ident, content: Option<ValidatorContent>) -> Result<ValidatorAttr> {
    let name = ident.to_string();

    // Handle "each" separately since it needs ownership of content
    if name == "each" {
        return parse_nested_validator(ident, content);
    }

    match name.as_str() {
        // No-argument validators
        "non_empty" => parse_no_arg_validator(ident, &content, ValidatorAttr::NonEmpty),
        "email" => parse_no_arg_validator(ident, &content, ValidatorAttr::Email),
        "url" => parse_no_arg_validator(ident, &content, ValidatorAttr::Url),
        "ip" => parse_no_arg_validator(ident, &content, ValidatorAttr::Ip),
        "uuid" => parse_no_arg_validator(ident, &content, ValidatorAttr::Uuid),
        "positive" => parse_no_arg_validator(ident, &content, ValidatorAttr::Positive),
        "negative" => parse_no_arg_validator(ident, &content, ValidatorAttr::Negative),
        "non_zero" => parse_no_arg_validator(ident, &content, ValidatorAttr::NonZero),
        "file_exists" => parse_no_arg_validator(ident, &content, ValidatorAttr::FileExists),
        "dir_exists" => parse_no_arg_validator(ident, &content, ValidatorAttr::DirExists),
        "parent_exists" => parse_no_arg_validator(ident, &content, ValidatorAttr::ParentExists),
        "nested" => parse_no_arg_validator(ident, &content, ValidatorAttr::Nested),
        "skip" => parse_no_arg_validator(ident, &content, ValidatorAttr::Skip),

        // Usize argument validators
        "min_length" => parse_usize_validator(
            ident,
            &content,
            ValidatorAttr::MinLength,
            "min_length requires a single integer argument",
        ),
        "max_length" => parse_usize_validator(
            ident,
            &content,
            ValidatorAttr::MaxLength,
            "max_length requires a single integer argument",
        ),

        // String argument validators
        "pattern" => parse_string_validator(
            ident,
            &content,
            ValidatorAttr::Pattern,
            "pattern requires a string argument",
        ),
        "extension" => parse_string_validator(
            ident,
            &content,
            ValidatorAttr::Extension,
            "extension requires a string argument",
        ),

        // Range validators
        "length" => parse_usize_range_validator(
            ident,
            &content,
            ValidatorAttr::Length,
            "length requires a range argument like 3..=100",
        ),
        "range" => parse_string_range_validator(
            ident,
            &content,
            ValidatorAttr::Range,
            "range requires a range argument like 1..=65535",
        ),

        // Custom validator
        "custom" => parse_custom_validator(ident, &content),

        // Check for typos or return unknown error
        _ => {
            if let Some(err) = check_typo_suggestion(&name, ident) {
                Err(err)
            } else {
                Err(Error::new(
                    ident.span(),
                    format!("unknown validator '{}'", name),
                ))
            }
        }
    }
}

/// Validator content that can be parsed from parentheses.
#[derive(Debug)]
enum ValidatorContent {
    SingleArg(Expr),
    NameValue(String),
    NestedValidator(Ident, Option<Box<ValidatorContent>>),
}

/// Parse a `#[validate(...)]` attribute into validators and optional message.
pub fn parse_validate_attr(attr: &Attribute) -> Result<(Vec<ValidatorAttr>, MessageOverride)> {
    let mut validators = Vec::new();
    let mut message = MessageOverride::default();
    let mut when_condition: Option<String> = None;

    let nested = attr.parse_args_with(Punctuated::<ValidateItem, Token![,]>::parse_terminated)?;

    for item in nested {
        match item {
            ValidateItem::Validator { name, content } => {
                let validator = parse_validator(&name, content)?;
                if let Some(ref cond) = when_condition {
                    validators.push(ValidatorAttr::When(cond.clone(), Box::new(validator)));
                } else {
                    validators.push(validator);
                }
            }
            ValidateItem::Message(msg) => {
                message = MessageOverride(Some(msg));
            }
            ValidateItem::When(cond) => {
                when_condition = Some(cond);
            }
        }
    }

    Ok((validators, message))
}

/// Parse struct-level `#[validate(custom = "fn_name")]` attribute.
pub fn parse_struct_validate_attr(attr: &Attribute) -> Result<StructValidation> {
    let mut validation = StructValidation::default();

    let nested = attr.parse_args_with(Punctuated::<ValidateItem, Token![,]>::parse_terminated)?;

    for item in nested {
        match item {
            ValidateItem::Validator { name, content } => {
                if name == "custom" {
                    if let Some(ValidatorContent::NameValue(fn_name)) = content {
                        validation.custom_fn = Some(fn_name);
                    } else {
                        return Err(Error::new(
                            name.span(),
                            "struct-level custom requires = \"fn_name\" syntax",
                        ));
                    }
                } else {
                    return Err(Error::new(
                        name.span(),
                        format!(
                            "unknown struct-level validator '{}'; only 'custom' is supported",
                            name
                        ),
                    ));
                }
            }
            ValidateItem::Message(_) => {
                return Err(Error::new(
                    attr.meta.span(),
                    "message is not supported at struct level",
                ));
            }
            ValidateItem::When(_) => {
                return Err(Error::new(
                    attr.meta.span(),
                    "when is not supported at struct level",
                ));
            }
        }
    }

    Ok(validation)
}

/// A single item within `#[validate(...)]`.
enum ValidateItem {
    Validator {
        name: Ident,
        content: Option<ValidatorContent>,
    },
    Message(String),
    When(String),
}

impl Parse for ValidateItem {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;

        // Check for `= "value"` syntax (message, custom, when)
        if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            let lit: LitStr = input.parse()?;
            let value = lit.value();

            return match name.to_string().as_str() {
                "message" => Ok(ValidateItem::Message(value)),
                "when" => Ok(ValidateItem::When(value)),
                "custom" => Ok(ValidateItem::Validator {
                    name,
                    content: Some(ValidatorContent::NameValue(value)),
                }),
                _ => Err(Error::new(
                    name.span(),
                    format!("unexpected = syntax for '{}'", name),
                )),
            };
        }

        // Check for `(...)` syntax
        if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);

            // Try to parse as nested validator first (for `each(non_empty)`)
            if content.peek(Ident) && !content.peek2(Token![..]) && !content.peek2(Token![=]) {
                let fork = content.fork();
                if let Ok(inner_name) = fork.parse::<Ident>() {
                    // Check if this looks like a nested validator
                    if is_validator_name(&inner_name.to_string()) {
                        content.parse::<Ident>()?; // consume the ident
                        let inner_content = if content.peek(syn::token::Paren) {
                            let inner_content_buf;
                            syn::parenthesized!(inner_content_buf in content);
                            let expr: Expr = inner_content_buf.parse()?;
                            Some(Box::new(ValidatorContent::SingleArg(expr)))
                        } else {
                            None
                        };
                        return Ok(ValidateItem::Validator {
                            name,
                            content: Some(ValidatorContent::NestedValidator(
                                inner_name,
                                inner_content,
                            )),
                        });
                    }
                }
            }

            // Otherwise parse as expression
            let expr: Expr = content.parse()?;
            return Ok(ValidateItem::Validator {
                name,
                content: Some(ValidatorContent::SingleArg(expr)),
            });
        }

        // No content
        Ok(ValidateItem::Validator {
            name,
            content: None,
        })
    }
}

/// Check if a name looks like a validator name.
fn is_validator_name(name: &str) -> bool {
    matches!(
        name,
        "non_empty"
            | "min_length"
            | "max_length"
            | "length"
            | "pattern"
            | "email"
            | "url"
            | "ip"
            | "uuid"
            | "range"
            | "positive"
            | "negative"
            | "non_zero"
            | "file_exists"
            | "dir_exists"
            | "parent_exists"
            | "extension"
            | "nested"
            | "each"
            | "skip"
            | "custom"
    )
}

/// Parse a usize from an expression.
fn parse_usize_expr(expr: &Expr) -> Result<usize> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(lit), ..
        }) => lit.base10_parse(),
        _ => Err(Error::new(expr.span(), "expected integer literal")),
    }
}

/// Parse a string from an expression.
fn parse_string_expr(expr: &Expr) -> Result<String> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(lit), ..
        }) => Ok(lit.value()),
        _ => Err(Error::new(expr.span(), "expected string literal")),
    }
}

/// Parse a range expression into (min, max) usize tuple.
fn parse_range_expr(expr: &Expr) -> Result<(usize, usize)> {
    match expr {
        Expr::Range(ExprRange {
            start, end, limits, ..
        }) => {
            let start_val = start
                .as_ref()
                .map(|e| parse_usize_expr(e))
                .transpose()?
                .unwrap_or(0);

            let end_val = end
                .as_ref()
                .map(|e| parse_usize_expr(e))
                .transpose()?
                .ok_or_else(|| Error::new(expr.span(), "range must have an end value"))?;

            // Check if inclusive
            match limits {
                syn::RangeLimits::HalfOpen(_) => Ok((start_val, end_val.saturating_sub(1))),
                syn::RangeLimits::Closed(_) => Ok((start_val, end_val)),
            }
        }
        _ => Err(Error::new(
            expr.span(),
            "expected range expression like 1..=100",
        )),
    }
}

/// Parse a range expression into (min, max) strings for numeric ranges.
fn parse_range_strings(expr: &Expr) -> Result<(String, String)> {
    match expr {
        Expr::Range(ExprRange {
            start, end, limits, ..
        }) => {
            let start_str = start
                .as_ref()
                .map(|e| expr_to_string(e))
                .transpose()?
                .unwrap_or_else(|| "0".to_string());

            let end_str = end
                .as_ref()
                .map(|e| expr_to_string(e))
                .transpose()?
                .ok_or_else(|| Error::new(expr.span(), "range must have an end value"))?;

            // For half-open ranges, we'd need to adjust, but for simplicity
            // we require inclusive ranges for numeric validators
            match limits {
                syn::RangeLimits::HalfOpen(_) => Err(Error::new(
                    expr.span(),
                    "range validator requires inclusive range (..=), not half-open (..)",
                )),
                syn::RangeLimits::Closed(_) => Ok((start_str, end_str)),
            }
        }
        _ => Err(Error::new(
            expr.span(),
            "expected range expression like 1..=65535",
        )),
    }
}

/// Convert an expression to its string representation.
fn expr_to_string(expr: &Expr) -> Result<String> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(lit), ..
        }) => Ok(lit.to_string()),
        Expr::Lit(ExprLit {
            lit: Lit::Float(lit),
            ..
        }) => Ok(lit.to_string()),
        Expr::Unary(unary) => {
            // Handle negative numbers like -1
            let inner = expr_to_string(&unary.expr)?;
            match unary.op {
                syn::UnOp::Neg(_) => Ok(format!("-{}", inner)),
                _ => Err(Error::new(expr.span(), "unsupported unary operator")),
            }
        }
        _ => Err(Error::new(expr.span(), "expected numeric literal in range")),
    }
}

/// Check if an attribute is a `#[validate(...)]` attribute.
pub fn is_validate_attr(attr: &Attribute) -> bool {
    attr.path().is_ident("validate")
}

/// Check if an attribute is a `#[sensitive]` attribute.
pub fn is_sensitive_attr(attr: &Attribute) -> bool {
    attr.path().is_ident("sensitive")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_validator_name() {
        assert!(is_validator_name("non_empty"));
        assert!(is_validator_name("range"));
        assert!(is_validator_name("nested"));
        assert!(!is_validator_name("foobar"));
    }
}
