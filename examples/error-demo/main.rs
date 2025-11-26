//! Error output demonstration.
//!
//! This example demonstrates premortem's detailed error output with source
//! location tracking. Configuration errors are accumulated and displayed
//! with their source locations for easy debugging.
//!
//! Run with:
//!   cargo run --example error-demo

use premortem::prelude::*;
use serde::Deserialize;

/// Database configuration with validation.
#[derive(Debug, Deserialize, DeriveValidate)]
struct DatabaseConfig {
    #[validate(non_empty)]
    host: String,

    #[validate(range(1..=65535))]
    port: u16,

    #[validate(range(1..=100))]
    pool_size: i32,
}

/// Server configuration.
#[derive(Debug, Deserialize, DeriveValidate)]
struct ServerConfig {
    #[validate(non_empty)]
    host: String,

    #[validate(range(1..=65535))]
    port: u16,
}

/// Application configuration.
#[derive(Debug, Deserialize, DeriveValidate)]
struct AppConfig {
    #[validate(nested)]
    server: ServerConfig,

    #[validate(nested)]
    database: DatabaseConfig,
}

fn main() {
    println!("=== Error Output Demo ===\n");

    // Demo 1: Valid configuration
    println!("--- Valid Configuration ---\n");
    demo_valid_config();

    // Demo 2: Invalid configuration with accumulated errors
    println!("\n--- Invalid Configuration (Accumulated Errors) ---\n");
    demo_invalid_config();

    // Demo 3: Error output matching README format
    println!("\n--- Error Output (README Format) ---\n");
    demo_readme_errors();

    // Demo 4: Pretty-printed errors grouped by source
    println!("\n--- Pretty-Printed Errors (Grouped by Source) ---");
    demo_pretty_errors();
}

fn demo_valid_config() {
    let config_toml = r#"
        [server]
        host = "localhost"
        port = 8080

        [database]
        host = "db.example.com"
        port = 5432
        pool_size = 10
    "#;

    let result = Config::<AppConfig>::builder()
        .source(Toml::string(config_toml).named("config.toml"))
        .build();

    match result {
        Ok(config) => {
            println!("Configuration loaded successfully!");
            println!("  Server: {}:{}", config.server.host, config.server.port);
            println!(
                "  Database: {}:{} (pool: {})",
                config.database.host, config.database.port, config.database.pool_size
            );
        }
        Err(errors) => {
            eprintln!("Unexpected errors: {}", errors);
        }
    }
}

fn demo_invalid_config() {
    // Config with multiple validation errors
    let invalid_toml = r#"
        [server]
        host = ""
        port = 0

        [database]
        host = ""
        port = 5432
        pool_size = -5
    "#;

    let result = Config::<AppConfig>::builder()
        .source(Toml::string(invalid_toml).named("config.toml"))
        .build();

    match result {
        Ok(_) => println!("Unexpected success"),
        Err(errors) => {
            // Display trait shows all accumulated errors
            print!("{}", errors);
        }
    }
}

fn demo_readme_errors() {
    // Manually construct errors matching README example:
    //
    // Configuration errors (3):
    //   [config.toml:8] missing required field 'database.host'
    //   [env:APP_PORT] value "abc" is not a valid integer
    //   [config.toml:10] 'pool_size' value -5 must be >= 1

    let errors = ConfigErrors::from_vec(vec![
        ConfigError::MissingField {
            path: "database.host".to_string(),
            source_location: Some(SourceLocation::new("config.toml").with_line(8)),
            searched_sources: vec!["config.toml".to_string(), "environment".to_string()],
        },
        ConfigError::ParseError {
            path: "port".to_string(),
            source_location: SourceLocation::env("APP_PORT"),
            expected_type: "integer".to_string(),
            actual_value: "abc".to_string(),
            message: "is not a valid integer".to_string(),
        },
        ConfigError::ValidationError {
            path: "pool_size".to_string(),
            source_location: Some(SourceLocation::new("config.toml").with_line(10)),
            value: Some("-5".to_string()),
            message: "must be >= 1".to_string(),
        },
    ])
    .unwrap();

    // Display trait output (matches README format)
    print!("{}", errors);
}

fn demo_pretty_errors() {
    // Errors from multiple sources - matching what real validation produces
    let errors = ConfigErrors::from_vec(vec![
        ConfigError::ValidationError {
            path: "server.host".to_string(),
            source_location: Some(SourceLocation::new("config.toml").with_line(3)),
            value: Some("".to_string()),
            message: "value cannot be empty".to_string(),
        },
        ConfigError::ValidationError {
            path: "database.host".to_string(),
            source_location: Some(SourceLocation::new("config.toml").with_line(8)),
            value: Some("".to_string()),
            message: "value cannot be empty".to_string(),
        },
        ConfigError::ValidationError {
            path: "database.pool_size".to_string(),
            source_location: Some(SourceLocation::new("config.toml").with_line(10)),
            value: Some("-5".to_string()),
            message: "must be >= 1".to_string(),
        },
        ConfigError::ParseError {
            path: "server.port".to_string(),
            source_location: SourceLocation::env("APP_SERVER_PORT"),
            expected_type: "integer".to_string(),
            actual_value: "not-a-number".to_string(),
            message: "invalid digit".to_string(),
        },
    ])
    .unwrap();

    // Pretty print with colors and grouping by source
    errors.pretty_print(&PrettyPrintOptions::default());
}
