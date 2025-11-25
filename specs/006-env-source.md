---
number: 6
title: Environment Variable Source
category: storage
priority: high
status: draft
dependencies: [1, 2]
created: 2025-11-25
---

# Specification 006: Environment Variable Source

**Category**: storage
**Priority**: high
**Status**: draft
**Dependencies**: [001 - Core Config Builder, 002 - Error Types]

## Context

Environment variables are the standard way to configure applications in production, especially in containerized environments. They typically override file-based configuration and are commonly prefixed to avoid collisions (e.g., `APP_DATABASE_HOST`).

### Stillwater Pattern

Like TOML source, Env implements the Source trait with Effect-based loading:

```rust
fn load(&self) -> Effect<ConfigValues, ConfigErrors, ()>
```

While `std::env::vars()` is technically I/O, it's:
- Fast and synchronous
- Always available (no file system errors)
- No parse errors possible (just key-value strings)

The transformation from env vars to ConfigValues is pure.

## Objective

Implement an `Env` source that loads configuration from environment variables using the Effect pattern, with support for prefixes, custom mappings, case sensitivity options, and list parsing.

## Requirements

### Functional Requirements

1. **Prefix Support**: Filter env vars by prefix (e.g., `APP_`)
2. **Path Mapping**: Convert `APP_DATABASE_HOST` to `database.host`
3. **Custom Mappings**: Override automatic path derivation
4. **Custom Separator**: Support `__` or other separators
5. **Case Sensitivity**: Option for case-sensitive or insensitive matching
6. **List Parsing**: Parse comma-separated values as arrays
7. **Type Inference**: Attempt to parse as number, bool, or keep as string

### Non-Functional Requirements

- No external dependencies (uses `std::env`)
- Clear source attribution (e.g., `env:APP_DATABASE_HOST`)
- Efficient single pass over environment

## Acceptance Criteria

- [ ] `Env::prefix("APP_")` filters and maps env vars
- [ ] `APP_DATABASE_HOST` maps to `database.host`
- [ ] `APP_DATABASE_POOL_SIZE` maps to `database.pool_size`
- [ ] `.separator("__")` uses `APP__DATABASE__HOST` format
- [ ] `.map("DB_HOST", "database.host")` for custom mappings
- [ ] `.case_sensitive()` requires exact case match
- [ ] `.list_separator(",")` parses `a,b,c` as `["a", "b", "c"]`
- [ ] Type inference: `"true"` -> bool, `"42"` -> int, `"3.14"` -> float
- [ ] Source location shows `env:VAR_NAME`
- [ ] Unit tests for all mapping scenarios

## Technical Details

### API Design

```rust
/// Environment variable configuration source
pub struct Env {
    prefix: String,
    separator: String,
    case_sensitive: bool,
    list_separator: Option<String>,
    custom_mappings: HashMap<String, String>,
    excluded: HashSet<String>,
}

impl Env {
    /// Create env source with given prefix
    ///
    /// # Example
    /// ```
    /// Env::prefix("APP_")
    /// // APP_DATABASE_HOST -> database.host
    /// // APP_SERVER_PORT -> server.port
    /// ```
    pub fn prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            separator: "_".to_string(),
            case_sensitive: false,
            list_separator: None,
            custom_mappings: HashMap::new(),
            excluded: HashSet::new(),
        }
    }

    /// Create env source without prefix (use all env vars)
    pub fn all() -> Self {
        Self::prefix("")
    }

    /// Set the separator used in environment variable names
    ///
    /// # Example
    /// ```
    /// Env::prefix("APP_").separator("__")
    /// // APP__DATABASE__HOST -> database.host
    /// ```
    pub fn separator(mut self, sep: impl Into<String>) -> Self {
        self.separator = sep.into();
        self
    }

    /// Add a custom mapping from env var to config path
    ///
    /// # Example
    /// ```
    /// Env::prefix("APP_")
    ///     .map("DB_HOST", "database.host")
    ///     .map("DB_PORT", "database.port")
    /// // APP_DB_HOST -> database.host (instead of db.host)
    /// ```
    pub fn map(mut self, env_suffix: impl Into<String>, path: impl Into<String>) -> Self {
        self.custom_mappings.insert(env_suffix.into(), path.into());
        self
    }

    /// Exclude specific environment variables
    pub fn exclude(mut self, var: impl Into<String>) -> Self {
        self.excluded.insert(var.into());
        self
    }

    /// Require exact case match (default: case insensitive)
    pub fn case_sensitive(mut self) -> Self {
        self.case_sensitive = true;
        self
    }

    /// Set case insensitive matching (default)
    pub fn case_insensitive(mut self) -> Self {
        self.case_sensitive = false;
        self
    }

    /// Parse values containing this separator as lists
    ///
    /// # Example
    /// ```
    /// Env::prefix("APP_").list_separator(",")
    /// // APP_ALLOWED_HOSTS=a.com,b.com -> ["a.com", "b.com"]
    /// ```
    pub fn list_separator(mut self, sep: impl Into<String>) -> Self {
        self.list_separator = Some(sep.into());
        self
    }
}
```

### Source Implementation

```rust
impl Source for Env {
    fn load(&self) -> Validation<ConfigValues, Vec<ConfigError>> {
        let mut values = ConfigValues::new();
        let prefix_lower = self.prefix.to_lowercase();

        for (key, value) in std::env::vars() {
            // Check prefix match
            let key_check = if self.case_sensitive {
                key.clone()
            } else {
                key.to_lowercase()
            };

            if !key_check.starts_with(&prefix_lower) {
                continue;
            }

            // Check exclusions
            if self.excluded.contains(&key) {
                continue;
            }

            // Get suffix after prefix
            let suffix = &key[self.prefix.len()..];

            // Check for custom mapping
            let path = if let Some(mapped) = self.custom_mappings.get(suffix) {
                mapped.clone()
            } else {
                // Convert suffix to config path
                self.suffix_to_path(suffix)
            };

            // Parse value
            let parsed_value = self.parse_value(&value);

            // Store with source location
            let source = SourceLocation::env(&key);
            values.insert(path, ConfigValue {
                value: parsed_value,
                source,
            });
        }

        Validation::success(values)
    }

