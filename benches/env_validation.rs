//! Performance benchmarks for environment variable validation.
//!
//! Compares manual imperative validation (90+ lines) vs declarative
//! source-level validation to verify <5% overhead requirement.

use criterion::{criterion_group, criterion_main, Criterion};
use premortem::env::{ConfigEnv, MockEnv};
use premortem::prelude::*;
use serde::Deserialize;
use std::hint::black_box;

// =============================================================================
// Manual Validation (Baseline)
// =============================================================================

mod manual {
    use super::*;

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    #[derive(Debug)]
    pub struct ConfigError(String);

    impl Config {
        pub fn load<E: ConfigEnv>(env: &E) -> Result<Self, ConfigError> {
            let jwt_secret = env
                .get_env("APP_JWTSECRET")
                .ok_or_else(|| ConfigError("APP_JWTSECRET is required".to_string()))?;

            if jwt_secret.len() < 32 {
                return Err(ConfigError(
                    "JWT_SECRET must be at least 32 characters long".to_string(),
                ));
            }

            let database_url = env
                .get_env("APP_DATABASEURL")
                .ok_or_else(|| ConfigError("APP_DATABASEURL is required".to_string()))?;

            let github_client_id = env
                .get_env("APP_GITHUBCLIENTID")
                .ok_or_else(|| ConfigError("APP_GITHUBCLIENTID is required".to_string()))?;

            let github_client_secret = env
                .get_env("APP_GITHUBCLIENTSECRET")
                .ok_or_else(|| ConfigError("APP_GITHUBCLIENTSECRET is required".to_string()))?;

            let redis_url = env
                .get_env("APP_REDISURL")
                .ok_or_else(|| ConfigError("APP_REDISURL is required".to_string()))?;

            let smtp_host = env
                .get_env("APP_SMTPHOST")
                .ok_or_else(|| ConfigError("APP_SMTPHOST is required".to_string()))?;

            let smtp_port = env
                .get_env("APP_SMTPPORT")
                .ok_or_else(|| ConfigError("APP_SMTPPORT is required".to_string()))?
                .parse::<u16>()
                .map_err(|_| ConfigError("APP_SMTPPORT must be a valid port number".to_string()))?;

            let smtp_username = env
                .get_env("APP_SMTPUSERNAME")
                .ok_or_else(|| ConfigError("APP_SMTPUSERNAME is required".to_string()))?;

            let smtp_password = env
                .get_env("APP_SMTPPASSWORD")
                .ok_or_else(|| ConfigError("APP_SMTPPASSWORD is required".to_string()))?;

            let api_key = env
                .get_env("APP_APIKEY")
                .ok_or_else(|| ConfigError("APP_APIKEY is required".to_string()))?;

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
// Declarative Validation (New Approach)
// =============================================================================

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

fn create_test_env() -> MockEnv {
    MockEnv::new()
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
        .with_env("APP_APIKEY", "api-key-xyz")
}

fn bench_manual_validation(c: &mut Criterion) {
    let env = create_test_env();

    c.bench_function("manual_validation", |b| {
        b.iter(|| {
            let config = manual::Config::load(black_box(&env));
            black_box(config.unwrap())
        })
    });
}

fn bench_declarative_validation(c: &mut Criterion) {
    let env = create_test_env();

    c.bench_function("declarative_validation", |b| {
        b.iter(|| {
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
                .build_with_env(black_box(&env));
            black_box(config.unwrap())
        })
    });
}

fn bench_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("env_validation_comparison");

    let env = create_test_env();

    group.bench_function("manual (baseline)", |b| {
        b.iter(|| {
            let config = manual::Config::load(black_box(&env));
            black_box(config.unwrap())
        })
    });

    group.bench_function("declarative (new)", |b| {
        b.iter(|| {
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
                .build_with_env(black_box(&env));
            black_box(config.unwrap())
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_manual_validation,
    bench_declarative_validation,
    bench_comparison
);
criterion_main!(benches);
