//! Environment variable validation example.
//!
//! This example demonstrates environment variable validation ergonomics,
//! showing how to declaratively mark environment variables as required
//! and get accumulated errors for all missing variables.
//!
//! Before: 90+ lines of imperative validation code
//! After: ~15 lines of declarative configuration
//!
//! Run with: cargo run --example env-validation

use premortem::prelude::*;
use serde::Deserialize;

// =============================================================================
// BEFORE: Manual Validation (90+ lines of boilerplate)
// =============================================================================

#[allow(dead_code)]
mod before {
    use premortem::env::ConfigEnv;

    pub struct Config {
        pub jwt_secret: String,
        pub database_url: String,
        pub github_client_id: String,
        pub github_client_secret: String,
        pub redis_url: String,
        pub smtp_host: String,
        pub smtp_port: u16,
        pub smtp_username: String,
        pub smtp_password: String,
        pub api_key: String,
    }

    #[derive(Debug)]
    pub struct ConfigError(String);

    impl Config {
        pub fn load<E: ConfigEnv>(env: &E) -> Result<Self, ConfigError> {
            // 90+ lines of repetitive validation code
            let jwt_secret = env
                .get_env("APP_JWT_SECRET")
                .ok_or_else(|| ConfigError("APP_JWT_SECRET is required".to_string()))?;

            if jwt_secret.len() < 32 {
                return Err(ConfigError(
                    "JWT_SECRET must be at least 32 characters long".to_string(),
                ));
            }

            let database_url = env
                .get_env("APP_DATABASE_URL")
                .ok_or_else(|| ConfigError("APP_DATABASE_URL is required".to_string()))?;

            let github_client_id = env
                .get_env("APP_GITHUB_CLIENT_ID")
                .ok_or_else(|| ConfigError("APP_GITHUB_CLIENT_ID is required".to_string()))?;

            let github_client_secret = env
                .get_env("APP_GITHUB_CLIENT_SECRET")
                .ok_or_else(|| ConfigError("APP_GITHUB_CLIENT_SECRET is required".to_string()))?;

            let redis_url = env
                .get_env("APP_REDIS_URL")
                .ok_or_else(|| ConfigError("APP_REDIS_URL is required".to_string()))?;

            let smtp_host = env
                .get_env("APP_SMTP_HOST")
                .ok_or_else(|| ConfigError("APP_SMTP_HOST is required".to_string()))?;

            let smtp_port = env
                .get_env("APP_SMTP_PORT")
                .ok_or_else(|| ConfigError("APP_SMTP_PORT is required".to_string()))?
                .parse::<u16>()
                .map_err(|_| {
                    ConfigError("APP_SMTP_PORT must be a valid port number".to_string())
                })?;

            let smtp_username = env
                .get_env("APP_SMTP_USERNAME")
                .ok_or_else(|| ConfigError("APP_SMTP_USERNAME is required".to_string()))?;

            let smtp_password = env
                .get_env("APP_SMTP_PASSWORD")
                .ok_or_else(|| ConfigError("APP_SMTP_PASSWORD is required".to_string()))?;

            let api_key = env
                .get_env("APP_API_KEY")
                .ok_or_else(|| ConfigError("APP_API_KEY is required".to_string()))?;

            Ok(Self {
                jwt_secret,
                database_url,
                github_client_id,
                github_client_secret,
                redis_url,
                smtp_host,
                smtp_port,
                smtp_username,
                smtp_password,
                api_key,
            })
        }
    }
}

// =============================================================================
// AFTER: Declarative Configuration (~15 lines)
// =============================================================================

/// Modern configuration with declarative source-level required vars
/// and value-level validation.
#[allow(dead_code)]
#[derive(Debug, Deserialize, DeriveValidate)]
struct AppConfig {
    #[validate(min_length(32))]
    jwtsecret: String,

    databaseurl: String,
    githubclientid: String,
    githubclientsecret: String,
    redisurl: String,
    smtphost: String,

    #[validate(range(1..=65535))]
    smtpport: u16,

    smtpusername: String,
    smtppassword: String,
    apikey: String,
}

fn main() {
    println!("=== Environment Variable Validation Example ===\n");

    // Example 1: All required variables present
    println!("Example 1: All required variables present");
    example_all_present();

    // Example 2: Some required variables missing (error accumulation)
    println!("\nExample 2: Multiple required variables missing");
    example_multiple_missing();

    // Example 3: Single missing variable
    println!("\nExample 3: Single required variable missing");
    example_single_missing();

    // Example 4: Required variables present but validation fails
    println!("\nExample 4: Required variables present but validation fails");
    example_validation_fails();
}

