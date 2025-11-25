---
number: 13
title: Integration Examples and Documentation
category: foundation
priority: high
status: draft
dependencies: [1, 2, 3, 5, 8, 9]
created: 2025-11-25
---

# Specification 013: Integration Examples and Documentation

**Category**: foundation
**Priority**: high
**Status**: draft
**Dependencies**: [001 - Core Config Builder, 002 - Error Types, 003 - Validate Trait, 005 - TOML Source, 008 - Pretty Errors, 009 - Value Tracing]

## Context

Premortem has a comprehensive feature set but lacks practical examples showing how to integrate it into real-world applications. Users need:

1. **Working examples** they can copy and adapt
2. **Best practices** for structuring configuration
3. **Common patterns** for validation, error handling, and testing
4. **Framework integration** guides (Axum, Actix, Tokio, etc.)
5. **Migration guides** for users coming from other config libraries

Good documentation and examples are critical for adoption. The current README is minimal and doesn't showcase premortem's strengths (error accumulation, validation, tracing).

## Objective

Create comprehensive integration examples and documentation that demonstrate premortem's capabilities, best practices, and integration patterns with common Rust frameworks and use cases.

## Requirements

### Functional Requirements

1. **Example Applications**: Complete, runnable example programs
2. **Framework Integration**: Examples for popular Rust web frameworks
3. **Pattern Documentation**: Common configuration patterns
4. **Testing Guides**: How to test configuration with MockEnv
5. **Migration Guides**: Coming from figment, config-rs, etc.
6. **API Documentation**: Comprehensive rustdoc with examples

### Non-Functional Requirements

- Examples must compile and run
- Documentation must be accurate and up-to-date
- Examples should demonstrate best practices
- Code should be copy-pasteable with minimal modification
- Clear, concise explanations

## Acceptance Criteria

- [ ] `examples/` directory with runnable example programs
- [ ] `examples/basic/` - Simple configuration loading
- [ ] `examples/web-server/` - Web server with configuration
- [ ] `examples/validation/` - Comprehensive validation examples
- [ ] `examples/testing/` - Configuration testing patterns
- [ ] `examples/layered/` - Multi-source configuration layering
- [ ] `examples/tracing/` - Value origin debugging
- [ ] README.md with quick start and feature overview
- [ ] PATTERNS.md with common configuration patterns
- [ ] TESTING.md with testing best practices
- [ ] All doc examples compile (no `ignore` without reason)
- [ ] Lib.rs module documentation comprehensive

## Technical Details

### Directory Structure

```
examples/
├── basic/
│   ├── Cargo.toml
│   └── src/
│       └── main.rs           # Minimal example
├── web-server/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs           # Axum server with config
│       └── config.rs         # Config struct definitions
├── validation/
│   ├── Cargo.toml
│   └── src/
│       └── main.rs           # All validator examples
├── testing/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs           # Demo runner
│       └── lib.rs            # Testable config patterns
├── layered/
│   ├── Cargo.toml
│   ├── config/
│   │   ├── base.toml
│   │   ├── development.toml
│   │   └── production.toml
│   └── src/
│       └── main.rs           # Environment-based layering
├── tracing/
│   ├── Cargo.toml
│   └── src/
│       └── main.rs           # Value origin tracking demo
└── README.md                 # Examples index

docs/
├── PATTERNS.md              # Common configuration patterns
├── TESTING.md               # Testing configuration guide
└── MIGRATION.md             # Migration from other libraries
```

### Example: Basic Configuration

```rust
// examples/basic/src/main.rs
//! Basic premortem configuration example.
//!
//! Run with: cargo run --example basic
//! Or with env override: APP_PORT=9000 cargo run --example basic

use premortem::{Config, ConfigError, DeriveValidate, Env, Toml};
use serde::Deserialize;

/// Application configuration with validation.
#[derive(Debug, Deserialize, DeriveValidate)]
struct AppConfig {
    /// Server hostname
    #[validate(non_empty, message = "host cannot be empty")]
    host: String,

    /// Server port (1-65535)
    #[validate(range(1..=65535))]
    port: u16,

    /// Enable debug mode
    #[serde(default)]
    debug: bool,
}

fn main() {
    // Build configuration from multiple sources
    let result = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml").optional())
        .source(Env::new().prefix("APP"))
        .build();

    match result {
        Ok(config) => {
            println!("Configuration loaded successfully!");
            println!("  Host: {}", config.host);
            println!("  Port: {}", config.port);
            println!("  Debug: {}", config.debug);
        }
        Err(errors) => {
            // Pretty print ALL configuration errors
            eprintln!("Configuration errors:");
            for error in errors.iter() {
                eprintln!("  - {}", error);
            }
            std::process::exit(1);
        }
    }
}
```

