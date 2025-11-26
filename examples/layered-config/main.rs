//! Layered configuration with value tracing.
//!
//! This example demonstrates premortem's layered configuration system with
//! value tracing to show where each configuration value originated.
//!
//! Configuration layers (in priority order, lowest to highest):
//!   1. Defaults - hardcoded default values
//!   2. Config file - TOML file overrides
//!   3. Environment variables - highest priority overrides
//!
//! Run with:
//!   cargo run --example layered-config
//!
//! With environment overrides:
//!   APP_SERVER_PORT=9000 APP_DEBUG=true cargo run --example layered-config

use premortem::prelude::*;
use serde::{Deserialize, Serialize};

/// Database configuration.
#[derive(Debug, Clone, Serialize, Deserialize, DeriveValidate)]
struct DatabaseConfig {
    #[validate(non_empty)]
    host: String,

    #[validate(range(1..=65535))]
    port: u16,

    #[validate(range(1..=100))]
    pool_size: i32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            pool_size: 10,
        }
    }
}

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize, DeriveValidate)]
struct ServerConfig {
    #[validate(non_empty)]
    host: String,

    #[validate(range(1..=65535))]
    port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }
    }
}

/// Application configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, DeriveValidate)]
struct AppConfig {
    #[validate(nested)]
    #[serde(default)]
    server: ServerConfig,

    #[validate(nested)]
    #[serde(default)]
    database: DatabaseConfig,

    #[serde(default)]
    debug: bool,
}

fn main() {
    println!("=== Layered Configuration Demo ===\n");

    // Configuration file that overrides some defaults
    let config_toml = r#"
        # Production config file
        [server]
        host = "0.0.0.0"
        port = 3000

        [database]
        host = "db.production.internal"
        # port uses default (5432)
        pool_size = 25
    "#;

    // Build with tracing to see where values come from
    let traced = Config::<AppConfig>::builder()
        // Layer 1: Defaults (lowest priority)
        .source(Defaults::from(AppConfig::default()))
        // Layer 2: Config file (overrides defaults)
        .source(Toml::string(config_toml).named("config.toml"))
        // Layer 3: Environment variables (highest priority)
        .source(Env::prefix("APP_"))
        .build_traced()
        .unwrap_or_else(|errors| {
            eprintln!("Configuration errors:");
            for error in errors.iter() {
                eprintln!("  {}", error);
            }
            std::process::exit(1);
        });

    // Show the layer structure
    println!("Configuration layers (lowest to highest priority):");
    println!("  1. Defaults (hardcoded)");
    println!("  2. config.toml");
    println!("  3. Environment variables (APP_* prefix)");
    println!();

    // Helper to get source name
    let source_of = |path: &str| -> String {
        traced
            .trace(path)
            .map(|t| t.final_value.source.source.clone())
            .unwrap_or_else(|| "unknown".to_string())
    };

    // Show final configuration with sources
    println!("Final configuration:");
    println!(
        "  server.host: {} (from {})",
        traced.server.host,
        source_of("server.host")
    );
    println!(
        "  server.port: {} (from {})",
        traced.server.port,
        source_of("server.port")
    );
    println!(
        "  database.host: {} (from {})",
        traced.database.host,
        source_of("database.host")
    );
    println!(
        "  database.port: {} (from {})",
        traced.database.port,
        source_of("database.port")
    );
    println!(
        "  database.pool_size: {} (from {})",
        traced.database.pool_size,
        source_of("database.pool_size")
    );
    println!("  debug: {} (from {})", traced.debug, source_of("debug"));
    println!();

    // Show override history for values that were overridden
    let overridden: Vec<&str> = traced.overridden_paths().collect();
    if overridden.is_empty() {
        println!("No values were overridden.");
    } else {
        println!("Override history ({} values changed):", overridden.len());
        for path in overridden {
            if let Some(trace) = traced.trace(path) {
                println!("  {}:", path);
                for entry in &trace.history {
                    let marker = if entry.is_final { "→" } else { " " };
                    println!("    {} [{}] {:?}", marker, entry.source, entry.value);
                }
            }
        }
    }
    println!();

    // Show full trace report
    println!("=== Full Trace Report ===\n");
    println!("{}", traced.trace_report());

    // Get the final config
    let config = traced.into_inner();
    println!("Ready to use: {:?}", config);
}
