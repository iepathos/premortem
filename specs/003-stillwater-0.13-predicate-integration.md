---
number: 3
title: Stillwater 0.13 Upgrade and Predicate Integration
category: foundation
priority: high
status: draft
dependencies: []
created: 2025-11-28
---

# Specification 003: Stillwater 0.13 Upgrade and Predicate Integration

**Category**: foundation
**Priority**: high
**Status**: draft
**Dependencies**: None

## Context

Premortem currently uses stillwater 0.11.0. Stillwater 0.12.0 introduced a comprehensive **Predicate Combinators** module that provides composable validation logic, and 0.13.0 added **Validation `.ensure()` combinators** and **Bifunctor methods** for more ergonomic error handling.

Currently, premortem implements ~500 lines of custom validators in `src/validate.rs`:
- String validators: `NonEmpty`, `MinLength`, `MaxLength`, `Length`, `Pattern`, `Email`, `Url`
- Numeric validators: `Range`, `Positive`, `Negative`, `NonZero`
- Collection validators: `NonEmptyCollection`, `MinItems`, `MaxItems`, `Each`
- Path validators: `FileExists`, `DirExists`, `ParentExists`, `Extension`

Stillwater's predicate system provides equivalent functionality with better composability:
- **String predicates**: `not_empty()`, `len_between()`, `len_min()`, `len_max()`, `starts_with()`, `ends_with()`, `contains()`, `is_alphabetic()`, `is_alphanumeric()`, `is_numeric()`
- **Numeric predicates**: `gt()`, `ge()`, `lt()`, `le()`, `eq()`, `ne()`, `between()`, `positive()`, `negative()`, `non_negative()`
- **Collection predicates**: `has_len()`, `has_min_len()`, `has_max_len()`, `is_empty()`, `is_not_empty()`, `all()`, `any()`, `contains_element()`
- **Composition**: `.and()`, `.or()`, `.not()`, `all_of()`, `any_of()`, `none_of()`

The predicate system offers several advantages:
1. **Code reduction**: Eliminates ~300 lines of custom validator implementations
2. **Composability**: Natural composition with `.and()`, `.or()`, `.not()` combinators
3. **Testing**: Already tested with property-based tests in stillwater
4. **Zero-cost**: All predicates compile to concrete types with no heap allocation
5. **Consistency**: Same validation primitives across stillwater and premortem

The `.ensure()` family of combinators provides declarative validation syntax that reduces boilerplate in manual `Validate` implementations.

Bifunctor methods (`.bimap()`, `.fold()`, `.unwrap_or_else()`, etc.) provide convenience for error handling but are lower priority.

This specification focuses on:
1. Upgrading to stillwater 0.13.0
2. Integrating predicate support alongside existing validators
3. Creating a bridge between `Predicate<T>` and `Validator<T>` traits
4. Updating examples and documentation

**Note**: This is a non-breaking, incremental change. Existing validators remain supported, and the derive macro continues to work unchanged.

## Objective

Upgrade premortem to stillwater 0.13.0 and integrate the predicate combinators system to:
1. Provide composable validation primitives from stillwater
2. Bridge `Predicate<T>` trait with premortem's `Validator<T>` trait
3. Reduce code duplication between premortem and stillwater
4. Enable more ergonomic validation patterns with `.and()`, `.or()`, `.not()`
5. Maintain backward compatibility with existing validators and derive macros

## Requirements

### Functional Requirements

1. **Upgrade Dependency**: Update `Cargo.toml` to use `stillwater = "0.13.0"`
2. **Predicate Re-exports**: Re-export stillwater predicates from `premortem::prelude`
3. **Predicate-Validator Bridge**: Create adapter to use `Predicate<T>` as `Validator<T>`
4. **Source Location Preservation**: Ensure predicates maintain source location tracking
5. **Composition Support**: Enable `.and()`, `.or()`, `.not()` on predicates in validation context
6. **Derive Macro Compatibility**: Existing `#[validate(...)]` attributes continue to work
7. **Documentation**: Provide examples showing both validator and predicate patterns

### Non-Functional Requirements

1. **Zero Breaking Changes**: All existing code continues to work
2. **Zero Performance Regression**: Predicates should compile to equivalent code
3. **Consistent API**: Predicates should feel natural in premortem context
4. **Type Safety**: Bridge layer maintains full type safety
5. **Error Messages**: Clear error messages when using predicates incorrectly

## Acceptance Criteria

