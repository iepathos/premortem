---
number: 14
title: Property-Based Testing
category: testing
priority: medium
status: draft
dependencies: []
created: 2025-01-25
---

# Specification 014: Property-Based Testing

**Category**: testing
**Priority**: medium
**Status**: draft
**Dependencies**: none

## Context

Premortem currently has 288 unit tests providing good coverage. However, unit tests only check specific examples chosen by the developer. Property-based testing can discover edge cases and invariants that example-based tests miss by generating hundreds of random inputs and checking that properties always hold.

The project already verifies one algebraic property manually (Semigroup associativity for `ConfigErrors` in `src/error.rs:597`), demonstrating the value of this approach. Property tests would systematically verify all algebraic laws and uncover edge cases in parsing, validation, and error handling.

## Objective

Add comprehensive property-based tests using the `proptest` crate to verify algebraic laws, parsing roundtrips, validation invariants, and error handling properties across the premortem codebase.

## Requirements

### Functional Requirements

1. **Semigroup Law Verification**
   - ConfigErrors associativity: `(a <> b) <> c == a <> (b <> c)`
   - NonEmptyVec combination preserves all elements
   - Error combining never loses information

2. **Value Type Roundtrips**
   - TOML parse -> serialize -> parse produces equivalent Value
   - JSON parse -> serialize -> parse produces equivalent Value
   - Environment variable string -> Value -> string preserves data

3. **ConfigValues Merging Properties**
   - Merging is associative: `merge(merge(a, b), c) == merge(a, merge(b, c))`
   - Later values override earlier: `merge(a, b).get(k) == b.get(k)` if k in b
   - Merging preserves keys from both sources

4. **Validator Invariants**
   - Validators are deterministic: same input always produces same output
   - Range(min, max) accepts all values where min <= v <= max
   - NonEmpty rejects exactly empty strings
   - Pattern matching is consistent with regex crate behavior

5. **Error Construction Properties**
   - ConfigErrors always contains at least one error (NonEmptyVec)
   - with_path_prefix prepends correctly for all path strings
   - Source location display is parseable back to components

6. **Path Handling Properties**
   - Dot-notation path parsing handles arbitrary nesting
   - Array index paths `foo.0.bar` work for any valid index
   - Environment variable path conversion is invertible

### Non-Functional Requirements

- Property tests should run in under 30 seconds total
- Use `proptest` crate (more actively maintained than `quickcheck`)
- Configure shrinking for useful minimal failure cases
- Tests should be deterministic given a seed (for CI reproducibility)

## Acceptance Criteria

- [ ] Add `proptest` as dev-dependency
- [ ] Property tests for Semigroup associativity on ConfigErrors
- [ ] Property tests for Value type roundtrips (TOML, JSON)
- [ ] Property tests for ConfigValues merge associativity
- [ ] Property tests for validator determinism and correctness
- [ ] Property tests for error path manipulation
- [ ] Property tests for environment variable path conversion
- [ ] All property tests pass with default config (256 cases)
- [ ] Tests complete in under 30 seconds
- [ ] Document property test patterns in code comments

## Technical Details

### Implementation Approach

#### 1. Add proptest Dependency

```toml
[dev-dependencies]
proptest = "1.9"
```

#### 2. Create Arbitrary Implementations

Create generators for core types in a new `src/proptest_support.rs` module (only compiled for tests):

```rust
#[cfg(test)]
pub mod proptest_support {
    use proptest::prelude::*;
    use crate::{Value, ConfigError, ConfigErrors, SourceLocation};

    pub fn arb_value() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(Value::Integer),
            any::<f64>().prop_filter("finite", |f| f.is_finite()).prop_map(Value::Float),
            ".*".prop_map(Value::String),
        ];

        leaf.prop_recursive(
            3,  // depth
            64, // max nodes
            10, // items per collection
            |inner| {
                prop_oneof![
                    prop::collection::vec(inner.clone(), 0..10).prop_map(Value::Array),
                    prop::collection::btree_map("\\w+", inner, 0..10).prop_map(Value::Table),
                ]
            },
        )
    }

    pub fn arb_source_location() -> impl Strategy<Value = SourceLocation> {
        (
            "[a-z_]+\\.toml|env:[A-Z_]+",
            proptest::option::of(1u32..1000),
            proptest::option::of(1u32..200),
        ).prop_map(|(source, line, column)| {
            let mut loc = SourceLocation::new(source);
            if let Some(l) = line { loc = loc.with_line(l); }
            if let Some(c) = column { loc = loc.with_column(c); }
            loc
        })
    }

    pub fn arb_config_error() -> impl Strategy<Value = ConfigError> {
        prop_oneof![
            ("[a-z.]+", ".*").prop_map(|(path, msg)| ConfigError::ValidationError {
                path,
                source_location: None,
                value: None,
                message: msg,
            }),
            "[a-z.]+".prop_map(|path| ConfigError::MissingField {
                path,
                source_location: None,
                searched: vec![],
            }),
        ]
    }

    pub fn arb_config_errors() -> impl Strategy<Value = ConfigErrors> {
        prop::collection::vec(arb_config_error(), 1..5)
            .prop_map(|errs| ConfigErrors::from_vec(errs).unwrap())
    }
}
```

