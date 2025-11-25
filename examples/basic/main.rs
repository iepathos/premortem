//! Basic premortem configuration example.
//!
//! This example demonstrates the simplest use case:
//! - Loading configuration from a TOML file
//! - Overriding with environment variables
//! - Basic validation with error accumulation
//!
//! Run with: cargo run
//! Or with env override: APP_PORT=9000 cargo run

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
}

fn main() {
    // Build configuration from multiple sources
    // Later sources override earlier ones
    let result = Config::<AppConfig>::builder()
        // Load from TOML file (optional - won't error if missing)
        .source(Toml::file("config.toml").optional())
        // Override with environment variables prefixed with APP_
        // APP_HOST -> host, APP_PORT -> port, APP_DEBUG -> debug
        .source(Env::prefix("APP_"))
        .build();

    match result {
        Ok(config) => {
            println!("Configuration loaded successfully!");
            println!("  Host: {}", config.host);
            println!("  Port: {}", config.port);
            println!("  Debug: {}", config.debug);
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
