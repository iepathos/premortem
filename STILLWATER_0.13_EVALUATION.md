# Stillwater 0.13 Evaluation for Premortem

**Current version:** premortem uses stillwater 0.11.0
**Latest version:** stillwater 0.13.0

## Summary

Stillwater 0.12 and 0.13 introduce several features that could significantly improve premortem, particularly the **Predicate Combinators** module which could replace and unify premortem's existing validator system.

## Priority 1: Predicate Combinators (0.12.0) ⭐⭐⭐

### What's New
- `Predicate<T>` trait with composable validation logic
- Logical combinators: `.and()`, `.or()`, `.not()`, `all_of()`, `any_of()`, `none_of()`
- Built-in predicates:
  - **String**: `not_empty()`, `len_between()`, `len_min()`, `len_max()`, `starts_with()`, `ends_with()`, `contains()`, `is_alphabetic()`, `is_alphanumeric()`, `is_numeric()`
  - **Numeric**: `gt()`, `ge()`, `lt()`, `le()`, `eq()`, `ne()`, `between()`, `positive()`, `negative()`, `non_negative()`
  - **Collection**: `has_len()`, `has_min_len()`, `has_max_len()`, `is_empty()`, `is_not_empty()`, `all()`, `any()`, `contains_element()`

### How Premortem Could Use This

#### Current Premortem Validators
```rust
// src/validate.rs - Current implementation
pub struct NonEmpty;
pub struct MinLength(pub usize);
pub struct MaxLength(pub usize);
pub struct Length(pub RangeInclusive<usize>);
pub struct Range<T>(pub RangeInclusive<T>);
pub struct Positive;
pub struct Negative;
pub struct NonZero;
```

#### With Stillwater Predicates
```rust
// Could replace with composable predicates:
use stillwater::predicate::*;

// Instead of validate_field(&host, "host", &[&NonEmpty])
// Could use: validate_field(&host, "host", not_empty())

// Instead of validate_field(&port, "port", &[&Range(1..=65535)])
// Could use: validate_field(&port, "port", between(1, 65535))

// Composition becomes easier:
let username_pred = len_between(3, 20)
    .and(is_alphanumeric())
    .and(starts_with(|c| c.is_alphabetic()));
```

#### Integration Path
1. **Phase 1**: Add stillwater predicates as alternative to existing validators
2. **Phase 2**: Integrate `Predicate<T>` trait with `Validator<T>` trait
3. **Phase 3**: Provide bridge between stillwater predicates and premortem validators
4. **Phase 4**: Consider deprecating custom validators in favor of predicates

### Benefits
- ✅ **Code reduction**: Remove ~500 lines of custom validator implementations
- ✅ **Composability**: `.and()`, `.or()`, `.not()` for complex validations
- ✅ **Consistency**: Same validation primitives across stillwater and premortem
- ✅ **Testing**: Stillwater predicates are already well-tested with property-based tests
- ✅ **Zero-cost**: All predicates compile to concrete types with no heap allocation

### Challenges
- Current `Validator<T>` trait uses `(value, path)` signature for source location tracking
- Stillwater predicates use `Predicate<T>` trait with just `(value)` signature
- Would need adapter layer to preserve source location information
- Need to maintain backward compatibility with existing validator macros

## Priority 2: Validation Combinators (0.12.0) ⭐⭐

### What's New
- `.ensure()` family for declarative validation
- For `Validation<T, E>`:
  - `.ensure(predicate, error)` - with `Predicate` trait
  - `.ensure_fn(predicate, error)` - with closure
  - `.ensure_with(predicate, error_fn)` - with lazy error factory
  - `.ensure_fn_with(predicate, error_fn)` - closure + lazy error
  - `.unless(predicate, error)` - inverse validation
  - `.filter_or(predicate, error)` - alias for `ensure_fn`

### How Premortem Could Use This

