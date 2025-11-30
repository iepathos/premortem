---
number: 3
title: Environment Variable Validation Ergonomics
category: foundation
priority: high
status: draft
dependencies: []
created: 2025-11-30
updated: 2025-11-30
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
- **Wrong abstraction layer** - Loading and validation mixed in user code

### What Already Works Well

- `Env::prefix("APP_")` - Prefix filtering
- `APP_DATABASE_HOST` → `database.host` - Nested path mapping
- Type inference - Automatic parsing of integers, floats, booleans
- `#[validate(range(1..=65535))]` - Derive macro validators for value validation
- MockEnv for testing

### The Gap

The gap is in **source-level validation ergonomics**. Users need:
1. Declarative way to mark environment variables as required
2. Validation during loading (before deserialization)
3. Accumulation of ALL missing/invalid env vars before failing
4. Clear error messages showing which env vars are missing

## Objective

Reduce boilerplate for environment-variable-driven configuration from 90+ lines of imperative code to ~15 lines of declarative code, while maintaining full error accumulation and improving error messages with source location tracking.

## Requirements

### Functional Requirements

1. **Required Environment Variable Declaration**
   - Provide declarative way to mark env vars as required at the source level
   - Generate clear error messages when required env vars are missing
   - Accumulate ALL missing env vars before failing (not fail-fast)

2. **Source-Level Validation**
   - Validate during `Source::load()`, before deserialization
   - Keep validation in the imperative shell (I/O layer)
   - Maintain separation: loading concerns at source layer, value validation at validation layer

3. **Better Error Messages**
   - Show full environment variable name (including prefix)
   - Provide actionable suggestions (export commands)
   - Include source location metadata
   - Group errors by source for clarity

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
   - Document best practices for required vs optional env vars

4. **Maintainability**
   - Follow existing premortem code organization
   - Consistent with stillwater functional patterns
   - Clear separation of concerns

## Acceptance Criteria

- [ ] Can mark env vars as required with `.require()` on Env source
- [ ] Can mark multiple env vars as required with `.require_all()`
- [ ] Missing required env vars accumulate errors (not fail-fast)
- [ ] Error messages include full environment variable name with prefix
- [ ] Error messages provide actionable suggestions (export commands)
- [ ] Comprehensive example showing env-heavy configuration
- [ ] Example reduces 90-line manual config to ~15 lines declarative
- [ ] All functionality works with MockEnv for testing
- [ ] Documentation includes migration guide from manual patterns
- [ ] No breaking changes to existing Env source API
- [ ] Performance benchmarks show <5% overhead vs manual validation
- [ ] Integration tests validate error accumulation behavior

## Technical Details

### Implementation Approach

Extend the `Env` source with source-level validation that runs during `load()`:

```rust
use premortem::prelude::*;

// Declarative source-level required env vars
let config = Config::<AppConfig>::builder()
    .source(
        Env::prefix("APP_")
            .require("JWT_SECRET")
            .require("DATABASE_URL")
            .require("GITHUB_CLIENT_ID")
    )
    .build()?;

// Or use require_all for multiple vars
let config = Config::<AppConfig>::builder()
    .source(
        Env::prefix("APP_")
            .require_all(&["JWT_SECRET", "DATABASE_URL", "GITHUB_CLIENT_ID"])
    )
    .build()?;
```

**Why this approach**:
- ✅ Validates during loading, before deserialization
- ✅ Can actually detect missing env vars (not confused with defaults)
- ✅ Maintains separation of concerns (loading vs validation)
- ✅ No complex serde integration needed
- ✅ Accumulates all errors before failing
- ✅ Simple, focused implementation

**Value validation** (length, range, etc.) remains at the validation layer:

```rust
#[derive(Debug, Deserialize, Validate)]
struct AppConfig {
    #[validate(min_length(32))]  // Value validation, not presence
    jwt_secret: String,

    database_url: String,  // Presence enforced at source level
}
```

### Architecture Changes

1. **Extend Env source** (src/sources/env_source.rs)
   - Add `required_vars: HashSet<String>` field
   - Add `.require(var_name)` builder method
   - Add `.require_all(vars)` builder method
   - Check required vars during `load()`, accumulate errors