### Example: Web Server Integration (Axum)

```rust
// examples/web-server/src/config.rs
use premortem::{ConfigError, ConfigErrors, ConfigValidation, DeriveValidate};
use serde::Deserialize;
use stillwater::Validation;

#[derive(Debug, Clone, Deserialize, DeriveValidate)]
pub struct ServerConfig {
    #[validate(non_empty)]
    pub host: String,

    #[validate(range(1..=65535))]
    pub port: u16,

    #[serde(default = "default_workers")]
    #[validate(range(1..=256))]
    pub workers: usize,
}

fn default_workers() -> usize {
    num_cpus::get()
}

#[derive(Debug, Clone, Deserialize, DeriveValidate)]
pub struct DatabaseConfig {
    #[validate(non_empty)]
    pub url: String,

    #[validate(range(1..=100))]
    pub pool_size: u32,

    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    30
}

#[derive(Debug, Clone, Deserialize, DeriveValidate)]
pub struct AppConfig {
    #[validate(nested)]
    pub server: ServerConfig,

    #[validate(nested)]
    pub database: DatabaseConfig,

    #[serde(default)]
    pub debug: bool,
}

// examples/web-server/src/main.rs
use axum::{routing::get, Router};
use premortem::{Config, Env, PrettyPrintOptions, Toml, ValidationExt};
use std::sync::Arc;

mod config;
use config::AppConfig;

#[tokio::main]
async fn main() {
    // Load and validate configuration
    let config = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .source(Env::new().prefix("APP"))
        .build()
        .unwrap_or_else(|errors| {
            // Pretty print errors with colors and grouping
            errors.pretty_print(&PrettyPrintOptions::default());
            std::process::exit(1);
        });

    let config = Arc::new(config.into_inner());

    // Build router with config
    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .with_state(config.clone());

    // Start server
    let addr = format!("{}:{}", config.server.host, config.server.port);
    println!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### Example: Comprehensive Validation

```rust
// examples/validation/src/main.rs
//! Demonstrates all built-in validators and custom validation.

use premortem::{
    Config, ConfigError, ConfigErrors, ConfigValidation,
    DeriveValidate, Toml, custom, validate_field,
};
use serde::Deserialize;
use stillwater::Validation;

/// User configuration with various validators.
#[derive(Debug, Deserialize, DeriveValidate)]
struct UserConfig {
    // String validators
    #[validate(non_empty)]
    name: String,

    #[validate(email)]
    email: String,

    #[validate(url)]
    website: Option<String>,

    #[validate(min_length(8), max_length(128))]
    api_key: String,

    #[validate(pattern(r"^[a-z][a-z0-9_]*$"))]
    username: String,

    // Numeric validators
    #[validate(range(1..=65535))]
    port: u16,

    #[validate(positive)]
    max_connections: i32,

    #[validate(non_zero)]
    retry_count: u32,

    // Collection validators
    #[validate(non_empty_collection)]
    allowed_hosts: Vec<String>,

    #[validate(min_items(1), max_items(10))]
    tags: Vec<String>,

    #[validate(each(non_empty))]
    paths: Vec<String>,

    // Path validators (when file system access needed)
    // #[validate(file_exists)]
    // cert_path: PathBuf,
}

/// Cross-field validation example
#[derive(Debug, Deserialize)]
struct RangeConfig {
    min_value: i32,
    max_value: i32,
}

impl premortem::Validate for RangeConfig {
    fn validate(&self) -> ConfigValidation<()> {
        let mut errors = Vec::new();

        if self.min_value >= self.max_value {
            errors.push(ConfigError::CrossFieldError {
                paths: vec!["min_value".to_string(), "max_value".to_string()],
                message: "min_value must be less than max_value".to_string(),
            });
        }

        match ConfigErrors::from_vec(errors) {
            Some(errs) => Validation::Failure(errs),
            None => Validation::Success(()),
        }
    }
}

/// Custom validator example
fn validate_even(value: &i32, path: &str) -> ConfigValidation<()> {
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
}

#[derive(Debug, Deserialize)]
struct CustomConfig {
    buffer_size: i32,
}

impl premortem::Validate for CustomConfig {
    fn validate(&self) -> ConfigValidation<()> {
        validate_even(&self.buffer_size, "buffer_size")
    }
}