#### Current Pattern (from examples/validation/main.rs)
```rust
impl Validate for RangeConfig {
    fn validate(&self) -> ConfigValidation<()> {
        if self.min_value >= self.max_value {
            Validation::Failure(ConfigErrors::single(ConfigError::CrossFieldError {
                paths: vec!["min_value".to_string(), "max_value".to_string()],
                message: "min_value must be less than max_value".to_string(),
            }))
        } else {
            Validation::Success(())
        }
    }
}
```

#### With `.ensure()` Combinators
```rust
impl Validate for RangeConfig {
    fn validate(&self) -> ConfigValidation<()> {
        Validation::Success(())
            .ensure_fn(
                |_| self.min_value < self.max_value,
                ConfigError::CrossFieldError {
                    paths: vec!["min_value".to_string(), "max_value".to_string()],
                    message: "min_value must be less than max_value".to_string(),
                }
            )
    }
}
```

Or with multiple validations:
```rust
impl Validate for ServerConfig {
    fn validate(&self) -> ConfigValidation<()> {
        Validation::Success(())
            .ensure_fn(
                |_| !self.host.is_empty(),
                ConfigError::validation("host", "cannot be empty")
            )
            .ensure_fn(
                |_| self.port > 0 && self.port <= 65535,
                ConfigError::validation("port", "must be in range 1-65535")
            )
    }
}
```

### Benefits
- ✅ **Code reduction**: Reduces verbose `if/else` validation patterns
- ✅ **Readability**: Declarative style is easier to understand
- ✅ **Chaining**: Natural composition of multiple validations
- ✅ **Zero-cost**: Compiles to concrete types

### Challenges
- Current validation code uses `validate_field()` helper extensively
- Would require rethinking validation API to use `.ensure()` style
- May not reduce code significantly for derive macro users

## Priority 3: Error Recovery Combinators (0.13.0) ⭐

### What's New
- `.recover(predicate, handler)` - Recover from specific errors
- `.recover_with(predicate, handler)` - Result-returning handler
- `.recover_some(handler)` - Pattern-matching recovery
- `.fallback(value)` - Default value on error
- `.fallback_to(effect)` - Alternative effect on error

### How Premortem Could Use This

#### Multi-Tier Config Loading with Fallbacks
```rust
// Current: Manual fallback handling
let config = Config::<AppConfig>::builder()
    .source(Toml::file("config.toml"))
    .source(Env::new().prefix("APP"))
    .build()?;

// With recovery combinators:
use stillwater::effect::prelude::*;

let config_effect = from_fn(|_| {
    Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .build()
})
.recover_with(
    is_not_found,  // predicate for NotFound errors
    |_| {
        // Fallback to environment variables only
        Config::<AppConfig>::builder()
            .source(Env::new().prefix("APP"))
            .build()
    }
)
.fallback_to(from_fn(|_| {
    // Ultimate fallback: use defaults
    Config::<AppConfig>::builder()
        .source(Defaults::new())
        .build()
}));
```

### Benefits
- ✅ **Graceful degradation**: Try config.toml → env vars → defaults
- ✅ **Conditional recovery**: Only recover from specific errors (file not found, not permission denied)
- ✅ **Cache fallback**: Could implement config caching with fallback strategies

### Challenges
- Premortem doesn't currently use Effects heavily
- Config building is synchronous, not effect-based
- Would require architectural changes to integrate
- **Recommendation**: Low priority unless moving to Effect-based architecture

## Priority 4: Bifunctor Interface (0.13.0) ⭐

### What's New
- `.bimap(f_err, f_success)` - Transform both channels
- `.fold(on_failure, on_success)` - Catamorphism to single value
- `.unwrap_or_else(f)` - Compute default from error
- `.unwrap_or(default)` - Static default
- `.unwrap_or_default()` - Use `Default` trait
- `.merge()` - When `T == E`, eliminate wrapper

### How Premortem Could Use This

