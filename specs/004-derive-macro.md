---
number: 4
title: Validate Derive Macro
category: foundation
priority: critical
status: draft
dependencies: [2, 3]
created: 2025-11-25
---

# Specification 004: Validate Derive Macro

**Category**: foundation
**Priority**: critical
**Status**: draft
**Dependencies**: [002 - Error Types, 003 - Validate Trait]

## Context

Manual implementation of the `Validate` trait is verbose and error-prone. A derive macro provides a declarative way to specify validation rules directly on struct fields, similar to how `serde` derives work. This is the primary user-facing API for validation.

## Objective

Implement a `#[derive(Validate)]` proc macro that generates `Validate` implementations from field attributes, supporting all built-in validators and custom validation functions.

## Requirements

### Functional Requirements

1. **Derive Macro**: `#[derive(Validate)]` generates trait implementation
2. **Field Attributes**: `#[validate(...)]` attributes on fields
3. **Struct Attributes**: `#[validate(custom = "...")]` for struct-level validation
4. **All Built-in Validators**: Support all validators from spec 003
5. **Custom Messages**: `message = "..."` override default error messages
6. **Conditional Validation**: `when = "..."` for conditional rules
7. **Nested Validation**: `#[validate(nested)]` for nested structs
8. **Skip**: `#[validate(skip)]` to exclude fields
9. **Sensitive Marking**: `#[sensitive]` to redact values in errors

### Non-Functional Requirements

- Clear compile-time error messages for invalid attributes
- Generated code should be efficient and readable
- Support for generic types where validation makes sense
- Documentation comments on generated code

## Acceptance Criteria

- [ ] `#[derive(Validate)]` generates working `Validate` impl
- [ ] String validators: `non_empty`, `min_length(n)`, `max_length(n)`, `length(n..=m)`, `pattern("...")`, `email`, `url`, `ip`, `uuid`
- [ ] Numeric validators: `range(n..=m)`, `positive`, `negative`, `non_zero`
- [ ] Collection validators: `non_empty`, `min_length(n)`, `max_length(n)`, `each(...)`
- [ ] Path validators: `file_exists`, `dir_exists`, `parent_exists`, `extension("...")`
- [ ] `#[validate(nested)]` validates nested structs
- [ ] `#[validate(skip)]` excludes field from validation
- [ ] `#[validate(custom = "fn_name")]` calls custom function
- [ ] `#[validate(when = "self.condition")]` conditional validation
- [ ] `message = "..."` overrides default error message
- [ ] `#[sensitive]` redacts value in error messages
- [ ] Struct-level `#[validate(custom = "fn_name")]` runs after field validation
- [ ] Compile errors for invalid attribute usage
- [ ] Unit tests for all attribute combinations

## Technical Details

### Attribute Syntax

```rust
#[derive(Validate)]
#[validate(custom = "validate_config")]  // struct-level validation
struct Config {
    // String validators
    #[validate(non_empty)]
    #[validate(min_length(3))]
    #[validate(max_length(100))]
    #[validate(length(3..=100))]
    #[validate(pattern(r"^[a-z]+$"))]
    #[validate(email)]
    #[validate(url)]
    #[validate(ip)]
    #[validate(uuid)]
    name: String,

    // Numeric validators
    #[validate(range(1..=65535))]
    #[validate(positive)]
    #[validate(negative)]
    #[validate(non_zero)]
    port: u16,

    // Collection validators
    #[validate(non_empty)]
    #[validate(min_length(1))]
    #[validate(max_length(10))]
    #[validate(each(non_empty))]  // validate each element
    items: Vec<String>,

    // Path validators
    #[validate(file_exists)]
    #[validate(dir_exists)]
    #[validate(parent_exists)]
    #[validate(extension("toml"))]
    path: PathBuf,

    // Nested validation
    #[validate(nested)]
    database: DatabaseConfig,

    // Conditional validation
    #[validate(url, when = "self.use_remote")]
    remote_url: Option<String>,

    use_remote: bool,

    // Custom message
    #[validate(range(1..=100), message = "Pool size must be between 1 and 100")]
    pool_size: u32,

    // Skip validation
    #[validate(skip)]
    internal_id: String,

    // Sensitive value (redact in errors)
    #[sensitive]
    #[validate(min_length(16))]
    api_key: String,
}
```

### Generated Code Example

For a simple struct:

```rust
#[derive(Validate)]
struct ServerConfig {
    #[validate(non_empty, message = "Host is required")]
    host: String,

    #[validate(range(1..=65535))]
    port: u16,

    #[validate(nested)]
    tls: Option<TlsConfig>,
}
```

Generates:

