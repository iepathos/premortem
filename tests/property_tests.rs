//! Property-based tests for premortem using proptest.
//!
//! These tests verify algebraic laws, roundtrip properties, and invariants
//! that must hold for all possible inputs, not just hand-picked examples.

use proptest::prelude::*;
use std::collections::BTreeMap;

use premortem::{
    ConfigError, ConfigErrors, ConfigValidation, ConfigValue, ConfigValues, SourceLocation,
    Validate, Validator, Value,
};
use stillwater::{Semigroup, Validation};

// ============================================================================
// Arbitrary Generators
// ============================================================================

/// Generate arbitrary Value types with controlled recursion depth.
///
/// Property: Generated values must be valid and representable.
fn arb_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::Integer),
        // Filter NaN and infinity since they have special equality semantics
        any::<f64>()
            .prop_filter("finite", |f| f.is_finite())
            .prop_map(Value::Float),
        "[a-zA-Z0-9_\\-]{0,50}".prop_map(Value::String),
    ];

    // Recursive strategy for nested structures
    leaf.prop_recursive(
        3,  // max depth
        64, // max nodes
        10, // items per collection
        |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..5).prop_map(Value::Array),
                prop::collection::btree_map("[a-z_]{1,10}", inner, 0..5).prop_map(Value::Table),
            ]
        },
    )
}

/// Generate arbitrary SourceLocation values.
fn arb_source_location() -> impl Strategy<Value = SourceLocation> {
    (
        prop_oneof!["[a-z_]{1,20}\\.toml", "env:[A-Z_]{1,20}", "[a-z_/]{1,30}",],
        proptest::option::of(1u32..1000),
        proptest::option::of(1u32..200),
    )
        .prop_map(|(source, line, column)| {
            let mut loc = SourceLocation::new(source);
            if let Some(l) = line {
                loc = loc.with_line(l);
            }
            if let Some(c) = column {
                loc = loc.with_column(c);
            }
            loc
        })
}

/// Generate arbitrary SourceErrorKind values.
fn arb_source_error_kind() -> impl Strategy<Value = premortem::SourceErrorKind> {
    use premortem::SourceErrorKind;
    prop_oneof![
        // NotFound
        "[a-z_/]{1,30}\\.toml".prop_map(|path| SourceErrorKind::NotFound { path }),
        // IoError
        "[a-zA-Z0-9 ]{1,30}".prop_map(|message| SourceErrorKind::IoError { message }),
        // ParseError
        (
            "[a-zA-Z0-9 ]{1,30}",
            proptest::option::of(1u32..1000),
            proptest::option::of(1u32..200),
        )
            .prop_map(|(message, line, column)| SourceErrorKind::ParseError {
                message,
                line,
                column,
            }),
        // ConnectionError
        "[a-zA-Z0-9 ]{1,30}".prop_map(|message| SourceErrorKind::ConnectionError { message }),
        // Other
        "[a-zA-Z0-9 ]{1,30}".prop_map(|message| SourceErrorKind::Other { message }),
    ]
}

/// Generate arbitrary ConfigError values (all 7 variants).
fn arb_config_error() -> impl Strategy<Value = ConfigError> {
    prop_oneof![
        // SourceError
        ("[a-z_]{1,20}\\.toml", arb_source_error_kind())
            .prop_map(|(source_name, kind)| { ConfigError::SourceError { source_name, kind } }),
        // ParseError
        (
            "[a-z][a-z.]{0,20}",
            arb_source_location(),
            prop_oneof!["integer", "string", "boolean", "float", "array", "table"],
            "[a-zA-Z0-9_\\-]{1,20}",
            "[a-zA-Z0-9 ]{1,50}",
        )
            .prop_map(
                |(path, source_location, expected_type, actual_value, message)| {
                    ConfigError::ParseError {
                        path,
                        source_location,
                        expected_type: expected_type.to_string(),
                        actual_value,
                        message,
                    }
                },
            ),
        // MissingField
        (
            "[a-z][a-z.]{0,20}",
            proptest::option::of(arb_source_location()),
            prop::collection::vec("[a-z_]{1,15}\\.toml", 1..3),
        )
            .prop_map(|(path, source_location, searched_sources)| {
                ConfigError::MissingField {
                    path,
                    source_location,
                    searched_sources,
                }
            }),
        // ValidationError
        (
            "[a-z][a-z.]{0,20}",
            proptest::option::of(arb_source_location()),
            proptest::option::of("[a-zA-Z0-9_\\-]{1,20}"),
            "[a-zA-Z0-9 ]{1,50}",
        )
            .prop_map(|(path, source_location, value, message)| {
                ConfigError::ValidationError {
                    path,
                    source_location,
                    value,
                    message,
                }
            }),
        // CrossFieldError
        (
            prop::collection::vec("[a-z]{1,10}", 2..4),
            "[a-zA-Z0-9 ]{1,50}"
        )
            .prop_map(|(paths, msg)| ConfigError::CrossFieldError {
                paths,
                message: msg,
            }),
        // UnknownField
        (
            "[a-z][a-z.]{0,20}",
            arb_source_location(),
            proptest::option::of("[a-z]{1,15}"),
        )
            .prop_map(
                |(path, source_location, did_you_mean)| ConfigError::UnknownField {
                    path,
                    source_location,
                    did_you_mean,
                }
            ),
        // NoSources
        Just(ConfigError::NoSources),
    ]
}

