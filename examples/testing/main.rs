//! Configuration testing patterns example.
//!
//! This example demonstrates how to test configuration loading without
//! real files or environment variables using MockEnv.
//!
//! Run tests with: cargo test

use premortem::prelude::*;
use serde::Deserialize;

/// Application configuration used throughout the tests.
#[derive(Debug, Deserialize, DeriveValidate, PartialEq)]
pub struct AppConfig {
    #[validate(non_empty)]
    pub host: String,

    #[validate(range(1..=65535))]
    pub port: u16,

    #[serde(default)]
    pub debug: bool,
}

fn main() {
    println!("This example is meant to be run with `cargo test`");
    println!();
    println!("Available tests:");
    println!("  - test_load_from_toml: Load config from mock TOML file");
    println!("  - test_env_override: Environment variable overrides");
    println!("  - test_validation_errors_accumulate: All errors collected");
    println!("  - test_missing_required_file: Error on missing file");
    println!("  - test_optional_file_missing: Optional file skipped gracefully");
    println!("  - test_permission_denied: I/O error handling");
    println!();
    println!("Run: cargo test");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test loading configuration from a mock TOML file.
    ///
    /// MockEnv allows testing file-based configuration without
    /// creating actual files on the filesystem.
    #[test]
    fn test_load_from_toml() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            host = "localhost"
            port = 8080
            "#,
        );

        let config = Config::<AppConfig>::builder()
            .source(Toml::file("config.toml"))
            .build_with_env(&env)
            .expect("should load successfully");

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 8080);
        assert!(!config.debug);
    }

    /// Test that environment variables override file values.
    ///
    /// This is a common pattern: load base config from file,
    /// allow overrides via environment variables.
    #[test]
    fn test_env_override() {
        let env = MockEnv::new()
            .with_file(
                "config.toml",
                r#"
                host = "localhost"
                port = 8080
                "#,
            )
            .with_env("APP_PORT", "9000")
            .with_env("APP_DEBUG", "true");

        let config = Config::<AppConfig>::builder()
            .source(Toml::file("config.toml"))
            .source(Env::prefix("APP_"))
            .build_with_env(&env)
            .expect("should load successfully");

        assert_eq!(config.host, "localhost"); // From file
        assert_eq!(config.port, 9000); // Overridden by env
        assert!(config.debug); // From env
    }

    /// Test that validation collects ALL errors, not just the first.
    ///
    /// This is premortem's core feature: find all configuration
    /// problems in one run instead of fix-run-fix cycles.
    #[test]
    fn test_validation_errors_accumulate() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            host = ""
            port = 0
            "#,
        );

        let result = Config::<AppConfig>::builder()
            .source(Toml::file("config.toml"))
            .build_with_env(&env);

        assert!(result.is_err());
        let errors = result.unwrap_err();

        // Both validation errors should be present
        assert_eq!(errors.len(), 2, "Expected 2 errors, got: {:?}", errors);
    }

    /// Test error handling for missing required files.
    ///
    /// When a file is not marked as optional, a missing file
    /// should result in a SourceError with NotFound kind.
    #[test]
    fn test_missing_required_file() {
        let env = MockEnv::new();

        let result = Config::<AppConfig>::builder()
            .source(Toml::file("missing.toml"))
            .build_with_env(&env);

        assert!(result.is_err());
        let errors = result.unwrap_err();

        // Check that we get a NotFound error
        match errors.first() {
            ConfigError::SourceError { kind, .. } => {
                assert!(
                    matches!(kind, SourceErrorKind::NotFound { .. }),
                    "Expected NotFound, got: {:?}",
                    kind
                );
            }
            other => panic!("Expected SourceError, got: {:?}", other),
        }
    }

    /// Test that optional files don't error when missing.
    ///
    /// Use `.optional()` when a config file might not exist.
    #[test]
    fn test_optional_file_missing() {
        let env = MockEnv::new()
            .with_env("APP_HOST", "localhost")
            .with_env("APP_PORT", "8080");

        let config = Config::<AppConfig>::builder()
            .source(Toml::file("config.toml").optional())
            .source(Env::prefix("APP_"))
            .build_with_env(&env)
            .expect("should load from env only");

        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 8080);
    }

    /// Test handling of permission denied errors.
    ///
    /// MockEnv can simulate various I/O error conditions.
    #[test]
    fn test_permission_denied() {
        let env = MockEnv::new().with_unreadable_file("secret.toml");

        let result = Config::<AppConfig>::builder()
            .source(Toml::file("secret.toml"))
            .build_with_env(&env);

        assert!(result.is_err());
        let errors = result.unwrap_err();

        // Check that we get an IoError, not NotFound
        match errors.first() {
            ConfigError::SourceError { kind, .. } => {
                assert!(
                    matches!(kind, SourceErrorKind::IoError { .. }),
                    "Expected IoError, got: {:?}",
                    kind
                );
            }
            other => panic!("Expected SourceError, got: {:?}", other),
        }
    }

    /// Test configuration with nested structures.
    #[test]
    fn test_nested_config() {
        #[derive(Debug, Deserialize, DeriveValidate)]
        struct ServerConfig {
            #[validate(non_empty)]
            host: String,
            #[validate(range(1..=65535))]
            port: u16,
        }

        #[derive(Debug, Deserialize, DeriveValidate)]
        struct NestedConfig {
            #[validate(nested)]
            server: ServerConfig,
        }

        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            [server]
            host = "localhost"
            port = 8080
            "#,
        );

        let config = Config::<NestedConfig>::builder()
            .source(Toml::file("config.toml"))
            .build_with_env(&env)
            .expect("should load successfully");

        assert_eq!(config.server.host, "localhost");
        assert_eq!(config.server.port, 8080);
    }

    /// Test that nested validation errors include proper paths.
    #[test]
    fn test_nested_validation_errors_have_paths() {
        #[derive(Debug, Deserialize, DeriveValidate)]
        struct ServerConfig {
            #[validate(non_empty)]
            host: String,
        }

        #[derive(Debug, Deserialize, DeriveValidate)]
        struct NestedConfig {
            #[validate(nested)]
            server: ServerConfig,
        }

        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            [server]
            host = ""
            "#,
        );

        let result = Config::<NestedConfig>::builder()
            .source(Toml::file("config.toml"))
            .build_with_env(&env);

        assert!(result.is_err());
        let errors = result.unwrap_err();

        // Error path should include the nested path
        let paths: Vec<_> = errors.iter().filter_map(|e| e.path()).collect();
        assert!(
            paths.iter().any(|p| p.contains("server")),
            "Expected path to include 'server', got: {:?}",
            paths
        );
    }

    /// Test dynamic file content changes.
    ///
    /// MockEnv allows changing file contents during test execution.
    #[test]
    fn test_dynamic_file_changes() {
        let env = MockEnv::new().with_file(
            "config.toml",
            r#"
            host = "localhost"
            port = 8080
            "#,
        );

        // First load
        let config1 = Config::<AppConfig>::builder()
            .source(Toml::file("config.toml"))
            .build_with_env(&env)
            .expect("should load");
        assert_eq!(config1.port, 8080);

        // Change file content
        env.set_file(
            "config.toml",
            r#"
            host = "production"
            port = 9000
            "#,
        );

        // Second load sees new content
        let config2 = Config::<AppConfig>::builder()
            .source(Toml::file("config.toml"))
            .build_with_env(&env)
            .expect("should load");
        assert_eq!(config2.port, 9000);
        assert_eq!(config2.host, "production");
    }
}
