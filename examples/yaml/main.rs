//! YAML configuration example.
//!
//! This example demonstrates loading configuration from a YAML file:
//! - Loading configuration from a YAML file
//! - Overriding with environment variables
//! - Basic validation with error accumulation
//!
//! Run with: cargo run --example yaml --features yaml
//! Or with env override: APP_PORT=9000 cargo run --example yaml --features yaml

use premortem::prelude::*;
use serde::Deserialize;

/// Application configuration with validation.
///
/// The `Validate` derive macro automatically generates validation
/// based on the `#[validate(...)]` attributes.
#[derive(Debug, Deserialize, DeriveValidate)]
struct AppConfig {
    /// Server hostname - cannot be empty
    #[validate(non_empty, message = "host cannot be empty")]
    host: String,

    /// Server port - must be valid port number (1-65535)
    #[validate(range(1..=65535))]
    port: u16,

    /// Enable debug mode - defaults to false
    #[serde(default)]
    debug: bool,

    /// Database configuration (nested)
    database: DatabaseConfig,
}

/// Nested database configuration.
#[derive(Debug, Deserialize, DeriveValidate)]
struct DatabaseConfig {
    /// Database hostname
    #[validate(non_empty, message = "database host cannot be empty")]
    host: String,

    /// Database port
    #[validate(range(1..=65535))]
    port: u16,

    /// Database name
    #[validate(non_empty)]
    name: String,
}

fn main() {
    // Build configuration from multiple sources
    // Later sources override earlier ones
    let result = Config::<AppConfig>::builder()
        // Load from YAML file (optional - won't error if missing)
        .source(Yaml::file("config.yaml").optional())
        // Override with environment variables prefixed with APP_
        // APP_HOST -> host, APP_PORT -> port, APP_DEBUG -> debug
        // APP_DATABASE_HOST -> database.host, etc.
        .source(Env::prefix("APP_"))
        .build();

    match result {
        Ok(config) => {
            println!("Configuration loaded successfully!");
            println!("Server:");
            println!("  Host: {}", config.host);
            println!("  Port: {}", config.port);
            println!("  Debug: {}", config.debug);
            println!("Database:");
            println!("  Host: {}", config.database.host);
            println!("  Port: {}", config.database.port);
            println!("  Name: {}", config.database.name);
        }
        Err(errors) => {
            // Premortem collects ALL errors, not just the first one
            eprintln!("Configuration errors ({}):", errors.len());
            for error in errors.iter() {
                eprintln!("  - {}", error);
            }
            std::process::exit(1);
        }
    }
}
