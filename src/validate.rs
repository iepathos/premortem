//! Validation trait for configuration types.
//!
//! This module provides the `Validate` trait that configuration types implement
//! to perform validation after deserialization.

use crate::error::ConfigValidation;

/// Trait for validating configuration values.
///
/// Types implementing this trait can perform custom validation logic
/// after deserialization. The validation uses stillwater's `Validation`
/// type to accumulate all errors.
///
/// # Example
///
/// ```ignore
/// use premortem::{Validate, ConfigValidation, ConfigError, ConfigErrors};
/// use stillwater::Validation;
///
/// struct DatabaseConfig {
///     host: String,
///     port: u16,
///     pool_size: u32,
/// }
///
/// impl Validate for DatabaseConfig {
///     fn validate(&self) -> ConfigValidation<()> {
///         Validation::all((
///             validate_non_empty(&self.host, "database.host"),
///             validate_port(self.port, "database.port"),
///             validate_range(self.pool_size, 1, 100, "database.pool_size"),
///         ))
///         .map(|_| ())
///     }
/// }
/// ```
pub trait Validate {
    /// Validate this configuration value.
    ///
    /// Returns `ConfigValidation<()>` - either `Success(())` if validation
    /// passes, or `Failure(ConfigErrors)` with all accumulated validation errors.
    fn validate(&self) -> ConfigValidation<()>;
}

/// Blanket implementation for types that don't need validation.
///
/// Any type that doesn't implement `Validate` will automatically pass validation.
/// This is implemented for the unit type as a no-op validator.
impl Validate for () {
    fn validate(&self) -> ConfigValidation<()> {
        stillwater::Validation::Success(())
    }
}

/// Implementation for Option<T> where T: Validate.
///
/// None values pass validation; Some values delegate to the inner type.
impl<T: Validate> Validate for Option<T> {
    fn validate(&self) -> ConfigValidation<()> {
        match self {
            Some(inner) => inner.validate(),
            None => stillwater::Validation::Success(()),
        }
    }
}

/// Implementation for Vec<T> where T: Validate.
///
/// Validates all elements and accumulates errors.
impl<T: Validate> Validate for Vec<T> {
    fn validate(&self) -> ConfigValidation<()> {
        use stillwater::Validation;

        if self.is_empty() {
            return Validation::Success(());
        }

        let validations: Vec<ConfigValidation<()>> =
            self.iter().map(|item| item.validate()).collect();
        Validation::all_vec(validations).map(|_| ())
    }
}

// Primitive types don't need validation
macro_rules! impl_validate_noop {
    ($($t:ty),*) => {
        $(
            impl Validate for $t {
                fn validate(&self) -> ConfigValidation<()> {
                    stillwater::Validation::Success(())
                }
            }
        )*
    };
}

impl_validate_noop!(
    bool, i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64, char, String
);

impl Validate for &str {
    fn validate(&self) -> ConfigValidation<()> {
        stillwater::Validation::Success(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stillwater::Validation;

    #[test]
    fn test_unit_validate() {
        let result = ().validate();
        assert!(result.is_success());
    }

    #[test]
    fn test_option_validate_none() {
        let opt: Option<String> = None;
        let result = opt.validate();
        assert!(result.is_success());
    }

    #[test]
    fn test_option_validate_some() {
        let opt = Some("value".to_string());
        let result = opt.validate();
        assert!(result.is_success());
    }

    #[test]
    fn test_vec_validate_empty() {
        let v: Vec<String> = vec![];
        let result = v.validate();
        assert!(result.is_success());
    }

    #[test]
    fn test_vec_validate_with_items() {
        let v = vec!["a".to_string(), "b".to_string()];
        let result = v.validate();
        assert!(result.is_success());
    }

    #[test]
    fn test_primitive_validate() {
        assert!(42i32.validate().is_success());
        assert!(3.14f64.validate().is_success());
        assert!(true.validate().is_success());
        assert!("hello".to_string().validate().is_success());
    }

    // Test a custom Validate implementation
    struct CustomConfig {
        value: i32,
    }

    impl Validate for CustomConfig {
        fn validate(&self) -> ConfigValidation<()> {
            if self.value > 0 {
                Validation::Success(())
            } else {
                use crate::error::{ConfigError, ConfigErrors};
                Validation::Failure(ConfigErrors::single(ConfigError::ValidationError {
                    path: "value".to_string(),
                    source_location: None,
                    value: Some(self.value.to_string()),
                    message: "must be positive".to_string(),
                }))
            }
        }
    }

    #[test]
    fn test_custom_validate_success() {
        let config = CustomConfig { value: 42 };
        assert!(config.validate().is_success());
    }

    #[test]
    fn test_custom_validate_failure() {
        let config = CustomConfig { value: -1 };
        let result = config.validate();
        assert!(result.is_failure());
    }

    #[test]
    fn test_vec_custom_validate() {
        let configs = vec![
            CustomConfig { value: 1 },
            CustomConfig { value: -1 },
            CustomConfig { value: -2 },
        ];

        let result = configs.validate();
        assert!(result.is_failure());

        // Should have accumulated 2 errors
        if let Validation::Failure(errors) = result {
            assert_eq!(errors.len(), 2);
        } else {
            panic!("Expected failure");
        }
    }
}