/// Generate arbitrary ConfigErrors (non-empty collection).
fn arb_config_errors() -> impl Strategy<Value = ConfigErrors> {
    prop::collection::vec(arb_config_error(), 1..5)
        .prop_map(|errs| ConfigErrors::from_vec(errs).expect("non-empty vec"))
}

/// Generate arbitrary ConfigValue (value with source tracking).
fn arb_config_value() -> impl Strategy<Value = ConfigValue> {
    (arb_value(), arb_source_location()).prop_map(|(value, source)| ConfigValue::new(value, source))
}

/// Generate arbitrary ConfigValues container.
fn arb_config_values() -> impl Strategy<Value = ConfigValues> {
    prop::collection::btree_map("[a-z][a-z.]{0,15}", arb_config_value(), 0..10).prop_map(|map| {
        let mut values = ConfigValues::empty();
        for (path, cv) in map {
            values.insert(path, cv);
        }
        values
    })
}

/// Generate valid path strings for config paths.
fn arb_path() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z]{1,10}",                             // simple path
        "[a-z]{1,10}\\.[a-z]{1,10}",               // nested path
        "[a-z]{1,10}\\.[a-z]{1,10}\\.[a-z]{1,10}", // deeply nested
    ]
}

/// Generate valid path prefix strings.
fn arb_prefix() -> impl Strategy<Value = String> {
    "[a-z]{1,10}"
}

// ============================================================================
// Semigroup Law Tests
// ============================================================================

mod semigroup_laws {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Property: ConfigErrors combination is associative.
        ///
        /// (a <> b) <> c == a <> (b <> c)
        ///
        /// This ensures error accumulation order doesn't affect the result.
        #[test]
        fn config_errors_associativity(
            a in arb_config_errors(),
            b in arb_config_errors(),
            c in arb_config_errors(),
        ) {
            let left = a.clone().combine(b.clone()).combine(c.clone());
            let right = a.combine(b.combine(c));

            // The combined error count should be the same regardless of grouping
            prop_assert_eq!(left.len(), right.len());
        }

        /// Property: Combining ConfigErrors preserves all errors.
        ///
        /// |a <> b| == |a| + |b|
        #[test]
        fn config_errors_preserves_count(
            a in arb_config_errors(),
            b in arb_config_errors(),
        ) {
            let a_len = a.len();
            let b_len = b.len();
            let combined = a.combine(b);

            prop_assert_eq!(combined.len(), a_len + b_len);
        }
    }
}

// ============================================================================
// Value Roundtrip Tests
// ============================================================================

mod value_roundtrips {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Property: Value to JSON and back preserves structure.
        ///
        /// Note: This is a one-way test since JSON may lose some type information.
        #[test]
        fn value_json_structural_preservation(value in arb_value()) {
            let json = value_to_json(&value);
            // Verify JSON conversion doesn't panic and produces valid JSON
            let json_str = serde_json::to_string(&json);
            prop_assert!(json_str.is_ok());
        }

        /// Property: Value type_name is always valid and non-empty.
        #[test]
        fn value_type_name_valid(value in arb_value()) {
            let type_name = value.type_name();
            prop_assert!(!type_name.is_empty());
            prop_assert!(["null", "boolean", "integer", "float", "string", "array", "table"]
                .contains(&type_name));
        }

