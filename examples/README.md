# Premortem Examples

This directory contains runnable examples demonstrating premortem's features.

## Running Examples

Each example is a standalone Rust project. To run an example:

```bash
cd examples/basic
cargo run
```

Or from the workspace root:

```bash
cargo run --example basic
```

## Examples Overview

### [basic](./basic/)

**Minimal configuration loading**

Demonstrates the simplest use case: loading configuration from a TOML file with environment variable overrides and basic validation.

```bash
cd examples/basic
cargo run

# With environment override:
APP_PORT=9000 cargo run
```

### [validation](./validation/)

**Comprehensive validation patterns**

Shows all built-in validators and custom validation patterns including:
- String validators (non_empty, email, url, pattern, length)
- Numeric validators (range, positive, non_zero)
- Collection validators (non_empty_collection, min_items, each)
- Custom validators and cross-field validation

```bash
cd examples/validation
cargo run
```

### [testing](./testing/)

**Configuration testing patterns with MockEnv**

Demonstrates how to test configuration loading without real files or environment variables using the `MockEnv` type.

```bash
cd examples/testing
cargo test
```

### [layered](./layered/)

**Multi-source configuration layering**

Shows environment-specific configuration with multiple layers:
1. Hardcoded defaults (lowest priority)
2. Base configuration file
3. Environment-specific file (development/production)
4. Environment variables (highest priority)

```bash
cd examples/layered

# Development mode (default)
cargo run

# Production mode
APP_ENV=production cargo run
```

### [tracing](./tracing/)

**Value origin debugging**

Demonstrates value tracing to debug where configuration values came from and their override history across sources.

```bash
cd examples/tracing
cargo run
```

## Key Concepts Demonstrated

### Error Accumulation

All examples demonstrate premortem's core feature: collecting ALL configuration errors instead of stopping at the first one.

### Source Layering

Later sources override earlier ones:
```rust
Config::<AppConfig>::builder()
    .source(Defaults::from(AppConfig::default()))  // Lowest priority
    .source(Toml::file("config.toml"))
    .source(Env::prefix("APP_"))                   // Highest priority
    .build()
```

### Testable I/O

All I/O is abstracted through `ConfigEnv`, enabling testing with `MockEnv`:
```rust
let env = MockEnv::new()
    .with_file("config.toml", "port = 8080")
    .with_env("APP_HOST", "localhost");

let config = Config::<AppConfig>::builder()
    .source(Toml::file("config.toml"))
    .source(Env::prefix("APP_"))
    .build_with_env(&env)?;
```

### Validation

Use the `Validate` trait or derive macro for validation:
```rust
#[derive(Deserialize, Validate)]
struct Config {
    #[validate(non_empty)]
    host: String,

    #[validate(range(1..=65535))]
    port: u16,
}
```