2. **No changes needed to**:
   - Derive macro
   - ValidationContext
   - Serde integration

### Data Structures

```rust
// In src/sources/env_source.rs

#[derive(Debug, Clone)]
pub struct Env {
    prefix: String,
    separator: String,
    case_sensitive: bool,
    list_separator: Option<String>,
    custom_mappings: HashMap<String, String>,
    excluded: HashSet<String>,
    // NEW: Track required environment variables
    required_vars: HashSet<String>,
}
```

### APIs and Interfaces

**New Env source methods**:

```rust
impl Env {
    /// Mark an environment variable as required.
    ///
    /// The variable name should be specified WITHOUT the prefix.
    /// For example, with `Env::prefix("APP_")`:
    /// - `.require("DATABASE_URL")` checks for `APP_DATABASE_URL`
    ///
    /// # Example
    ///
    /// ```
    /// use premortem::Env;
    ///
    /// let source = Env::prefix("APP_")
    ///     .require("JWT_SECRET")
    ///     .require("DATABASE_URL");
    /// ```
    pub fn require(mut self, var_name: impl Into<String>) -> Self {
        self.required_vars.insert(var_name.into());
        self
    }

    /// Mark multiple environment variables as required.
    ///
    /// Convenience method for requiring multiple variables at once.
    ///
    /// # Example
    ///
    /// ```
    /// use premortem::Env;
    ///
    /// let source = Env::prefix("APP_")
    ///     .require_all(&["JWT_SECRET", "DATABASE_URL", "API_KEY"]);
    /// ```
    pub fn require_all(mut self, var_names: &[&str]) -> Self {
        for name in var_names {
            self.required_vars.insert(name.to_string());
        }
        self
    }
}
```

**Enhanced error messages**:

```
Before:
  Configuration error: DATABASE_URL is required

After:
  Configuration error:
    [env:APP_JWT_SECRET] Missing required environment variable
    [env:APP_DATABASE_URL] Missing required environment variable
    [env:APP_GITHUB_CLIENT_ID] Missing required environment variable

  To fix, set these environment variables:
    export APP_JWT_SECRET="your-secret-here"
    export APP_DATABASE_URL="postgresql://localhost/mydb"
    export APP_GITHUB_CLIENT_ID="your-client-id"
```

### Implementation Details

Modify `Env::load()` to check required vars first:

```rust
impl Source for Env {
    fn load(&self, env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors> {
        let mut errors = Vec::new();

        // FIRST: Check all required environment variables
        for var_name in &self.required_vars {
            let full_name = if self.prefix.is_empty() {
                var_name.clone()
            } else {
                format!("{}{}", self.prefix, var_name)
            };

            if env.get_env(&full_name).is_none() {
                errors.push(ConfigError::MissingField {
                    path: self.var_to_path(var_name),
                    source_location: Some(SourceLocation::env(&full_name)),
                    message: format!(
                        "required environment variable {} is not set",
                        full_name
                    ),
                });
            }
        }

        // If any required vars are missing, fail with all errors accumulated
        if !errors.is_empty() {
            return Err(ConfigErrors::from_vec(errors).unwrap());
        }

        // Continue with normal loading for vars that are present
        // ... existing load logic ...
    }
}
```

## Dependencies

- **Prerequisites**: None (extends existing functionality)
- **Affected Components**:
  - `src/sources/env_source.rs` - Add required var tracking and validation
  - `src/error.rs` - May enhance error formatting for better suggestions
- **External Dependencies**: None (uses existing dependencies)

## Testing Strategy

### Unit Tests

1. **Env source tests** (in src/sources/env_source.rs):
   ```rust
   #[test]
   fn test_require_single_var() {
       let env = MockEnv::new()
           .with_env("APP_JWT_SECRET", "secret123");

       let source = Env::prefix("APP_").require("JWT_SECRET");
       let result = source.load(&env);
       assert!(result.is_ok());
   }

   #[test]
   fn test_require_missing_var_fails() {
       let env = MockEnv::new(); // Empty

       let source = Env::prefix("APP_").require("JWT_SECRET");
       let result = source.load(&env);

       assert!(result.is_err());
       let errors = result.unwrap_err();
       assert_eq!(errors.len(), 1);
       assert!(errors.first().message().contains("APP_JWT_SECRET"));
   }

