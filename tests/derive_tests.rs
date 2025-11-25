//! Integration tests for the Validate derive macro.

use premortem::{ConfigError, ConfigErrors, ConfigValidation, Validate};
use premortem_derive::Validate as DeriveValidate;
use stillwater::Validation;

// ============================================================================
// Basic Derive Tests
// ============================================================================

#[derive(DeriveValidate)]
struct EmptyConfig {}

#[test]
fn test_empty_struct() {
    let config = EmptyConfig {};
    assert!(config.validate().is_success());
}

#[derive(DeriveValidate)]
struct SimpleConfig {
    host: String,
    port: u16,
}

#[test]
fn test_no_validators() {
    let config = SimpleConfig {
        host: "localhost".to_string(),
        port: 8080,
    };
    assert!(config.validate().is_success());
}

// ============================================================================
// String Validator Tests
// ============================================================================

#[derive(DeriveValidate)]
struct StringValidators {
    #[validate(non_empty)]
    name: String,

    #[validate(min_length(3))]
    code: String,

    #[validate(max_length(10))]
    short: String,

    #[validate(length(2..=5))]
    bounded: String,
}

#[test]
fn test_string_validators_success() {
    let config = StringValidators {
        name: "test".to_string(),
        code: "abc".to_string(),
        short: "hello".to_string(),
        bounded: "hi".to_string(),
    };
    assert!(config.validate().is_success());
}

#[test]
fn test_non_empty_failure() {
    let config = StringValidators {
        name: "".to_string(),
        code: "abc".to_string(),
        short: "hello".to_string(),
        bounded: "hi".to_string(),
    };
    let result = config.validate();
    assert!(result.is_failure());
}

#[test]
fn test_min_length_failure() {
    let config = StringValidators {
        name: "test".to_string(),
        code: "ab".to_string(), // too short
        short: "hello".to_string(),
        bounded: "hi".to_string(),
    };
    let result = config.validate();
    assert!(result.is_failure());
}

#[test]
fn test_max_length_failure() {
    let config = StringValidators {
        name: "test".to_string(),
        code: "abc".to_string(),
        short: "this is too long".to_string(), // too long
        bounded: "hi".to_string(),
    };
    let result = config.validate();
    assert!(result.is_failure());
}

#[test]
fn test_length_failure() {
    let config = StringValidators {
        name: "test".to_string(),
        code: "abc".to_string(),
        short: "hello".to_string(),
        bounded: "x".to_string(), // too short for 2..=5
    };
    let result = config.validate();
    assert!(result.is_failure());
}

// ============================================================================
// Pattern Validator Tests
// ============================================================================

#[derive(DeriveValidate)]
struct PatternConfig {
    #[validate(pattern(r"^\d{3}-\d{4}$"))]
    phone: String,
}

#[test]
fn test_pattern_success() {
    let config = PatternConfig {
        phone: "123-4567".to_string(),
    };
    assert!(config.validate().is_success());
}

#[test]
fn test_pattern_failure() {
    let config = PatternConfig {
        phone: "invalid".to_string(),
    };
    assert!(config.validate().is_failure());
}

// ============================================================================
// Email and URL Validator Tests
// ============================================================================

#[derive(DeriveValidate)]
struct ContactConfig {
    #[validate(email)]
    email: String,

    #[validate(url)]
    website: String,
}

#[test]
fn test_email_url_success() {
    let config = ContactConfig {
        email: "user@example.com".to_string(),
        website: "https://example.com".to_string(),
    };
    assert!(config.validate().is_success());
}

#[test]
fn test_email_failure() {
    let config = ContactConfig {
        email: "not-an-email".to_string(),
        website: "https://example.com".to_string(),
    };
    assert!(config.validate().is_failure());
}

#[test]
fn test_url_failure() {
    let config = ContactConfig {
        email: "user@example.com".to_string(),
        website: "not-a-url".to_string(),
    };
    assert!(config.validate().is_failure());
}

// ============================================================================
// Numeric Validator Tests
// ============================================================================

#[derive(DeriveValidate)]
struct NumericConfig {
    #[validate(range(1..=65535))]
    port: u16,

    #[validate(positive)]
    count: i32,

    #[validate(non_zero)]
    divisor: i32,
}

#[test]
fn test_numeric_validators_success() {
    let config = NumericConfig {
        port: 8080,
        count: 10,
        divisor: 5,
    };
    assert!(config.validate().is_success());
}

#[test]
fn test_range_failure() {
    let config = NumericConfig {
        port: 0, // out of range
        count: 10,
        divisor: 5,
    };
    assert!(config.validate().is_failure());
}

#[test]
fn test_positive_failure() {
    let config = NumericConfig {
        port: 8080,
        count: -1, // not positive
        divisor: 5,
    };
    assert!(config.validate().is_failure());
}

#[test]
fn test_non_zero_failure() {
    let config = NumericConfig {
        port: 8080,
        count: 10,
        divisor: 0, // zero
    };
    assert!(config.validate().is_failure());
}

// ============================================================================
// Nested Validation Tests
// ============================================================================