fn main() {
    println!("Validation examples - see source for all patterns");

    // Demo with intentionally invalid config
    let invalid_config = r#"
        name = ""
        email = "not-an-email"
        api_key = "short"
        username = "123invalid"
        port = 70000
        max_connections = -5
        retry_count = 0
        allowed_hosts = []
        tags = []
        paths = ["valid", ""]
    "#;

    let result = Config::<UserConfig>::builder()
        .source(Toml::string(invalid_config))
        .build();

    match result {
        Ok(_) => println!("Config valid (unexpected)"),
        Err(errors) => {
            println!("Found {} validation errors:", errors.len());
            for (i, error) in errors.iter().enumerate() {
                println!("  {}. {}", i + 1, error);
            }
        }
    }
}
```

### Example: Testing Configuration

```rust
// examples/testing/src/lib.rs
//! Demonstrates testing patterns for configuration.

use premortem::{Config, ConfigErrors, DeriveValidate, Env, MockEnv, Toml};
use serde::Deserialize;

#[derive(Debug, Deserialize, DeriveValidate, PartialEq)]
pub struct AppConfig {
    #[validate(non_empty)]
    pub host: String,

    #[validate(range(1..=65535))]
    pub port: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test loading from TOML file
    #[test]
    fn test_load_from_toml() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            host = "localhost"
            port = 8080
            "#,
        );

        let config = Config::<AppConfig>::builder()
            .source(Toml::file("config.toml"))
            .build_with_env(&env)
            .expect("should load successfully");

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 8080);
    }

    /// Test environment variable override
    #[test]
    fn test_env_override() {
        let env = MockEnv::new()
            .with_file("config.toml", r#"host = "localhost"\nport = 8080"#)
            .with_env("APP_PORT", "9000");

        let config = Config::<AppConfig>::builder()
            .source(Toml::file("config.toml"))
            .source(Env::new().prefix("APP"))
            .build_with_env(&env)
            .expect("should load successfully");

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 9000); // Overridden by env
    }

    /// Test validation error accumulation
    #[test]
    fn test_validation_errors_accumulate() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            host = ""
            port = 0
            "#,
        );

        let result = Config::<AppConfig>::builder()
            .source(Toml::file("config.toml"))
            .build_with_env(&env);

        assert!(result.is_err());
        let errors = result.unwrap_err();
        // Both validations should fail
        assert_eq!(errors.len(), 2);
    }

    /// Test missing required file
    #[test]
    fn test_missing_required_file() {
        let env = MockEnv::new();

        let result = Config::<AppConfig>::builder()
            .source(Toml::file("missing.toml"))
            .build_with_env(&env);

        assert!(result.is_err());
    }

    /// Test optional file doesn't error when missing
    #[test]
    fn test_optional_file_missing() {
        let env = MockEnv::new().with_env("APP_HOST", "localhost").with_env("APP_PORT", "8080");

        let config = Config::<AppConfig>::builder()
            .source(Toml::file("config.toml").optional())
            .source(Env::new().prefix("APP"))
            .build_with_env(&env)
            .expect("should load from env only");

        assert_eq!(config.host, "localhost");
    }

    /// Test permission denied error
    #[test]
    fn test_permission_denied() {
        let env = MockEnv::new().with_unreadable_file("secret.toml");

        let result = Config::<AppConfig>::builder()
            .source(Toml::file("secret.toml"))
            .build_with_env(&env);

        assert!(result.is_err());
        // Check it's an IoError, not NotFound
    }
}
```

### Example: Layered Configuration

```rust
// examples/layered/src/main.rs
//! Demonstrates environment-specific configuration layering.
//!
//! Configuration is loaded in layers:
//! 1. Base defaults (hardcoded)
//! 2. Base config file (config/base.toml)
//! 3. Environment-specific file (config/{env}.toml)
//! 4. Environment variables (highest priority)

use premortem::{Config, Defaults, DeriveValidate, Env, Toml};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, DeriveValidate, Default)]
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

