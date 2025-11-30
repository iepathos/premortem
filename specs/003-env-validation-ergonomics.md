---
number: 3
title: Environment Variable Validation Ergonomics
category: foundation
priority: high
status: draft
dependencies: []
created: 2025-11-30
---

# Specification 003: Environment Variable Validation Ergonomics

**Category**: foundation
**Priority**: high
**Status**: draft
**Dependencies**: None

## Context

Premortem already has excellent environment variable support through the `Env` source with features like prefix filtering, nested paths, type inference, and array notation. However, the common pattern of "required environment variable with validation" requires significant boilerplate code.

### Current Pain Point

A typical configuration loading pattern with required environment variables looks like this:

```rust
// 90+ lines of repetitive code
let database_url = env
    .get_env("DATABASE_URL")
    .ok_or_else(|| ConfigError("DATABASE_URL is required".to_string()))?;

let jwt_secret = env
    .get_env("JWT_SECRET")
    .ok_or_else(|| ConfigError("JWT_SECRET is required".to_string()))?;

if jwt_secret.len() < 32 {
    return Err(ConfigError(
        "JWT_SECRET must be at least 32 characters long".to_string(),
    ));
}

let github_client_id = env
    .get_env("GITHUB_CLIENT_ID")
    .ok_or_else(|| ConfigError("GITHUB_CLIENT_ID is required".to_string()))?;

// ... repeated 10+ more times
```

This violates premortem's core principles:
- **No error accumulation** - Fails on first missing variable
- **Imperative, not functional** - 90 lines of manual I/O and validation mixing
- **Poor composability** - Can't reuse validation logic
- **Verbose** - Repetitive `.ok_or_else()` chains

### What Already Works Well

- `Env::prefix("APP_")` - Prefix filtering
- `APP_DATABASE_HOST` → `database.host` - Nested path mapping
- Type inference - Automatic parsing of integers, floats, booleans
- `#[validate(range(1..=65535))]` - Derive macro validators
- MockEnv for testing

### The Gap

The gap is **ergonomics**, not functionality. Users need:
1. Declarative required field validation
2. Better integration between env loading and validation
3. Clear examples showing best practices for env-heavy configs
4. Accumulation of ALL missing/invalid env vars before failing

## Objective

Reduce boilerplate for environment-variable-driven configuration from 90+ lines of imperative code to ~20 lines of declarative code, while maintaining full error accumulation and improving error messages with source location tracking.

## Requirements

### Functional Requirements

1. **Required Field Validation**
   - Provide declarative way to mark fields as required
   - Generate clear error messages when required fields are missing
   - Accumulate ALL missing fields before failing (not fail-fast)

2. **Composable Validators**
   - Allow chaining multiple validators per field
   - Support combining built-in and custom validators
   - Maintain functional composition principles

3. **Better Error Messages**
   - Show which environment variable was checked
   - Display expected format/constraints
   - Provide source location when available
   - Suggest related configuration options

4. **Backward Compatibility**
   - Existing `Env` source API remains unchanged
   - Current validation patterns continue to work
   - No breaking changes to public API

### Non-Functional Requirements

1. **Performance**
   - No performance regression for existing use cases
   - Validation overhead should be negligible (<1ms for typical configs)

2. **Testability**
   - All new code must be testable with MockEnv
   - Examples must include test cases
   - Maintain "pure core, imperative shell" pattern

3. **Documentation**
   - Comprehensive example showing env-heavy configuration
   - Migration guide from manual validation to ergonomic patterns
   - Document best practices for required vs optional fields

4. **Maintainability**
   - Follow existing premortem code organization
   - Consistent with stillwater functional patterns
   - Clear separation of concerns

## Acceptance Criteria

