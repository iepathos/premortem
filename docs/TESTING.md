# Testing Configuration

This guide covers testing patterns for configuration with premortem. The key feature enabling testability is the `MockEnv` type, which allows testing configuration loading without real files or environment variables.

## Table of Contents

- [Using MockEnv](#using-mockenv)
- [Testing Error Cases](#testing-error-cases)
- [Testing Validation](#testing-validation)
- [Testing Source Priority](#testing-source-priority)
- [Testing Nested Configuration](#testing-nested-configuration)
- [Advanced Patterns](#advanced-patterns)

## Using MockEnv

The `MockEnv` type allows testing file-based configuration without creating actual files:

```rust
use premortem::prelude::*;

#[test]
fn test_config_loading() {
    let env = MockEnv::new()
        .with_file("config.toml", r#"
            host = "localhost"
            port = 8080
        "#)
        .with_env("APP_DEBUG", "true");

    let config = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .source(Env::prefix("APP_"))
        .build_with_env(&env)
        .expect("should load");

    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 8080);
    assert!(config.debug);
}
```

### MockEnv Methods

| Method | Description |
|--------|-------------|
| `with_file(path, content)` | Add a mock file with content |
| `with_env(name, value)` | Set an environment variable |
| `with_envs(iter)` | Set multiple environment variables |
| `with_missing_file(path)` | Mark a file as explicitly missing |
| `with_unreadable_file(path)` | Simulate permission denied error |
| `with_directory(path)` | Add a mock directory |
| `set_file(path, content)` | Update file content during test |
| `remove_file(path)` | Remove a file during test |
| `set_env(name, value)` | Update env var during test |
| `remove_env(name)` | Remove env var during test |

## Testing Error Cases

### Missing Required File

```rust
#[test]
fn test_missing_required_file() {
    let env = MockEnv::new();

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("missing.toml"))
        .build_with_env(&env);

    assert!(result.is_err());

    let errors = result.unwrap_err();
    match errors.first() {
        ConfigError::SourceError { kind, .. } => {
            assert!(matches!(kind, SourceErrorKind::NotFound { .. }));
        }
        other => panic!("Expected SourceError, got: {:?}", other),
    }
}
```

### Optional File Missing

```rust
#[test]
fn test_optional_file_missing() {
    let env = MockEnv::new()
        .with_env("APP_HOST", "localhost")
        .with_env("APP_PORT", "8080");

    // Optional file doesn't error when missing
    let config = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml").optional())
        .source(Env::prefix("APP_"))
        .build_with_env(&env)
        .expect("should load from env only");

    assert_eq!(config.host, "localhost");
}
```

### Permission Denied

```rust
#[test]
fn test_permission_denied() {
    let env = MockEnv::new()
        .with_unreadable_file("secrets.toml");

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("secrets.toml"))
        .build_with_env(&env);

    assert!(result.is_err());

    let errors = result.unwrap_err();
    match errors.first() {
        ConfigError::SourceError { kind, .. } => {
            assert!(matches!(kind, SourceErrorKind::IoError { .. }));
        }
        _ => panic!("Expected IoError"),
    }
}
```

### Parse Errors

```rust
#[test]
fn test_invalid_toml() {
    let env = MockEnv::new()
        .with_file("config.toml", "this is not valid toml {{{");

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env);

    assert!(result.is_err());

    let errors = result.unwrap_err();
    match errors.first() {
        ConfigError::SourceError { kind, .. } => {
            assert!(matches!(kind, SourceErrorKind::ParseError { .. }));
        }
        _ => panic!("Expected ParseError"),
    }
}
```

## Testing Validation

### Error Accumulation

Premortem's key feature is collecting ALL errors:

```rust
#[test]
fn test_all_errors_accumulated() {
    let env = MockEnv::new().with_file("config.toml", r#"
        host = ""     # Invalid: empty
        port = 0      # Invalid: out of range
    "#);

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env);

    assert!(result.is_err());
    let errors = result.unwrap_err();

    // Both validation errors should be present
    assert_eq!(errors.len(), 2);
}
```

### Specific Validation Errors

```rust
#[test]
fn test_specific_validation_error() {
    let env = MockEnv::new().with_file("config.toml", r#"
        host = "localhost"
        port = 99999  # Invalid: exceeds max port
    "#);

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env);

    assert!(result.is_err());
    let errors = result.unwrap_err();

    // Check the error is for the port field
    let has_port_error = errors.iter().any(|e| {
        matches!(e, ConfigError::ValidationError { path, .. } if path == "port")
    });
    assert!(has_port_error, "Expected port validation error");
}
```

### Cross-Field Validation

```rust
#[test]
fn test_cross_field_validation() {
    #[derive(Debug, Deserialize)]
    struct RangeConfig {
        min: i32,
        max: i32,
    }

    impl Validate for RangeConfig {
        fn validate(&self) -> ConfigValidation<()> {
            if self.min > self.max {
                Validation::Failure(ConfigErrors::single(
                    ConfigError::CrossFieldError {
                        paths: vec!["min".to_string(), "max".to_string()],
                        message: "min cannot exceed max".to_string(),
                    }
                ))
            } else {
                Validation::Success(())
            }
        }
    }

    let env = MockEnv::new().with_file("config.toml", r#"
        min = 100
        max = 50
    "#);

    let result = Config::<RangeConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(matches!(errors.first(), ConfigError::CrossFieldError { .. }));
}
```

## Testing Source Priority

### Environment Overrides File

```rust
#[test]
fn test_env_overrides_file() {
    let env = MockEnv::new()
        .with_file("config.toml", r#"
            host = "from-file"
            port = 8080
        "#)
        .with_env("APP_HOST", "from-env");

    let config = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .source(Env::prefix("APP_"))  // Higher priority
        .build_with_env(&env)
        .expect("should load");

    assert_eq!(config.host, "from-env");  // Env wins
    assert_eq!(config.port, 8080);        // From file
}
```

### Multiple File Sources

```rust
#[test]
fn test_file_layering() {
    let env = MockEnv::new()
        .with_file("base.toml", r#"
            host = "base-host"
            port = 8080
            debug = false
        "#)
        .with_file("override.toml", r#"
            host = "override-host"
            debug = true
        "#);

    let config = Config::<AppConfig>::builder()
        .source(Toml::file("base.toml"))
        .source(Toml::file("override.toml"))  // Higher priority
        .build_with_env(&env)
        .expect("should load");

    assert_eq!(config.host, "override-host");  // Overridden
    assert_eq!(config.port, 8080);             // From base
    assert!(config.debug);                      // Overridden
}
```

## Testing Nested Configuration

```rust
#[derive(Debug, Deserialize, Validate)]
struct ServerConfig {
    #[validate(non_empty)]
    host: String,
    #[validate(range(1..=65535))]
    port: u16,
}

#[derive(Debug, Deserialize, Validate)]
struct AppConfig {
    #[validate(nested)]
    server: ServerConfig,
}

#[test]
fn test_nested_config() {
    let env = MockEnv::new().with_file("config.toml", r#"
        [server]
        host = "localhost"
        port = 8080
    "#);

    let config = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env)
        .expect("should load");

    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.port, 8080);
}

#[test]
fn test_nested_validation_error_paths() {
    let env = MockEnv::new().with_file("config.toml", r#"
        [server]
        host = ""
        port = 8080
    "#);

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env);

    assert!(result.is_err());
    let errors = result.unwrap_err();

    // Error path should include nested path
    let paths: Vec<_> = errors.iter().filter_map(|e| e.path()).collect();
    assert!(paths.iter().any(|p| p.contains("server")));
}
```

## Advanced Patterns

### Dynamic File Changes

MockEnv supports changing content during test execution:

```rust
#[test]
fn test_dynamic_changes() {
    let env = MockEnv::new()
        .with_file("config.toml", r#"
            host = "initial"
            port = 8080
        "#);

    // First load
    let config1 = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env)
        .unwrap();
    assert_eq!(config1.host, "initial");

    // Change file content
    env.set_file("config.toml", r#"
        host = "updated"
        port = 9000
    "#);

    // Second load sees new content
    let config2 = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env)
        .unwrap();
    assert_eq!(config2.host, "updated");
    assert_eq!(config2.port, 9000);
}
```

### Testing with Traced Builds

```rust
#[test]
fn test_value_tracing() {
    let env = MockEnv::new()
        .with_file("config.toml", "host = \"from-file\"")
        .with_env("APP_HOST", "from-env");

    let traced = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .source(Env::prefix("APP_"))
        .build_traced_with_env(&env)
        .expect("should load");

    // Verify override was tracked
    assert!(traced.was_overridden("host"));

    let host_trace = traced.trace("host").unwrap();
    assert_eq!(host_trace.history.len(), 2);
    assert_eq!(host_trace.final_value.value.as_str(), Some("from-env"));
}
```

### Testing Error Messages

```rust
#[test]
fn test_error_messages_are_helpful() {
    let env = MockEnv::new().with_file("config.toml", r#"
        host = ""
        port = -1
    "#);

    let result = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env);

    let errors = result.unwrap_err();

    for error in errors.iter() {
        let msg = error.to_string();
        // Errors should mention the field name
        assert!(
            msg.contains("host") || msg.contains("port"),
            "Error should mention field: {}",
            msg
        );
    }
}
```

### Helper Functions for Tests

Create test utilities to reduce boilerplate:

```rust
mod test_helpers {
    use super::*;

    pub fn mock_config(toml: &str) -> MockEnv {
        MockEnv::new().with_file("config.toml", toml)
    }

    pub fn load_test_config(env: &MockEnv) -> Result<Config<AppConfig>, ConfigErrors> {
        Config::<AppConfig>::builder()
            .source(Toml::file("config.toml"))
            .source(Env::prefix("APP_"))
            .build_with_env(env)
    }

    pub fn assert_validation_error_for(errors: &ConfigErrors, path: &str) {
        let has_error = errors.iter().any(|e| {
            e.path().map(|p| p.contains(path)).unwrap_or(false)
        });
        assert!(has_error, "Expected error for path '{}' in {:?}", path, errors);
    }
}

#[test]
fn test_with_helpers() {
    use test_helpers::*;

    let env = mock_config(r#"
        host = ""
        port = 8080
    "#);

    let result = load_test_config(&env);
    assert!(result.is_err());

    assert_validation_error_for(&result.unwrap_err(), "host");
}
```