fn main() {
    // Determine environment from APP_ENV or default to "development"
    let environment = std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_string());

    println!("Loading configuration for environment: {}", environment);

    // Build layered configuration
    let config = Config::<AppConfig>::builder()
        // Layer 1: Hardcoded defaults
        .source(Defaults::from::<AppConfig>())
        // Layer 2: Base configuration (optional)
        .source(Toml::file("config/base.toml").optional())
        // Layer 3: Environment-specific config (optional)
        .source(Toml::file(format!("config/{}.toml", environment)).optional())
        // Layer 4: Environment variables (highest priority)
        .source(Env::new().prefix("APP"))
        .build()
        .unwrap_or_else(|errors| {
            eprintln!("Configuration errors:");
            for error in errors.iter() {
                eprintln!("  - {}", error);
            }
            std::process::exit(1);
        });

    println!("\nFinal configuration:");
    println!("  Host: {}", config.host);
    println!("  Port: {}", config.port);
    println!("  Debug: {}", config.debug);
    println!("  Log Level: {}", config.log_level);
}
```

### Example: Value Tracing

```rust
// examples/tracing/src/main.rs
//! Demonstrates value origin tracking for debugging.

use premortem::{Config, Defaults, DeriveValidate, Env, Toml};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, DeriveValidate, Default)]
struct AppConfig {
    #[serde(default = "default_host")]
    host: String,

    #[serde(default = "default_port")]
    port: u16,

    #[serde(default)]
    debug: bool,
}

fn default_host() -> String {
    "localhost".to_string()
}

fn default_port() -> u16 {
    8080
}

fn main() {
    // Use build_traced() to track value origins
    let traced = Config::<AppConfig>::builder()
        .source(Defaults::from::<AppConfig>())
        .source(Toml::file("config.toml").optional())
        .source(Env::new().prefix("APP"))
        .build_traced()
        .unwrap_or_else(|errors| {
            eprintln!("Configuration errors:");
            for error in errors.iter() {
                eprintln!("  - {}", error);
            }
            std::process::exit(1);
        });

    println!("Configuration loaded successfully!\n");

    // Show where each value came from
    println!("Value origins:");
    for (path, trace) in traced.traces() {
        println!("  {} = {:?}", path, trace.final_value());
        println!("    Source: {}", trace.final_source());
        if trace.was_overridden() {
            println!("    (overridden {} times)", trace.override_count());
        }
    }

    // Get detailed trace for specific path
    if let Some(trace) = traced.trace("port") {
        println!("\nDetailed trace for 'port':");
        for entry in trace.history() {
            println!("  {} -> {:?}", entry.source, entry.value);
        }
    }

    // Get the actual config
    let config = traced.into_inner();
    println!("\nFinal config: {:?}", config);
}
```

### Documentation: PATTERNS.md

```markdown
# Common Configuration Patterns