### Dependency Upgrade
- [ ] `Cargo.toml` updated to `stillwater = "0.13.0"`
- [ ] All existing tests pass with new version
- [ ] No deprecation warnings from stillwater usage

### Predicate Re-exports
- [ ] `premortem::prelude::*` includes stillwater predicates
- [ ] String predicates available: `not_empty()`, `len_between()`, `len_min()`, `len_max()`, `starts_with()`, `ends_with()`, `contains()`
- [ ] Numeric predicates available: `gt()`, `ge()`, `lt()`, `le()`, `eq()`, `ne()`, `between()`, `positive()`, `negative()`, `non_negative()`
- [ ] Collection predicates available: `has_len()`, `has_min_len()`, `has_max_len()`, `is_empty()`, `is_not_empty()`, `all()`, `any()`, `contains_element()`
- [ ] Composition functions available: `all_of()`, `any_of()`, `none_of()`

### Predicate-Validator Bridge
- [ ] `from_predicate(pred)` converts `Predicate<T>` to `impl Validator<T>`
- [ ] Bridge preserves source location tracking via thread-local context
- [ ] Composition works: `from_predicate(not_empty().and(len_min(3)))`
- [ ] Error messages include path and source location

### Validation API Extensions
- [ ] `validate_with_predicate(value, path, predicate)` helper function
- [ ] Works with existing `validate_field()` for backward compatibility
- [ ] Integrates with `ConfigValidation<T>` type

### Documentation and Examples
- [ ] Example showing predicate usage in manual `Validate` impl
- [ ] Example showing predicate composition (`.and()`, `.or()`, `.not()`)
- [ ] Documentation in CLAUDE.md explaining predicate integration
- [ ] Migration guide for users wanting to switch from validators to predicates
- [ ] Updated CHANGELOG.md documenting new features

### Backward Compatibility
- [ ] All existing validators continue to work
- [ ] Derive macro `#[validate(non_empty)]` still works
- [ ] All existing examples compile and run unchanged
- [ ] All existing tests pass

### Performance
- [ ] Predicates compile to zero-cost abstractions (verified by inspecting assembly)
- [ ] No heap allocations introduced by predicate bridge
- [ ] Benchmark shows no regression vs custom validators

## Technical Details

### Implementation Approach

#### 1. Update Dependency

```toml
# Cargo.toml
[dependencies]
stillwater = "0.13.0"
```

Run tests to ensure compatibility:
```bash
cargo test --all-features
```

#### 2. Re-export Predicates

```rust
// src/lib.rs
pub use stillwater::predicate::{
    // Re-export predicate module for direct access
    predicate,
    // Re-export commonly used types
    Predicate, PredicateExt,
};

// src/prelude.rs
pub use stillwater::predicate::prelude::*;
```

This makes predicates available in two ways:
```rust
use premortem::prelude::*;  // All common predicates
use premortem::predicate::*; // Explicit predicate module access
```

#### 3. Create Predicate-Validator Bridge

```rust
// src/validate.rs

use stillwater::predicate::Predicate;

/// Adapter that converts a `Predicate<T>` into a `Validator<T>`.
///
/// This enables using stillwater's composable predicates within premortem's
/// validation framework while maintaining source location tracking.
///
/// # Example
///
/// ```ignore
/// use premortem::validate::from_predicate;
/// use stillwater::predicate::*;
///
/// let validator = from_predicate(
///     not_empty().and(len_between(3, 20))
/// );
///
/// // Use in validate_field
/// validate_field(&username, "username", &[&validator])
/// ```
pub fn from_predicate<T>(predicate: impl Predicate<T>) -> impl Validator<T>
where
    T: ?Sized,
{
    PredicateValidator(predicate)
}

/// Internal adapter struct that implements Validator for any Predicate.
struct PredicateValidator<P>(P);

impl<T, P> Validator<T> for PredicateValidator<P>
where
    T: ?Sized,
    P: Predicate<T>,
{
    fn validate(&self, value: &T, path: &str) -> ConfigValidation<()> {
        if self.0.test(value) {
            Validation::Success(())
        } else {
            // Get source location from thread-local context
            let source_location = current_source_location(path);

            Validation::Failure(ConfigErrors::single(
                ConfigError::ValidationError {
                    path: path.to_string(),
                    source_location,
                    value: None, // Predicates don't provide value formatting
                    message: "validation failed".to_string(),
                }
            ))
        }
    }
}

