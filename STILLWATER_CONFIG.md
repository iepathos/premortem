# premortem

> Know how your app will die—before it does.

## Overview

`premortem` is a configuration library built on stillwater's Validation philosophy. It loads configuration from multiple sources, merges them by priority, and validates everything—returning **all errors at once** instead of failing on the first problem.

```rust
let config: Validation<AppConfig, Vec<ConfigError>> = Config::builder()
    .source(Defaults::new())
    .source(Toml::file("config.toml").optional())
    .source(Env::prefix("APP_"))
    .build();

match config {
    Validation::Success(cfg) => run(cfg),
    Validation::Failure(errors) => {
        for e in &errors {
            eprintln!("  {}", e);
        }
        std::process::exit(1);
    }
}
```

## The Problem

### Current State of Rust Configuration

Existing libraries (figment, config-rs) handle multi-source loading well, but validation is an afterthought:

```
$ ./myapp
Error: missing field `database.host`

$ # fix it, try again
$ ./myapp
Error: invalid type for `database.port`: expected integer, found string

$ # fix it, try again
$ ./myapp
Error: `database.pool_size` must be positive

$ # three round trips to find three errors
```

This is a **postmortem** experience—you discover each cause of death one at a time, after the app dies.

### What We Want

```
$ ./myapp
Configuration errors (4):
  [config.toml:8] missing required field 'database.host'
  [env:APP_DATABASE_PORT] value "abc" is not a valid integer
  [config.toml:10] 'database.pool_size' value -5 must be >= 1
  [config.toml:15] 'cache.ttl_seconds' required when 'cache.enabled' is true
```

One run. All errors. Clear sources. A **premortem**—know how your app would die before it does.

## Core Principles

### 1. Accumulate All Errors

Never stop at the first error. Configuration problems tend to cluster—if someone misconfigured the database section, they probably also misconfigured other sections. Show everything.

### 2. Trace Value Origins

Every value should know where it came from. When debugging "why is my app connecting to the wrong database?", you need to know which source provided that value.

### 3. Validate Holistically

