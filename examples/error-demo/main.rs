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
    // Demonstrate missing field error - config is incomplete
    let config_toml = r#"[server]
host = "localhost"
port = 8080

[database]
port = 5432
pool_size = 10"#;
    // Note: database.host is missing!

    let env = MockEnv::new().with_file("config.toml", config_toml);

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env);

    match result {
        Ok(_) => println!("Unexpected success"),
        Err(errors) => {
            print!("{}", errors);
        }
    }
}

fn demo_pretty_errors() {
    // Demonstrate pretty printing with validation errors grouped by source.
    // The config has multiple validation failures that will be grouped by file.
    let config_toml = r#"[server]
host = ""
port = 0

[database]
host = ""
port = 5432
pool_size = -5"#;

    let env = MockEnv::new().with_file("config.toml", config_toml);

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env);

    match result {
        Ok(_) => println!("Unexpected success"),
        Err(errors) => {
            // Pretty print with colors and grouping by source
            errors.pretty_print(&PrettyPrintOptions::default());
        }
    }
}