/// Validate a field using a predicate with custom error message.
///
/// This is a convenience function that combines predicate testing with
/// premortem's error accumulation.
///
/// # Example
///
/// ```ignore
/// use premortem::validate::validate_with_predicate;
/// use stillwater::predicate::*;
///
/// validate_with_predicate(
///     &port,
///     "port",
///     between(1, 65535),
///     "port must be between 1 and 65535"
/// )
/// ```
pub fn validate_with_predicate<T>(
    value: &T,
    path: &str,
    predicate: impl Predicate<T>,
    message: impl Into<String>,
) -> ConfigValidation<()>
where
    T: ?Sized,
{
    if predicate.test(value) {
        Validation::Success(())
    } else {
        let source_location = current_source_location(path);

        Validation::Failure(ConfigErrors::single(
            ConfigError::ValidationError {
                path: path.to_string(),
                source_location,
                value: None,
                message: message.into(),
            }
        ))
    }
}
```

#### 4. Extend Prelude

```rust
// src/prelude.rs

pub use crate::validate::{from_predicate, validate_with_predicate};

// Re-export stillwater predicates
pub use stillwater::predicate::prelude::*;
```

#### 5. Update Documentation

Add to `CLAUDE.md`:

```markdown
## Validation with Predicates

Premortem integrates stillwater's predicate combinators for composable validation.

### Basic Predicate Usage

```rust
use premortem::prelude::*;
use premortem::validate::validate_with_predicate;

impl Validate for ServerConfig {
    fn validate(&self) -> ConfigValidation<()> {
        let validations = vec![
            validate_with_predicate(
                &self.host,
                "host",
                not_empty(),
                "host cannot be empty"
            ),
            validate_with_predicate(
                &self.port,
                "port",
                between(1, 65535),
                "port must be in range 1-65535"
            ),
        ];
        Validation::all_vec(validations).map(|_| ())
    }
}
```

### Predicate Composition

Predicates can be composed using `.and()`, `.or()`, and `.not()`:

```rust
let username_pred = len_between(3, 20)
    .and(is_alphanumeric())
    .and(starts_with(|c: &char| c.is_alphabetic()));

validate_with_predicate(
    &username,
    "username",
    username_pred,
    "username must be 3-20 alphanumeric chars starting with a letter"
)
```

### Using Predicates with validate_field

You can use predicates with the existing `validate_field` API via the bridge:

```rust
use premortem::validate::{from_predicate, validate_field};

validate_field(
    &email,
    "email",
    &[&from_predicate(not_empty().and(contains('@')))]
)
```

### Available Predicates

**String Predicates:**
- `not_empty()` - string is not empty
- `len_between(min, max)` - length in range
- `len_min(n)`, `len_max(n)` - minimum/maximum length
- `starts_with(prefix)`, `ends_with(suffix)` - prefix/suffix match
- `contains(substring)` - contains substring
- `is_alphabetic()`, `is_alphanumeric()`, `is_numeric()` - character type checks

**Numeric Predicates:**
- `between(min, max)` - value in range (inclusive)
- `gt(n)`, `ge(n)`, `lt(n)`, `le(n)` - comparison predicates
- `eq(n)`, `ne(n)` - equality predicates
- `positive()`, `negative()`, `non_negative()` - sign predicates

**Collection Predicates:**
- `has_len(n)` - collection has exact length
- `has_min_len(n)`, `has_max_len(n)` - length bounds
- `is_empty()`, `is_not_empty()` - emptiness checks
- `all(predicate)` - all elements match predicate
- `any(predicate)` - any element matches predicate
- `contains_element(value)` - collection contains value

**Composition:**
- `predicate.and(other)` - both must be true
- `predicate.or(other)` - either must be true
- `predicate.not()` - invert predicate
- `all_of([p1, p2, p3])` - all predicates must match
- `any_of([p1, p2, p3])` - any predicate must match
- `none_of([p1, p2, p3])` - no predicate must match
```

### Architecture Changes

1. **New exports in `src/lib.rs`**:
   - Re-export `stillwater::predicate` module
   - Re-export `Predicate` and `PredicateExt` traits

2. **Modified `src/validate.rs`**:
   - Add `from_predicate()` function
   - Add `PredicateValidator` adapter struct
   - Add `validate_with_predicate()` helper

3. **Modified `src/prelude.rs`**:
   - Re-export stillwater predicate prelude
   - Include new validation helpers

4. **New example `examples/predicates/main.rs`**:
   - Demonstrate predicate usage
   - Show composition patterns
   - Compare with existing validators

### Data Structures

#### PredicateValidator

```rust
/// Internal adapter that wraps a Predicate to implement Validator.
///
/// Maintains source location tracking via thread-local context.
struct PredicateValidator<P>(P);