#### Error Message Transformation
```rust
// Current: manual match
match config_result {
    Validation::Success(cfg) => cfg,
    Validation::Failure(errs) => {
        eprintln!("Config errors: {}", errs);
        std::process::exit(1);
    }
}

// With bifunctor:
let config = config_result
    .bimap(
        |errs| {
            eprintln!("Config errors: {}", errs);
            std::process::exit(1)
        },
        |cfg| cfg
    );

// Or with fold:
config_result.fold(
    |errs| { eprintln!("Errors: {}", errs); std::process::exit(1) },
    |cfg| cfg
)

// Or unwrap with handler:
config_result.unwrap_or_else(|errs| {
    eprintln!("Using defaults due to errors: {}", errs);
    AppConfig::default()
})
```

### Benefits
- ✅ **Convenience**: Simpler error handling patterns
- ✅ **Type transformations**: Easy to map both success and error types
- ✅ **Fallbacks**: `unwrap_or_else` for default configs

### Challenges
- Premortem already has good error handling
- Most benefit is convenience, not new capabilities
- **Recommendation**: Nice-to-have, not critical

## Other Updates (Less Relevant)

### Zero-Cost Effect API (0.11.0)
- Premortem doesn't use Effects extensively
- Config loading is synchronous
- **Impact**: Low - not applicable to current architecture

### Testing Utilities (0.8.0)
- `MockEnv`, `assert_success!`, `assert_failure!`
- Premortem already has `MockEnv` pattern
- **Impact**: Already using similar patterns

## Migration Recommendations

### Immediate (Version 0.5.0)
1. **Upgrade to stillwater 0.13.0** (in Cargo.toml)
2. **Add predicate support** alongside existing validators
3. **Document predicate usage** in examples

### Short-term (Version 0.6.0)
1. **Integrate `.ensure()` combinators** for manual validation implementations
2. **Add predicate-based validator bridge** (convert `Predicate<T>` → `Validator<T>`)
3. **Update examples** to show both patterns

### Medium-term (Version 0.7.0)
1. **Consider deprecating custom validators** in favor of predicates
2. **Migrate derive macro** to generate predicate-based validation
3. **Add `.bimap()` and `.fold()` convenience methods**

### Long-term (Future)
1. **Effect-based config loading** for async sources and recovery patterns
2. **Full predicate integration** with source location tracking

## Implementation Checklist

- [ ] Update `Cargo.toml`: `stillwater = "0.13.0"`
- [ ] Add `use stillwater::predicate::*;` to prelude
- [ ] Create `Validator` ↔ `Predicate` bridge trait
- [ ] Add example showing predicate usage
- [ ] Document migration path in CHANGELOG
- [ ] Add tests for predicate integration
- [ ] Update validation examples to show both styles
- [ ] Consider feature flag for predicate validators

## Code Impact Estimate

| Change | Files Modified | Lines Added | Lines Removed | Risk |
|--------|---------------|-------------|---------------|------|
| Upgrade to 0.13 | 1 (Cargo.toml) | 1 | 1 | Low |
| Add predicate support | 2-3 | 100-150 | 0 | Low |
| Integrate `.ensure()` | 5-10 | 50-100 | 50-100 | Medium |
| Replace validators | 10-15 | 200-300 | 400-500 | High |

## Conclusion

**Recommended Action**: Upgrade to stillwater 0.13.0 and integrate Predicate Combinators.

**Highest Value**:
1. **Predicate Combinators** (0.12.0) - Would unify and simplify validation
2. **Validation `.ensure()` family** (0.12.0) - Reduces boilerplate
3. **Bifunctor methods** (0.13.0) - Nice convenience improvements

**Lower Priority**:
- Error Recovery - Not applicable to current sync architecture
- Zero-cost Effects - Premortem doesn't use Effects heavily

**Breaking Changes**: None required - can integrate incrementally while maintaining backward compatibility.
