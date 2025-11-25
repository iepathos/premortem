//! Validator attribute handling and type classification.
//!
//! This module provides utilities for working with validators at the type level,
//! determining which validators apply to which types.

use crate::parse::ValidatorAttr;

impl ValidatorAttr {
    /// Check if this validator should skip all other validations.
    pub fn is_skip(&self) -> bool {
        matches!(self, ValidatorAttr::Skip)
    }

    /// Check if this is a nested validator.
    pub fn is_nested(&self) -> bool {
        matches!(self, ValidatorAttr::Nested)
    }

    /// Get the validator name for error messages.
    pub fn name(&self) -> &'static str {
        match self {
            ValidatorAttr::NonEmpty => "non_empty",
            ValidatorAttr::MinLength(_) => "min_length",
            ValidatorAttr::MaxLength(_) => "max_length",
            ValidatorAttr::Length(_, _) => "length",
            ValidatorAttr::Pattern(_) => "pattern",
            ValidatorAttr::Email => "email",
            ValidatorAttr::Url => "url",
            ValidatorAttr::Ip => "ip",
            ValidatorAttr::Uuid => "uuid",
            ValidatorAttr::Range(_, _) => "range",
            ValidatorAttr::Positive => "positive",
            ValidatorAttr::Negative => "negative",
            ValidatorAttr::NonZero => "non_zero",
            ValidatorAttr::FileExists => "file_exists",
            ValidatorAttr::DirExists => "dir_exists",
            ValidatorAttr::ParentExists => "parent_exists",
            ValidatorAttr::Extension(_) => "extension",
            ValidatorAttr::Nested => "nested",
            ValidatorAttr::Each(_) => "each",
            ValidatorAttr::Skip => "skip",
            ValidatorAttr::Custom(_) => "custom",
            ValidatorAttr::When(_, _) => "when",
        }
    }
}

/// Validate that a set of validators are compatible with each other.
pub fn validate_validator_combination(validators: &[ValidatorAttr]) -> Result<(), String> {
    let has_skip = validators.iter().any(|v| v.is_skip());
    let has_nested = validators.iter().any(|v| v.is_nested());

    // Skip cannot be combined with other validators
    if has_skip && validators.len() > 1 {
        return Err("'skip' validator cannot be combined with other validators".to_string());
    }

    // Nested cannot be combined with type-specific validators
    if has_nested {
        for v in validators {
            match v {
                ValidatorAttr::Nested | ValidatorAttr::Custom(_) | ValidatorAttr::When(_, _) => {}
                _ => {
                    return Err(format!(
                        "'nested' validator cannot be combined with '{}' validator",
                        v.name()
                    ));
                }
            }
        }
    }

    Ok(())
}
