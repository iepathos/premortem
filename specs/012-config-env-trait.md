---
number: 12
title: ConfigEnv Trait for Testable I/O
category: foundation
priority: high
status: draft
dependencies: [1, 2]
created: 2025-11-25
---

# Specification 012: ConfigEnv Trait for Testable I/O

**Category**: foundation
**Priority**: high
**Status**: draft
**Dependencies**: [001 - Core Config Builder, 002 - Error Types]

## Context

Stillwater's `Effect<T, E, Env>` pattern separates I/O from pure logic by injecting an environment at runtime. Currently, premortem specs use `Effect<..., ()>` (unit environment), which means I/O operations happen directly inside effects and cannot be mocked for testing.

By defining a `ConfigEnv` trait, we enable:
1. **Testable sources** - Mock file systems and environment variables
2. **Proper Effect usage** - Environment injection as stillwater intended
3. **Consistent I/O boundary** - All I/O goes through the trait

### Stillwater Philosophy Alignment

This follows stillwater's **"Pure Core, Imperative Shell"** pattern more faithfully:

```
┌─────────────────────────────────────────────────────────────────┐
│  IMPERATIVE SHELL (ConfigEnv provides I/O)                      │
│                                                                  │
│   ConfigEnv::read_file()   ConfigEnv::get_env()                 │
│          │                        │                              │
│          └────────────┬───────────┘                              │
│                       ▼                                          │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                    PURE CORE                             │    │
│  │   parse_toml()  validate()  merge()  deserialize()      │    │
│  │              (all pure functions)                        │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

## Objective

Define a `ConfigEnv` trait that abstracts file system and environment variable access, enabling dependency injection for testing and proper Effect-based I/O handling.

## Requirements

### Functional Requirements

1. **ConfigEnv Trait**: Abstract file and environment access
2. **RealEnv Implementation**: Production implementation using std
3. **MockEnv Implementation**: Test implementation with in-memory state
4. **Effect Integration**: Sources use `Effect<T, E, impl ConfigEnv>`
5. **Builder Integration**: ConfigBuilder accepts environment parameter

### Non-Functional Requirements

- Zero overhead for production (RealEnv is zero-cost)
- Ergonomic testing API for MockEnv
- Thread-safe implementations
- No breaking change to basic API (default to RealEnv)

## Acceptance Criteria

- [ ] `ConfigEnv` trait defined with file and env var methods
- [ ] `RealEnv` struct implements `ConfigEnv` using std
- [ ] `MockEnv` struct implements `ConfigEnv` with builder pattern
- [ ] `Source::load()` signature uses `ConfigEnv`
- [ ] `ConfigBuilder::build()` uses `RealEnv` by default
- [ ] `ConfigBuilder::build_with_env(env)` accepts custom environment
- [ ] Unit tests demonstrate mocking file content
- [ ] Unit tests demonstrate mocking environment variables
- [ ] Integration tests use MockEnv for deterministic testing

## Technical Details

### ConfigEnv Trait

```rust
use std::io;
use std::path::Path;

/// Environment trait for configuration I/O operations.
///
/// This trait abstracts file system and environment variable access,
/// enabling dependency injection for testing.
///
/// # Stillwater Integration
///
/// Used as the `Env` parameter in `Effect<T, E, Env>`:
///
/// ```rust
/// fn load(&self) -> Effect<ConfigValues, ConfigErrors, impl ConfigEnv>
/// ```
///
/// # Example
///
/// ```rust
/// // Production
/// let config = Config::<App>::builder()
///     .source(Toml::file("config.toml"))
///     .build();  // Uses RealEnv
///
/// // Testing
/// let env = MockEnv::new()
///     .with_file("config.toml", "[server]\nport = 8080");
/// let config = Config::<App>::builder()
///     .source(Toml::file("config.toml"))
///     .build_with_env(&env);
/// ```
pub trait ConfigEnv: Send + Sync {
    /// Read a file's contents as a UTF-8 string.
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if:
    /// - File does not exist (`ErrorKind::NotFound`)
    /// - File is not valid UTF-8
    /// - Permission denied
    /// - Other I/O errors
    fn read_file(&self, path: &Path) -> io::Result<String>;

    /// Check if a file exists.
    fn file_exists(&self, path: &Path) -> bool;

    /// Check if a path is a directory.
    fn is_directory(&self, path: &Path) -> bool;

