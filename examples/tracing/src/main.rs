//! Value tracing example.
//!
//! This example demonstrates value origin tracking for debugging.
//! When multiple sources provide configuration, it can be hard to know
//! which source "won" for each value. Value tracing shows:
//!
//! - The final value for each configuration path
//! - Which source provided that value
//! - The complete history of overrides
//!
//! Run with:
//!   cargo run
//!   APP_HOST=override cargo run  # See override in trace

use premortem::prelude::*;
use serde::{Deserialize, Serialize};

/// Application configuration.
#[derive(Debug, Clone, Serialize, Deserialize, DeriveValidate)]
struct AppConfig {
    #[serde(default = "default_host")]
    host: String,

    #[serde(default = "default_port")]
    port: u16,

    #[serde(default)]
    debug: bool,

    #[serde(default = "default_log_level")]
    log_level: String,
}

fn default_host() -> String {
    "localhost".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            debug: false,
            log_level: default_log_level(),
        }
    }
}

fn main() {
    println!("=== Value Tracing Demo ===\n");

    // Use build_traced() instead of build() to track value origins
    let traced = Config::<AppConfig>::builder()
        // Layer 1: Defaults
        .source(Defaults::from(AppConfig::default()))
        // Layer 2: Config file (optional)
        .source(Toml::file("config.toml").optional())
        // Layer 3: Environment variables
        .source(Env::prefix("APP_"))
        .build_traced()
        .unwrap_or_else(|errors| {
            eprintln!("Configuration errors:");
            for error in errors.iter() {
                eprintln!("  - {}", error);
            }
            std::process::exit(1);
        });

    println!("Configuration loaded successfully!\n");

    // Show final values
    println!("Final Configuration:");
    println!("  host: {}", traced.host);
    println!("  port: {}", traced.port);
    println!("  debug: {}", traced.debug);
    println!("  log_level: {}", traced.log_level);
    println!();

    // Show which paths were overridden
    let overridden: Vec<&str> = traced.overridden_paths().collect();
    if overridden.is_empty() {
        println!("No values were overridden (all from defaults)");
    } else {
        println!("Overridden paths ({}):", overridden.len());
        for path in &overridden {
            println!("  - {}", path);
        }
    }
    println!();

    // Show detailed trace for specific paths
    println!("=== Detailed Traces ===\n");

    for path in &["host", "port", "debug", "log_level"] {
        if let Some(trace) = traced.trace(path) {
            println!("{}:", path);
            println!(
                "  Final value: {:?} (from {})",
                trace.final_value.value, trace.final_value.source
            );

            if trace.was_overridden() {
                println!("  Override history:");
                for entry in &trace.history {
                    let marker = if entry.is_final { "→" } else { " " };
                    let note = if !entry.is_final {
                        " (overridden)"
                    } else {
                        " (final)"
                    };
                    println!("    {} [{}] {:?}{}", marker, entry.source, entry.value, note);
                }
            }
            println!();
        }
    }

    // Generate full trace report
    println!("=== Full Trace Report ===\n");
    println!("{}", traced.trace_report());

    // Get the actual config value when done
    let config = traced.into_inner();
    println!("Ready to use config: {:?}", config);
}