fn example_all_present() {
    let env = MockEnv::new()
        .with_env(
            "APP_JWTSECRET",
            "this-is-a-very-long-secret-key-with-more-than-32-chars",
        )
        .with_env("APP_DATABASEURL", "postgresql://localhost/mydb")
        .with_env("APP_GITHUBCLIENTID", "client123")
        .with_env("APP_GITHUBCLIENTSECRET", "secret456")
        .with_env("APP_REDISURL", "redis://localhost:6379")
        .with_env("APP_SMTPHOST", "smtp.example.com")
        .with_env("APP_SMTPPORT", "587")
        .with_env("APP_SMTPUSERNAME", "user@example.com")
        .with_env("APP_SMTPPASSWORD", "password123")
        .with_env("APP_APIKEY", "api-key-xyz");

    let config = Config::<AppConfig>::builder()
        .source(Env::prefix("APP_").require_all(&[
            "JWTSECRET",
            "DATABASEURL",
            "GITHUBCLIENTID",
            "GITHUBCLIENTSECRET",
            "REDISURL",
            "SMTPHOST",
            "SMTPPORT",
            "SMTPUSERNAME",
            "SMTPPASSWORD",
            "APIKEY",
        ]))
        .build_with_env(&env);

    match config {
        Ok(cfg) => {
            println!("✓ Configuration loaded successfully!");
            println!("  Database: {}", cfg.databaseurl);
            println!("  SMTP: {}:{}", cfg.smtphost, cfg.smtpport);
        }
        Err(e) => {
            println!("✗ Configuration failed: {}", e);
        }
    }
}

fn example_multiple_missing() {
    // Only provide some of the required variables
    let env = MockEnv::new()
        .with_env(
            "APP_JWTSECRET",
            "this-is-a-very-long-secret-key-with-more-than-32-chars",
        )
        .with_env("APP_DATABASEURL", "postgresql://localhost/mydb");
    // Missing: GITHUBCLIENTID, GITHUBCLIENTSECRET, REDISURL, etc.

    let config = Config::<AppConfig>::builder()
        .source(Env::prefix("APP_").require_all(&[
            "JWTSECRET",
            "DATABASEURL",
            "GITHUBCLIENTID",
            "GITHUBCLIENTSECRET",
            "REDISURL",
            "SMTPHOST",
            "SMTPPORT",
            "SMTPUSERNAME",
            "SMTPPASSWORD",
            "APIKEY",
        ]))
        .build_with_env(&env);

    match config {
        Ok(_) => {
            println!("✗ Should have failed!");
        }
        Err(errors) => {
            println!("✓ Caught {} missing environment variables:", errors.len());
            for error in errors.iter() {
                if let Some(source_loc) = error.source_location() {
                    println!("  - {}", source_loc.source);
                }
            }
            println!("\n  All errors reported at once (not fail-fast)!");
        }
    }
}

fn example_single_missing() {
    let env = MockEnv::new()
        .with_env("APP_DATABASEURL", "postgresql://localhost/mydb")
        .with_env("APP_GITHUBCLIENTID", "client123")
        .with_env("APP_GITHUBCLIENTSECRET", "secret456")
        .with_env("APP_REDISURL", "redis://localhost:6379")
        .with_env("APP_SMTPHOST", "smtp.example.com")
        .with_env("APP_SMTPPORT", "587")
        .with_env("APP_SMTPUSERNAME", "user@example.com")
        .with_env("APP_SMTPPASSWORD", "password123")
        .with_env("APP_APIKEY", "api-key-xyz");
    // Missing only JWTSECRET

    let config = Config::<AppConfig>::builder()
        .source(Env::prefix("APP_").require_all(&[
            "JWTSECRET",
            "DATABASEURL",
            "GITHUBCLIENTID",
            "GITHUBCLIENTSECRET",
            "REDISURL",
            "SMTPHOST",
            "SMTPPORT",
            "SMTPUSERNAME",
            "SMTPPASSWORD",
            "APIKEY",
        ]))
        .build_with_env(&env);

    match config {
        Ok(_) => {
            println!("✗ Should have failed!");
        }
        Err(errors) => {
            println!("✓ Caught missing environment variable:");
            for error in errors.iter() {
                if let Some(source_loc) = error.source_location() {
                    println!("  - {}", source_loc.source);
                }
            }
        }
    }
}

fn example_validation_fails() {
    let env = MockEnv::new()
        .with_env("APP_JWTSECRET", "short") // Too short!
        .with_env("APP_DATABASEURL", "postgresql://localhost/mydb")
        .with_env("APP_GITHUBCLIENTID", "client123")
        .with_env("APP_GITHUBCLIENTSECRET", "secret456")
        .with_env("APP_REDISURL", "redis://localhost:6379")
        .with_env("APP_SMTPHOST", "smtp.example.com")
        .with_env("APP_SMTPPORT", "99999") // Invalid port!
        .with_env("APP_SMTPUSERNAME", "user@example.com")
        .with_env("APP_SMTPPASSWORD", "password123")
        .with_env("APP_APIKEY", "api-key-xyz");

    let config = Config::<AppConfig>::builder()
        .source(Env::prefix("APP_").require_all(&[
            "JWTSECRET",
            "DATABASEURL",
            "GITHUBCLIENTID",
            "GITHUBCLIENTSECRET",
            "REDISURL",
            "SMTPHOST",
            "SMTPPORT",
            "SMTPUSERNAME",
            "SMTPPASSWORD",
            "APIKEY",
        ]))
        .build_with_env(&env);

    match config {
        Ok(_) => {
            println!("✗ Should have failed validation!");
        }
        Err(errors) => {
            println!("✓ Caught {} validation errors:", errors.len());
            for error in errors.iter() {
                if let Some(path) = error.path() {
                    println!("  - {}: validation failed", path);
                }
            }
            println!("\n  Note: Source-level (presence) validation happens BEFORE");
            println!("  value-level validation, so missing vars are caught first!");
        }
    }
}