        /// Property: Value::is_null correctly identifies null values.
        #[test]
        fn value_is_null_correct(value in arb_value()) {
            let is_null = value.is_null();
            let is_null_variant = matches!(value, Value::Null);
            prop_assert_eq!(is_null, is_null_variant);
        }
    }

    /// Convert Value to serde_json::Value for roundtrip testing.
    fn value_to_json(value: &Value) -> serde_json::Value {
        match value {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::Integer(i) => serde_json::Value::Number((*i).into()),
            Value::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Value::String(s) => serde_json::Value::String(s.clone()),
            Value::Array(arr) => serde_json::Value::Array(arr.iter().map(value_to_json).collect()),
            Value::Table(table) => {
                let map: serde_json::Map<String, serde_json::Value> = table
                    .iter()
                    .map(|(k, v)| (k.clone(), value_to_json(v)))
                    .collect();
                serde_json::Value::Object(map)
            }
        }
    }
}

// ============================================================================
// ConfigValues Merge Properties
// ============================================================================

mod merge_properties {
    use super::*;
    use premortem::merge_config_values;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Property: Merging is associative for key presence.
        ///
        /// Keys present in merge(merge(a, b), c) should equal keys in merge(a, merge(b, c))
        #[test]
        fn merge_associative_keys(
            a in arb_config_values(),
            b in arb_config_values(),
            c in arb_config_values(),
        ) {
            let left = merge_config_values(vec![
                merge_config_values(vec![a.clone(), b.clone()]),
                c.clone(),
            ]);
            let right = merge_config_values(vec![
                a,
                merge_config_values(vec![b, c]),
            ]);

            // Same keys should be present
            let left_paths: Vec<_> = left.paths().collect();
            let right_paths: Vec<_> = right.paths().collect();
            prop_assert_eq!(left_paths, right_paths);
        }

        /// Property: Later source wins in merge.
        ///
        /// For any key in b, merge(a, b).get(k) should equal b.get(k)
        /// This verifies both presence AND value equality.
        #[test]
        fn later_source_wins(
            a in arb_config_values(),
            b in arb_config_values(),
        ) {
            let merged = merge_config_values(vec![a, b.clone()]);

            for path in b.paths() {
                let merged_value = merged.get(path);
                let b_value = b.get(path);

                // The merged value should exist
                prop_assert!(
                    merged_value.is_some(),
                    "Path '{}' from later source should exist in merged result",
                    path
                );

                // The merged value should equal the later source's value
                let merged_cv = merged_value.unwrap();
                let b_cv = b_value.unwrap();

                prop_assert_eq!(
                    &merged_cv.value,
                    &b_cv.value,
                    "Path '{}': merged value {:?} should equal later source value {:?}",
                    path,
                    &merged_cv.value,
                    &b_cv.value
                );
            }
        }

        /// Property: Merge preserves all keys from both sources.
        #[test]
        fn merge_preserves_all_keys(
            a in arb_config_values(),
            b in arb_config_values(),
        ) {
            let a_paths: std::collections::HashSet<_> = a.paths().cloned().collect();
            let b_paths: std::collections::HashSet<_> = b.paths().cloned().collect();
            let expected_paths: std::collections::HashSet<_> = a_paths.union(&b_paths).cloned().collect();

            let merged = merge_config_values(vec![a, b]);
            let merged_paths: std::collections::HashSet<_> = merged.paths().cloned().collect();

            prop_assert_eq!(expected_paths, merged_paths);
        }

        /// Property: Empty merge is identity.
        #[test]
        fn empty_merge_identity(a in arb_config_values()) {
            let merged_left = merge_config_values(vec![ConfigValues::empty(), a.clone()]);
            let merged_right = merge_config_values(vec![a.clone(), ConfigValues::empty()]);

            // Both should have same paths as original
            prop_assert_eq!(a.len(), merged_left.len());
            prop_assert_eq!(a.len(), merged_right.len());
        }
    }
}

// ============================================================================
// Validator Property Tests
// ============================================================================

mod validator_properties {
    use super::*;
    use premortem::validate::{validate_field, validators::*};

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Property: Validators are deterministic.
        ///
        /// Same input always produces same validation result.
        #[test]
        fn non_empty_deterministic(s in ".*") {
            let v1 = NonEmpty.validate(&s, "test");
            let v2 = NonEmpty.validate(&s, "test");

            prop_assert_eq!(v1.is_success(), v2.is_success());
        }