## Table of Contents
- [Layered Configuration](#layered-configuration)
- [Feature Flags](#feature-flags)
- [Secrets Management](#secrets-management)
- [Nested Configuration](#nested-configuration)
- [Optional Fields](#optional-fields)
- [Default Values](#default-values)
- [Cross-Field Validation](#cross-field-validation)

## Layered Configuration

Load configuration from multiple sources with increasing priority:

\`\`\`rust
let config = Config::<AppConfig>::builder()
    .source(Defaults::from::<AppConfig>())     // Lowest priority
    .source(Toml::file("base.toml").optional())
    .source(Toml::file("local.toml").optional())
    .source(Env::new().prefix("APP"))           // Highest priority
    .build()?;
\`\`\`

## Feature Flags

Use environment variables for feature flags:

\`\`\`rust
#[derive(Deserialize, DeriveValidate)]
struct Features {
    #[serde(default)]
    enable_new_ui: bool,

    #[serde(default)]
    enable_analytics: bool,
}
\`\`\`

## Secrets Management

Never put secrets in config files. Load from environment:

\`\`\`rust
#[derive(Deserialize, DeriveValidate)]
struct Config {
    // Safe to put in files
    database_host: String,

    // MUST come from environment
    #[serde(default)]
    database_password: Option<String>,
}

// Validate secret is present
impl Validate for Config {
    fn validate(&self) -> ConfigValidation<()> {
        if self.database_password.is_none() {
            return Validation::Failure(ConfigErrors::single(
                ConfigError::MissingField {
                    path: "database_password".to_string(),
                    message: "DATABASE_PASSWORD environment variable required".to_string(),
                }
            ));
        }
        Validation::Success(())
    }
}
\`\`\`

## Nested Configuration

Organize related settings in nested structs:

\`\`\`rust
#[derive(Deserialize, DeriveValidate)]
struct AppConfig {
    #[validate(nested)]
    server: ServerConfig,

    #[validate(nested)]
    database: DatabaseConfig,

    #[validate(nested)]
    logging: LogConfig,
}
\`\`\`

TOML:
\`\`\`toml
[server]
host = "0.0.0.0"
port = 8080

[database]
url = "postgres://localhost/myapp"
pool_size = 10

[logging]
level = "info"
format = "json"
\`\`\`

## Optional Fields

Use `Option<T>` with `#[serde(default)]`:

\`\`\`rust
#[derive(Deserialize, DeriveValidate)]
struct Config {
    // Required
    host: String,

    // Optional with None default
    #[serde(default)]
    tls_cert: Option<PathBuf>,

    // Optional nested (validate only if present)
    #[validate(optional_nested)]
    metrics: Option<MetricsConfig>,
}
\`\`\`

## Default Values

Provide defaults with functions:

\`\`\`rust
#[derive(Deserialize, DeriveValidate)]
struct Config {
    #[serde(default = "default_port")]
    port: u16,

    #[serde(default = "default_workers")]
    workers: usize,
}

fn default_port() -> u16 { 8080 }
fn default_workers() -> usize { num_cpus::get() }
\`\`\`

## Cross-Field Validation

Validate relationships between fields:

\`\`\`rust
impl Validate for PoolConfig {
    fn validate(&self) -> ConfigValidation<()> {
        if self.min_size > self.max_size {
            return Validation::Failure(ConfigErrors::single(
                ConfigError::CrossFieldError {
                    paths: vec!["min_size".into(), "max_size".into()],
                    message: "min_size cannot exceed max_size".into(),
                }
            ));
        }
        Validation::Success(())
    }
}
\`\`\`
```

### Documentation: TESTING.md

```markdown
# Testing Configuration

## Using MockEnv

The `MockEnv` type allows testing configuration without real files or environment:

\`\`\`rust
use premortem::{Config, MockEnv, Toml, Env};

#[test]
fn test_config_loading() {
    let env = MockEnv::new()
        .with_file("config.toml", r#"host = "localhost""#)
        .with_env("APP_PORT", "9000");

    let config = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .source(Env::new().prefix("APP"))
        .build_with_env(&env)
        .expect("should load");

    assert_eq!(config.port, 9000);
}
\`\`\`

## Testing Error Cases

\`\`\`rust
#[test]
fn test_missing_required_file() {
    let env = MockEnv::new();

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("missing.toml"))
        .build_with_env(&env);

    assert!(matches!(
        result.unwrap_err().first(),
        ConfigError::SourceError { kind: SourceErrorKind::NotFound { .. }, .. }
    ));
}

#[test]
fn test_permission_denied() {
    let env = MockEnv::new()
        .with_unreadable_file("secret.toml");

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("secret.toml"))
        .build_with_env(&env);

    assert!(matches!(
        result.unwrap_err().first(),
        ConfigError::SourceError { kind: SourceErrorKind::IoError { .. }, .. }
    ));
}
\`\`\`

## Testing Validation

\`\`\`rust
#[test]
fn test_all_errors_accumulated() {
    let env = MockEnv::new().with_file("config.toml", r#"
        host = ""     # Invalid: empty
        port = 0      # Invalid: out of range
    "#);

    let errors = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env)
        .unwrap_err();

    // Premortem collects ALL errors, not just the first
    assert_eq!(errors.len(), 2);
}
\`\`\`

## Property-Based Testing

Use proptest for comprehensive validation testing:

\`\`\`rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn port_validation_works(port in 0u16..=70000) {
        let toml = format!(r#"host = "localhost"\nport = {}"#, port);
        let env = MockEnv::new().with_file("config.toml", &toml);

        let result = Config::<AppConfig>::builder()
            .source(Toml::file("config.toml"))
            .build_with_env(&env);

        if port >= 1 && port <= 65535 {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
        }
    }
}
\`\`\`
```

## Dependencies

- **Prerequisites**: Specs 001, 002, 003, 005, 008, 009
- **Affected Components**: Documentation, examples directory
- **External Dependencies**:
  - `axum` (example dependency only)
  - `tokio` (example dependency only)

## Testing Strategy

- **Documentation Tests**: All code blocks must compile
- **Example Programs**: Each example must `cargo run` successfully
- **CI Integration**: Examples should be built/tested in CI

## Documentation Requirements

- **README.md**: Quick start guide with compelling example
- **PATTERNS.md**: Common configuration patterns
- **TESTING.md**: Testing best practices
- **Rustdoc**: Comprehensive module and type documentation

## Implementation Notes

- Examples should be minimal but complete
- Avoid external service dependencies in examples
- Use MockEnv in all testing documentation
- Show error handling patterns, not just happy paths
- Include comments explaining why, not just what

## Migration and Compatibility

Not applicable - documentation only.
