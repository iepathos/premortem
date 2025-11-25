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

/// Parse a single validator from the attribute content.
fn parse_validator(ident: &Ident, content: Option<ValidatorContent>) -> Result<ValidatorAttr> {
    let name = ident.to_string();

    match name.as_str() {
        // String validators
        "non_empty" => {
            if content.is_some() {
                return Err(Error::new(ident.span(), "non_empty takes no arguments"));
            }
            Ok(ValidatorAttr::NonEmpty)
        }
        "min_length" => match content {
            Some(ValidatorContent::SingleArg(expr)) => {
                let n = parse_usize_expr(&expr)?;
                Ok(ValidatorAttr::MinLength(n))
            }
            _ => Err(Error::new(
                ident.span(),
                "min_length requires a single integer argument",
            )),
        },
        "max_length" => match content {
            Some(ValidatorContent::SingleArg(expr)) => {
                let n = parse_usize_expr(&expr)?;
                Ok(ValidatorAttr::MaxLength(n))
            }
            _ => Err(Error::new(
                ident.span(),
                "max_length requires a single integer argument",
            )),
        },
        "length" => match content {
            Some(ValidatorContent::SingleArg(expr)) => {
                let (min, max) = parse_range_expr(&expr)?;
                Ok(ValidatorAttr::Length(min, max))
            }
            _ => Err(Error::new(
                ident.span(),
                "length requires a range argument like 3..=100",
            )),
        },
        "pattern" => match content {
            Some(ValidatorContent::SingleArg(expr)) => {
                let s = parse_string_expr(&expr)?;
                Ok(ValidatorAttr::Pattern(s))
            }
            _ => Err(Error::new(
                ident.span(),
                "pattern requires a string argument",
            )),
        },
        "email" => {
            if content.is_some() {
                return Err(Error::new(ident.span(), "email takes no arguments"));
            }
            Ok(ValidatorAttr::Email)
        }
        "url" => {
            if content.is_some() {
                return Err(Error::new(ident.span(), "url takes no arguments"));
            }
            Ok(ValidatorAttr::Url)
        }
        "ip" => {
            if content.is_some() {
                return Err(Error::new(ident.span(), "ip takes no arguments"));
            }
            Ok(ValidatorAttr::Ip)
        }
        "uuid" => {
            if content.is_some() {
                return Err(Error::new(ident.span(), "uuid takes no arguments"));
            }
            Ok(ValidatorAttr::Uuid)
        }

        // Numeric validators
        "range" => match content {
            Some(ValidatorContent::SingleArg(expr)) => {
                let (min, max) = parse_range_strings(&expr)?;
                Ok(ValidatorAttr::Range(min, max))
            }
            _ => Err(Error::new(
                ident.span(),
                "range requires a range argument like 1..=65535",
            )),
        },
        "positive" => {
            if content.is_some() {
                return Err(Error::new(ident.span(), "positive takes no arguments"));
            }
            Ok(ValidatorAttr::Positive)
        }
        "negative" => {
            if content.is_some() {
                return Err(Error::new(ident.span(), "negative takes no arguments"));
            }
            Ok(ValidatorAttr::Negative)
        }
        "non_zero" => {
            if content.is_some() {
                return Err(Error::new(ident.span(), "non_zero takes no arguments"));
            }
            Ok(ValidatorAttr::NonZero)
        }

        // Path validators
        "file_exists" => {
            if content.is_some() {
                return Err(Error::new(ident.span(), "file_exists takes no arguments"));
            }
            Ok(ValidatorAttr::FileExists)
        }
        "dir_exists" => {
            if content.is_some() {
                return Err(Error::new(ident.span(), "dir_exists takes no arguments"));
            }
            Ok(ValidatorAttr::DirExists)
        }
        "parent_exists" => {
            if content.is_some() {
                return Err(Error::new(ident.span(), "parent_exists takes no arguments"));
            }
            Ok(ValidatorAttr::ParentExists)
        }
        "extension" => match content {
            Some(ValidatorContent::SingleArg(expr)) => {
                let s = parse_string_expr(&expr)?;
                Ok(ValidatorAttr::Extension(s))
            }
            _ => Err(Error::new(
                ident.span(),
                "extension requires a string argument",
            )),
        },

        // Structural validators
        "nested" => {
            if content.is_some() {
                return Err(Error::new(ident.span(), "nested takes no arguments"));
            }
            Ok(ValidatorAttr::Nested)
        }
        "each" => match content {
            Some(ValidatorContent::NestedValidator(inner_ident, inner_content)) => {
                let inner = parse_validator(&inner_ident, inner_content.map(|b| *b))?;
                Ok(ValidatorAttr::Each(Box::new(inner)))
            }
            _ => Err(Error::new(
                ident.span(),
                "each requires a validator argument like each(non_empty)",
            )),
        },
        "skip" => {
            if content.is_some() {
                return Err(Error::new(ident.span(), "skip takes no arguments"));
            }
            Ok(ValidatorAttr::Skip)
        }

        // Custom validators
        "custom" => match content {
            Some(ValidatorContent::NameValue(s)) => Ok(ValidatorAttr::Custom(s)),
            _ => Err(Error::new(
                ident.span(),
                "custom requires a function name like custom = \"validate_fn\"",
            )),
        },

        // Suggestions for common typos
        "rang" => Err(Error::new(
            ident.span(),
            "unknown validator 'rang'; did you mean 'range'?",
        )),
        "nonempty" | "not_empty" => Err(Error::new(
            ident.span(),
            format!("unknown validator '{}'; did you mean 'non_empty'?", name),
        )),
        "nonzero" | "not_zero" => Err(Error::new(
            ident.span(),
            format!("unknown validator '{}'; did you mean 'non_zero'?", name),
        )),
        "minlength" | "min_len" => Err(Error::new(
            ident.span(),
            format!("unknown validator '{}'; did you mean 'min_length'?", name),
        )),
        "maxlength" | "max_len" => Err(Error::new(
            ident.span(),
            format!("unknown validator '{}'; did you mean 'max_length'?", name),
        )),

        _ => Err(Error::new(
            ident.span(),
            format!("unknown validator '{}'", name),
        )),
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