        /// Property: NonEmpty accepts exactly non-empty strings.
        #[test]
        fn non_empty_correct(s in ".*") {
            let result = NonEmpty.validate(&s, "test");
            let expected_success = !s.is_empty();
            prop_assert_eq!(result.is_success(), expected_success);
        }

        /// Property: Range validator accepts exactly values in range.
        #[test]
        fn range_accepts_in_bounds(
            min in -1000i64..1000,
            max in -1000i64..1000,
            val in -2000i64..2000,
        ) {
            // Ensure min <= max
            let (min, max) = if min <= max { (min, max) } else { (max, min) };

            let validator = Range(min..=max);
            let result = validator.validate(&val, "test");

            let should_succeed = val >= min && val <= max;
            prop_assert_eq!(result.is_success(), should_succeed);
        }

        /// Property: Positive validator accepts exactly positive values.
        #[test]
        fn positive_correct(val in any::<i32>()) {
            let result = Positive.validate(&val, "test");
            prop_assert_eq!(result.is_success(), val > 0);
        }

        /// Property: Negative validator accepts exactly negative values.
        #[test]
        fn negative_correct(val in any::<i32>()) {
            let result = Negative.validate(&val, "test");
            prop_assert_eq!(result.is_success(), val < 0);
        }

        /// Property: NonZero validator accepts exactly non-zero values.
        #[test]
        fn non_zero_correct(val in any::<i32>()) {
            let result = NonZero.validate(&val, "test");
            prop_assert_eq!(result.is_success(), val != 0);
        }

        /// Property: MinLength validator accepts strings at or above minimum.
        #[test]
        fn min_length_correct(s in ".{0,100}", min_len in 0usize..50) {
            let result = MinLength(min_len).validate(&s, "test");
            prop_assert_eq!(result.is_success(), s.len() >= min_len);
        }

        /// Property: MaxLength validator accepts strings at or below maximum.
        #[test]
        fn max_length_correct(s in ".{0,100}", max_len in 0usize..100) {
            let result = MaxLength(max_len).validate(&s, "test");
            prop_assert_eq!(result.is_success(), s.len() <= max_len);
        }

        /// Property: Multiple validators accumulate all errors.
        ///
        /// If both validators fail, we should get 2 errors.
        #[test]
        fn validators_accumulate_errors(s in ".{0,5}") {
            // MinLength(10) and NonEmpty will both potentially fail
            let result = validate_field(
                &s,
                "field",
                &[&NonEmpty, &MinLength(10)],
            );

            if s.is_empty() {
                // Both should fail
                if let Validation::Failure(errors) = result {
                    prop_assert_eq!(errors.len(), 2);
                }
            } else if s.len() < 10 {
                // Only MinLength should fail
                if let Validation::Failure(errors) = result {
                    prop_assert_eq!(errors.len(), 1);
                }
            } else {
                // Both should pass
                prop_assert!(result.is_success());
            }
        }
    }
}

// ============================================================================
// Path Handling Properties
// ============================================================================

mod path_properties {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Property: with_path_prefix prepends correctly for all paths.
        ///
        /// The resulting path should start with the prefix.
        #[test]
        fn path_prefix_prepends(
            prefix in arb_prefix(),
            path in arb_path(),
        ) {
            let err = ConfigError::ValidationError {
                path: path.clone(),
                source_location: None,
                value: None,
                message: "test".into(),
            };

            let prefixed = err.with_path_prefix(&prefix);

            if let Some(prefixed_path) = prefixed.path() {
                prop_assert!(prefixed_path.starts_with(&prefix));
                prop_assert!(prefixed_path.contains(&path));
            } else {
                prop_assert!(false, "Expected path after prefixing");
            }
        }

        /// Property: with_path_prefix formats correctly with dot separator.
        #[test]
        fn path_prefix_dot_separator(
            prefix in arb_prefix(),
            path in "[a-z]{1,10}",  // Simple non-array path
        ) {
            let err = ConfigError::ValidationError {
                path: path.clone(),
                source_location: None,
                value: None,
                message: "test".into(),
            };

            let prefixed = err.with_path_prefix(&prefix);

            if let Some(prefixed_path) = prefixed.path() {
                // Should be "prefix.path"
                let expected = format!("{}.{}", prefix, path);
                prop_assert_eq!(prefixed_path, &expected);
            }
        }