- [ ] Can mark fields as required with `#[validate(required)]` attribute
- [ ] Missing required fields accumulate errors (not fail-fast)
- [ ] Error messages include environment variable name and prefix context
- [ ] Can combine `required` with other validators (e.g., `#[validate(required, min_length(32))]`)
- [ ] Optional fields with `Option<T>` don't generate required errors
- [ ] Comprehensive example showing env-heavy configuration (like maat-api config)
- [ ] Example reduces 90-line manual config to ~20 lines declarative
- [ ] All validators work with MockEnv for testing
- [ ] Documentation includes migration guide from manual patterns
- [ ] No breaking changes to existing Env source API
- [ ] Performance benchmarks show <5% overhead vs manual validation
- [ ] Integration tests validate error accumulation behavior

## Technical Details

### Implementation Approach

#### Option 1: Enhance Derive Macro (Recommended)

Add `#[validate(required)]` support to the existing `#[derive(Validate)]` macro:

```rust
use premortem::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize, Validate)]
struct Config {
    // Required with validation
    #[validate(required, min_length(32))]
    jwt_secret: String,

    // Required, any non-empty value
    #[validate(required)]
    database_url: String,

    // Optional, validated only if present
    #[validate(optional, range(1..=65535))]
    custom_port: Option<u16>,

    // Default value, no validation needed
    #[serde(default = "default_host")]
    host: String,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}
```

Generated validation accumulates errors:
- Check all required fields, collect missing ones
- Validate present fields, collect validation errors
- Return all errors at once via stillwater's Validation

#### Option 2: Builder Pattern for Env Source

Add validation hints to Env source itself:

```rust
let config = Config::<AppConfig>::builder()
    .source(
        Env::prefix("APP_")
            .require("JWT_SECRET", validators::min_length(32))
            .require("DATABASE_URL", validators::non_empty())
            .optional("CUSTOM_PORT", validators::range(1..=65535))
    )
    .build()?;
```

**Decision**: Option 1 is preferred because:
- Leverages existing derive macro
- Keeps validation logic with struct definition
- More consistent with current patterns
- Better integration with serde defaults

### Architecture Changes

1. **Extend premortem-derive macro**
   - Add `required` attribute parser
   - Generate code to check field presence before validation
   - Accumulate missing field errors with environment variable context

2. **Add ValidationContext enhancement**
   - Track which environment variables map to which fields
   - Include env var names in error messages
   - Support prefix-aware error reporting

3. **New validators module additions**
   ```rust
   // In src/validate.rs or new src/validators/env.rs

   /// Validator that checks field is present (non-default)
   pub struct Required;

   impl<T> Validator<T> for Required {
       fn validate(&self, value: &T, path: &str) -> ConfigValidation<()> {
           // Check if value came from source or is default
           // Use ValidationContext to determine
       }
   }
   ```

### Data Structures

```rust
// Enhancement to ValidationContext
pub struct ValidationContext {
    locations: SourceLocationMap,
    // NEW: Track which fields were loaded vs defaulted
    loaded_fields: HashSet<String>,
    // NEW: Track env var to field path mapping
    env_var_map: HashMap<String, String>,
}

impl ValidationContext {
    pub fn is_field_loaded(&self, path: &str) -> bool {
        self.loaded_fields.contains(path)
    }

    pub fn env_var_for_field(&self, path: &str) -> Option<&str> {
        self.env_var_map.get(path).map(|s| s.as_str())
    }
}
```

### APIs and Interfaces

**New validate attribute**:
```rust
#[validate(required)]              // Basic required check
#[validate(required, min_length(32))]  // Required + validation
#[validate(optional, range(1..=100))]  // Optional, validate if present
```

**Enhanced error messages**:
```
Before:
  Configuration error: JWT_SECRET is required

After:
  Configuration error:
    [env:JWT_SECRET] Missing required field 'jwt_secret'
    Expected: String with minimum length 32 characters
    Checked: APP_JWT_SECRET (with prefix APP_)
```

## Dependencies

