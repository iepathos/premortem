# Common Configuration Patterns

This guide covers common patterns for structuring and managing configuration with premortem.

## Table of Contents

- [Layered Configuration](#layered-configuration)
- [Feature Flags](#feature-flags)
- [Secrets Management](#secrets-management)
- [Nested Configuration](#nested-configuration)
- [Optional Fields](#optional-fields)
- [Default Values](#default-values)
- [Cross-Field Validation](#cross-field-validation)
- [Environment-Specific Config](#environment-specific-config)

## Layered Configuration

Load configuration from multiple sources with increasing priority:

```rust
use premortem::prelude::*;

let config = Config::<AppConfig>::builder()
    .source(Defaults::from(AppConfig::default()))     // Lowest priority
    .source(Toml::file("base.toml").optional())
    .source(Toml::file("local.toml").optional())
    .source(Env::prefix("APP_"))                      // Highest priority
    .build()?;
```

**Why this order?**
- Defaults ensure the app always has valid values
- Base config provides environment-agnostic settings
- Local config allows developer-specific overrides (add to .gitignore)
- Environment variables for deployment-time configuration

## Feature Flags

Use environment variables for runtime feature flags:

```rust
use premortem::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize, Validate)]
struct Features {
    #[serde(default)]
    enable_new_ui: bool,

    #[serde(default)]
    enable_analytics: bool,

    #[serde(default)]
    enable_beta_features: bool,
}

#[derive(Debug, Deserialize, Validate)]
struct AppConfig {
    #[validate(nested)]
    features: Features,
}
```

Enable with: `APP_FEATURES_ENABLE_NEW_UI=true`

## Secrets Management

**Never put secrets in config files.** Load from environment:

```rust
use premortem::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    // Safe to put in files
    database_host: String,
    database_port: u16,

    // MUST come from environment
    #[serde(default)]
    database_password: Option<String>,
}

impl Validate for Config {
    fn validate(&self) -> ConfigValidation<()> {
        if self.database_password.is_none() {
            Validation::Failure(ConfigErrors::single(
                ConfigError::MissingField {
                    path: "database_password".to_string(),
                    searched_sources: vec!["environment".to_string()],
                }
            ))
        } else {
            Validation::Success(())
        }
    }
}
```

**Best practices:**
- Exclude secrets from config files
- Use `Option<String>` with validation to require at runtime
- Consider using a secrets manager (Vault, AWS Secrets Manager)
- Use `Env::prefix("APP_").exclude("APP_DATABASE_PASSWORD")` in non-production

## Nested Configuration

Organize related settings in nested structs for clarity:

```rust
use premortem::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize, Validate)]
struct ServerConfig {
    #[validate(non_empty)]
    host: String,

    #[validate(range(1..=65535))]
    port: u16,

    #[validate(range(1..=10000))]
    max_connections: u32,
}

#[derive(Debug, Deserialize, Validate)]
struct DatabaseConfig {
    #[validate(non_empty)]
    url: String,

    #[validate(range(1..=100))]
    pool_size: u32,

    #[serde(default = "default_timeout")]
    timeout_secs: u64,
}

fn default_timeout() -> u64 { 30 }

#[derive(Debug, Deserialize, Validate)]
struct LogConfig {
    #[serde(default = "default_level")]
    level: String,

    #[serde(default)]
    json_format: bool,
}

fn default_level() -> String { "info".to_string() }

#[derive(Debug, Deserialize, Validate)]
struct AppConfig {
    #[validate(nested)]
    server: ServerConfig,

    #[validate(nested)]
    database: DatabaseConfig,

    #[validate(nested)]
    logging: LogConfig,
}
```

TOML representation:
```toml
[server]
host = "0.0.0.0"
port = 8080
max_connections = 1000

[database]
url = "postgres://localhost/myapp"
pool_size = 10

[logging]
level = "info"
json_format = false
```

## Optional Fields

Use `Option<T>` with `#[serde(default)]` for truly optional settings:

```rust
use premortem::prelude::*;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Validate)]
struct Config {
    // Required - will error if missing
    host: String,

    // Optional with None default
    #[serde(default)]
    tls_cert: Option<PathBuf>,

    // Optional with specific default
    #[serde(default = "default_timeout")]
    timeout: u64,
}

fn default_timeout() -> u64 { 30 }
```

For optional nested structs that should only be validated when present:

```rust
#[derive(Debug, Deserialize, Validate)]
struct MetricsConfig {
    #[validate(non_empty)]
    endpoint: String,

    #[validate(range(1..=3600))]
    interval_secs: u64,
}

#[derive(Debug, Deserialize, Validate)]
struct Config {
    // Only validated if Some
    #[validate(optional_nested)]
    metrics: Option<MetricsConfig>,
}
```

## Default Values

Provide defaults with functions for complex initialization:

```rust
use premortem::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
struct Config {
    #[serde(default = "default_port")]
    port: u16,

    #[serde(default = "default_workers")]
    workers: usize,

    #[serde(default = "default_data_dir")]
    data_dir: String,
}

fn default_port() -> u16 { 8080 }

fn default_workers() -> usize {
    // Use available CPU cores
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
}

fn default_data_dir() -> String {
    // Use platform-specific data directory
    dirs::data_dir()
        .map(|p| p.join("myapp").to_string_lossy().to_string())
        .unwrap_or_else(|| "/var/lib/myapp".to_string())
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: default_port(),
            workers: default_workers(),
            data_dir: default_data_dir(),
        }
    }
}
```

Use with `Defaults::from()`:

```rust
let config = Config::<Config>::builder()
    .source(Defaults::from(Config::default()))
    .source(Toml::file("config.toml").optional())
    .source(Env::prefix("APP_"))
    .build()?;
```

## Cross-Field Validation

Validate relationships between fields by implementing `Validate` manually:

```rust
use premortem::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct PoolConfig {
    min_size: u32,
    max_size: u32,
    timeout_secs: u64,
}

impl Validate for PoolConfig {
    fn validate(&self) -> ConfigValidation<()> {
        let mut errors = Vec::new();

        // Cross-field validation
        if self.min_size > self.max_size {
            errors.push(ConfigError::CrossFieldError {
                paths: vec!["min_size".to_string(), "max_size".to_string()],
                message: "min_size cannot exceed max_size".to_string(),
            });
        }

        // Can still do field-level validation
        if self.timeout_secs == 0 {
            errors.push(ConfigError::ValidationError {
                path: "timeout_secs".to_string(),
                source_location: None,
                value: Some("0".to_string()),
                message: "timeout must be positive".to_string(),
            });
        }

        match ConfigErrors::from_vec(errors) {
            Some(errs) => Validation::Failure(errs),
            None => Validation::Success(()),
        }
    }
}
```

## Environment-Specific Config

Use environment detection to load appropriate config files:

```rust
use premortem::prelude::*;

fn load_config() -> Result<Config<AppConfig>, ConfigErrors> {
    // Get environment from APP_ENV, default to development
    let env = std::env::var("APP_ENV")
        .unwrap_or_else(|_| "development".to_string());

    Config::<AppConfig>::builder()
        // Base config shared by all environments
        .source(Toml::file("config/base.toml").optional())
        // Environment-specific overrides
        .source(Toml::file(format!("config/{}.toml", env)).optional())
        // Local overrides (gitignored)
        .source(Toml::file("config/local.toml").optional())
        // Environment variables always win
        .source(Env::prefix("APP_"))
        .build()
}
```

Directory structure:
```
config/
├── base.toml        # Shared settings
├── development.toml # Dev-specific (debug=true, etc.)
├── production.toml  # Prod-specific (optimized settings)
├── staging.toml     # Staging-specific
└── local.toml       # Local overrides (in .gitignore)
```

## Partial Defaults

Use `Defaults::partial()` when you only want defaults for specific paths:

```rust
use premortem::prelude::*;

let config = Config::<AppConfig>::builder()
    .source(Defaults::partial()
        .set("server.timeout_secs", 30i64)
        .set("database.pool_size", 10i64)
        .set("cache.enabled", false))
    .source(Toml::file("config.toml"))
    .source(Env::prefix("APP_"))
    .build()?;
```

This is useful when:
- You don't want to define a full `Default` impl
- You want different defaults for specific deployment scenarios
- You're building configuration programmatically