impl<T, P> Validator<T> for PredicateValidator<P>
where
    T: ?Sized,
    P: Predicate<T>,
{
    fn validate(&self, value: &T, path: &str) -> ConfigValidation<()> {
        // Test predicate
        // Fetch source location from thread-local
        // Return Success or Failure with ConfigError::ValidationError
    }
}
```

### APIs and Interfaces

#### Public API Additions

```rust
// New functions in src/validate.rs
pub fn from_predicate<T>(predicate: impl Predicate<T>) -> impl Validator<T>;

pub fn validate_with_predicate<T>(
    value: &T,
    path: &str,
    predicate: impl Predicate<T>,
    message: impl Into<String>,
) -> ConfigValidation<()>;
```

#### Re-exports

```rust
// In src/lib.rs
pub use stillwater::predicate::{self, Predicate, PredicateExt};

// In src/prelude.rs
pub use stillwater::predicate::prelude::*;
pub use crate::validate::{from_predicate, validate_with_predicate};
```

## Dependencies

- **Prerequisites**: None
- **Affected Components**:
  - `Cargo.toml` - dependency version
  - `src/lib.rs` - re-exports
  - `src/prelude.rs` - predicate re-exports
  - `src/validate.rs` - bridge implementation
  - `examples/` - new predicate examples
  - `CLAUDE.md` - documentation updates
  - `CHANGELOG.md` - version history
- **External Dependencies**:
  - `stillwater` crate upgraded to 0.13.0

## Testing Strategy

### Unit Tests

1. **Bridge Functionality** (`src/validate.rs`)
   - `test_from_predicate_success` - Predicate that passes
   - `test_from_predicate_failure` - Predicate that fails
   - `test_predicate_composition_and` - `.and()` combinator
   - `test_predicate_composition_or` - `.or()` combinator
   - `test_predicate_composition_not` - `.not()` combinator
   - `test_validate_with_predicate_success` - Helper function success
   - `test_validate_with_predicate_failure` - Helper function failure
   - `test_validate_with_predicate_custom_message` - Custom error message

2. **Source Location Tracking**
   - `test_predicate_preserves_source_location` - Source location in errors
   - `test_predicate_with_nested_validation` - Path prefix handling

3. **Integration with Existing Validators**
   - `test_predicate_and_validator_together` - Mix predicates and validators
   - `test_validate_field_with_predicate` - Use via `validate_field()`

4. **Composition Patterns**
   - `test_complex_string_predicate` - `not_empty().and(len_between(3, 20))`
   - `test_numeric_range_predicate` - `gt(0).and(le(100))`
   - `test_all_of_combinator` - `all_of([p1, p2, p3])`
   - `test_any_of_combinator` - `any_of([p1, p2, p3])`

### Integration Tests

1. **Full Config Validation with Predicates**
   - Create config struct using predicates in `Validate` impl
   - Load from TOML and validate
   - Verify error accumulation works correctly

2. **Backward Compatibility**
   - Existing examples continue to compile
   - Existing tests continue to pass
   - No warnings or deprecations

### Performance Tests

1. **Zero-Cost Abstraction Verification**
   - Create benchmark comparing custom validators vs predicates
   - Verify no performance regression
   - Inspect generated assembly to confirm optimization

### User Acceptance

- Predicates should feel natural and ergonomic
- Error messages should be clear and actionable
- Documentation should make migration path obvious

## Documentation Requirements

### Code Documentation

- Module-level docs in `src/validate.rs` explaining predicate integration
- Comprehensive rustdoc on `from_predicate()` with examples
- Rustdoc on `validate_with_predicate()` with examples
- Examples showing composition patterns

### User Documentation

- **CLAUDE.md**:
  - Add "Validation with Predicates" section
  - Document all available predicates
  - Show composition patterns
  - Provide migration examples

- **CHANGELOG.md**:
  - Document stillwater 0.13.0 upgrade
  - List new predicate features
  - Note backward compatibility

- **README.md**:
  - Update feature list to mention predicates
  - Add example showing predicate usage

### Architecture Updates

- Document predicate-validator bridge pattern
- Explain how source location tracking works with predicates
- Note integration with existing validation system

## Implementation Notes

### Custom Error Messages

The bridge adapter uses a generic "validation failed" message by default. For custom messages, users should use `validate_with_predicate()`:

```rust
// Generic message
from_predicate(positive())

