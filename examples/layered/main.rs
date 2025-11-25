//! Layered configuration example.
//!
//! This example demonstrates environment-specific configuration layering:
//! 1. Hardcoded defaults (lowest priority)
//! 2. Base configuration file (config/base.toml)
//! 3. Environment-specific file (config/{env}.toml)
//! 4. Environment variables (highest priority)
//!
//! Run with:
//!   cargo run                    # Development mode (default)
//!   APP_ENV=production cargo run # Production mode

use premortem::prelude::*;
use serde::{Deserialize, Serialize};

/// Application configuration with sensible defaults.
#[derive(Debug, Clone, Serialize, Deserialize, DeriveValidate)]
struct AppConfig {
    #[validate(non_empty)]
    #[serde(default = "default_host")]
    host: String,

    #[validate(range(1..=65535))]
    #[serde(default = "default_port")]
    port: u16,

    #[serde(default)]
    debug: bool,

    #[serde(default = "default_log_level")]
    log_level: String,

    #[serde(default = "default_max_connections")]
    max_connections: u32,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_max_connections() -> u32 {
    100
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            debug: false,
            log_level: default_log_level(),
            max_connections: default_max_connections(),
        }
    }
}

fn main() {
    // Determine environment from APP_ENV or default to "development"
    let environment = std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_string());

    println!("Loading configuration for environment: {}", environment);
    println!();

    // Build layered configuration
    //
    // Priority (lowest to highest):
    // 1. Defaults::from(AppConfig::default()) - hardcoded defaults
    // 2. config/base.toml - shared base configuration
    // 3. config/{environment}.toml - environment-specific overrides
    // 4. Env::prefix("APP_") - environment variables
    let config = Config::<AppConfig>::builder()
        // Layer 1: Hardcoded defaults
        .source(Defaults::from(AppConfig::default()))
        // Layer 2: Base configuration (optional - use partial defaults if missing)
        .source(Toml::file("config/base.toml").optional())
        // Layer 3: Environment-specific config (optional)
        .source(Toml::file(format!("config/{}.toml", environment)).optional())
        // Layer 4: Environment variables (highest priority)
        .source(Env::prefix("APP_"))
        .build()
        .unwrap_or_else(|errors| {
            eprintln!("Configuration errors ({}):", errors.len());
            for error in errors.iter() {
                eprintln!("  - {}", error);
            }
            std::process::exit(1);
        });

    println!("Final configuration:");
    println!("  Host: {}", config.host);
    println!("  Port: {}", config.port);
    println!("  Debug: {}", config.debug);
    println!("  Log Level: {}", config.log_level);
    println!("  Max Connections: {}", config.max_connections);
    println!();

    // Show which environment we're running in
    match environment.as_str() {
        "development" => {
            println!("Running in DEVELOPMENT mode");
            if config.debug {
                println!("  Debug mode enabled - verbose logging active");
            }
        }
        "production" => {
            println!("Running in PRODUCTION mode");
            if config.debug {
                println!("  WARNING: Debug mode enabled in production!");
            }
        }
        _ => {
            println!("Running in {} mode", environment.to_uppercase());
        }
    }
}