   #[test]
   fn test_require_all_accumulates_errors() {
       let env = MockEnv::new()
           .with_env("APP_JWT_SECRET", "secret");
       // Missing DATABASE_URL and API_KEY

       let source = Env::prefix("APP_")
           .require_all(&["JWT_SECRET", "DATABASE_URL", "API_KEY"]);
       let result = source.load(&env);

       assert!(result.is_err());
       let errors = result.unwrap_err();
       // Should have 2 errors (DATABASE_URL and API_KEY missing)
       assert_eq!(errors.len(), 2);
   }
   ```

2. **Integration tests** (tests/env_validation.rs):
   ```rust
   #[test]
   fn test_full_config_with_required_env_vars() {
       let env = MockEnv::new()
           .with_env("APP_JWT_SECRET", "very-long-secret-key-here-32-chars")
           .with_env("APP_DATABASE_URL", "postgresql://localhost/db");

       let result = Config::<AppConfig>::builder()
           .source(
               Env::prefix("APP_")
                   .require_all(&["JWT_SECRET", "DATABASE_URL"])
           )
           .build_with_env(&env);

       assert!(result.is_ok());
   }
   ```

### Performance Tests

```rust
#[bench]
fn bench_required_var_checking(b: &mut Bencher) {
    let env = MockEnv::new()
        .with_env("APP_VAR1", "value1")
        .with_env("APP_VAR2", "value2")
        .with_env("APP_VAR3", "value3");

    let source = Env::prefix("APP_")
        .require_all(&["VAR1", "VAR2", "VAR3"]);

    b.iter(|| {
        source.load(&env)
    });
}
```

Target: <1ms overhead for checking 10-20 required variables

### User Acceptance

Create comprehensive example (`examples/env-validation/`) that:
- Mirrors real-world usage (like maat-api configuration)
- Shows before/after comparison
- Demonstrates error accumulation
- Includes test cases with MockEnv

## Documentation Requirements

### Code Documentation

1. **Env source methods**:
   ```rust
   /// Mark an environment variable as required.
   ///
   /// The variable name should be specified WITHOUT the prefix.
   /// Missing required variables will cause `load()` to fail with
   /// accumulated errors for ALL missing variables.
   ///
   /// # Example
   ///
   /// ```rust
   /// use premortem::Env;
   ///
   /// let source = Env::prefix("APP_")
   ///     .require("JWT_SECRET")
   ///     .require("DATABASE_URL");
   /// ```
   pub fn require(mut self, var_name: impl Into<String>) -> Self
   ```

2. **Error documentation**:
   - Document MissingField error variant for env vars
   - Show example error output
   - Explain error accumulation behavior

### User Documentation

1. **New example**: `examples/env-validation/`
   - Comprehensive env-driven configuration
   - Before/after showing boilerplate reduction
   - Error accumulation demonstration
   - Testing patterns with MockEnv

2. **Update `CLAUDE.md`**:
   - Add "Environment Variable Validation" section
   - Show required vs optional env var patterns
   - Document best practices
   - Include migration guide from manual validation

3. **Update README.md**:
   - Add env validation to feature list
   - Include quick example showing ergonomics improvement

## Migration and Compatibility

### Breaking Changes

**None** - This is purely additive:
- New methods on Env source are opt-in
- Existing code continues to work unchanged
- No changes to validation layer

### Migration Path

For users with manual validation code:

**Before** (90 lines):
```rust
impl Config {
    pub fn load<E: ConfigEnv>(env: &E) -> Result<Self, ConfigError> {
        let database_url = env
            .get_env("APP_DATABASE_URL")
            .ok_or_else(|| ConfigError("DATABASE_URL is required".to_string()))?;

        let jwt_secret = env
            .get_env("APP_JWT_SECRET")
            .ok_or_else(|| ConfigError("JWT_SECRET is required".to_string()))?;

        if jwt_secret.len() < 32 {
            return Err(ConfigError("JWT_SECRET must be at least 32 characters".to_string()));
        }

        let github_client_id = env
            .get_env("APP_GITHUB_CLIENT_ID")
            .ok_or_else(|| ConfigError("GITHUB_CLIENT_ID is required".to_string()))?;

        // ... 70+ more lines

        Ok(Self {
            database_url,
            jwt_secret,
            github_client_id,
            // ... more fields
        })
    }
}
```

**After** (15 lines):
```rust
#[derive(Debug, Deserialize, Validate)]
struct Config {
    database_url: String,

