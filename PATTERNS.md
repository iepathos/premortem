# Premortem Patterns Guide

This guide explains the functional programming patterns used in premortem, all derived from [stillwater](https://github.com/iepathos/stillwater).

## Core Philosophy

Premortem follows stillwater's **"Pure Core, Imperative Shell"** architecture:

```
┌─────────────────────────────────────────────────────────────────┐
│                    IMPERATIVE SHELL (I/O)                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │ TOML Source │  │ Env Source  │  │ Defaults    │              │
│  │  (file I/O) │  │ (env vars)  │  │ (pure)      │              │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘              │
│         │                │                │                      │
│         └────────────────┼────────────────┘                      │
│                          ▼                                       │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                    PURE CORE                             │    │
│  │   merge() ──► deserialize() ──► validate()              │    │
│  │              (all pure functions)                        │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

## Pattern 1: ConfigErrors, Not Vec<ConfigError>

### Why?

`ConfigErrors` wraps `NonEmptyVec<ConfigError>` from stillwater, providing:

1. **Guaranteed non-empty** - If you have errors, you have at least one
2. **Semigroup implementation** - Errors combine via `combine()`
3. **Safe first access** - `errors.first()` returns `&ConfigError`, not `Option`

### Wrong

```rust
// ❌ Can be empty, no Semigroup, awkward API
fn validate(&self) -> Result<(), Vec<ConfigError>>

// ❌ Have to handle empty case
if errors.is_empty() {
    // This shouldn't happen, but we have to check
}
```

### Right

```rust
// ✓ Guaranteed non-empty, implements Semigroup
fn validate(&self) -> ConfigValidation<()>

// ✓ Safe access - always at least one error
let first = errors.first();  // No Option!
```

### Type Aliases

```rust
/// Always use these for consistency:
pub type ConfigValidation<T> = Validation<T, ConfigErrors>;

/// ConfigErrors wraps NonEmptyVec
pub struct ConfigErrors(pub NonEmptyVec<ConfigError>);
```

## Pattern 2: Validation::all() for Error Accumulation

### Why?

Standard `Result` short-circuits on first error. `Validation::all()` runs ALL validations and accumulates ALL errors.

### Wrong

```rust
// ❌ Stops at first error - user fixes one, finds another, repeat
fn validate(&self) -> Result<(), ConfigError> {
    validate_host(&self.host)?;
    validate_port(self.port)?;  // Never runs if host fails
    validate_pool(&self.pool)?;
    Ok(())
}
```

### Right

```rust
// ✓ Collects ALL errors - user sees everything at once
fn validate(&self) -> ConfigValidation<()> {
    Validation::all((
        validate_host(&self.host),
        validate_port(self.port),
        validate_pool(self.pool),
    ))
    .map(|_| ())
}
```

### How Semigroup Enables This

`Validation::all()` uses `Semigroup::combine()` to merge errors:

```rust
// When both fail:
let v1: Validation<A, ConfigErrors> = Validation::Failure(errors1);
let v2: Validation<B, ConfigErrors> = Validation::Failure(errors2);

// Validation::all() calls:
errors1.combine(errors2)  // Returns ConfigErrors with all errors
```

## Pattern 3: Effect for I/O, Pure Functions for Logic

### Why?

Separating I/O from logic enables:
- **Testability** - Mock I/O via ConfigEnv
- **Clarity** - See exactly where side effects happen
- **Composition** - Chain Effects before executing

### I/O Lives in Effect

```rust
// ✓ I/O wrapped in Effect
fn load<E: ConfigEnv>(&self) -> Effect<ConfigValues, ConfigErrors, E> {
    let path = self.path.clone();

    Effect::from_fn(move |env: &E| {
        // I/O happens here, through ConfigEnv
        let content = env.read_file(&path)?;

        // Pure parsing (no I/O)
        parse_toml(&content)
    })
}
```

### Logic Is Pure

```rust
// ✓ No I/O - just data transformation
fn parse_toml(content: &str) -> Result<ConfigValues, ConfigErrors> {
    // Pure parsing, no file access
}

// ✓ No I/O - just validation
fn validate_port(port: u16) -> ConfigValidation<u16> {
    if port == 0 {
        Validation::Failure(ConfigErrors::single(...))
    } else {
        Validation::Success(port)
    }
}
```

## Pattern 4: ConfigEnv for Testable I/O

### Why?

Unit tests shouldn't touch the file system. `ConfigEnv` abstracts I/O for injection.

### Production

```rust
// Uses real file system and environment
let config = Config::<App>::builder()
    .source(Toml::file("config.toml"))
    .source(Env::prefix("APP_"))
    .build();  // Implicitly uses RealEnv
```

### Testing

```rust
#[test]
fn test_config_loading() {
    // Mock environment - no real files!
    let env = MockEnv::new()
        .with_file("config.toml", r#"
            [server]
            port = 8080
        "#)
        .with_env("APP_SERVER_HOST", "localhost");

    let config = Config::<App>::builder()
        .source(Toml::file("config.toml"))
        .source(Env::prefix("APP_"))
        .build_with_env(&env)  // Inject mock
        .unwrap();

    assert_eq!(config.get().server.port, 8080);
}
```

## Pattern 5: traverse() for Collection Validation

### Why?

When validating collections, you want ALL element errors, not just the first.

### Wrong

```rust
// ❌ Stops at first invalid server
for server in &servers {
    server.validate()?;
}
```

### Right

```rust
// ✓ Validates all servers, accumulates all errors
Validation::traverse(
    servers.iter().enumerate(),
    |(i, server)| server.validate_at(&format!("servers[{}]", i))
)
```

### Result

```
Configuration errors (3):

  config.toml:
    • 'servers[0].host' = "": value cannot be empty
    • 'servers[2].port' = 0: value must be >= 1
    • 'servers[4].host' = "": value cannot be empty
```

## Pattern 6: Context for Error Trails

### Why?

Deep call stacks lose context. Add context at each layer.

### Without Context

```
Error: No such file or directory
```

### With Context

```rust
source.load()
    .context("Loading configuration")
    .and_then(|values| parse(values))
    .context("Parsing configuration")
    .and_then(|config| config.validate())
    .context("Validating configuration")
```

Output:
```
Error: No such file or directory
  -> Loading configuration
  -> Parsing configuration
  -> Validating configuration
```

## Pattern 7: Derive Macro Uses Validation::all()

### Generated Code Pattern

```rust
#[derive(Validate)]
struct DatabaseConfig {
    #[validate(non_empty)]
    host: String,

    #[validate(range(1..=65535))]
    port: u16,
}

// Generates:
impl Validate for DatabaseConfig {
    fn validate(&self) -> ConfigValidation<()> {
        Validation::all((
            validate_field(&self.host, "host", &[&NonEmpty]),
            validate_field(&self.port, "port", &[&Range(1..=65535)]),
        ))
        .map(|_| ())
    }
}
```

## Quick Reference

| Stillwater Type | Premortem Usage |
|-----------------|-----------------|
| `Validation<T, E>` | `ConfigValidation<T>` (E = ConfigErrors) |
| `NonEmptyVec<T>` | Wrapped by `ConfigErrors` |
| `Semigroup` | Implemented by `ConfigErrors` for error combination |
| `Effect<T, E, Env>` | Source loading with ConfigEnv injection |
| `Validation::all()` | Combine independent validations |
| `Validation::traverse()` | Validate collections |
| `.context()` | Add error trail context |

## Anti-Patterns to Avoid

### ❌ Using Vec<ConfigError> directly

```rust
// Wrong - loses Semigroup, can be empty
fn load(&self) -> Result<ConfigValues, Vec<ConfigError>>
```

### ❌ Using Result for independent validations

```rust
// Wrong - stops at first error
let host = validate_host(&input.host)?;
let port = validate_port(input.port)?;
```

### ❌ Direct I/O in validation

```rust
// Wrong - can't test without real files
fn validate_path(&self) -> ConfigValidation<()> {
    if std::fs::metadata(&self.path).is_err() {  // Direct I/O!
        Validation::Failure(...)
    }
}
```

### ❌ Ignoring error context

```rust
// Wrong - errors lose origin
source.load().run(&env)?  // No context added
```

## Further Reading

- [stillwater README](https://github.com/iepathos/stillwater/blob/main/README.md) - Core library
- [stillwater PHILOSOPHY](https://github.com/iepathos/stillwater/blob/main/PHILOSOPHY.md) - Design principles
- [Railway Oriented Programming](https://fsharpforfunandprofit.com/rop/) - Error handling pattern
- [Functional Core, Imperative Shell](https://www.destroyallsoftware.com/screencasts/catalog/functional-core-imperative-shell) - Architecture pattern