- **Prerequisites**: None (extends existing functionality)
- **Affected Components**:
  - `premortem-derive` - Add required/optional attribute support
  - `src/validate.rs` - Enhance ValidationContext
  - `src/error.rs` - Potentially enhance error message formatting
- **External Dependencies**: None (uses existing dependencies)

## Testing Strategy

### Unit Tests

1. **Derive macro tests** (in premortem-derive/tests/):
   ```rust
   #[test]
   fn test_required_attribute_generates_validation() {
       #[derive(Validate)]
       struct TestConfig {
           #[validate(required)]
           field: String,
       }
       // Test generated code checks field presence
   }
   ```

2. **Validator tests** (in src/validate.rs):
   ```rust
   #[test]
   fn test_required_validator_with_mock_env() {
       let env = MockEnv::new()
           .with_env("APP_FIELD", "value");
       // Test Required validator behavior
   }
   ```

3. **Error accumulation tests**:
   ```rust
   #[test]
   fn test_multiple_required_fields_accumulate_errors() {
       let env = MockEnv::new(); // Empty
       let result = Config::load(&env);
       assert!(matches!(result, Err(errors) if errors.len() >= 3));
   }
   ```

### Integration Tests

1. **Full config loading flow** (examples/env-validation/):
   ```rust
   // Test loading config with required fields from env
   // Verify error accumulation
   // Validate error message quality
   ```

2. **Backward compatibility** (tests/integration/):
   ```rust
   // Ensure existing examples still work
   // No breaking changes to Env source
   ```

### Performance Tests

```rust
#[bench]
fn bench_validation_overhead(b: &mut Bencher) {
    let env = create_test_env_with_all_fields();
    b.iter(|| {
        Config::<TestConfig>::builder()
            .source(Env::prefix("APP_"))
            .build()
    });
}
```

Target: <5% overhead compared to manual validation

### User Acceptance

Create comprehensive example (`examples/env-validation/`) that:
- Mirrors real-world usage (like maat-api configuration)
- Shows before/after comparison
- Demonstrates error accumulation
- Includes test cases

## Documentation Requirements

### Code Documentation

1. **Derive macro attributes**:
   ```rust
   /// Validates that a field is required (non-default).
   ///
   /// # Examples
   ///
   /// ```rust
   /// #[derive(Validate)]
   /// struct Config {
   ///     #[validate(required)]
   ///     database_url: String,
   /// }
   /// ```
   ```

2. **Validator documentation**:
   - Document Required validator behavior
   - Explain interaction with serde defaults
   - Clarify Option<T> handling

### User Documentation

1. **New example**: `examples/env-validation/`
   - Comprehensive env-driven configuration
   - Before/after showing boilerplate reduction
   - Error accumulation demonstration
   - Testing patterns with MockEnv

2. **Update `docs/PATTERNS.md`**:
   - Add "Environment Variable Patterns" section
   - Show required vs optional field patterns
   - Document best practices
   - Include migration guide from manual validation

3. **Update README.md**:
   - Add env validation to feature list
   - Include quick example showing ergonomics improvement

### Architecture Updates

Update `ARCHITECTURE.md` if it exists to document:
- ValidationContext enhancements
- Required field validation flow
- Integration between Env source and validation

## Implementation Notes

### Complexity Considerations

The main challenge is **distinguishing loaded fields from defaulted fields**. Three approaches:

1. **Track during deserialization** (Recommended)
   - Extend ValidationContext during ConfigValues → Config deserialization
   - Mark which paths were present in ConfigValues
   - Required validator checks this tracking

2. **Use serde-specific markers**
   - Leverage serde's visitor pattern
   - Track field presence during deserialize
   - More invasive, harder to implement

3. **Infer from source locations**
   - Fields with source locations were loaded
   - Fields without were defaulted
   - Simpler but less explicit

**Recommendation**: Use approach 1 for explicitness and testability.

### Error Message Quality

Focus on actionable error messages:

```
❌ Bad:
  field 'jwt_secret' validation failed