    /// Get an environment variable by name.
    ///
    /// Returns `None` if the variable is not set.
    fn get_env(&self, name: &str) -> Option<String>;

    /// Get all environment variables matching a prefix.
    ///
    /// Returns tuples of (full_name, value).
    fn env_vars_with_prefix(&self, prefix: &str) -> Vec<(String, String)>;

    /// Get all environment variables.
    ///
    /// Used by Env source when no prefix is specified.
    fn all_env_vars(&self) -> Vec<(String, String)>;
}
```

### RealEnv Implementation

```rust
/// Production environment using standard library I/O.
///
/// This is a zero-cost abstraction - all methods are simple wrappers
/// around std functions.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealEnv;

impl RealEnv {
    /// Create a new real environment.
    pub fn new() -> Self {
        Self
    }
}

impl ConfigEnv for RealEnv {
    fn read_file(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn file_exists(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_directory(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn get_env(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn env_vars_with_prefix(&self, prefix: &str) -> Vec<(String, String)> {
        std::env::vars()
            .filter(|(k, _)| k.starts_with(prefix))
            .collect()
    }

    fn all_env_vars(&self) -> Vec<(String, String)> {
        std::env::vars().collect()
    }
}
```

### MockEnv Implementation

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

/// Mock environment for testing configuration loading.
///
/// # Example
///
/// ```rust
/// let env = MockEnv::new()
///     .with_file("config.toml", r#"
///         [database]
///         host = "localhost"
///         port = 5432
///     "#)
///     .with_file("secrets.toml", r#"
///         [database]
///         password = "secret123"
///     "#)
///     .with_env("APP_DATABASE_HOST", "prod-db.example.com")
///     .with_env("APP_LOG_LEVEL", "debug");
///
/// let config = Config::<AppConfig>::builder()
///     .source(Toml::file("config.toml"))
///     .source(Toml::file("secrets.toml"))
///     .source(Env::prefix("APP_"))
///     .build_with_env(&env)
///     .unwrap();
/// ```
#[derive(Debug, Default)]
pub struct MockEnv {
    files: RwLock<HashMap<PathBuf, MockFile>>,
    env_vars: RwLock<HashMap<String, String>>,
    directories: RwLock<Vec<PathBuf>>,
}

#[derive(Debug, Clone)]
enum MockFile {
    Content(String),
    NotFound,
    PermissionDenied,
    IoError(String),
}

impl MockEnv {
    /// Create a new empty mock environment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file with content.
    ///
    /// The path can be relative or absolute - it will be normalized.
    pub fn with_file(self, path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        self.files
            .write()
            .unwrap()
            .insert(path.into(), MockFile::Content(content.into()));
        self
    }

    /// Add a file that will return "not found" error.
    ///
    /// Useful for testing optional file handling.
    pub fn with_missing_file(self, path: impl Into<PathBuf>) -> Self {
        self.files
            .write()
            .unwrap()
            .insert(path.into(), MockFile::NotFound);
        self
    }

    /// Add a file that will return "permission denied" error.
    pub fn with_unreadable_file(self, path: impl Into<PathBuf>) -> Self {
        self.files
            .write()
            .unwrap()
            .insert(path.into(), MockFile::PermissionDenied);
        self
    }

    /// Add a directory path.
    pub fn with_directory(self, path: impl Into<PathBuf>) -> Self {
        self.directories.write().unwrap().push(path.into());
        self
    }

    /// Set an environment variable.
    pub fn with_env(self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars
            .write()
            .unwrap()
            .insert(name.into(), value.into());
        self
    }

    /// Set multiple environment variables from an iterator.
    pub fn with_envs<I, K, V>(self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let mut env_vars = self.env_vars.write().unwrap();
        for (k, v) in vars {
            env_vars.insert(k.into(), v.into());
        }
        drop(env_vars);
        self
    }

    /// Mutate the mock environment after creation.
    ///
    /// Useful for tests that modify files during execution.
    pub fn set_file(&self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.files
            .write()
            .unwrap()
            .insert(path.into(), MockFile::Content(content.into()));
    }

    /// Remove a file from the mock environment.
    pub fn remove_file(&self, path: impl AsRef<Path>) {
        self.files.write().unwrap().remove(path.as_ref());
    }

    /// Update an environment variable.
    pub fn set_env(&self, name: impl Into<String>, value: impl Into<String>) {
        self.env_vars
            .write()
            .unwrap()
            .insert(name.into(), value.into());
    }

    /// Remove an environment variable.
    pub fn remove_env(&self, name: &str) {
        self.env_vars.write().unwrap().remove(name);
    }
}

impl ConfigEnv for MockEnv {
    fn read_file(&self, path: &Path) -> io::Result<String> {
        let files = self.files.read().unwrap();

        match files.get(path) {
            Some(MockFile::Content(content)) => Ok(content.clone()),
            Some(MockFile::NotFound) | None => {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("mock file not found: {}", path.display()),
                ))
            }
            Some(MockFile::PermissionDenied) => {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("mock permission denied: {}", path.display()),
                ))
            }
            Some(MockFile::IoError(msg)) => {
                Err(io::Error::new(io::ErrorKind::Other, msg.clone()))
            }
        }
    }

