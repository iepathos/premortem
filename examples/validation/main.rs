//! Comprehensive validation example.
//!
//! This example demonstrates all built-in validators and custom validation patterns:
//! - String validators (non_empty, email, url, pattern, min_length, max_length)
//! - Numeric validators (range, positive, negative, non_zero)
//! - Collection validators (each)
//! - Nested struct validation
//! - Custom validators and cross-field validation
//!
//! Run with: cargo run

use premortem::prelude::*;
use premortem::validate::{custom, validate_field};
use serde::Deserialize;
use stillwater::Validation;

// =============================================================================
// Example 1: String Validators
// =============================================================================

/// Configuration demonstrating string validators.
#[derive(Debug, Deserialize, DeriveValidate)]
struct StringConfig {
    /// Must not be empty
    #[validate(non_empty)]
    name: String,

    /// Must be between 8 and 128 characters
    #[validate(min_length(8), max_length(128))]
    api_key: String,

    /// Must match the pattern for a valid username
    #[validate(pattern(r"^[a-z][a-z0-9_]*$"))]
    username: String,
}

// =============================================================================
// Example 2: Numeric Validators
// =============================================================================

/// Configuration demonstrating numeric validators.
#[derive(Debug, Deserialize, DeriveValidate)]
struct NumericConfig {
    /// Must be in valid port range
    #[validate(range(1..=65535))]
    port: u16,

    /// Must be positive
    #[validate(positive)]
    max_connections: i32,

    /// Must not be zero
    #[validate(non_zero)]
    retry_count: u32,

    /// Timeout in seconds (1-3600)
    #[validate(range(1..=3600))]
    timeout_secs: u64,
}

// =============================================================================
// Example 3: Collection Validators
// =============================================================================

/// Configuration demonstrating collection validators.
///
/// For collection validation beyond `each`, implement the Validate trait manually
/// using the runtime validators from `premortem::validate::validators`.
#[derive(Debug, Deserialize, DeriveValidate)]
struct CollectionConfig {
    /// Each host must be non-empty
    #[validate(each(non_empty))]
    allowed_hosts: Vec<String>,

    /// Each tag must be non-empty
    #[validate(each(non_empty))]
    tags: Vec<String>,
}

// =============================================================================
// Example 4: Nested Validation
// =============================================================================

/// Server configuration.
#[derive(Debug, Deserialize, DeriveValidate)]
struct ServerConfig {
    #[validate(non_empty)]
    host: String,

    #[validate(range(1..=65535))]
    port: u16,
}

/// Database configuration.
#[derive(Debug, Deserialize, DeriveValidate)]
struct DatabaseConfig {
    #[validate(non_empty)]
    url: String,

    #[validate(range(1..=100))]
    pool_size: u32,
}

/// Application configuration with nested structs.
#[derive(Debug, Deserialize, DeriveValidate)]
struct AppConfig {
    /// Server settings - validation cascades to nested struct
    #[validate(nested)]
    server: ServerConfig,

    /// Database settings - validation cascades to nested struct
    #[validate(nested)]
    database: DatabaseConfig,
}

// =============================================================================
// Example 5: Cross-Field Validation
// =============================================================================

/// Configuration with cross-field validation.
///
/// For cross-field validation, implement the Validate trait manually
/// instead of using the derive macro.
#[derive(Debug, Deserialize)]
struct RangeConfig {
    min_value: i32,
    max_value: i32,
}

impl Validate for RangeConfig {
    fn validate(&self) -> ConfigValidation<()> {
        if self.min_value >= self.max_value {
            Validation::Failure(ConfigErrors::single(ConfigError::CrossFieldError {
                paths: vec!["min_value".to_string(), "max_value".to_string()],
                message: "min_value must be less than max_value".to_string(),
            }))
        } else {
            Validation::Success(())
        }
    }
}

// =============================================================================
// Example 6: Custom Validator
// =============================================================================

/// Configuration using a custom validator.
#[derive(Debug, Deserialize)]
struct CustomConfig {
    /// Must be an even number
    buffer_size: i32,
}

impl Validate for CustomConfig {
    fn validate(&self) -> ConfigValidation<()> {
        // Create a custom validator inline
        let even_validator = custom(|value: &i32, path: &str| {
            if value % 2 == 0 {
                Validation::Success(())
            } else {
                Validation::Failure(ConfigErrors::single(ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location: None,
                    value: Some(value.to_string()),
                    message: "value must be even".to_string(),
                }))
            }
        });

        validate_field(&self.buffer_size, "buffer_size", &[&even_validator])
    }
}

// =============================================================================
// Demo Runner
// =============================================================================

fn main() {
    println!("=== Validation Examples ===\n");

    // Demo 1: Valid configuration
    demo_valid_config();

    // Demo 2: Invalid configuration with multiple errors
    demo_invalid_config();

    // Demo 3: Cross-field validation
    demo_cross_field_validation();

    // Demo 4: Custom validator
    demo_custom_validator();
}

fn demo_valid_config() {
    println!("--- Valid Configuration ---");

    let valid_toml = r#"
        [server]
        host = "localhost"
        port = 8080

        [database]
        url = "postgres://localhost/mydb"
        pool_size = 10
    "#;

    let result = Config::<AppConfig>::builder()
        .source(Toml::string(valid_toml))
        .build();

    match result {
        Ok(config) => {
            println!("Config loaded successfully!");
            println!("  Server: {}:{}", config.server.host, config.server.port);
            println!(
                "  Database: {} (pool: {})",
                config.database.url, config.database.pool_size
            );
        }
        Err(errors) => {
            println!("Unexpected errors: {:?}", errors);
        }
    }
    println!();
}

fn demo_invalid_config() {
    println!("--- Invalid Configuration (Multiple Errors) ---");

    // This config has multiple validation errors
    let invalid_toml = r#"
        [server]
        host = ""
        port = 0

        [database]
        url = ""
        pool_size = 200
    "#;

    let result = Config::<AppConfig>::builder()
        .source(Toml::string(invalid_toml))
        .build();

    match result {
        Ok(_) => println!("Unexpected success"),
        Err(errors) => {
            println!("Found {} validation errors:", errors.len());
            for (i, error) in errors.iter().enumerate() {
                println!("  {}. {}", i + 1, error);
            }
        }
    }
    println!();
}

fn demo_cross_field_validation() {
    println!("--- Cross-Field Validation ---");

    let invalid_range = r#"
        min_value = 100
        max_value = 50
    "#;

    let result = Config::<RangeConfig>::builder()
        .source(Toml::string(invalid_range))
        .build();

    match result {
        Ok(_) => println!("Unexpected success"),
        Err(errors) => {
            println!("Cross-field validation caught error:");
            for error in errors.iter() {
                println!("  {}", error);
            }
        }
    }
    println!();
}

fn demo_custom_validator() {
    println!("--- Custom Validator ---");

    // Odd number should fail
    let odd_config = r#"buffer_size = 7"#;

    let result = Config::<CustomConfig>::builder()
        .source(Toml::string(odd_config))
        .build();

    match result {
        Ok(_) => println!("Unexpected success"),
        Err(errors) => {
            println!("Custom validator caught error:");
            for error in errors.iter() {
                println!("  {}", error);
            }
        }
    }

    // Even number should succeed
    let even_config = r#"buffer_size = 8"#;

    let result = Config::<CustomConfig>::builder()
        .source(Toml::string(even_config))
        .build();

    match result {
        Ok(config) => println!("Even value accepted: buffer_size = {}", config.buffer_size),
        Err(errors) => println!("Unexpected errors: {:?}", errors),
    }
    println!();
}