✅ Good:
  [env:APP_JWT_SECRET] Missing required field 'jwt_secret'
  Expected: String with minimum length 32 characters

  Checked environment variables:
    - APP_JWT_SECRET (with prefix APP_)

  Make sure to set: export APP_JWT_SECRET="your-secret-key-here"
```

### Interaction with Serde Defaults

Clear behavior definition:

```rust
#[derive(Validate)]
struct Config {
    // Required: Must be in env, no default fallback
    #[validate(required)]
    database_url: String,

    // Optional with default: Uses default if missing, no error
    #[serde(default = "default_host")]
    host: String,

    // Optional: None if missing, validated if present
    #[validate(optional, range(1..=100))]
    pool_size: Option<u32>,
}
```

Rules:
- `#[validate(required)]` + no `#[serde(default)]` = Error if missing
- `#[serde(default)]` + no `#[validate(required)]` = Use default, no error
- `Option<T>` + `#[validate(optional, ...)]` = None if missing, validate if present
- `#[validate(required)]` + `#[serde(default)]` = Compile error (conflicting intent)

## Migration and Compatibility

### Breaking Changes

**None** - This is purely additive:
- New derive macro attributes are opt-in
- Existing code continues to work unchanged
- No changes to public Env source API

### Migration Path

For users with manual validation code:

**Before** (90 lines):
```rust
impl Config {
    pub fn load<E: ConfigEnv>(env: &E) -> Result<Self, ConfigError> {
        let database_url = env
            .get_env("DATABASE_URL")
            .ok_or_else(|| ConfigError("DATABASE_URL is required".to_string()))?;

        let jwt_secret = env
            .get_env("JWT_SECRET")
            .ok_or_else(|| ConfigError("JWT_SECRET is required".to_string()))?;

        if jwt_secret.len() < 32 {
            return Err(ConfigError("JWT_SECRET must be at least 32 characters".to_string()));
        }

        // ... 80+ more lines
    }
}
```

**After** (20 lines):
```rust
#[derive(Debug, Deserialize, Validate)]
struct Config {
    #[validate(required)]
    database_url: String,

    #[validate(required, min_length(32))]
    jwt_secret: String,

    #[validate(required)]
    github_client_id: String,

    // ... just field definitions
}

// Usage:
let config = Config::<Config>::builder()
    .source(Env::prefix("APP_"))
    .build()?;
```

### Deprecation Timeline

No deprecations needed - this is additive only.

## Success Metrics

1. **Boilerplate Reduction**: 90+ line manual validation → <20 lines declarative
2. **Error Quality**: All missing/invalid env vars reported in single run
3. **Adoption**: At least one comprehensive example in examples/
4. **Performance**: <5% overhead vs manual validation
5. **Testing**: 100% test coverage for new validators
6. **Documentation**: Complete migration guide and patterns documentation

## Open Questions

1. **Should we support custom error messages per field?**
   ```rust
   #[validate(required, error = "JWT_SECRET must be set for authentication")]
   ```
   - Pro: Better user-facing error messages
   - Con: More complexity in derive macro

2. **Should validation context automatically track env var mappings?**
   - Currently would need to enhance Env source to populate context
   - Alternative: Infer from SourceLocation

3. **How to handle cross-field validation with required checks?**
   - Example: "If TLS enabled, cert path required"
   - Might need separate spec for cross-field patterns

## Future Enhancements

Potential follow-up specifications:

1. **Spec 004: Cross-field Validation Ergonomics**
   - Declarative cross-field constraints
   - Conditional required fields
   - Complex validation rules

2. **Spec 005: Configuration Profiles**
   - Environment-specific validation rules
   - Development vs production requirements
   - Profile-based defaults

3. **Spec 006: Configuration Documentation Generation**
   - Auto-generate config docs from validation attributes
   - Example .env file generation
   - Configuration schema export