Validation isn't just "does this parse?" It's:
- Type validation (is it an integer?)
- Range validation (is it between 1 and 65535?)
- Format validation (is it a valid URL?)
- Cross-field validation (if X is set, Y must also be set)
- Business rule validation (replica can't equal primary)

### 4. Fail Fast at Startup

Configuration errors should crash the app immediately with a helpful message. Don't let misconfiguration surface as mysterious runtime errors later.

## API Design

### Defining Configuration

```rust
use premortem::Validate;
use std::net::SocketAddr;
use url::Url;

#[derive(Debug, Clone, Validate, Deserialize)]
pub struct AppConfig {
    /// Server configuration
    #[validate(nested)]
    pub server: ServerConfig,

    /// Database configuration
    #[validate(nested)]
    pub database: DatabaseConfig,

    /// Optional cache configuration
    #[validate(nested)]
    pub cache: Option<CacheConfig>,

    /// Feature flags
    #[validate(nested)]
    pub features: FeatureFlags,
}

#[derive(Debug, Clone, Validate, Deserialize)]
pub struct ServerConfig {
    /// Address to bind to
    #[validate(message = "Invalid socket address")]
    pub bind: SocketAddr,

    /// Request timeout in seconds
    #[validate(range(1..=300), message = "Timeout must be 1-300 seconds")]
    pub timeout_seconds: u32,

    /// Maximum request body size in bytes
    #[validate(range(1024..=104857600), message = "Body size must be 1KB-100MB")]
    pub max_body_size: usize,

    /// TLS configuration (optional)
    #[validate(nested)]
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, Validate, Deserialize)]
pub struct TlsConfig {
    /// Path to certificate file
    #[validate(file_exists, message = "Certificate file not found")]
    pub cert_path: PathBuf,

    /// Path to private key file
    #[validate(file_exists, message = "Key file not found")]
    pub key_path: PathBuf,
}

#[derive(Debug, Clone, Validate, Deserialize)]
pub struct DatabaseConfig {
    /// Database host
    #[validate(non_empty, message = "Database host is required")]
    pub host: String,

    /// Database port
    #[validate(range(1..=65535), message = "Port must be 1-65535")]
    pub port: u16,

    /// Database name
    #[validate(non_empty, message = "Database name is required")]
    pub name: String,

    /// Connection pool size
    #[validate(range(1..=100), message = "Pool size must be 1-100")]
    pub pool_size: u32,

    /// Connection timeout in seconds
    #[validate(range(1..=60), message = "Connection timeout must be 1-60 seconds")]
    pub connect_timeout_seconds: u32,

    /// Optional read replica
    #[validate(nested)]
    pub replica: Option<ReplicaConfig>,
}

#[derive(Debug, Clone, Validate, Deserialize)]
#[validate(custom = "validate_replica")]
pub struct ReplicaConfig {
    pub host: String,
    pub port: u16,

    /// Percentage of reads to send to replica (0-100)
    #[validate(range(0..=100))]
    pub read_percent: u8,
}

fn validate_replica(cfg: &ReplicaConfig, parent: &DatabaseConfig) -> Validation<(), String> {
    if cfg.host == parent.host && cfg.port == parent.port {
        Validation::fail("Replica cannot be the same as primary database")
    } else {
        Validation::success(())
    }
}

#[derive(Debug, Clone, Validate, Deserialize)]
#[validate(custom = "validate_cache")]
pub struct CacheConfig {
    /// Enable caching
    pub enabled: bool,

    /// Cache backend URL (redis://... or memory://)
    #[validate(url)]
    pub backend_url: Option<Url>,

    /// Default TTL in seconds
    #[validate(range(1..=86400))]
    pub default_ttl_seconds: Option<u32>,

    /// Maximum cache entries (for memory backend)
    #[validate(range(100..=1000000))]
    pub max_entries: Option<usize>,
}

fn validate_cache(cfg: &CacheConfig) -> Validation<(), Vec<String>> {
    let mut errors = vec![];

    if cfg.enabled {
        if cfg.backend_url.is_none() {
            errors.push("'backend_url' is required when cache is enabled".into());
        }
        if cfg.default_ttl_seconds.is_none() {
            errors.push("'default_ttl_seconds' is required when cache is enabled".into());
        }
    }

    if let Some(url) = &cfg.backend_url {
        if url.scheme() == "memory" && cfg.max_entries.is_none() {
            errors.push("'max_entries' is required for memory cache backend".into());
        }
    }

    Validation::from_errors(errors)
}

#[derive(Debug, Clone, Validate, Deserialize)]
pub struct FeatureFlags {
    /// Enable experimental API endpoints
    pub experimental_api: bool,

    /// Enable detailed request logging
    pub verbose_logging: bool,

    /// Enable metrics collection
    pub metrics_enabled: bool,

    /// Metrics endpoint (required if metrics enabled)
    #[validate(url, when = "self.metrics_enabled")]
    pub metrics_endpoint: Option<Url>,
}
```

### Loading Configuration

```rust
use premortem::{Config, Env, Toml, Json, Yaml, Defaults};

// Simple case: single file + environment
let config = Config::<AppConfig>::builder()
    .source(Toml::file("config.toml"))
    .source(Env::prefix("APP_"))
    .build()?;

// Full case: multiple sources with priority
let config = Config::<AppConfig>::builder()
    // Lowest priority: hardcoded defaults
    .source(Defaults::from(AppConfig::default()))

    // Base configuration file
    .source(Toml::file("config/base.toml"))

    // Environment-specific overrides
    .source(Toml::file(format!("config/{}.toml", env)).optional())

    // Local developer overrides (not committed to git)
    .source(Toml::file("config/local.toml").optional())

    // Environment variables override everything
    .source(Env::prefix("APP_"))

    // CLI arguments have highest priority
    .source(CliArgs::from(args))

    .build();
```

### Source Types

#### File Sources

```rust
// TOML files
Toml::file("config.toml")                    // Required file
Toml::file("config.toml").optional()         // Optional file
Toml::file("config.toml").required()         // Explicit required (default)
Toml::string(toml_content)                   // From string

// JSON files
Json::file("config.json")
Json::string(json_content)

// YAML files
Yaml::file("config.yaml")
Yaml::string(yaml_content)

// Auto-detect by extension
File::auto("config.toml")  // Picks parser by extension
```

#### Environment Variables

```rust
// With prefix (recommended)
Env::prefix("APP_")
// APP_SERVER_BIND -> server.bind
// APP_DATABASE_HOST -> database.host
// APP_DATABASE_POOL_SIZE -> database.pool_size

// Custom mapping
Env::prefix("APP_")
    .map("DB_HOST", "database.host")     // Override specific mappings
    .map("DB_PORT", "database.port")
    .separator("__")                      // APP__SERVER__BIND instead of APP_SERVER_BIND

// Case sensitivity
Env::prefix("APP_").case_sensitive()      // Exact match only
Env::prefix("APP_").case_insensitive()    // APP_db_host matches (default)

// List handling
Env::prefix("APP_")
    .list_separator(",")                  // APP_ALLOWED_HOSTS=a.com,b.com -> ["a.com", "b.com"]
```

#### Defaults

```rust
// From Default trait
Defaults::from(AppConfig::default())

// From closure
Defaults::from_fn(|| AppConfig {
    server: ServerConfig {
        bind: "127.0.0.1:8080".parse().unwrap(),
        timeout_seconds: 30,
        ..Default::default()
    },
    ..Default::default()
})

// Partial defaults (specific paths only)
Defaults::partial()
    .set("server.timeout_seconds", 30)
    .set("database.pool_size", 10)
```

#### CLI Arguments

```rust
// Integration with clap
#[derive(Parser)]
struct Args {
    #[arg(long)]
    config: Option<PathBuf>,

    #[arg(long)]
    db_host: Option<String>,

    #[arg(long)]
    verbose: bool,
}

CliArgs::from(args)
    .map("db_host", "database.host")
    .map("verbose", "features.verbose_logging")

// Or automatic mapping
CliArgs::from(args).auto_map()  // --db-host -> database.host
```

#### Remote Sources (Optional Feature)

```rust
// Consul
Consul::new("http://localhost:8500")
    .path("myapp/config")
    .token(consul_token)

// etcd
Etcd::new("http://localhost:2379")
    .prefix("/config/myapp/")

// AWS Parameter Store
AwsParameterStore::new()
    .path("/myapp/production/")
    .region("us-east-1")

// Vault
Vault::new("https://vault.example.com")
    .path("secret/data/myapp")
    .token(vault_token)

// HTTP endpoint
Http::get("https://config.example.com/myapp.json")
    .header("Authorization", format!("Bearer {}", token))
    .timeout(Duration::from_secs(5))
```

### Build Options

```rust
let config = Config::<AppConfig>::builder()
    .source(Toml::file("config.toml"))
    .source(Env::prefix("APP_"))

    // Validation options
    .validate(true)                          // Enable validation (default)
    .validate(false)                         // Skip validation (not recommended)

    // Error handling
    .on_missing_field(MissingField::Error)   // Error on missing required (default)
    .on_missing_field(MissingField::UseDefault) // Use Default::default()

    // Unknown fields
    .on_unknown_field(UnknownField::Ignore)  // Ignore unknown fields (default)
    .on_unknown_field(UnknownField::Warn)    // Log warning
    .on_unknown_field(UnknownField::Error)   // Treat as error

    // Secret redaction in errors
    .redact_secrets(true)                    // Hide values of fields marked #[sensitive]

    .build();
```

### Traced Values

```rust
// Build with tracing to see where values came from
let traced = Config::<AppConfig>::builder()
    .source(Toml::file("config.toml"))
    .source(Env::prefix("APP_"))
    .build_traced()?;

// Get the final config
let config: &AppConfig = traced.value();

// Trace a specific field
let trace = traced.trace("database.host");
println!("{}", trace);
// database.host = "prod-db.example.com"
//   [config.toml:12] "localhost"           <- overridden
//   [env:APP_DATABASE_HOST] "prod-db.example.com"  <- final value

// Trace all values
for (path, trace) in traced.traces() {
    if trace.was_overridden() {
        println!("{}: {} sources", path, trace.sources().len());
    }
}

// Export trace for debugging
let report = traced.trace_report();
std::fs::write("config-trace.txt", report)?;
```

## Error Types

### ConfigError

```rust
#[derive(Debug, Clone)]
pub enum ConfigError {
    /// Source failed to load
    SourceError {
        source_name: String,
        error: SourceErrorKind,
    },

    /// Field failed to parse
    ParseError {
        path: String,
        source_location: SourceLocation,
        expected_type: String,
        actual_value: String,
        error: String,
    },

    /// Required field is missing
    MissingField {
        path: String,
        searched_sources: Vec<String>,
    },

    /// Validation failed
    ValidationError {
        path: String,
        source_location: Option<SourceLocation>,
        value: Option<String>,  // Redacted if #[sensitive]
        message: String,
    },

    /// Cross-field validation failed
    CrossFieldError {
        paths: Vec<String>,
        message: String,
    },

    /// Unknown field (if on_unknown_field is Error)
    UnknownField {
        path: String,
        source_location: SourceLocation,
        did_you_mean: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct SourceLocation {
    pub source: String,      // "config.toml", "env:APP_FOO", etc.
    pub line: Option<u32>,   // Line number if applicable
    pub column: Option<u32>, // Column if applicable
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingField { path, searched_sources } => {
                write!(f, "missing required field '{}' (searched: {})",
                    path,
                    searched_sources.join(", "))
            }
            ConfigError::ParseError { path, source_location, expected_type, actual_value, error } => {
                write!(f, "[{}] '{}' expected {}, got \"{}\": {}",
                    source_location, path, expected_type, actual_value, error)
            }
            ConfigError::ValidationError { path, source_location, message, .. } => {
                match source_location {
                    Some(loc) => write!(f, "[{}] '{}': {}", loc, path, message),
                    None => write!(f, "'{}': {}", path, message),
                }
            }
            // ... etc
        }
    }
}
```

### Error Reporting

```rust
// Pretty-print all errors
fn report_errors(errors: &[ConfigError]) {
    eprintln!("Configuration errors ({}):\n", errors.len());

    // Group by source
    let by_source = group_by_source(errors);

    for (source, errs) in by_source {
        eprintln!("  {}:", source);
        for e in errs {
            eprintln!("    - {}", e.message());
        }
        eprintln!();
    }

    // Suggest fixes
    for error in errors {
        if let Some(suggestion) = error.suggestion() {
            eprintln!("hint: {}", suggestion);
        }
    }
}

// Built-in reporter
let config = Config::<AppConfig>::builder()
    .source(Toml::file("config.toml"))
    .build()
    .unwrap_or_else(|errors| {
        ConfigError::pretty_print(&errors, PrettyPrintOptions {
            color: true,
            group_by_source: true,
            show_suggestions: true,
            max_errors: Some(20),
        });
        std::process::exit(1);
    });
```

## Hot Reload (Optional Feature)

```rust
use premortem::{Config, Watcher};

// Create config with watcher
let (config, mut watcher) = Config::<AppConfig>::builder()
    .source(Toml::file("config.toml"))
    .source(Env::prefix("APP_"))
    .build_watched()?;

// Get current config (Arc for cheap cloning)
let current: Arc<AppConfig> = config.current();

// Subscribe to changes
watcher.on_change(|event| {
    match event {
        ConfigEvent::Reloaded(new_config) => {
            println!("Config reloaded successfully");
            // Update your app's config reference
        }
        ConfigEvent::ReloadFailed(errors) => {
            // Validation failed - keep using old config
            eprintln!("Config reload failed:");
            for e in errors {
                eprintln!("  {}", e);
            }
        }
        ConfigEvent::SourceChanged(source) => {
            println!("Source changed: {}", source);
        }
    }
});

// Manual reload
watcher.reload()?;

// Graceful shutdown
watcher.stop();
```

## Derive Macro Details

### Basic Validation Attributes

```rust
#[derive(Validate)]
struct Config {
    // String validations
    #[validate(non_empty)]
    #[validate(min_length(3))]
    #[validate(max_length(100))]
    #[validate(length(3..=100))]          // Range
    #[validate(pattern(r"^[a-z]+$"))]     // Regex
    #[validate(email)]
    #[validate(url)]
    #[validate(ip)]
    #[validate(uuid)]
    name: String,

    // Numeric validations
    #[validate(range(1..=100))]
    #[validate(positive)]
    #[validate(negative)]
    #[validate(non_zero)]
    count: i32,

    // Collection validations
    #[validate(non_empty)]
    #[validate(min_length(1))]
    #[validate(max_length(10))]
    #[validate(each(non_empty))]          // Validate each element
    items: Vec<String>,

    // Path validations
    #[validate(file_exists)]
    #[validate(dir_exists)]
    #[validate(parent_exists)]
    #[validate(extension("toml"))]
    config_path: PathBuf,

    // Custom validation
    #[validate(custom = "validate_port")]
    port: u16,

    // Conditional validation
    #[validate(url, when = "self.use_remote")]
    remote_url: Option<String>,

    use_remote: bool,

    // Nested structs
    #[validate(nested)]
    database: DatabaseConfig,

    // Skip validation for this field
    #[validate(skip)]
    internal_id: String,

    // Custom error message
    #[validate(range(1..=65535), message = "Port must be between 1 and 65535")]
    server_port: u16,

    // Mark as sensitive (redact in error messages)
    #[sensitive]
    #[validate(min_length(16))]
    api_key: String,
}
```

### Struct-Level Validation

```rust
#[derive(Validate)]
#[validate(custom = "validate_config")]
struct Config {
    primary: String,
    backup: Option<String>,
    use_backup: bool,
}

fn validate_config(cfg: &Config) -> Validation<(), Vec<String>> {
    let mut errors = vec![];

    if cfg.use_backup && cfg.backup.is_none() {
        errors.push("'backup' is required when 'use_backup' is true".into());
    }

    if let Some(backup) = &cfg.backup {
        if backup == &cfg.primary {
            errors.push("'backup' cannot be the same as 'primary'".into());
        }
    }

    Validation::from_errors(errors)
}
```

### Generated Code

The `#[derive(Validate)]` macro generates:

```rust
impl Validate for Config {
    fn validate(&self) -> Validation<(), Vec<ConfigError>> {
        Validation::all((
            validate_field(&self.name, "name", &[
                Validator::NonEmpty,
                Validator::Length(3..=100),
            ]),
            validate_field(&self.count, "count", &[
                Validator::Range(1..=100),
            ]),
            validate_nested(&self.database, "database"),
            // ... etc
        ))
        .and_then(|_| validate_config(self))  // Struct-level validation
    }
}
```

## Integration Examples

### With Axum

```rust
use axum::{Router, Extension};
use premortem::Config;

#[tokio::main]
async fn main() {
    // Load config at startup
    let config = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .source(Env::prefix("APP_"))
        .build()
        .unwrap_or_else(|errors| {
            ConfigError::pretty_print(&errors, Default::default());
            std::process::exit(1);
        });

    let config = Arc::new(config);

    let app = Router::new()
        .route("/health", get(health))
        .layer(Extension(config.clone()));

    let addr = config.server.bind;
    println!("Listening on {}", addr);

    axum::serve(
        TcpListener::bind(addr).await.unwrap(),
        app
    ).await.unwrap();
}

async fn health(Extension(config): Extension<Arc<AppConfig>>) -> &'static str {
    "ok"
}
```

### With Clap

```rust
use clap::Parser;
use premortem::{Config, Toml, Env, CliArgs};

#[derive(Parser)]
struct Args {
    /// Config file path
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Override database host
    #[arg(long)]
    db_host: Option<String>,

    /// Override database port
    #[arg(long)]
    db_port: Option<u16>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();

    let config = Config::<AppConfig>::builder()
        .source(Toml::file(&args.config))
        .source(Env::prefix("APP_"))
        .source(CliArgs::from(&args)
            .map("db_host", "database.host")
            .map("db_port", "database.port")
            .map("verbose", "features.verbose_logging"))
        .build()
        .unwrap_or_else(|errors| {
            ConfigError::pretty_print(&errors, Default::default());
            std::process::exit(1);
        });

    run(config);
}
```

### Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use premortem::{Config, Defaults};

    fn test_config() -> AppConfig {
        AppConfig {
            server: ServerConfig {
                bind: "127.0.0.1:8080".parse().unwrap(),
                timeout_seconds: 30,
                max_body_size: 1024 * 1024,
                tls: None,
            },
            database: DatabaseConfig {
                host: "localhost".into(),
                port: 5432,
                name: "test".into(),
                pool_size: 5,
                connect_timeout_seconds: 5,
                replica: None,
            },
            cache: None,
            features: FeatureFlags {
                experimental_api: false,
                verbose_logging: false,
                metrics_enabled: false,
                metrics_endpoint: None,
            },
        }
    }

    #[test]
    fn test_valid_config() {
        let result = Config::<AppConfig>::builder()
            .source(Defaults::from(test_config()))
            .build();

        assert!(result.is_success());
    }

    #[test]
    fn test_invalid_pool_size() {
        let mut cfg = test_config();
        cfg.database.pool_size = 0;  // Invalid: must be >= 1

        let result = Config::<AppConfig>::builder()
            .source(Defaults::from(cfg))
            .build();

        assert!(result.is_failure());
        let errors = result.unwrap_failure();
        assert!(errors.iter().any(|e|
            e.path() == Some("database.pool_size")
        ));
    }

    #[test]
    fn test_cross_field_validation() {
        let mut cfg = test_config();
        cfg.features.metrics_enabled = true;
        cfg.features.metrics_endpoint = None;  // Required when metrics_enabled

        let result = Config::<AppConfig>::builder()
            .source(Defaults::from(cfg))
            .build();

        assert!(result.is_failure());
    }

    #[test]
    fn test_env_override() {
        std::env::set_var("TEST_APP_DATABASE_HOST", "prod-db");

        let result = Config::<AppConfig>::builder()
            .source(Defaults::from(test_config()))
            .source(Env::prefix("TEST_APP_"))
            .build();

        std::env::remove_var("TEST_APP_DATABASE_HOST");

        let config = result.unwrap();
        assert_eq!(config.database.host, "prod-db");
    }
}
```

## Feature Flags

```toml
[dependencies]
premortem = "0.1"