#[derive(DeriveValidate)]
struct InnerConfig {
    #[validate(non_empty)]
    value: String,
}

#[derive(DeriveValidate)]
struct OuterConfig {
    #[validate(nested)]
    inner: InnerConfig,
}

#[test]
fn test_nested_success() {
    let config = OuterConfig {
        inner: InnerConfig {
            value: "test".to_string(),
        },
    };
    assert!(config.validate().is_success());
}

#[test]
fn test_nested_failure() {
    let config = OuterConfig {
        inner: InnerConfig {
            value: "".to_string(),
        },
    };
    let result = config.validate();
    assert!(result.is_failure());

    // Check that the path is prefixed correctly
    if let Validation::Failure(errors) = result {
        let path = errors.first().path().unwrap();
        assert!(
            path.starts_with("inner."),
            "Expected 'inner.' prefix, got: {}",
            path
        );
    }
}

// ============================================================================
// Optional Nested Validation Tests
// ============================================================================

#[derive(DeriveValidate)]
struct OptionalNestedConfig {
    #[validate(nested)]
    maybe: Option<InnerConfig>,
}

#[test]
fn test_optional_nested_none() {
    let config = OptionalNestedConfig { maybe: None };
    assert!(config.validate().is_success());
}

#[test]
fn test_optional_nested_some_valid() {
    let config = OptionalNestedConfig {
        maybe: Some(InnerConfig {
            value: "test".to_string(),
        }),
    };
    assert!(config.validate().is_success());
}

#[test]
fn test_optional_nested_some_invalid() {
    let config = OptionalNestedConfig {
        maybe: Some(InnerConfig {
            value: "".to_string(),
        }),
    };
    assert!(config.validate().is_failure());
}

// ============================================================================
// Skip Validation Tests
// ============================================================================

#[derive(DeriveValidate)]
struct SkipConfig {
    #[validate(skip)]
    internal: String,

    #[validate(non_empty)]
    required: String,
}

#[test]
fn test_skip_allows_invalid() {
    let config = SkipConfig {
        internal: "".to_string(), // Would fail non_empty, but is skipped
        required: "value".to_string(),
    };
    assert!(config.validate().is_success());
}

// ============================================================================
// Custom Message Tests
// ============================================================================

#[derive(DeriveValidate)]
struct CustomMessageConfig {
    #[validate(non_empty, message = "Host cannot be blank")]
    host: String,
}

#[test]
fn test_custom_message() {
    let config = CustomMessageConfig {
        host: "".to_string(),
    };
    let result = config.validate();
    assert!(result.is_failure());

    if let Validation::Failure(errors) = result {
        let msg = match errors.first() {
            ConfigError::ValidationError { message, .. } => message.as_str(),
            _ => "",
        };
        assert_eq!(msg, "Host cannot be blank");
    }
}

// ============================================================================
// Sensitive Field Tests
// ============================================================================

#[derive(DeriveValidate)]
struct SensitiveConfig {
    #[sensitive]
    #[validate(min_length(8))]
    password: String,
}

#[test]
fn test_sensitive_redacts_value() {
    let config = SensitiveConfig {
        password: "short".to_string(),
    };
    let result = config.validate();
    assert!(result.is_failure());

    if let Validation::Failure(errors) = result {
        let value = match errors.first() {
            ConfigError::ValidationError { value, .. } => value.clone(),
            _ => Some("not redacted".to_string()),
        };
        assert!(value.is_none(), "Expected value to be redacted (None)");
    }
}

// ============================================================================
// Multiple Validators on Single Field Tests
// ============================================================================

#[derive(DeriveValidate)]
struct MultiValidatorConfig {
    #[validate(non_empty)]
    #[validate(min_length(3))]
    #[validate(max_length(20))]
    username: String,
}

#[test]
fn test_multiple_validators_success() {
    let config = MultiValidatorConfig {
        username: "alice".to_string(),
    };
    assert!(config.validate().is_success());
}

#[test]
fn test_multiple_validators_failure_empty() {
    let config = MultiValidatorConfig {
        username: "".to_string(),
    };
    let result = config.validate();
    assert!(result.is_failure());

    // Should fail both non_empty and min_length
    if let Validation::Failure(errors) = result {
        assert!(
            errors.len() >= 2,
            "Expected multiple errors, got {}",
            errors.len()
        );
    }
}

// ============================================================================
// Error Accumulation Tests
// ============================================================================

#[derive(DeriveValidate)]
struct MultiFieldConfig {
    #[validate(non_empty)]
    field1: String,

    #[validate(positive)]
    field2: i32,

    #[validate(non_empty)]
    field3: String,
}

#[test]
fn test_error_accumulation() {
    let config = MultiFieldConfig {
        field1: "".to_string(),
        field2: -1,
        field3: "".to_string(),
    };
    let result = config.validate();
    assert!(result.is_failure());

    // Should have 3 errors - one for each failing field
    if let Validation::Failure(errors) = result {
        assert_eq!(errors.len(), 3, "Expected 3 errors, got {}", errors.len());
    }
}