        /// Property: Array index paths don't get dot separator.
        #[test]
        fn array_index_no_dot(
            prefix in arb_prefix(),
            index in 0usize..100,
        ) {
            let path = format!("[{}]", index);
            let err = ConfigError::ValidationError {
                path,
                source_location: None,
                value: None,
                message: "test".into(),
            };

            let prefixed = err.with_path_prefix(&prefix);

            if let Some(prefixed_path) = prefixed.path() {
                // Should be "prefix[index]", not "prefix.[index]"
                let expected = format!("{}[{}]", prefix, index);
                prop_assert_eq!(prefixed_path, &expected);
            }
        }

        /// Property: Empty path becomes just the prefix.
        #[test]
        fn empty_path_becomes_prefix(prefix in arb_prefix()) {
            let err = ConfigError::ValidationError {
                path: String::new(),
                source_location: None,
                value: None,
                message: "test".into(),
            };

            let prefixed = err.with_path_prefix(&prefix);

            if let Some(prefixed_path) = prefixed.path() {
                prop_assert_eq!(prefixed_path, &prefix);
            }
        }

        /// Property: ConfigErrors.with_path_prefix applies to all errors.
        #[test]
        fn errors_prefix_applies_to_all(
            prefix in arb_prefix(),
            errors in arb_config_errors(),
        ) {
            let original_len = errors.len();
            let prefixed = errors.with_path_prefix(&prefix);

            // Same number of errors
            prop_assert_eq!(prefixed.len(), original_len);

            // All errors with paths should have the prefix
            for err in prefixed.iter() {
                if let Some(path) = err.path() {
                    prop_assert!(
                        path.starts_with(&prefix),
                        "Path '{}' should start with prefix '{}'",
                        path,
                        prefix
                    );
                }
            }
        }
    }
}

// ============================================================================
// Error Construction Properties
// ============================================================================

mod error_properties {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Property: ConfigErrors always has at least one error.
        ///
        /// This is guaranteed by NonEmptyVec, but we verify it holds
        /// through all our construction paths.
        #[test]
        fn config_errors_always_nonempty(errors in arb_config_errors()) {
            prop_assert!(!errors.is_empty());
        }

        /// Property: ConfigErrors::single creates exactly one error.
        #[test]
        fn single_creates_one_error(error in arb_config_error()) {
            let errors = ConfigErrors::single(error);
            prop_assert_eq!(errors.len(), 1);
        }

        /// Property: ConfigErrors::from_vec returns None for empty vec.
        #[test]
        fn from_vec_empty_returns_none(_dummy in any::<bool>()) {
            let empty: Vec<ConfigError> = vec![];
            prop_assert!(ConfigErrors::from_vec(empty).is_none());
        }

        /// Property: ConfigErrors::from_vec returns Some for non-empty vec.
        #[test]
        fn from_vec_nonempty_returns_some(errors in prop::collection::vec(arb_config_error(), 1..5)) {
            let len = errors.len();
            let result = ConfigErrors::from_vec(errors);
            prop_assert!(result.is_some());
            prop_assert_eq!(result.unwrap().len(), len);
        }

        /// Property: SourceLocation display is consistent.
        #[test]
        fn source_location_display_format(loc in arb_source_location()) {
            let display = format!("{}", loc);

            // Should always contain the source name
            prop_assert!(display.contains(&loc.source));

            // If line is present, should contain it
            if let Some(line) = loc.line {
                let line_str = format!(":{}", line);
                prop_assert!(display.contains(&line_str));
            }

            // If column is present (and line is present), should contain it
            if loc.column.is_some() && loc.line.is_some() {
                // Column is only shown if line is also present
                let col = loc.column.unwrap();
                let col_str = format!(":{}", col);
                prop_assert!(display.contains(&col_str));
            }
        }

        /// Property: with_context adds context to ValidationError.
        #[test]
        fn with_context_adds_to_validation_error(
            path in arb_path(),
            message in "[a-zA-Z0-9 ]{1,30}",
            context in "[a-zA-Z0-9 ]{1,30}",
        ) {
            let err = ConfigError::ValidationError {
                path,
                source_location: None,
                value: None,
                message: message.clone(),
            };

            let with_ctx = err.with_context(&context);

            if let ConfigError::ValidationError { message: new_msg, .. } = with_ctx {
                prop_assert!(new_msg.contains(&context));
                prop_assert!(new_msg.contains(&message));
            } else {
                prop_assert!(false, "Expected ValidationError");
            }
        }
    }
}