# Optional features
premortem = { version = "0.1", features = ["toml"] }        # TOML support (default)
premortem = { version = "0.1", features = ["json"] }        # JSON support
premortem = { version = "0.1", features = ["yaml"] }        # YAML support
premortem = { version = "0.1", features = ["watch"] }       # Hot reload
premortem = { version = "0.1", features = ["remote"] }      # Remote sources
premortem = { version = "0.1", features = ["full"] }        # Everything

[features]
default = ["toml"]
toml = ["dep:toml"]
json = ["dep:serde_json"]
yaml = ["dep:serde_yaml"]
watch = ["dep:notify"]
remote = ["dep:reqwest", "dep:tokio"]
full = ["toml", "json", "yaml", "watch", "remote"]
```

## Comparison with Alternatives

| Feature | premortem | figment | config-rs |
|---------|-----------|---------|-----------|
| Multi-source | ✓ | ✓ | ✓ |
| Type-safe | ✓ | ✓ | ✓ |
| **All errors at once** | ✓ | ✗ | ✗ |
| **Value tracing** | ✓ | Partial | ✗ |
| **Cross-field validation** | ✓ | Manual | Manual |
| **Conditional validation** | ✓ | ✗ | ✗ |
| **Derive macro** | ✓ | ✗ | ✗ |
| Hot reload | ✓ | ✗ | ✗ |
| Remote sources | ✓ | ✗ | ✗ |
| Error suggestions | ✓ | ✗ | ✗ |
| Async | ✓ | ✓ | ✓ |

## Implementation Plan

### Phase 1: Core (MVP)
- [ ] Basic source loading (Toml, Env)
- [ ] Source merging with priority
- [ ] Integration with stillwater Validation
- [ ] Basic derive macro (non_empty, range)
- [ ] Error types with source location

### Phase 2: Validation
- [ ] Full derive macro (all validators)
- [ ] Conditional validation (#[validate(when = ...)])
- [ ] Cross-field validation
- [ ] Custom validators
- [ ] Nested struct validation

### Phase 3: Developer Experience
- [ ] Pretty error printing
- [ ] Value tracing
- [ ] "Did you mean?" suggestions
- [ ] Sensitive field redaction

### Phase 4: Extended Sources
- [ ] JSON support
- [ ] YAML support
- [ ] CLI args integration
- [ ] Remote sources (feature-gated)

### Phase 5: Advanced
- [ ] Hot reload / file watching
- [ ] Schema generation (JSON Schema)
- [ ] Documentation generation

## Open Questions

1. **Should this be a separate crate or a feature of stillwater?**
   - Separate crate: cleaner deps, independent versioning
   - Feature: easier adoption, single import
   - **Recommendation**: Separate crate, stillwater as dependency

2. **How to handle secrets?**
   - Mark with `#[sensitive]` - redact in errors
   - Integration with vault/AWS SSM as sources
   - Never log or display in traces

3. **Async loading?**
   - Remote sources need async
   - File sources could be sync
   - **Recommendation**: Async-first with sync wrappers

4. **Schema generation?**
   - Generate JSON Schema from Validate derive
   - Useful for documentation, IDE support
   - **Recommendation**: Phase 5 feature

---

*"Know how your app will die—before it does."*
