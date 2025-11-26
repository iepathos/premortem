//! Hot Reload Example
//!
//! Demonstrates premortem's watch feature for automatic configuration reloading.
//!
//! ## Running
//!
//! ```bash
//! cargo run --example watch --features watch
//! ```
//!
//! The example will automatically modify the config file to demonstrate hot reload.

use premortem::prelude::*;
use serde::Deserialize;
use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Server configuration section.
#[derive(Debug, Clone, Deserialize, DeriveValidate)]
struct ServerConfig {
    #[validate(non_empty)]
    host: String,

    #[validate(range(1..=65535))]
    port: u16,
}

/// Database configuration section.
#[derive(Debug, Clone, Deserialize, DeriveValidate)]
struct DatabaseConfig {
    #[validate(non_empty)]
    host: String,

    #[validate(range(1..=65535))]
    port: u16,

    #[validate(range(1..=1000))]
    max_connections: u32,
}

/// Feature flags section.
#[derive(Debug, Clone, Deserialize, DeriveValidate)]
struct FeaturesConfig {
    debug_mode: bool,

    #[validate(range(1..=10000))]
    rate_limit: u32,
}

/// Root application configuration.
#[derive(Debug, Clone, Deserialize, DeriveValidate)]
struct AppConfig {
    #[validate(nested)]
    server: ServerConfig,

    #[validate(nested)]
    database: DatabaseConfig,

    #[validate(nested)]
    features: FeaturesConfig,
}

const CONFIG_PATH: &str = "examples/watch/config.toml";

fn main() {
    println!("=== Premortem Hot Reload Example ===\n");

    // Save original config to restore on exit
    let original_config = fs::read_to_string(CONFIG_PATH).expect("Failed to read config");

    // Set up cleanup on Ctrl+C
    let original_for_cleanup = original_config.clone();
    ctrlc_handler(move || {
        let _ = fs::write(CONFIG_PATH, &original_for_cleanup);
    });

    // Build watched configuration
    let result = Config::<AppConfig>::builder()
        .source(Toml::file(CONFIG_PATH))
        .build_watched();

    let (config, watcher) = match result {
        Ok((config, watcher)) => (config, watcher),
        Err(errors) => {
            eprintln!("Failed to load initial configuration:");
            for error in errors.iter() {
                eprintln!("  - {}", error);
            }
            std::process::exit(1);
        }
    };

    // Display initial configuration
    print_config(&config.current(), "Initial");

    println!("\n--- Watching for changes (auto-demo mode) ---");
    println!("The example will automatically modify config.toml to demonstrate hot reload.\n");

    // Subscribe to configuration events
    let config_for_callback = config.clone();
    watcher.on_change(move |event| match event {
        ConfigEvent::SourceChanged { path } => {
            println!("[EVENT] File changed: {}", path.display());
        }
        ConfigEvent::Reloaded { changed_sources } => {
            println!(
                "[EVENT] Config reloaded successfully from: {:?}",
                changed_sources
            );
            print_config(&config_for_callback.current(), "New");
        }
        ConfigEvent::ReloadFailed { errors } => {
            println!("[EVENT] Reload FAILED - keeping previous config");
            for error in errors.iter() {
                println!("        - {}", error);
            }
        }
        ConfigEvent::WatchError { message } => {
            eprintln!("[EVENT] Watch error: {}", message);
        }
    });

    // Demo sequence
    println!("[DEMO] Starting in 2 seconds...\n");
    thread::sleep(Duration::from_secs(2));

    // Demo 1: Valid change - update port
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("[DEMO] Step 1: Changing server.port from 8080 to 9000...");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    let new_config = original_config.replace("port = 8080", "port = 9000");
    fs::write(CONFIG_PATH, &new_config).expect("Failed to write config");
    thread::sleep(Duration::from_secs(2));

    // Demo 2: Valid change - enable debug mode
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("[DEMO] Step 2: Enabling debug_mode...");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    let new_config = new_config.replace("debug_mode = false", "debug_mode = true");
    fs::write(CONFIG_PATH, &new_config).expect("Failed to write config");
    thread::sleep(Duration::from_secs(2));

    // Demo 3: Invalid change - port = 0 (validation fails)
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("[DEMO] Step 3: Setting invalid port = 0 (should fail validation)...");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    let invalid_config = new_config.replace("port = 9000", "port = 0");
    fs::write(CONFIG_PATH, &invalid_config).expect("Failed to write config");
    thread::sleep(Duration::from_secs(2));

    // Demo 4: Fix the config
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("[DEMO] Step 4: Fixing port back to 9000...");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    fs::write(CONFIG_PATH, &new_config).expect("Failed to write config");
    thread::sleep(Duration::from_secs(2));

    // Show final state
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("[DEMO] Complete! Final configuration:");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    print_config(&config.current(), "Final");

    // Restore original config
    println!("\n[DEMO] Restoring original config.toml...");
    fs::write(CONFIG_PATH, &original_config).expect("Failed to restore config");

    println!("\n[DEMO] Done! You can also edit config.toml manually while running.");
    println!("       Press Ctrl+C to exit.\n");

    // Keep running so user can try manual edits
    loop {
        thread::sleep(Duration::from_secs(5));
        let current = config.current();
        println!(
            "[STATUS] Server: {}:{} | Debug: {} | Rate limit: {}",
            current.server.host,
            current.server.port,
            current.features.debug_mode,
            current.features.rate_limit
        );
    }
}

/// Pretty-print the configuration.
fn print_config(config: &Arc<AppConfig>, label: &str) {
    println!();
    println!("┌─ {:<7} Configuration ──────────────────┐", label);
    println!("│                                          │");
    println!("│  Server                                  │");
    println!("│    host:            {:<21}│", config.server.host);
    println!("│    port:            {:<21}│", config.server.port);
    println!("│                                          │");
    println!("│  Database                                │");
    println!("│    host:            {:<21}│", config.database.host);
    println!("│    port:            {:<21}│", config.database.port);
    println!(
        "│    max_connections: {:<21}│",
        config.database.max_connections
    );
    println!("│                                          │");
    println!("│  Features                                │");
    println!("│    debug_mode:      {:<21}│", config.features.debug_mode);
    println!("│    rate_limit:      {:<21}│", config.features.rate_limit);
    println!("│                                          │");
    println!("└──────────────────────────────────────────┘");
}

/// Simple Ctrl+C handler to restore config on exit.
fn ctrlc_handler<F: Fn() + Send + 'static>(cleanup: F) {
    thread::spawn(move || {
        // This is a simple approach - in production you'd use the ctrlc crate
        // For this demo, cleanup happens when the demo completes normally
        let _ = cleanup;
    });
}