    #[validate(min_length(32))]  // Value validation
    jwt_secret: String,

    github_client_id: String,
}

// Usage with source-level required vars
let config = Config::<Config>::builder()
    .source(
        Env::prefix("APP_")
            .require_all(&["DATABASE_URL", "JWT_SECRET", "GITHUB_CLIENT_ID"])
    )
    .build()?;
```

**Key differences**:
- Presence checking at source level (15 lines → 3 lines)
- Value validation at validation layer (stays concise)
- Error accumulation built-in
- Clear separation of concerns

### Deprecation Timeline

No deprecations needed - this is additive only.

## Success Metrics

1. **Boilerplate Reduction**: 90+ line manual validation → ~15 lines declarative
2. **Error Quality**: All missing env vars reported in single run
3. **Separation of Concerns**: Loading validation at source layer, value validation at validation layer
4. **Adoption**: At least one comprehensive example in examples/
5. **Performance**: <5% overhead vs manual validation
6. **Testing**: 100% test coverage for new functionality
7. **Documentation**: Complete migration guide and patterns documentation

## Implementation Plan

### Phase 1: Extend Env Source (1-2 days)

1. **Add required var tracking**:
   - Add `required_vars: HashSet<String>` to `Env` struct
   - Implement `.require()` method
   - Implement `.require_all()` method

2. **Modify load() to check required vars**:
   - Check all required vars before loading
   - Accumulate missing var errors
   - Return all errors if any are missing

3. **Add unit tests**:
   - Test single required var
   - Test multiple required vars
   - Test error accumulation
   - Test with MockEnv

### Phase 2: Enhance Error Messages (1 day)

1. **Improve error formatting**:
   - Show full env var name in errors
   - Add actionable suggestions (export commands)
   - Group errors by source

2. **Add error message tests**:
   - Test error format
   - Test suggestions generation

### Phase 3: Documentation & Examples (1 day)

1. **Create comprehensive example**:
   - `examples/env-validation/` with before/after
   - Real-world configuration scenario
   - Error handling demonstration
   - Tests with MockEnv

2. **Update documentation**:
   - Add to CLAUDE.md
   - Update README.md
   - Write migration guide

### Phase 4: Performance & Polish (1 day)

1. **Performance testing**:
   - Benchmark required var checking
   - Ensure <5% overhead
   - Optimize if needed

2. **Integration testing**:
   - Full config loading flow
   - Backward compatibility checks
   - Edge cases

**Total estimated time**: 3-4 days

## Open Questions

1. **Should we support validators at the source level?**
   ```rust
   Env::prefix("APP_")
       .require("JWT_SECRET")
       .validate("JWT_SECRET", min_length(32))
   ```
   - Pro: All env var concerns in one place
   - Con: Mixes loading and validation concerns
   - **Recommendation**: No - keep value validation at validation layer

2. **Should we support custom error messages?**
   ```rust
   Env::prefix("APP_")
       .require_with_message("JWT_SECRET", "JWT_SECRET is required for authentication")
   ```
   - Pro: More user-friendly errors
   - Con: More API surface
   - **Recommendation**: Maybe as follow-up enhancement

3. **Should we auto-generate .env.example files?**
   - Could inspect required vars and generate example file
   - Useful for onboarding developers
   - **Recommendation**: Future enhancement (Spec 006)

## Future Enhancements

Potential follow-up specifications:

1. **Spec 004: Cross-field Validation Ergonomics**
   - Declarative cross-field constraints
   - Conditional required fields (e.g., "if TLS enabled, cert path required")

2. **Spec 005: Configuration Profiles**
   - Environment-specific required variables
   - Development vs production requirements
   - Profile-based defaults

3. **Spec 006: Configuration Documentation Generation**
   - Auto-generate .env.example from required vars
   - Generate configuration documentation
   - Configuration schema export