    fn name(&self) -> &str {
        "environment"
    }
}
```

### Path Conversion

```rust
impl Env {
    /// Convert env var suffix to config path
    /// DATABASE_HOST -> database.host
    /// DATABASE_POOL_SIZE -> database.pool_size
    fn suffix_to_path(&self, suffix: &str) -> String {
        suffix
            .split(&self.separator)
            .map(|part| part.to_lowercase())
            .collect::<Vec<_>>()
            .join(".")
    }
}
```

### Value Parsing

```rust
impl Env {
    fn parse_value(&self, value: &str) -> Value {
        // Check for list
        if let Some(sep) = &self.list_separator {
            if value.contains(sep) {
                let items: Vec<Value> = value
                    .split(sep)
                    .map(|s| self.parse_scalar(s.trim()))
                    .collect();
                return Value::Array(items);
            }
        }

        self.parse_scalar(value)
    }

    fn parse_scalar(&self, value: &str) -> Value {
        // Try boolean
        match value.to_lowercase().as_str() {
            "true" | "yes" | "1" | "on" => return Value::Bool(true),
            "false" | "no" | "0" | "off" => return Value::Bool(false),
            _ => {}
        }

        // Try integer
        if let Ok(i) = value.parse::<i64>() {
            return Value::Integer(i);
        }

        // Try float
        if let Ok(f) = value.parse::<f64>() {
            return Value::Float(f);
        }

        // Keep as string
        Value::String(value.to_string())
    }
}
```

### Edge Cases

```rust
// Handling array indices in env vars
// APP_SERVERS_0_HOST=host1.com
// APP_SERVERS_1_HOST=host2.com
// -> servers[0].host, servers[1].host

fn suffix_to_path(&self, suffix: &str) -> String {
    let parts: Vec<&str> = suffix.split(&self.separator).collect();
    let mut path_parts = Vec::new();

    for part in parts {
        let lower = part.to_lowercase();
        // Check if it's a numeric index
        if let Ok(idx) = lower.parse::<usize>() {
            // Convert previous part to array index
            if let Some(last) = path_parts.last_mut() {
                *last = format!("{}[{}]", last, idx);
                continue;
            }
        }
        path_parts.push(lower);
    }

    path_parts.join(".")
}
```

## Dependencies

- **Prerequisites**: Specs 001, 002
- **Affected Components**: Config builder integration
- **External Dependencies**: None (std only)

## Testing Strategy

- **Unit Tests**:
  - Basic prefix filtering
  - Path conversion (separator, case)
  - Custom mappings
  - List parsing
  - Type inference (bool, int, float, string)
  - Array index handling
  - Exclusions
- **Integration Tests**:
  - Combined with file sources
  - Override behavior

## Documentation Requirements

- **Code Documentation**: Doc comments with env var examples
- **User Documentation**: Environment variable configuration guide

## Implementation Notes

- Use `std::env::vars()` for iteration
- Consider `std::env::var_os()` for non-UTF8 handling (and appropriate error)
- Be careful with concurrent modification of env vars in tests
- Consider providing a way to pass a custom env map for testing

## Migration and Compatibility

Not applicable - new project.