// ============================================================================
// Environment Variable Path Conversion Properties
// ============================================================================

mod env_path_properties {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Property: SourceLocation::env creates properly formatted location.
        #[test]
        fn env_location_format(var_name in "[A-Z][A-Z0-9_]{0,20}") {
            let loc = SourceLocation::env(&var_name);

            prop_assert!(loc.source.starts_with("env:"));
            prop_assert!(loc.source.contains(&var_name));
            prop_assert_eq!(loc.source, format!("env:{}", var_name));
        }

        /// Property: SourceLocation::file preserves all components.
        #[test]
        fn file_location_preserves_components(
            path in "[a-z/]{1,30}\\.toml",
            line in proptest::option::of(1u32..1000),
            column in proptest::option::of(1u32..200),
        ) {
            let loc = SourceLocation::file(&path, line, column);

            prop_assert_eq!(loc.source, path);
            prop_assert_eq!(loc.line, line);
            prop_assert_eq!(loc.column, column);
        }
    }
}

// ============================================================================
// ConfigValidation Properties
// ============================================================================

mod validation_properties {
    use super::*;
    use premortem::ConfigValidationExt;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Property: fail_with creates a Failure with the given error.
        #[test]
        fn fail_with_creates_failure(error in arb_config_error()) {
            let result: ConfigValidation<()> = ConfigValidation::fail_with(error);

            prop_assert!(result.is_failure());
            if let Validation::Failure(errors) = result {
                prop_assert_eq!(errors.len(), 1);
            }
        }

        /// Property: Validate for () always succeeds.
        #[test]
        fn unit_validate_always_succeeds(_dummy in any::<bool>()) {
            let result = ().validate();
            prop_assert!(result.is_success());
        }

        /// Property: Option<T>::validate delegates to Some or succeeds for None.
        #[test]
        fn option_validate_behavior(opt in proptest::option::of(any::<String>())) {
            let result = opt.validate();
            // Strings always pass validation (no custom validation)
            prop_assert!(result.is_success());
        }

        /// Property: Vec<T>::validate with empty vec always succeeds.
        #[test]
        fn empty_vec_validate_succeeds(_dummy in any::<bool>()) {
            let v: Vec<String> = vec![];
            let result = v.validate();
            prop_assert!(result.is_success());
        }
    }
}

// ============================================================================
// Value Get Path Properties
// ============================================================================

mod value_path_properties {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Property: get_path on empty path returns None.
        ///
        /// Empty string is not a valid path. If you need the root value,
        /// use it directly rather than calling get_path("").
        #[test]
        fn get_empty_path_returns_none(value in arb_value()) {
            let result = value.get_path("");
            prop_assert!(result.is_none(), "Empty path should return None, got {:?}", result);
        }

        /// Property: get_path on Table returns correct nested value.
        #[test]
        fn get_path_table_access(
            key in "[a-z]{1,10}",
            inner_value in arb_value(),
        ) {
            let mut table = BTreeMap::new();
            table.insert(key.clone(), inner_value.clone());
            let value = Value::Table(table);

            let result = value.get_path(&key);
            prop_assert_eq!(result, Some(&inner_value));
        }

        /// Property: get_path on non-Table returns None for any path.
        #[test]
        fn get_path_non_table_returns_none(
            path in "[a-z]{1,10}",
            value in prop_oneof![
                Just(Value::Null),
                any::<bool>().prop_map(Value::Bool),
                any::<i64>().prop_map(Value::Integer),
                any::<f64>().prop_filter("finite", |f| f.is_finite()).prop_map(Value::Float),
                "[a-zA-Z0-9_]{0,20}".prop_map(Value::String),
                prop::collection::vec(Just(Value::Null), 0..3).prop_map(Value::Array),
            ],
        ) {
            // Non-table values should return None for path access
            // (except for Table which we excluded)
            if !matches!(value, Value::Table(_)) {
                let result = value.get_path(&path);
                prop_assert!(result.is_none());
            }
        }
    }
}