// ============================================================================
// Struct-Level Custom Validation Tests
// ============================================================================

fn validate_port_range(cfg: &PortRangeConfig) -> ConfigValidation<()> {
    if cfg.start_port < cfg.end_port {
        Validation::Success(())
    } else {
        Validation::Failure(ConfigErrors::single(ConfigError::CrossFieldError {
            paths: vec!["start_port".to_string(), "end_port".to_string()],
            message: "start_port must be less than end_port".to_string(),
        }))
    }
}

#[derive(DeriveValidate)]
#[validate(custom = "validate_port_range")]
struct PortRangeConfig {
    #[validate(range(1..=65535))]
    start_port: u16,

    #[validate(range(1..=65535))]
    end_port: u16,
}

#[test]
fn test_struct_level_custom_success() {
    let config = PortRangeConfig {
        start_port: 8000,
        end_port: 9000,
    };
    assert!(config.validate().is_success());
}

#[test]
fn test_struct_level_custom_failure() {
    let config = PortRangeConfig {
        start_port: 9000,
        end_port: 8000,
    };
    let result = config.validate();
    assert!(result.is_failure());

    if let Validation::Failure(errors) = result {
        let is_cross_field = matches!(errors.first(), ConfigError::CrossFieldError { .. });
        assert!(is_cross_field, "Expected CrossFieldError");
    }
}

// ============================================================================
// Each Validator Tests
// ============================================================================

#[derive(DeriveValidate)]
struct CollectionConfig {
    #[validate(each(non_empty))]
    items: Vec<String>,
}

#[test]
fn test_each_success() {
    let config = CollectionConfig {
        items: vec!["a".to_string(), "b".to_string(), "c".to_string()],
    };
    assert!(config.validate().is_success());
}

#[test]
fn test_each_failure() {
    let config = CollectionConfig {
        items: vec!["a".to_string(), "".to_string(), "c".to_string()],
    };
    let result = config.validate();
    assert!(result.is_failure());
}

#[test]
fn test_each_empty_collection() {
    let config = CollectionConfig { items: vec![] };
    assert!(config.validate().is_success());
}

// ============================================================================
// IP and UUID Tests
// ============================================================================

#[derive(DeriveValidate)]
struct NetworkConfig {
    #[validate(ip)]
    address: String,
}

#[test]
fn test_ip_success() {
    let config = NetworkConfig {
        address: "192.168.1.1".to_string(),
    };
    assert!(config.validate().is_success());
}

#[test]
fn test_ip_failure() {
    let config = NetworkConfig {
        address: "not-an-ip".to_string(),
    };
    assert!(config.validate().is_failure());
}

#[derive(DeriveValidate)]
struct IdConfig {
    #[validate(uuid)]
    id: String,
}

#[test]
fn test_uuid_success() {
    let config = IdConfig {
        id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
    };
    assert!(config.validate().is_success());
}

#[test]
fn test_uuid_failure() {
    let config = IdConfig {
        id: "not-a-uuid".to_string(),
    };
    assert!(config.validate().is_failure());
}

// ============================================================================
// Negative Number Tests
// ============================================================================

#[derive(DeriveValidate)]
struct SignedConfig {
    #[validate(negative)]
    adjustment: i32,
}

#[test]
fn test_negative_success() {
    let config = SignedConfig { adjustment: -5 };
    assert!(config.validate().is_success());
}

#[test]
fn test_negative_failure() {
    let config = SignedConfig { adjustment: 5 };
    assert!(config.validate().is_failure());
}

// ============================================================================
// Complex Nested Structure Tests
// ============================================================================

#[derive(DeriveValidate)]
struct DatabaseConfig {
    #[validate(non_empty)]
    host: String,

    #[validate(range(1..=65535))]
    port: u16,
}

#[derive(DeriveValidate)]
struct CacheConfig {
    #[validate(non_empty)]
    backend: String,

    #[validate(positive)]
    ttl: i32,
}

#[derive(DeriveValidate)]
struct AppConfig {
    #[validate(nested)]
    database: DatabaseConfig,

    #[validate(nested)]
    cache: Option<CacheConfig>,
}

#[test]
fn test_complex_nested_success() {
    let config = AppConfig {
        database: DatabaseConfig {
            host: "localhost".to_string(),
            port: 5432,
        },
        cache: Some(CacheConfig {
            backend: "redis".to_string(),
            ttl: 3600,
        }),
    };
    assert!(config.validate().is_success());
}

#[test]
fn test_complex_nested_failure() {
    let config = AppConfig {
        database: DatabaseConfig {
            host: "".to_string(), // invalid
            port: 5432,
        },
        cache: Some(CacheConfig {
            backend: "redis".to_string(),
            ttl: -1, // invalid
        }),
    };
    let result = config.validate();
    assert!(result.is_failure());

    if let Validation::Failure(errors) = result {
        // Should have 2 errors - one from database, one from cache
        assert_eq!(errors.len(), 2, "Expected 2 errors, got {}", errors.len());
    }
}