```rust
impl Validate for ServerConfig {
    fn validate(&self) -> Validation<(), Vec<ConfigError>> {
        use premortem::validators::*;

        Validation::all((
            // host: non_empty
            validate_field(
                &self.host,
                "host",
                &NonEmpty,
                Some("Host is required"),
            ),

            // port: range(1..=65535)
            validate_field(
                &self.port,
                "port",
                &Range(1..=65535),
                None,
            ),

            // tls: nested (Option<T> where T: Validate)
            match &self.tls {
                Some(value) => value.validate_at("tls"),
                None => Validation::success(()),
            },
        )).map(|_| ())
    }
}
```

### Struct-Level Validation

```rust
#[derive(Validate)]
#[validate(custom = "validate_database_config")]
struct DatabaseConfig {
    host: String,
    port: u16,
    replica_host: Option<String>,
    replica_port: Option<u16>,
}

fn validate_database_config(cfg: &DatabaseConfig) -> Validation<(), Vec<ConfigError>> {
    let mut errors = vec![];

    if let (Some(rh), Some(rp)) = (&cfg.replica_host, cfg.replica_port) {
        if rh == &cfg.host && rp == cfg.port {
            errors.push(ConfigError::CrossFieldError {
                paths: vec!["replica_host".into(), "replica_port".into()],
                message: "Replica cannot be same as primary".into(),
            });
        }
    }

    Validation::from_errors(errors)
}
```

Generates:

```rust
impl Validate for DatabaseConfig {
    fn validate(&self) -> Validation<(), Vec<ConfigError>> {
        // Field validations...
        let field_result = Validation::all((...)).map(|_| ());

        // Then struct-level validation
        field_result.and_then(|_| validate_database_config(self))
    }
}
```

### Conditional Validation

```rust
#[derive(Validate)]
struct CacheConfig {
    enabled: bool,

    #[validate(url, when = "self.enabled")]
    backend_url: Option<String>,

    #[validate(range(1..=86400), when = "self.enabled")]
    ttl_seconds: Option<u32>,
}
```

Generates:

```rust
impl Validate for CacheConfig {
    fn validate(&self) -> Validation<(), Vec<ConfigError>> {
        Validation::all((
            // backend_url: url, when = "self.enabled"
            if self.enabled {
                match &self.backend_url {
                    Some(value) => validate_field(value, "backend_url", &Url, None),
                    None => Validation::fail(vec![ConfigError::ValidationError {
                        path: "backend_url".into(),
                        source_location: None,
                        value: None,
                        message: "required when 'enabled' is true".into(),
                    }]),
                }
            } else {
                Validation::success(())
            },

            // ttl_seconds: similar...
        )).map(|_| ())
    }
}
```

### Sensitive Field Handling

```rust
#[derive(Validate)]
struct Credentials {
    #[sensitive]
    #[validate(min_length(8))]
    password: String,
}
```

When validation fails, the error will have `value: None` instead of showing the actual password.

### Macro Implementation Structure

```
premortem-derive/
├── Cargo.toml
├── src/
│   ├── lib.rs           # proc-macro entry point
│   ├── parse.rs         # attribute parsing
│   ├── validate.rs      # Validate derive implementation
│   ├── validators.rs    # validator attribute handling
│   └── codegen.rs       # code generation
```

### Error Messages

The macro should provide helpful compile-time errors:

```
error: unknown validator 'rang'
  --> src/config.rs:12:15
   |
12 |     #[validate(rang(1..10))]
   |               ^^^^ did you mean 'range'?

error: 'nested' validator cannot be combined with other validators
  --> src/config.rs:15:15
   |
15 |     #[validate(nested, non_empty)]
   |               ^^^^^^

error: struct-level custom validator 'validate_foo' not found
  --> src/config.rs:5:24
   |
5  | #[validate(custom = "validate_foo")]
   |                      ^^^^^^^^^^^^^
```

## Dependencies

- **Prerequisites**: Specs 002, 003
- **Affected Components**: All user configuration types
- **External Dependencies**:
  - `syn` for parsing
  - `quote` for code generation
  - `proc-macro2` for token manipulation

## Testing Strategy

- **Unit Tests** (in derive crate):
  - Attribute parsing
  - Code generation for each validator
  - Edge cases (empty structs, enums, generics)
- **Integration Tests** (in main crate):
  - Full derive + validate cycle
  - Error message verification
  - All validator combinations
- **Compile-fail Tests**:
  - Invalid attribute syntax
  - Unknown validators
  - Type mismatches

## Documentation Requirements

- **Code Documentation**: Complete docs on all attributes
- **User Documentation**: Derive macro guide with examples
- **Error Catalog**: List of compile-time errors and fixes

## Implementation Notes

- Use `syn::parse_macro_input!` for parsing
- Use `quote!` for code generation
- Consider `darling` crate for attribute parsing (cleaner API)
- Test with `trybuild` for compile-fail tests
- Support `#[validate]` on enums for variant validation (future)

## Migration and Compatibility

Not applicable - new project.