    fn file_exists(&self, path: &Path) -> bool {
        let files = self.files.read().unwrap();
        matches!(files.get(path), Some(MockFile::Content(_)))
    }

    fn is_directory(&self, path: &Path) -> bool {
        self.directories.read().unwrap().contains(&path.to_path_buf())
    }

    fn get_env(&self, name: &str) -> Option<String> {
        self.env_vars.read().unwrap().get(name).cloned()
    }

    fn env_vars_with_prefix(&self, prefix: &str) -> Vec<(String, String)> {
        self.env_vars
            .read()
            .unwrap()
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn all_env_vars(&self) -> Vec<(String, String)> {
        self.env_vars
            .read()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}
```

### Updated Source Trait

```rust
use stillwater::Effect;
use crate::error::{ConfigErrors, ConfigValidation};
use crate::env::ConfigEnv;

/// Trait for configuration sources.
///
/// Sources load configuration from external locations (files, environment,
/// remote services) using stillwater's Effect type for I/O.
///
/// # Stillwater Integration
///
/// The `load` method returns an `Effect` that defers I/O until executed.
/// The `ConfigEnv` parameter enables dependency injection for testing.
///
/// # Example Implementation
///
/// ```rust
/// impl Source for MySource {
///     fn load<E: ConfigEnv>(&self) -> Effect<ConfigValues, ConfigErrors, E> {
///         let path = self.path.clone();
///
///         Effect::from_fn(move |env: &E| {
///             // I/O through injected environment
///             let content = env.read_file(&path)
///                 .map_err(|e| ConfigErrors::single(ConfigError::SourceError {
///                     source_name: path.display().to_string(),
///                     kind: SourceErrorKind::IoError { message: e.to_string() },
///                 }))?;
///
///             // Pure parsing
///             parse_content(&content)
///         })
///     }
/// }
/// ```
pub trait Source: Send + Sync {
    /// Load configuration values from this source.
    ///
    /// Returns an Effect that, when executed with a ConfigEnv, performs
    /// I/O and returns ConfigValues or ConfigErrors.
    fn load<E: ConfigEnv>(&self) -> Effect<ConfigValues, ConfigErrors, E>;

    /// Human-readable name of this source for error messages.
    fn name(&self) -> &str;

    /// Path to watch for hot reload, if applicable.
    #[cfg(feature = "watch")]
    fn watch_path(&self) -> Option<PathBuf> {
        None
    }
}
```

### Updated ConfigBuilder

```rust
impl<T: DeserializeOwned + Validate> ConfigBuilder<T> {
    /// Build configuration using the real environment (production).
    ///
    /// This is the standard entry point for production code.
    pub fn build(self) -> ConfigValidation<Config<T>> {
        self.build_with_env(&RealEnv)
    }

    /// Build configuration with a custom environment.
    ///
    /// Use this for testing with `MockEnv`:
    ///
    /// ```rust
    /// let env = MockEnv::new()
    ///     .with_file("config.toml", "port = 8080");
    ///
    /// let config = Config::<App>::builder()
    ///     .source(Toml::file("config.toml"))
    ///     .build_with_env(&env);
    /// ```
    pub fn build_with_env<E: ConfigEnv>(self, env: &E) -> ConfigValidation<Config<T>> {
        // Load all sources using the provided environment
        let source_results: Vec<Result<ConfigValues, ConfigErrors>> = self
            .sources
            .iter()
            .map(|source| {
                source
                    .load::<E>()
                    .context(format!("Loading {}", source.name()))
                    .run(env)
            })
            .collect();

        // Convert to Validations for error accumulation
        let validations: Vec<ConfigValidation<ConfigValues>> = source_results
            .into_iter()
            .map(|r| match r {
                Ok(values) => Validation::Success(values),
                Err(errors) => Validation::Failure(errors),
            })
            .collect();

        // Accumulate all source loading errors
        let all_values = Validation::all(validations);

        // Pure core: merge, deserialize, validate
        all_values.and_then(|values_vec| {
            let merged = merge_config_values(values_vec);
            deserialize_config::<T>(&merged).and_then(|config| {
                config.validate().map(|_| Config { value: config })
            })
        })
    }
}
```

### Testing Examples

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toml_source_parses_config() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            [server]
            host = "localhost"
            port = 8080
            "#,
        );

        let config = Config::<ServerConfig>::builder()
            .source(Toml::file("config.toml"))
            .build_with_env(&env)
            .unwrap();

        assert_eq!(config.get().server.host, "localhost");
        assert_eq!(config.get().server.port, 8080);
    }

    #[test]
    fn test_missing_required_file_errors() {
        let env = MockEnv::new(); // No files!

        let result = Config::<ServerConfig>::builder()
            .source(Toml::file("config.toml"))
            .build_with_env(&env);

        assert!(result.is_failure());
        let errors = result.unwrap_failure();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ConfigError::SourceError { kind: SourceErrorKind::NotFound { .. }, .. })));
    }

    #[test]
    fn test_optional_file_missing_is_ok() {
        let env = MockEnv::new().with_file("base.toml", "port = 8080");

        let result = Config::<ServerConfig>::builder()
            .source(Toml::file("base.toml"))
            .source(Toml::file("local.toml").optional()) // Missing but optional
            .build_with_env(&env);

        assert!(result.is_success());
    }

    #[test]
    fn test_env_vars_override_file() {
        let env = MockEnv::new()
            .with_file("config.toml", r#"
                [server]
                host = "localhost"
                port = 8080
            "#)
            .with_env("APP_SERVER_HOST", "prod.example.com")
            .with_env("APP_SERVER_PORT", "443");

        let config = Config::<ServerConfig>::builder()
            .source(Toml::file("config.toml"))
            .source(Env::prefix("APP_"))
            .build_with_env(&env)
            .unwrap();

        // Env vars override file
        assert_eq!(config.get().server.host, "prod.example.com");
        assert_eq!(config.get().server.port, 443);
    }

    #[test]
    fn test_validation_errors_accumulated() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            [server]
            host = ""
            port = 0
            "#,
        );

        let result = Config::<ServerConfig>::builder()
            .source(Toml::file("config.toml"))
            .build_with_env(&env);

        let errors = result.unwrap_failure();
        // Should have errors for both host (empty) and port (0)
        assert!(errors.len() >= 2);
    }

    #[test]
    fn test_parse_error_includes_location() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            [server
            host = "localhost"
            "#, // Missing closing bracket
        );

        let result = Config::<ServerConfig>::builder()
            .source(Toml::file("config.toml"))
            .build_with_env(&env);

        let errors = result.unwrap_failure();
        let error = errors.first();
        assert!(matches!(
            error,
            ConfigError::SourceError { kind: SourceErrorKind::ParseError { line: Some(_), .. }, .. }
        ));
    }
}
```

## Dependencies

- **Prerequisites**: Specs 001, 002
- **Affected Components**: All source implementations, ConfigBuilder
- **External Dependencies**: None (uses std only)

## Testing Strategy

- **Unit Tests**:
  - MockEnv builder methods
  - File reading (success, not found, permission denied)
  - Environment variable access
  - Thread safety of MockEnv
- **Integration Tests**:
  - Full config loading with MockEnv
  - Source override behavior
  - Error accumulation with mocked failures

## Documentation Requirements

- **Code Documentation**: Comprehensive trait and impl docs with examples
- **User Documentation**: Testing guide showing MockEnv patterns
- **Example Tests**: In examples/ directory showing testing patterns

## Implementation Notes

- RealEnv should be `Copy` for zero-cost passing
- MockEnv uses `RwLock` for thread safety in async tests
- Consider `Arc<MockEnv>` for sharing across threads
- Path normalization may be needed for cross-platform tests

## Migration and Compatibility

Not applicable - new project.