#### 3. Property Test Module Structure

Create `tests/property_tests.rs`:

```rust
use proptest::prelude::*;
use premortem::*;
use stillwater::Semigroup;

mod semigroup_laws {
    use super::*;

    proptest! {
        #[test]
        fn config_errors_associativity(
            a in arb_config_errors(),
            b in arb_config_errors(),
            c in arb_config_errors(),
        ) {
            let left = a.clone().combine(b.clone()).combine(c.clone());
            let right = a.combine(b.combine(c));
            prop_assert_eq!(left.len(), right.len());
        }
    }
}

mod value_roundtrips {
    use super::*;

    proptest! {
        #[test]
        fn json_roundtrip(value in arb_value()) {
            let json = serde_json::to_string(&value_to_json(&value)).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
            let back = json_to_value(parsed);
            prop_assert_eq!(value, back);
        }
    }
}

mod merge_properties {
    use super::*;

    proptest! {
        #[test]
        fn merge_is_associative(
            a in arb_config_values(),
            b in arb_config_values(),
            c in arb_config_values(),
        ) {
            let left = a.clone().merge(b.clone()).merge(c.clone());
            let right = a.merge(b.merge(c));
            // Keys should match
            prop_assert_eq!(
                left.keys().collect::<Vec<_>>(),
                right.keys().collect::<Vec<_>>()
            );
        }

        #[test]
        fn later_source_wins(
            a in arb_config_values(),
            b in arb_config_values(),
        ) {
            let merged = a.clone().merge(b.clone());
            for key in b.keys() {
                prop_assert_eq!(merged.get(key), b.get(key));
            }
        }
    }
}

mod validator_properties {
    use super::*;

    proptest! {
        #[test]
        fn range_accepts_in_bounds(min in 0i64..100, max in 100i64..200, val in 100i64..=100) {
            // Adjust val to be in range
            let val = min + (val.abs() % (max - min + 1));
            let validator = Range::new(min, max);
            let result = validator.validate(&val, "test");
            prop_assert!(result.is_success());
        }

        #[test]
        fn non_empty_deterministic(s in ".*") {
            let v1 = NonEmpty.validate(&s, "test");
            let v2 = NonEmpty.validate(&s, "test");
            prop_assert_eq!(v1.is_success(), v2.is_success());
        }
    }
}

mod path_properties {
    use super::*;

    proptest! {
        #[test]
        fn path_prefix_prepends(prefix in "[a-z]+", path in "[a-z.]+") {
            let err = ConfigError::ValidationError {
                path: path.clone(),
                source_location: None,
                value: None,
                message: "test".into(),
            };
            let prefixed = err.with_path_prefix(&prefix);
            prop_assert!(prefixed.path().starts_with(&prefix));
            prop_assert!(prefixed.path().contains(&path));
        }
    }
}
```

### Architecture Changes

- Add `proptest` to dev-dependencies only (no runtime impact)
- Create `tests/property_tests.rs` for integration-level property tests
- Optionally add `#[cfg(test)]` module in `src/lib.rs` for internal generators

### Test Organization

```
tests/
├── derive_tests.rs        # Existing derive macro tests
└── property_tests.rs      # New property-based tests
    ├── semigroup_laws     # Algebraic law verification
    ├── value_roundtrips   # Serialization roundtrips
    ├── merge_properties   # ConfigValues merging
    ├── validator_props    # Validator invariants
    └── path_properties    # Path manipulation
```

## Dependencies

- **Prerequisites**: None (tests only)
- **Affected Components**: Test suite only
- **External Dependencies**: `proptest = "1.9"` (dev-dependency)

## Testing Strategy

- **Unit Tests**: Property tests supplement existing unit tests
- **Integration Tests**: Property tests in `tests/` directory
- **Performance Tests**: Verify 256 cases complete in < 30 seconds
- **CI Integration**: Property tests run with `cargo test --all-features`

## Documentation Requirements

- **Code Documentation**: Document each property being tested with comments explaining the invariant
- **User Documentation**: None required (internal testing only)
- **Architecture Updates**: None required

## Implementation Notes

### Proptest Configuration

Use a consistent seed for CI reproducibility:

```rust
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    // tests...
}
```

### Shrinking

Proptest automatically shrinks failing cases. For complex types like `Value`, the recursive strategy handles shrinking automatically.

### Float Handling

Filter out NaN and infinity for float properties since they have special equality semantics:

```rust
any::<f64>().prop_filter("finite", |f| f.is_finite())
```

### Common Pitfalls

1. **Non-determinism**: Avoid `SystemTime::now()` or random sources in generators
2. **Slow strategies**: Limit recursion depth and collection sizes
3. **Equality edge cases**: Handle floating point comparison carefully
4. **String encoding**: Generate valid UTF-8 only

## Migration and Compatibility

No breaking changes. Property tests are additive and only affect the test suite.

## Future Enhancements

After initial implementation, consider:

1. **Stateful property tests**: Test sequences of operations (load, modify, save)
2. **Fuzz testing integration**: Use proptest inputs for cargo-fuzz
3. **Mutation testing**: Verify properties catch real bugs
4. **Coverage-guided generation**: Focus on uncovered code paths