// Custom message
validate_with_predicate(&value, "field", positive(), "must be positive")
```

### Performance Considerations

Predicates compile to concrete types via monomorphization. The `PredicateValidator` wrapper should inline completely, resulting in zero-cost abstraction.

Verify with:
```bash
cargo rustc --release -- --emit asm
# Check that PredicateValidator doesn't appear in assembly
```

### Future Enhancements

After this initial integration, consider:

1. **Derive Macro Support** (v0.6.0):
   ```rust
   #[derive(DeriveValidate)]
   struct Config {
       #[validate(predicate = "not_empty().and(len_min(3))")]
       username: String,
   }
   ```

2. **Predicate-Specific Error Messages** (v0.6.0):
   Enhance bridge to extract semantic meaning from predicates for better error messages

3. **Custom Predicate Helpers** (v0.6.0):
   Provide premortem-specific predicates like `valid_email()`, `valid_url()`

4. **Replace Custom Validators** (v0.7.0):
   After proving predicate system works, consider deprecating custom validators

## Migration and Compatibility

### Breaking Changes

None. This is a purely additive change.

### Migration Requirements

None. Existing code continues to work unchanged.

### Compatibility Considerations

- Predicates work alongside existing validators
- Users can incrementally adopt predicates
- Derive macro continues to work with existing validator syntax
- No performance impact on code not using predicates

### Migration Path for Users

**Optional migration** from custom validators to predicates:

Before:
```rust
use premortem::validate::validators::*;

validate_field(&host, "host", &[&NonEmpty, &MinLength(3)])
```

After:
```rust
use premortem::prelude::*;

validate_with_predicate(
    &host,
    "host",
    not_empty().and(len_min(3)),
    "host must be non-empty and at least 3 characters"
)
```

Users can choose to migrate when it makes sense for their codebase.

## Implementation Phases

### Phase 1: Basic Integration (v0.5.0)
- [ ] Update dependency to stillwater 0.13.0
- [ ] Re-export predicates from prelude
- [ ] Implement `from_predicate()` bridge
- [ ] Implement `validate_with_predicate()` helper
- [ ] Add unit tests for bridge
- [ ] Update CLAUDE.md with predicate documentation

### Phase 2: Examples and Polish (v0.5.0)
- [ ] Create `examples/predicates/main.rs`
- [ ] Update existing examples to show predicates option
- [ ] Add integration tests
- [ ] Update CHANGELOG.md
- [ ] Update README.md

### Phase 3: Performance Validation (v0.5.0)
- [ ] Create benchmarks
- [ ] Verify zero-cost abstraction
- [ ] Document performance characteristics

### Future: Advanced Features (v0.6.0+)
- [ ] Derive macro support for predicates
- [ ] Enhanced error messages from predicate semantics
- [ ] Custom premortem-specific predicates
- [ ] Consider deprecating custom validators

## Success Metrics

1. **Code Reduction**: Predicates available to users, reducing need for custom validators
2. **Zero Breaking Changes**: All existing tests pass
3. **Zero Performance Regression**: Benchmarks show no slowdown
4. **User Adoption**: Examples demonstrate predicate usage clearly
5. **Documentation Quality**: Users understand how to use predicates from docs alone

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Stillwater 0.13 breaking changes | High | Review changelog, run full test suite |
| Performance regression from bridge | Medium | Benchmark and verify zero-cost via assembly inspection |
| User confusion about two validation styles | Medium | Clear documentation showing when to use each |
| Source location tracking broken | High | Comprehensive tests of thread-local context |
| Predicate errors are too generic | Low | Provide `validate_with_predicate()` for custom messages |

## Appendix: Stillwater 0.13 Feature Summary

### Predicate Combinators (0.12.0)
- Composable validation logic
- String, numeric, and collection predicates
- Logical combinators: `.and()`, `.or()`, `.not()`
- Zero-cost abstraction

### Validation Combinators (0.12.0)
- `.ensure()` family for declarative validation
- Reduces boilerplate in manual implementations
- Works with both `Predicate` trait and closures

### Bifunctor Interface (0.13.0)
- `.bimap()`, `.fold()`, `.unwrap_or_else()`, `.unwrap_or_default()`
- Convenience methods for error handling
- Lower priority for premortem integration

### Error Recovery Combinators (0.13.0)
- `.recover()`, `.fallback()`, `.fallback_to()`
- Requires Effect-based architecture
- Not applicable to current premortem (sync config loading)
