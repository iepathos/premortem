//! Integration tests for required environment variable validation.
//!
//! These tests verify the end-to-end behavior of the `.require()` and
//! `.require_all()` methods on the Env source, including error accumulation.

use premortem::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize, DeriveValidate)]
struct TestConfig {
    #[validate(min_length(10))]
    apikey: String,
    databaseurl: String,
    #[validate(range(1..=65535))]
    port: u16,
}

#[test]
fn test_all_required_vars_present() {
    let env = MockEnv::new()
        .with_env("APP_APIKEY", "secret-key-123")
        .with_env("APP_DATABASEURL", "postgresql://localhost/test")
        .with_env("APP_PORT", "5432");

    let result = Config::<TestConfig>::builder()
        .source(
            Env::prefix("APP_")
                .require("APIKEY")
                .require("DATABASEURL")
                .require("PORT"),
        )
        .build_with_env(&env);

    assert!(result.is_ok());
    let config = result.unwrap();
    assert_eq!(config.apikey, "secret-key-123");
    assert_eq!(config.databaseurl, "postgresql://localhost/test");
    assert_eq!(config.port, 5432);
}

#[test]
fn test_require_all_with_all_present() {
    let env = MockEnv::new()
        .with_env("APP_APIKEY", "secret-key-123")
        .with_env("APP_DATABASEURL", "postgresql://localhost/test")
        .with_env("APP_PORT", "5432");

    let result = Config::<TestConfig>::builder()
        .source(
            Env::prefix("APP_").require_all(&["APIKEY", "DATABASEURL", "PORT"]),
        )
        .build_with_env(&env);

    assert!(result.is_ok());
}

#[test]
fn test_single_required_var_missing() {
    let env = MockEnv::new()
        .with_env("APP_DATABASEURL", "postgresql://localhost/test")
        .with_env("APP_PORT", "5432");
    // Missing APP_APIKEY

    let result = Config::<TestConfig>::builder()
        .source(
            Env::prefix("APP_")
                .require("APIKEY")
                .require("DATABASEURL")
                .require("PORT"),
        )
        .build_with_env(&env);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1);

    let error = errors.iter().next().unwrap();
    assert_eq!(error.path(), Some("apikey"));
    assert!(matches!(error, ConfigError::MissingField { .. }));
}

#[test]
fn test_multiple_required_vars_missing_error_accumulation() {
    let env = MockEnv::new()
        .with_env("APP_PORT", "5432");
    // Missing APP_APIKEY and APP_DATABASEURL

    let result = Config::<TestConfig>::builder()
        .source(
            Env::prefix("APP_").require_all(&["APIKEY", "DATABASEURL", "PORT"]),
        )
        .build_with_env(&env);

    assert!(result.is_err());
    let errors = result.unwrap_err();

    // Should accumulate ALL missing variable errors, not fail fast
    assert_eq!(errors.len(), 2);

    let paths: Vec<Option<&str>> = errors.iter().map(|e| e.path()).collect();
    assert!(paths.contains(&Some("apikey")));
    assert!(paths.contains(&Some("databaseurl")));
}

#[test]
fn test_all_required_vars_missing() {
    let env = MockEnv::new();
    // No environment variables set

    let result = Config::<TestConfig>::builder()
        .source(
            Env::prefix("APP_").require_all(&["APIKEY", "DATABASEURL", "PORT"]),
        )
        .build_with_env(&env);

    assert!(result.is_err());
    let errors = result.unwrap_err();

    // All three variables should be reported as missing
    assert_eq!(errors.len(), 3);
}

#[test]
fn test_required_vars_present_but_validation_fails() {
    let env = MockEnv::new()
        .with_env("APP_APIKEY", "short") // Too short! Must be >= 10 chars
        .with_env("APP_DATABASEURL", "postgresql://localhost/test")
        .with_env("APP_PORT", "8080");

    let result = Config::<TestConfig>::builder()
        .source(
            Env::prefix("APP_").require_all(&["APIKEY", "DATABASEURL", "PORT"]),
        )
        .build_with_env(&env);

    assert!(result.is_err());
    let errors = result.unwrap_err();

    // Source-level validation (presence) passes, but value-level validation fails
    // apikey validation should fail (too short)
    assert_eq!(errors.len(), 1);

    let has_apikey_error = errors
        .iter()
        .any(|e| e.path() == Some("apikey") && matches!(e, ConfigError::ValidationError { .. }));

    assert!(has_apikey_error);
}

#[test]
fn test_source_location_tracking_for_missing_vars() {
    let env = MockEnv::new()
        .with_env("APP_DATABASEURL", "postgresql://localhost/test")
        .with_env("APP_PORT", "5432");
    // Missing APP_APIKEY

    let result = Config::<TestConfig>::builder()
        .source(Env::prefix("APP_").require("APIKEY"))
        .build_with_env(&env);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    let error = errors.iter().next().unwrap();

    // Check that source location is tracked
    let source_loc = error.source_location();
    assert!(source_loc.is_some());
    assert_eq!(source_loc.unwrap().source, "env:APP_APIKEY");
}

#[test]
fn test_mixed_required_and_optional_vars() {
    #[derive(Debug, Deserialize, DeriveValidate)]
    struct MixedConfig {
        requiredvar: String,
        #[serde(default)]
        optionalvar: Option<String>,
    }

    let env = MockEnv::new().with_env("APP_REQUIREDVAR", "value");
    // APP_OPTIONALVAR is not set

    let result = Config::<MixedConfig>::builder()
        .source(Env::prefix("APP_").require("REQUIREDVAR"))
        .build_with_env(&env);

    assert!(result.is_ok());
    let config = result.unwrap();
    assert_eq!(config.requiredvar, "value");
    assert_eq!(config.optionalvar, None);
}

#[test]
fn test_require_var_not_in_prefix() {
    let env = MockEnv::new()
        .with_env("OTHER_APIKEY", "secret")
        .with_env("APP_DATABASEURL", "postgresql://localhost/test")
        .with_env("APP_PORT", "5432");

    let result = Config::<TestConfig>::builder()
        .source(
            Env::prefix("APP_").require_all(&["APIKEY", "DATABASEURL", "PORT"]),
        )
        .build_with_env(&env);

    assert!(result.is_err());
    let errors = result.unwrap_err();

    // APIKEY is not set with APP_ prefix, so it should be missing
    assert_eq!(errors.len(), 1);
    assert_eq!(errors.iter().next().unwrap().path(), Some("apikey"));
}
